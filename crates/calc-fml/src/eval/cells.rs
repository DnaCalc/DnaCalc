//! The one reader of an evaluation. The caller's [`ReferenceReader`] supplies every cell: the
//! evaluator reads a single cell through it directly, and a kernel reads a reference argument
//! through it as the `calc_func::CellReader` that [`ReaderCells`] makes of it. A run of values is
//! visited value by value, a run of blank cells is skipped (a cell a `CellReader` does not visit
//! is blank), and a run of missing cells ends the read as [`Unreadable`], recording the first
//! missing cell so that the evaluation reports which input it lacked.

use super::run::EvalFault;
use crate::reference::{CellRun, ReadContractError, ReferenceReader, read_cells};
use calc_func::value::{CalcValue, CellKey, GridArea, ReferenceLike};
use calc_func::{CellReader, Unreadable};
use std::cell::{Cell, RefCell};
use std::ops::ControlFlow;

/// Why the reader could not supply an area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReadRefusal {
    /// The reader reported this cell as missing: an input it cannot supply.
    Missing(CellKey),
    /// The reader's runs did not cover the area as its contract requires.
    Contract(ReadContractError),
}

impl ReadRefusal {
    pub(super) fn fault(self) -> EvalFault {
        match self {
            ReadRefusal::Missing(cell) => EvalFault::MissingInput { cell },
            ReadRefusal::Contract(error) => EvalFault::ReadContract(error),
        }
    }
}

/// The caller's reader, shared by the evaluator and every kernel call of one evaluation. A lazy
/// call's arguments run while its kernel holds the services, so both reach the reader through
/// this shared cell; a read borrows it only while the read runs, and no read starts inside
/// another (a kernel's visit only consumes values).
pub(super) struct SharedReader<'r> {
    reader: RefCell<&'r mut dyn ReferenceReader>,
    refusal: Cell<Option<ReadRefusal>>,
}

impl<'r> SharedReader<'r> {
    pub(super) fn new(reader: &'r mut dyn ReferenceReader) -> Self {
        SharedReader {
            reader: RefCell::new(reader),
            refusal: Cell::new(None),
        }
    }

    /// Forgets any refusal, before a call whose refusal must be told apart.
    pub(super) fn clear_refusal(&self) {
        self.refusal.set(None);
    }

    /// The refusal recorded since the last [`SharedReader::clear_refusal`], taken.
    pub(super) fn take_refusal(&self) -> Option<ReadRefusal> {
        self.refusal.take()
    }

    /// The value of a single-cell reference, where one value is needed: the cell's value, or
    /// [`CalcValue::empty`] for a blank cell. A reference to more than one cell is
    /// [`EvalFault::AreaAsValue`]: Excel gives such a reference one value by implicit
    /// intersection or spills it as an array, neither of which is modelled.
    pub(super) fn value_of(&self, reference: &ReferenceLike) -> Result<CalcValue, EvalFault> {
        let area = match reference.areas() {
            [area] if area.cell_count() == 1 => *area,
            areas => return Err(EvalFault::AreaAsValue { area: areas[0] }),
        };
        let mut value = None;
        let mut missing = false;
        let mut reader = self.reader.borrow_mut();
        let read = read_cells(&mut **reader, area, &mut |_, run| {
            match run {
                CellRun::Values(values) => value = values.first().cloned(),
                CellRun::Blank(_) => value = Some(CalcValue::empty()),
                CellRun::Missing(_) => missing = true,
            }
            ControlFlow::Break(())
        });
        if let Err(error) = read {
            return Err(EvalFault::ReadContract(error));
        }
        if missing {
            return Err(EvalFault::MissingInput { cell: area.first() });
        }
        value.ok_or(EvalFault::ReadContract(ReadContractError {
            area,
            covered: 0,
        }))
    }
}

/// The shared reader as calc-func's [`CellReader`].
pub(super) struct ReaderCells<'a, 'r> {
    shared: &'a SharedReader<'r>,
}

impl<'a, 'r> ReaderCells<'a, 'r> {
    pub(super) fn new(shared: &'a SharedReader<'r>) -> Self {
        ReaderCells { shared }
    }
}

