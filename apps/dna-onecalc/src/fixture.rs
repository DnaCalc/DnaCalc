//! The session's sheets and inputs as calc-fml sees them: a [`ReferenceCatalog`] over the named
//! sheets and a [`ReferenceReader`] over the typed inputs that records every read.

use crate::session::InputValue;
use calc_fml::reference::CellRun;
use calc_fml::{ReferenceCatalog, ReferenceReader};
use calc_func::value::{CalcValue, CellKey, GridArea, IdAllocator, SheetId};
use std::collections::BTreeMap;
use std::ops::ControlFlow;

/// Named sheets and the inputs on them.
#[derive(Debug, Default)]
pub(crate) struct Fixture {
    ids: IdAllocator<SheetId>,
    /// Sheet names in the order they were added.
    sheets: Vec<(String, SheetId)>,
    /// Inputs in row-major order: keys order by sheet, then row, then column.
    inputs: BTreeMap<CellKey, InputValue>,
}

impl Fixture {
    /// Adds a sheet; the caller has checked the name.
    pub(crate) fn add_sheet(&mut self, name: &str) -> SheetId {
        let id = self
            .ids
            .issue()
            .expect("a session adds far fewer sheets than an allocator has slots");
        self.sheets.push((name.to_owned(), id));
        id
    }

    pub(crate) fn sheets(&self) -> &[(String, SheetId)] {
        &self.sheets
    }

    pub(crate) fn sheet_name(&self, id: SheetId) -> Option<&str> {
        self.sheets
            .iter()
            .find(|(_, sheet)| *sheet == id)
            .map(|(name, _)| name.as_str())
    }

    pub(crate) fn inputs(&self) -> &BTreeMap<CellKey, InputValue> {
        &self.inputs
    }

    pub(crate) fn inputs_mut(&mut self) -> &mut BTreeMap<CellKey, InputValue> {
        &mut self.inputs
    }
}

impl ReferenceCatalog for Fixture {
    /// Sheet names match regardless of case, compared by Unicode lowercase; Excel's exact case
    /// folding for non-ASCII names is not measured here.
    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        let wanted = name.to_lowercase();
        self.sheets
            .iter()
            .find(|(held, _)| held.to_lowercase() == wanted)
            .map(|(_, id)| *id)
    }
}

/// One area an evaluation asked for, and the runs of cells the reader returned for it, in the
/// order returned.
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    pub area: GridArea,
    pub runs: Vec<ReadRun>,
}

/// Consecutive cells of a read, in row-major order, starting at `start`.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadRun {
    pub start: CellKey,
    pub cells: RunCells,
}

/// What a run of a read held.
#[derive(Clone, Debug, PartialEq)]
pub enum RunCells {
    /// Cells holding these input values, one per cell.
    Values(Vec<CalcValue>),
    /// This many cells whose input is blank.
    Blank(u64),
    /// This many cells the session holds no input for: the reader reports them missing, and the
    /// evaluation that needs one stops with a missing-input fault.
    Missing(u64),
}

/// The reader one evaluation runs with: it reads the fixture's inputs and records each request
/// and each run it returns.
pub(crate) struct TracingReader<'f> {
    fixture: &'f Fixture,
    reads: Vec<Read>,
}

impl<'f> TracingReader<'f> {
    pub(crate) fn new(fixture: &'f Fixture) -> Self {
        TracingReader {
            fixture,
            reads: Vec::new(),
        }
    }

    pub(crate) fn into_reads(self) -> Vec<Read> {
        self.reads
    }
}

/// The class of a stored input within a run: a value or a blank.
enum Stored<'a> {
    Value(&'a CalcValue),
    Blank,
}

impl ReferenceReader for TracingReader<'_> {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        let first = area.first();
        let last = area.last();
        let width = u64::from(area.column_count());
        let total = area.cell_count();
        let at = |offset: u64| {
            let row = first.row() + u32::try_from(offset / width).expect("a row of the area");
            let column =
                first.column() + u32::try_from(offset % width).expect("a column of the area");
            CellKey::new(first.sheet(), row, column).expect("a cell of the area")
        };
        let mut runs = Vec::new();
        let mut emit = |run: CellRun<'_>, start: u64| {
            runs.push(ReadRun {
                start: at(start),
                cells: match run {
                    CellRun::Values(values) => RunCells::Values(values.to_vec()),
                    CellRun::Blank(count) => RunCells::Blank(count),
                    CellRun::Missing(count) => RunCells::Missing(count),
                },
            });
            sink(run)
        };
        // Stored inputs between the area's first and last cells come in row-major order; those
        // outside its columns are skipped. Every cell between two stored inputs of the area is
        // one the session holds nothing for.
        let fixture: &Fixture = self.fixture;
        let stored = fixture
            .inputs
            .range(first..=last)
            .filter(|(key, _)| area.contains(**key))
            .map(|(key, input)| {
                let offset = u64::from(key.row() - first.row()) * width
                    + u64::from(key.column() - first.column());
                let value = input.value();
                let class = if value.is_empty() {
                    Stored::Blank
                } else {
                    Stored::Value(value)
                };
                (offset, class)
            });
        let mut next = 0u64;
        let mut values: Vec<CalcValue> = Vec::new();
        let mut values_start = 0u64;
        let mut blanks = 0u64;
        let mut blanks_start = 0u64;
        let mut stopped = false;
        for (offset, class) in stored {
            // A run ends where the next stored cell does not continue it.
            let follows = offset == next;
            let continues_values = follows && matches!(class, Stored::Value(_));
            let continues_blanks = follows && matches!(class, Stored::Blank);
            if !values.is_empty() && !continues_values {
                if emit(CellRun::Values(&values), values_start).is_break() {
                    stopped = true;
                    break;
                }
                values.clear();
            }
            if blanks > 0 && !continues_blanks {
                if emit(CellRun::Blank(blanks), blanks_start).is_break() {
                    stopped = true;
                    break;
                }
                blanks = 0;
            }
            if offset > next && emit(CellRun::Missing(offset - next), next).is_break() {
                stopped = true;
                break;
            }
            match class {
                Stored::Value(value) => {
                    if values.is_empty() {
                        values_start = offset;
                    }
                    values.push(value.clone());
                }
                Stored::Blank => {
                    if blanks == 0 {
                        blanks_start = offset;
                    }
                    blanks += 1;
                }
            }
            next = offset + 1;
        }
        if !stopped {
            let tail = if !values.is_empty() {
                emit(CellRun::Values(&values), values_start)
            } else if blanks > 0 {
                emit(CellRun::Blank(blanks), blanks_start)
            } else {
                ControlFlow::Continue(())
            };
            if tail.is_continue() && next < total {
                let _ = emit(CellRun::Missing(total - next), next);
            }
        }
        self.reads.push(Read { area, runs });
    }
}
