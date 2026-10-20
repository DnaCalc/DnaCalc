//! What a recalculation's evaluations read: the sheets of the input snapshot for CalcFml's
//! binding, and the cells of the workbook for its evaluation.
//!
//! A literal cell's value is its authored input, read from the snapshot's page. A formula cell's
//! value is the result this recalculation staged for it, or else the published result when that
//! was calculated; a file cache, a result not modelled and a formula this recalculation has yet
//! to evaluate are never handed out as values. The engine's reader is the only source of what an
//! evaluation reads: a caller's [`ReadObserver`] sees the runs it reports and cannot change them.

use crate::result::FormulaResult;
use crate::state::EngineState;
use calc_doc_model::{CellInput, ModelRoot};
use calc_fml::ReferenceCatalog;
use calc_fml::reference::{CellRun, ReferenceReader};
use calc_value::{CalcValue, CellKey, GridArea, NodeId, SheetId};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

/// Observes the reads of a recalculation's evaluations, apart from what the recalculation
/// publishes (plan 6.5): [`crate::step`] calls it with each run the engine's reader reports for an
/// area that the evaluation of the formula at `formula` asks for, before the evaluation receives
/// that run, so it sees exactly the runs the evaluation receives. `offset` is the index of the
/// run's first cell within `area`, in row-major order from 0: a run at offset 0 begins a read.
///
/// An observer cannot change what an evaluation reads: it is given no reader and no sink, only
/// the run, borrowed. Its one effect is to stop the recalculation: returning
/// [`ControlFlow::Break`] withholds the run, the evaluation receives the rest of the area as cells
/// the reader cannot supply, and the recalculation fails with
/// [`crate::RecalcFailure::ReadFailed`] and publishes nothing.
pub trait ReadObserver {
    fn observe(
        &mut self,
        formula: CellKey,
        area: GridArea,
        offset: u64,
        run: CellRun<'_>,
    ) -> ControlFlow<()>;
}

/// The observer that records nothing and never stops a recalculation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Unobserved;

impl ReadObserver for Unobserved {
    fn observe(&mut self, _: CellKey, _: GridArea, _: u64, _: CellRun<'_>) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }
}

/// The sheets of a snapshot by name, for CalcFml's binding. A name matches a sheet whose name has
/// the same UTF-16 code units, or, when both decode, the same characters once each is mapped to
/// its Unicode uppercase. Excel compares sheet names without case; which characters its
/// comparison folds is not measured: bead dc-85qk measures Excel's sheet-name rules, case-blind
/// matching included.
pub(crate) struct SnapshotCatalog<'r>(pub(crate) &'r ModelRoot);

impl ReferenceCatalog for SnapshotCatalog<'_> {
    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        let wanted: Vec<u16> = name.encode_utf16().collect();
        self.0
            .sheets()
            .iter()
            .find(|sheet| {
                let units = sheet.name().text().utf16_code_units();
                units == wanted.as_slice()
                    || String::from_utf16(units).is_ok_and(|held| {
                        held.chars()
                            .flat_map(char::to_uppercase)
                            .eq(name.chars().flat_map(char::to_uppercase))
                    })
            })
            .map(|sheet| sheet.id())
    }
}

/// The engine's reader of the workbook's cells during one recalculation.
pub(crate) struct WorkbookReader<'a> {
    pub(crate) inputs: &'a ModelRoot,
    pub(crate) state: &'a EngineState,
    /// The results this recalculation has staged.
    pub(crate) staged: &'a BTreeMap<NodeId, FormulaResult>,
    /// The nodes this recalculation evaluates.
    pub(crate) scheduled: &'a BTreeSet<NodeId>,
    /// The first formula cell read that this recalculation schedules but has not evaluated yet.
    pub(crate) unscheduled: Option<CellKey>,
}

impl<'a> WorkbookReader<'a> {
    /// The value of the formula cell `at`, or `None` when it has none to hand out. A published
    /// value it hands out was calculated under this recalculation's numeric mode: when the mode
    /// differs from the published generation's, [`crate::begin_recalc`] schedules every formula
    /// node, and no published value is handed out at all.
    fn formula_value(&mut self, at: CellKey) -> Option<&'a CalcValue> {
        let (state, staged) = (self.state, self.staged);
        let node = state.node_at(at)?;
        let result = match staged.get(&node) {
            Some(staged) => staged,
            None if self.scheduled.contains(&node) => {
                self.unscheduled.get_or_insert(at);
                return None;
            }
            None => state.result(at)?,
        };
        result.calculated().map(|(value, _)| value)
    }
}

impl ReferenceReader for WorkbookReader<'_> {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        let total = area.cell_count();
        let inputs = self.inputs;
        let Some(occupied) = inputs.cells(area) else {
            let _ = sink(CellRun::Missing(total));
            return;
        };
        let (first, width) = (area.first(), u64::from(area.column_count()));
        let mut covered = 0u64;
        for (at, input) in occupied {
            let index =
                u64::from(at.row() - first.row()) * width + u64::from(at.column() - first.column());
            if index > covered && sink(CellRun::Blank(index - covered)).is_break() {
                return;
            }
            covered = index + 1;
            let flow = match literal_value(input) {
                Some(value) => sink(CellRun::Values(&[value])),
                None => match self.formula_value(at) {
                    Some(value) => sink(CellRun::Values(std::slice::from_ref(value))),
                    None => sink(CellRun::Missing(1)),
                },
            };
            if flow.is_break() {
                return;
            }
        }
        if total > covered {
            let _ = sink(CellRun::Blank(total - covered));
        }
    }
}

/// The value of a literal input; `None` for a formula, whose value is its result.
fn literal_value(input: &CellInput) -> Option<CalcValue> {
    match input {
        CellInput::Number(number) => Some(CalcValue::number(number.get())),
        CellInput::Text(text) => Some(CalcValue::text(text.text().clone())),
        CellInput::Logical(logical) => Some(CalcValue::logical(*logical)),
        CellInput::Error(code) => Some(CalcValue::error(*code)),
        CellInput::Formula(_) => None,
    }
}
