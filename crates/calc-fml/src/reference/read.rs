//! Value access for placed references: a [`ReferenceReader`] reports an area's cells as row-major
//! [`CellRun`]s, and [`read_cells`] asks it for one area and checks that the runs account for
//! every cell exactly once.

use calc_func::locale::CalcEnvironment;
use calc_func::{HostFacts, SubtotalExclusions};
use calc_func::value::{CalcValue, CellKey, GridArea};
use std::ops::ControlFlow;

/// Supplies the cells of grid areas.
pub trait ReferenceReader {
    /// Reports the cells of `area` to `sink` as runs in row-major order (each row left to right,
    /// rows top to bottom), starting at the area's first cell and continuing from the end of the
    /// previous run; a run may continue from the end of one row of the area to the start of the
    /// next. The runs cover every cell of the area exactly once, so blank and unsupplied cells
    /// are counted, not skipped. Once `sink` returns [`ControlFlow::Break`] the reader stops.
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>);

    /// The range spilled from the dynamic-array formula at `anchor`, which a spill reference
    /// (`A1#`) names. The default is [`SpillExtent::NotModelled`]: a reader that does not know
    /// spills says so, and the reference is refused.
    fn spill_extent(&mut self, anchor: CellKey) -> SpillExtent {
        let _ = anchor;
        SpillExtent::NotModelled
    }

    /// What SUBTOTAL and AGGREGATE leave out of `area` (calc-func's
    /// [`calc_func::CellReader::subtotal_exclusions`]): the cells holding a SUBTOTAL or AGGREGATE
    /// formula and the hidden and filtered rows. The default, `None`, says the reader cannot
    /// tell, and those functions refuse a reference rather than count cells Excel leaves out.
    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<SubtotalExclusions> {
        let _ = area;
        None
    }

    /// The calculation environment of the workbook (calc-func's
    /// [`calc_func::CellReader::environment`]): its date system and the host's locale profile and
    /// clock. The default, `None`, refuses the date, time and number-format functions.
    fn environment(&self) -> Option<CalcEnvironment> {
        None
    }

    /// The workbook's host facts (calc-func's [`calc_func::HostFacts`]) for the formula being
    /// evaluated. The default, `None`, refuses ISFORMULA, FORMULATEXT, SHEET and the like.
    fn host_facts(&mut self) -> Option<&mut dyn HostFacts> {
        None
    }
}

/// What a [`ReferenceReader`] knows of the range spilled from a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpillExtent {
    /// The cell's formula spills over this area, the cell its first.
    Area(GridArea),
    /// The cell spills nothing: the spill reference is #REF!.
    None,
    /// The reader does not know spills.
    NotModelled,
}

/// Consecutive cells of a read, in row-major order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellRun<'a> {
    /// Cells holding these values, borrowed from the reader's storage, one value per cell.
    Values(&'a [CalcValue]),
    /// This many cells that exist and hold nothing.
    Blank(u64),
    /// This many cells whose values the reader cannot supply, such as inputs a fixture leaves
    /// unresolved. They are neither blank nor any value, and a consumer must not treat them as
    /// either. (Unrelated to [`CalcValue::missing`], an omitted argument.)
    Missing(u64),
}

impl CellRun<'_> {
    /// The number of cells in the run.
    pub fn len(&self) -> u64 {
        match self {
            CellRun::Values(values) => values.len() as u64,
            CellRun::Blank(count) | CellRun::Missing(count) => *count,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A reader broke its contract for `area`: its runs covered `covered` cells, more than the area
/// holds, or fewer although the visit never stopped the read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadContractError {
    pub area: GridArea,
    pub covered: u64,
}

/// Reads `area` through `reader`, passing `visit` each non-empty run with the cell it starts at,
/// until the runs end or `visit` returns [`ControlFlow::Break`].
pub fn read_cells(
    reader: &mut dyn ReferenceReader,
    area: GridArea,
    visit: &mut dyn FnMut(CellKey, CellRun<'_>) -> ControlFlow<()>,
) -> Result<(), ReadContractError> {
    let total = area.cell_count();
    let width = u64::from(area.column_count());
    let first = area.first();
    let mut covered = 0u64;
    let mut overrun = false;
    let mut stopped = false;
    reader.read_area(area, &mut |run| {
        if stopped || overrun {
            return ControlFlow::Break(());
        }
        let len = run.len();
        if len == 0 {
            return ControlFlow::Continue(());
        }
        if len > total - covered {
            overrun = true;
            return ControlFlow::Break(());
        }
        // `covered` is below `total`, so the row offset fits the area's rows and the column
        // offset its columns.
        let row = first.row() + (covered / width) as u32;
        let column = first.column() + (covered % width) as u32;
        let Ok(start) = CellKey::new(first.sheet(), row, column) else {
            overrun = true;
            return ControlFlow::Break(());
        };
        covered += len;
        let flow = visit(start, run);
        if flow.is_break() {
            stopped = true;
        }
        flow
    });
    if overrun || (!stopped && covered != total) {
        return Err(ReadContractError { area, covered });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_func::value::{IdAllocator, SheetId};

    /// A reader that replays fixed runs, whatever area it is asked for.
    struct Replay(Vec<CellRun<'static>>);

    impl ReferenceReader for Replay {
        fn read_area(&mut self, _: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
            for run in &self.0 {
                if sink(*run).is_break() {
                    return;
                }
            }
        }
    }

    fn area_b2_c3() -> GridArea {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        GridArea::new(
            CellKey::new(sheet, 2, 2).unwrap(),
            CellKey::new(sheet, 3, 3).unwrap(),
        )
        .unwrap()
    }

    /// Each visited run's starting row and column and its length.
    type Starts = Vec<(u32, u32, u64)>;

    fn starts(runs: Vec<CellRun<'static>>) -> (Result<(), ReadContractError>, Starts) {
        let mut seen = Vec::new();
        let result = read_cells(&mut Replay(runs), area_b2_c3(), &mut |start, run| {
            seen.push((start.row(), start.column(), run.len()));
            ControlFlow::Continue(())
        });
        (result, seen)
    }

    static ONE: [CalcValue; 1] = [CalcValue::new(calc_func::value::CoreValue::Logical(true))];

    /// Runs wrap from the end of one row of the area to the start of the next, and each starts
    /// where the previous one ended.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn runs_start_where_the_previous_run_ended() {
        let (result, seen) = starts(vec![
            CellRun::Values(&ONE),
            CellRun::Blank(0),
            CellRun::Blank(2),
            CellRun::Missing(1),
        ]);
        assert_eq!(result, Ok(()));
        assert_eq!(seen, [(2, 2, 1), (2, 3, 2), (3, 3, 1)]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn too_few_or_too_many_cells_break_the_contract() {
        let area = area_b2_c3();
        let (short, _) = starts(vec![CellRun::Blank(3)]);
        assert_eq!(short, Err(ReadContractError { area, covered: 3 }));
        let (long, seen) = starts(vec![CellRun::Blank(3), CellRun::Missing(2)]);
        assert_eq!(long, Err(ReadContractError { area, covered: 3 }));
        assert_eq!(seen, [(2, 2, 3)]);
    }

    /// A visit that stops the read early is not a short read.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_stopped_read_is_not_short() {
        let mut visits = 0;
        let result = read_cells(
            &mut Replay(vec![
                CellRun::Blank(1),
                CellRun::Blank(1),
                CellRun::Blank(2),
            ]),
            area_b2_c3(),
            &mut |_, _| {
                visits += 1;
                ControlFlow::Break(())
            },
        );
        assert_eq!(result, Ok(()));
        assert_eq!(visits, 1);
    }
}
