//! A formula that no cell holds, evaluated against the published workbook (dc-6yw): the formula
//! of a conditional format's rule, of a data validation, a chart's series. It is compiled through
//! CalcFml against the snapshot's sheets and names like a cell's formula, placed at a cell, and
//! evaluated by CalcFml's one evaluator through the engine's own reader of the workbook, which
//! hands out each formula cell's published result, never a value the evaluation computes itself.
//!
//! Nothing is staged, published or filed in the graph: the evaluation reads the state and the
//! snapshot only. A read of a formula cell that has no published calculated value (a file cache,
//! a refusal) is refused as CalcFml refuses a missing input. Explicit name diagnostics also
//! refuse stale formula values. Existing rendering consumers retain their published-value
//! policy until they share a freshness snapshot across their whole projection context.

#[path = "detached_name.rs"]
mod name_inspection;

use crate::read::{Progress, RecalcView, SnapshotCatalog, WorkbookReader};
use crate::result::NotModeled;
use crate::spill::SpillIndex;
use crate::state::EngineState;
use calc_doc_model::ModelRoot;
use calc_fml::reference::{FormulaAnchor, ReferenceNotation};
use calc_fml::{
    CompiledPlan, EvalFault, EvalFrame, EvalScratch, FormulaEvaluationContext, FormulaForm,
    PlanCache, compile_loaded_with, evaluate, instantiate, parse_formula,
};
use calc_value::{CalcValue, CellKey, NumericMode};
use std::cell::Cell;
use std::sync::Arc;

/// A formula compiled for detached evaluation: its text read in A1 notation for a formula at
/// `origin` (a conditional format's formula is relative to the top-left cell of its range), to be
/// placed at any cell.
#[derive(Clone, Debug)]
pub struct DetachedFormula {
    reference_result: bool,
    plan: Arc<CompiledPlan>,
}

/// Evidence from an explicit diagnostic; it installs no cell or shared name result.
#[derive(Clone, Debug, PartialEq)]
pub struct DetachedNameEvaluation {
    pub outcome: Result<CalcValue, NotModeled>,
    pub numeric: NumericMode,
    pub invocations: Vec<calc_fml::NameInvocation>,
    pub omitted: u64,
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
            &SnapshotCatalog::new(inputs),
            &mut plans,
            form,
        )
        .map_err(NotModeled::Compile)?;
        Ok(DetachedFormula {
            plan,
            reference_result: false,
        })
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
        self.evaluate_recording(state, inputs, at, None, false)
    }

    /// Execute through the same diagnostic reader and evaluator while retaining bounded
    /// name-body evidence, including refused completions. No publication occurs.
    pub fn evaluate_with_name_evidence(
        &self,
        state: &EngineState,
        inputs: &ModelRoot,
        at: CellKey,
    ) -> DetachedNameEvaluation {
        let mut effects = calc_fml::UserFunctionEffects::default();
        let outcome = self.evaluate_recording(state, inputs, at, Some(&mut effects), true);
        DetachedNameEvaluation {
            outcome,
            numeric: state.published_mode.clone().unwrap_or(NumericMode::Fast),
            invocations: effects.name_invocations().to_vec(),
            omitted: effects.omitted_name_invocations(),
        }
    }

    fn evaluate_recording(
        &self,
        state: &EngineState,
        inputs: &ModelRoot,
        at: CellKey,
        effects: Option<&mut calc_fml::UserFunctionEffects>,
        fresh_snapshot: bool,
    ) -> Result<CalcValue, NotModeled> {
        let instance = instantiate(&self.plan, FormulaAnchor::new(at));
        let mut progress = Progress::new(usize::MAX);
        if fresh_snapshot {
            // One dirty closure per explicit name diagnostic. Rendering loops need
            // a shared context before adopting this stricter snapshot policy.
            let stale = state.stale(inputs).ok_or(NotModeled::Evaluation(
                EvalFault::NotModelled {
                    construct: "a diagnostic snapshot from another authored revision",
                    owner: "dc-hsa5.6",
                },
            ))?;
            for at in stale {
                if let Some(node) = state.node_at(at) {
                    progress.schedule(node);
                }
            }
        }
        let spills = SpillIndex::default();
        let view = RecalcView {
            linked_workbooks: &state.linked_workbooks,
            cached_observation: false,
            caller: None,
            scope: None,
            range_refused: None,
            boundary: None,
            boundary_caches: None,
            boundary_used: None,
            inputs,
            state,
            workbook_facts: state.workbook_facts.as_ref(),
            progress: &progress,
            spills: &spills,
            iterates: None,
            kept: None,
        };
        let uncomputed = Cell::new(None);
        let mut reader = WorkbookReader::new(view, &uncomputed);
        let mut host = WorkbookReader::new(view, &uncomputed);
        let catalog = SnapshotCatalog::new(inputs);
        let mode = state.published_mode.clone().unwrap_or(NumericMode::Fast);
        let mut scratch = EvalScratch::new();
        let outcome = {
            let mut services = FormulaEvaluationContext::new(
                &mode,
                &mut reader,
                state.environment(),
                Some(&mut host),
                calc_fml::ReadRequest::DIAGNOSTIC,
            )
            .with_catalog(&catalog);
            if let Some(effects) = effects {
                services = services.with_user_function_effects(effects);
            }
            if self.reference_result {
                services = services.with_reference_result();
            }
            evaluate(EvalFrame::new(&instance, &mut scratch), &mut services)
        };
        if uncomputed.get().is_some() {
            return Err(NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "a detached formula that reads a formula cell with no published value",
                owner: "dc-6yw",
            }));
        }
        if fresh_snapshot && (reader.read_external_links() || host.read_external_links()) {
            return Err(NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "a name diagnostic that consumes unresolved external-link cache values",
                owner: "dc-hsa5.6",
            }));
        }
        outcome.map_err(NotModeled::Evaluation)
    }
}
