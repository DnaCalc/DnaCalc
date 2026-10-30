//! A formula that no cell holds, evaluated against the published workbook (dc-6yw): the formula
//! of a conditional format's rule, of a data validation, a chart's series. It is compiled through
//! CalcFml against the snapshot's sheets and names like a cell's formula, placed at a cell, and
//! evaluated by CalcFml's one evaluator through the engine's own reader of the workbook, which
//! hands out each formula cell's published result, never a value the evaluation computes itself.
//!
//! Nothing is staged, published or filed in the graph: the evaluation reads the state and the
//! snapshot only. A read of a formula cell that has no published calculated value (a file cache,
//! a refusal, a stale result) is refused as CalcFml refuses a missing input, so a detached value
//! is never computed over a value nothing here calculated.

use crate::read::{Progress, RecalcView, SnapshotCatalog, WorkbookReader};
use crate::result::NotModeled;
use crate::spill::SpillIndex;
use crate::state::EngineState;
use calc_doc_model::ModelRoot;
use calc_fml::reference::{CellRead, CellRun, ReadContractError, ReferenceReader, SpillExtent};
use calc_fml::reference::{FormulaAnchor, ReferenceNotation};
use calc_fml::{CalcEnvironment, HostFacts, SubtotalExclusions};
use calc_fml::{
    CompiledPlan, EvalFault, EvalFrame, EvalScratch, EvaluationServices, FormulaForm, PlanCache,
    compile_loaded_with, evaluate, instantiate, parse_formula,
};
use calc_value::{CalcValue, CellKey, GridArea, NumericMode, SheetId};
use std::cell::Cell;
use std::ops::ControlFlow;
use std::sync::Arc;

/// A formula compiled for detached evaluation: its text read in A1 notation for a formula at
/// `origin` (a conditional format's formula is relative to the top-left cell of its range), to be
/// placed at any cell.
#[derive(Clone, Debug)]
pub struct DetachedFormula {
    plan: Arc<CompiledPlan>,
}

impl DetachedFormula {
    /// Compiles `text` (A1 notation, without `=`) as a legacy formula at `origin`, its sheets and
    /// names bound through `inputs`.
    pub fn compile(inputs: &ModelRoot, text: &str, origin: CellKey) -> Result<Self, NotModeled> {
        Self::compile_in(inputs, text, origin, FormulaForm::Legacy)
    }

    /// [`DetachedFormula::compile`] in the dynamic-array form, where a range of one area is the
    /// array of its values rather than the cell implicit intersection picks (a validation list's
    /// source).
    pub fn compile_dynamic(
        inputs: &ModelRoot,
        text: &str,
        origin: CellKey,
    ) -> Result<Self, NotModeled> {
        Self::compile_in(inputs, text, origin, FormulaForm::Dynamic)
    }

    fn compile_in(
        inputs: &ModelRoot,
        text: &str,
        origin: CellKey,
        form: FormulaForm,
    ) -> Result<Self, NotModeled> {
        let source = format!("={text}");
        let mut plans = PlanCache::new();
        let plan = compile_loaded_with(
            &parse_formula(&source),
            ReferenceNotation::A1,
            FormulaAnchor::new(origin),
            &SnapshotCatalog(inputs),
            &mut plans,
            form,
        )
        .map_err(NotModeled::Compile)?;
        Ok(DetachedFormula { plan })
    }

    /// The formula's value placed at `at` (its relative references moved by `at`'s offset from
    /// the origin it was compiled at), reading the workbook as `state` published it for
    /// `inputs`. A value a cell could not hold (a reference, an array) is given as CalcFml
    /// returned it, for the caller to read.
    pub fn evaluate(
        &self,
        state: &EngineState,
        inputs: &ModelRoot,
        at: CellKey,
    ) -> Result<CalcValue, NotModeled> {
        let instance = instantiate(&self.plan, FormulaAnchor::new(at));
        let progress = Progress::new(0);
        let spills = SpillIndex::default();
        let view = RecalcView {
            inputs,
            state,
            progress: &progress,
            spills: &spills,
            iterates: None,
            kept: None,
        };
        let uncomputed = Cell::new(None);
        let mut reader = Placed {
            cells: WorkbookReader::new(view, &uncomputed),
            at,
        };
        let catalog = SnapshotCatalog(inputs);
        let mode = state.published_mode.clone().unwrap_or(NumericMode::Fast);
        let mut scratch = EvalScratch::new();
        let outcome = {
            let mut services = EvaluationServices::new(&mode, &mut reader).with_catalog(&catalog);
            evaluate(EvalFrame::new(&instance, &mut scratch), &mut services)
        };
        if uncomputed.get().is_some() {
            return Err(NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "a detached formula that reads a formula cell with no published value",
                owner: "dc-6yw",
            }));
        }
        outcome.map_err(NotModeled::Evaluation)
    }
}

/// The engine's reader, with the cell the formula is placed at as its caller (ROW(), COLUMN() and
/// CELL without a reference read it).
struct Placed<'a> {
    cells: WorkbookReader<'a>,
    at: CellKey,
}

impl ReferenceReader for Placed<'_> {
    fn read_cell(&mut self, at: CellKey) -> Result<CellRead, ReadContractError> {
        self.cells.read_cell(at)
    }

    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        self.cells.read_area(area, sink);
    }

    fn spill_extent(&mut self, anchor: CellKey) -> SpillExtent {
        self.cells.spill_extent(anchor)
    }

    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<SubtotalExclusions> {
        self.cells.subtotal_exclusions(area)
    }

    fn environment(&self) -> Option<CalcEnvironment> {
        self.cells.environment()
    }

    fn host_facts(&mut self) -> Option<&mut dyn HostFacts> {
        Some(self)
    }
}

impl HostFacts for Placed<'_> {
    fn caller(&mut self) -> Option<CellKey> {
        Some(self.at)
    }

    fn is_formula(&mut self, cell: CellKey) -> Option<bool> {
        self.cells.host_facts()?.is_formula(cell)
    }

    fn formula_text(&mut self, cell: CellKey) -> Option<Option<String>> {
        self.cells.host_facts()?.formula_text(cell)
    }

    fn sheet_index(&mut self, sheet: SheetId) -> Option<u32> {
        self.cells.host_facts()?.sheet_index(sheet)
    }

    fn sheet_named(&mut self, name: &str) -> Option<SheetId> {
        self.cells.host_facts()?.sheet_named(name)
    }

    fn sheet_name(&mut self, sheet: SheetId) -> Option<String> {
        self.cells.host_facts()?.sheet_name(sheet)
    }

    fn sheet_count(&mut self) -> Option<u32> {
        self.cells.host_facts()?.sheet_count()
    }

    fn workbook_facts(&mut self) -> Option<std::sync::Arc<dyn calc_fml::WorkbookFacts>> {
        self.cells.host_facts()?.workbook_facts()
    }
}