impl CellReader for ReaderCells<'_, '_> {
    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
    ) -> Result<(), Unreadable> {
        let first = area.first();
        let last = area.last();
        let mut missing = None;
        let mut reader = self.shared.reader.borrow_mut();
        let read = read_cells(&mut **reader, area, &mut |start, run| {
            match run {
                CellRun::Values(values) => {
                    // `read_cells` has checked that the run lies inside the area, so each value's
                    // cell is the next one of the area in row-major order.
                    let (mut row, mut column) = (start.row(), start.column());
                    for value in values {
                        let cell = CellKey::new(first.sheet(), row, column)
                            .expect("read_cells keeps every run inside the area");
                        visit(cell, value)?;
                        if column == last.column() {
                            column = first.column();
                            row += 1;
                        } else {
                            column += 1;
                        }
                    }
                    ControlFlow::Continue(())
                }
                CellRun::Blank(_) => ControlFlow::Continue(()),
                CellRun::Missing(_) => {
                    missing = Some(start);
                    ControlFlow::Break(())
                }
            }
        });
        drop(reader);
        let refusal = match (read, missing) {
            (Err(error), _) => ReadRefusal::Contract(error),
            (Ok(()), Some(cell)) => ReadRefusal::Missing(cell),
            (Ok(()), None) => return Ok(()),
        };
        self.shared.refusal.set(Some(refusal));
        Err(Unreadable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_func::value::{IdAllocator, SheetId};

    /// A reader that replays fixed runs and counts the areas it is asked for.
    struct Replay {
        runs: Vec<CellRun<'static>>,
        asked: usize,
    }

    impl ReferenceReader for Replay {
        fn read_area(&mut self, _: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
            self.asked += 1;
            for run in &self.runs {
                if sink(*run).is_break() {
                    return;
                }
            }
        }
    }

    static TWO: [CalcValue; 2] = [
        CalcValue::new(calc_func::value::CoreValue::Logical(true)),
        CalcValue::new(calc_func::value::CoreValue::Logical(false)),
    ];

    fn b2_c4() -> GridArea {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        GridArea::new(
            CellKey::new(sheet, 2, 2).unwrap(),
            CellKey::new(sheet, 4, 3).unwrap(),
        )
        .unwrap()
    }

    /// Each value is visited with its own cell, wrapping from the end of a row of the area to the
    /// start of the next; blank runs are not visited.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn values_are_visited_cell_by_cell_and_blanks_skipped() {
        let area = b2_c4();
        let mut reader = Replay {
            runs: vec![
                CellRun::Blank(1),
                CellRun::Values(&TWO),
                CellRun::Blank(1),
                CellRun::Values(&TWO),
            ],
            asked: 0,
        };
        let shared = SharedReader::new(&mut reader);
        let mut seen = Vec::new();
        let read = ReaderCells::new(&shared).read_area(area, &mut |cell, value| {
            seen.push((cell.row(), cell.column(), value.as_logical()));
            ControlFlow::Continue(())
        });
        assert_eq!(read, Ok(()));
        assert_eq!(
            seen,
            [
                (2, 3, Some(true)),
                (3, 2, Some(false)),
                (4, 2, Some(true)),
                (4, 3, Some(false)),
            ]
        );
        assert_eq!(shared.take_refusal(), None);
    }

    /// A missing run ends the read as Unreadable and records its first cell; a visit that stops
    /// the read is not a refusal.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn missing_cells_are_unreadable() {
        let area = b2_c4();
        let mut reader = Replay {
            runs: vec![
                CellRun::Values(&TWO),
                CellRun::Missing(1),
                CellRun::Blank(3),
            ],
            asked: 0,
        };
        let shared = SharedReader::new(&mut reader);
        let mut visits = 0;
        let read = ReaderCells::new(&shared).read_area(area, &mut |_, _| {
            visits += 1;
            ControlFlow::Continue(())
        });
        assert_eq!(read, Err(Unreadable));
        assert_eq!(visits, 2);
        assert_eq!(
            shared.take_refusal(),
            Some(ReadRefusal::Missing(
                CellKey::new(area.sheet(), 3, 2).unwrap()
            ))
        );
        let stopped = ReaderCells::new(&shared).read_area(area, &mut |_, _| ControlFlow::Break(()));
        assert_eq!(stopped, Ok(()));
        assert_eq!(shared.take_refusal(), None);
        assert_eq!(reader.asked, 2);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_short_read_breaks_the_contract() {
        let area = b2_c4();
        let mut reader = Replay {
            runs: vec![CellRun::Blank(5)],
            asked: 0,
        };
        let shared = SharedReader::new(&mut reader);
        let read = ReaderCells::new(&shared).read_area(area, &mut |_, _| ControlFlow::Continue(()));
        assert_eq!(read, Err(Unreadable));
        assert_eq!(
            shared.take_refusal(),
            Some(ReadRefusal::Contract(ReadContractError {
                area,
                covered: 5
            }))
        );
    }
}
