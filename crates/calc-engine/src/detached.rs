//! A formula that no cell holds, evaluated against the published workbook (dc-6yw): the formula
//! of a conditional format's rule, of a data validation, a chart's series. It is compiled through
//! CalcFml against the snapshot's sheets and names like a cell's formula, placed at a cell, and
//! evaluated by CalcFml's one evaluator through the engine's own reader of the workbook, which
//! hands out each formula cell's published result, never a value the evaluation computes itself.
//!
//! Nothing is staged, published or filed in the graph: the evaluation reads the state and the
//! snapshot only. A read of a formula cell that has no published calculated value (a file cache,
//! a refusal) is refused as CalcFml refuses a missing input. Explicit name diagnostics,
//! validation audits and filter decisions also refuse stale or qualified cached inputs
//! through a shared current-value context. Rendering keeps its published-value policy.
//! Selected names can separately admit an owned NameCalculationWork. That explicit
//! pass can retain user-function effects across yields without publishing a cell;
//! ordinary detached contexts and diagnostics keep their existing read-only policy.

#[path = "detached_name.rs"]
mod name_inspection;
#[path = "detached_work.rs"]
mod calculation;
pub use calculation::{NameCalculationProgress, NameCalculationResult, NameCalculationWork};

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

/// Which published values a detached consumer is allowed to consume. Neither policy
/// admits calculation, effects, graph edges or publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetachedReadPolicy {
    /// A display may inspect historical published values. The outcome makes no
    /// claim that its inputs are current.
    PublishedValues,
    /// A diagnostic or authoring decision refuses stale and external-cache inputs.
    CurrentValues,
}

/// One immutable workbook borrow for a batch of detached evaluations. The dirty
/// closure is computed once for CurrentValues, including possible spill expansion.
/// Its borrow prevents edits, publication and input changes while it is used.
pub struct DetachedEvaluationContext<'a> {
    state: &'a EngineState,
    inputs: &'a ModelRoot,
    policy: DetachedReadPolicy,
    numeric: NumericMode,
    stale: Vec<CellKey>,
    all_values_current: bool,
    progress: Progress,
    spills: SpillIndex,
}

impl<'a> DetachedEvaluationContext<'a> {
    /// As with the other engine entry points, `state` and `inputs` must belong to
    /// the same workbook. A revision mismatch is refused before any reads.
    pub fn new(
        state: &'a EngineState,
        inputs: &'a ModelRoot,
        policy: DetachedReadPolicy,
    ) -> Result<Self, NotModeled> {
        Self::new_with_numeric_mode(
            state,
            inputs,
            policy,
            state.published_mode.as_ref().unwrap_or(&NumericMode::Fast),
        )
    }

    /// An owner-selected policy for the whole batch, including before publication.
    /// Current consumers cannot read formula values published under another mode;
    /// unused branches and literal inputs remain available without producer work.
    pub fn new_with_numeric_mode(
        state: &'a EngineState,
        inputs: &'a ModelRoot,
        policy: DetachedReadPolicy,
        numeric: &NumericMode,
    ) -> Result<Self, NotModeled> {
        if state.revision() != inputs.revision() {
            return Err(NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "a diagnostic snapshot from another authored revision",
                owner: "dc-hsa5.6",
            }));
        }
        let mut stale = match policy {
            DetachedReadPolicy::CurrentValues => state.stale(inputs).expect("revision checked"),
            DetachedReadPolicy::PublishedValues => Vec::new(),
        };
        if policy == DetachedReadPolicy::CurrentValues
            && state
                .published_mode
                .as_ref()
                .is_some_and(|published| published != numeric)
        {
            stale.extend(state.nodes().map(|node| node.anchor()));
            stale.sort_unstable();
            stale.dedup();
        }
        let mut progress = Progress::new(state.nodes.len());
        for &at in &stale {
            if let Some(node) = state.node_at(at) {
                progress.schedule(node);
            }
        }
        let mut all_values_current = stale.is_empty();
        if policy == DetachedReadPolicy::CurrentValues {
            for node in state.nodes() {
                let calculation = state
                    .result(node.anchor())
                    .and_then(crate::FormulaResult::calculated)
                    .map(|(_, c)| c);
                let qualified = calculation.is_some_and(|calculation| {
                    calculation.held_by_circular_rule()
                        || calculation.stamp().is_some_and(|stamp| {
                            stamp.reads.uses_boundary_cache
                                || stamp.reads.uses_imported_cache
                                || stamp.reads.legacy_boundary
                        })
                });
                if qualified {
                    progress.schedule(node.id());
                }
                all_values_current &= calculation
                    .is_some_and(|calculation| !qualified && !calculation.reads_external_links());
            }
        }
        Ok(Self {
            state,
            inputs,
            policy,
            numeric: numeric.clone(),
            stale,
            all_values_current,
            progress,
            spills: SpillIndex::default(),
        })
    }

    /// Available only for a current-value context; a historical display context has
    /// deliberately not computed freshness.
    pub fn stale_cells(&self) -> Option<&[CellKey]> {
        (self.policy == DetachedReadPolicy::CurrentValues).then_some(&self.stale)
    }

    /// Conservative admission for an aggregate consumer without its own executed
    /// read certificate. Refusals, file/held/qualified caches and stale results fail.
    pub fn all_formula_values_current(&self) -> Option<bool> {
        (self.policy == DetachedReadPolicy::CurrentValues).then_some(self.all_values_current)
    }

    /// Bind against this snapshot's actual callable metadata without granting a runtime.
    /// Evaluation still applies the diagnostic read policy to any reached user call.
    pub fn compile(
        &self,
        text: &str,
        origin: CellKey,
        form: FormulaForm,
    ) -> Result<DetachedFormula, NotModeled> {
        DetachedFormula::compile_in(self.inputs, text, origin, form, Some(&self.state.callables))
    }

    fn view(&self) -> RecalcView<'_> {
        RecalcView {
            linked_workbooks: &self.state.linked_workbooks,
            cached_observation: false,
            caller: None,
            scope: None,
            range_refused: None,
            boundary: None,
            boundary_caches: None,
            boundary_used: None,
            inputs: self.inputs,
            state: self.state,
            workbook_facts: self.state.workbook_facts.as_ref(),
            progress: &self.progress,
            spills: &self.spills,
            iterates: None,
            kept: None,
        }
    }

    /// Read a returned reference through the same freshness and occupancy policy as
    /// the expression that produced it, without falling back to display caches.
    pub fn read_cell(&self, at: CellKey) -> Result<CalcValue, NotModeled> {
        use calc_fml::reference::{CellRead, ReferenceReader};
        if self.inputs.sheet_by_id(at.sheet()).is_none()
            && self
                .inputs
                .definitions()
                .external_sheet(at.sheet())
                .is_none()
        {
            return Err(NotModeled::Evaluation(EvalFault::MissingInput { cell: at }));
        }
        let uncomputed = Cell::new(None);
        let mut reader = WorkbookReader::new(self.view(), &uncomputed);
        let value = reader
            .read_cell(calc_fml::ReadRequest::DIAGNOSTIC, at)
            .map_err(|error| NotModeled::Evaluation(EvalFault::ReadContract(error)))?;
        if uncomputed.get().is_some()
            || (self.policy == DetachedReadPolicy::CurrentValues && reader.read_external_links())
        {
            return Err(NotModeled::Evaluation(EvalFault::MissingInput { cell: at }));
        }
        match value {
            CellRead::Value(value) => Ok(value),
            CellRead::Blank => Ok(CalcValue::empty()),
            CellRead::Missing => Err(NotModeled::Evaluation(EvalFault::MissingInput { cell: at })),
        }
    }
}

/// A formula compiled for detached evaluation: its text read in A1 notation for a formula at
/// `origin` (a conditional format's formula is relative to the top-left cell of its range), to be
/// placed at any cell.
#[derive(Clone)]
pub struct DetachedFormula {
    reference_result: bool,
    plan: Arc<CompiledPlan>,
    compiled_inputs: Arc<ModelRoot>,
    caller_sheet: calc_value::SheetId,
    selected_name: Option<calc_fml::reference::BoundName>,
}

impl std::fmt::Debug for DetachedFormula {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetachedFormula")
            .field("reference_result", &self.reference_result)
            .field("plan", &self.plan)
            .field("compiled_revision", &self.compiled_inputs.revision())
            .field("caller_sheet", &self.caller_sheet)
            .finish()
    }
}

/// Evidence from an explicit diagnostic; it installs no cell or shared name result.
#[derive(Clone, Debug, PartialEq)]
pub struct DetachedNameEvaluation {
    pub outcome: Result<CalcValue, NotModeled>,
    pub numeric: NumericMode,
    /// Present for a selected, admitted name pass; never a synthetic cell root.
    pub root: Option<crate::contract::NameInvocationId>,
    pub inputs: crate::CalculationInputVersion,
    /// The supplied owner clock, not an implicit time sample during evaluation.
    pub clock: Option<f64>,
    pub random_draws: crate::RandomDrawEvidence,
    pub invocations: Vec<calc_fml::NameInvocation>,
    pub omitted: u64,
}

impl DetachedFormula {
    /// Compiles `text` (A1 notation, without `=`) as a legacy formula at `origin`, its sheets and
    /// names bound through `inputs`.
    pub fn compile(inputs: &ModelRoot, text: &str, origin: CellKey) -> Result<Self, NotModeled> {
        Self::compile_in(inputs, text, origin, FormulaForm::Legacy, None)
    }

    /// [`DetachedFormula::compile`] in the dynamic-array form, where a range of one area is the
    /// array of its values rather than the cell implicit intersection picks (a validation list's
    /// source).
    pub fn compile_dynamic(
        inputs: &ModelRoot,
        text: &str,
        origin: CellKey,
    ) -> Result<Self, NotModeled> {
        Self::compile_in(inputs, text, origin, FormulaForm::Dynamic, None)
    }

    fn compile_in(
        inputs: &ModelRoot,
        text: &str,
        origin: CellKey,
        form: FormulaForm,
        functions: Option<&calc_fml::CallableBindings>,
    ) -> Result<Self, NotModeled> {
        let source = format!("={text}");
        let mut plans = PlanCache::new();
        let tree = parse_formula(&source);
        let catalog = SnapshotCatalog::new(inputs);
        let plan = match functions {
            Some(functions) => calc_fml::compile_loaded_with_bindings(
                &tree,
                ReferenceNotation::A1,
                FormulaAnchor::new(origin),
                &catalog,
                &mut plans,
                form,
                functions,
            ),
            None => compile_loaded_with(
                &tree,
                ReferenceNotation::A1,
                FormulaAnchor::new(origin),
                &catalog,
                &mut plans,
                form,
            ),
        }
        .map_err(NotModeled::Compile)?;
        Ok(DetachedFormula {
            plan,
            reference_result: false,
            compiled_inputs: inputs.pin(),
            caller_sheet: origin.sheet(),
            selected_name: None,
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
        let context =
            DetachedEvaluationContext::new(state, inputs, DetachedReadPolicy::PublishedValues)?;
        self.evaluate_in(&context, at)
    }

    /// Evaluate using an explicitly selected, reusable published-input context.
    /// CurrentValues requires the compilation model and caller worksheet to remain
    /// valid; compile again after a structural, namespace or file-identity change.
    pub fn evaluate_in(
        &self,
        context: &DetachedEvaluationContext<'_>,
        at: CellKey,
    ) -> Result<CalcValue, NotModeled> {
        self.evaluate_recording(context, at, None, None)
    }

    /// Execute through the same diagnostic reader and evaluator while retaining bounded
    /// name-body evidence, including refused completions. A selected name receives
    /// its own pass and random stream. Generic snapshot consumers keep absent pass
    /// services. No producer work, cell result or shared name value is installed.
    pub fn evaluate_with_name_evidence(
        &self,
        state: &EngineState,
        inputs: &ModelRoot,
        at: CellKey,
        numeric: &NumericMode,
    ) -> DetachedNameEvaluation {
        let mut effects = calc_fml::UserFunctionEffects::default();
        let mut root = None;
        let mut random_draws = crate::RandomDrawEvidence::default();
        let outcome = DetachedEvaluationContext::new_with_numeric_mode(
            state,
            inputs,
            DetachedReadPolicy::CurrentValues,
            numeric,
        )
        .and_then(|context| {
            self.validate_context(&context, at)?;
            let mut random = self.selected_name.as_ref().map(|definition| {
                let (identity, random) = name_pass(state, definition.clone(), at);
                root = Some(identity);
                random
            });
            let outcome = self.evaluate_recording(
                &context,
                at,
                Some(&mut effects),
                random
                    .as_mut()
                    .map(|r| r as &mut dyn calc_fml::RandomSource),
            );
            if let Some(random) = random {
                random_draws = random.into_evidence();
            }
            outcome
        });
        DetachedNameEvaluation {
            outcome,
            numeric: numeric.clone(),
            root,
            inputs: state.input_version(),
            clock: state.environment().and_then(|environment| environment.now),
            random_draws,
            invocations: effects.name_invocations().to_vec(),
            omitted: effects.omitted_name_invocations(),
        }
    }

    fn validate_context(
        &self,
        context: &DetachedEvaluationContext<'_>,
        at: CellKey,
    ) -> Result<(), NotModeled> {
        self.validate_inputs(context.state, context.inputs, at,
            context.policy == DetachedReadPolicy::CurrentValues)
    }

    fn validate_inputs(&self, state: &EngineState, inputs: &ModelRoot, at: CellKey,
        current: bool) -> Result<(), NotModeled> {
        if state.revision() != inputs.revision()
            || !inputs.same_lineage(&self.compiled_inputs)
            || inputs.sheet_by_id(at.sheet()).is_none()
            || (at.sheet() != self.caller_sheet && !self.plan.name_bindings().is_empty())
            || (current
                && (inputs.revision() != self.compiled_inputs.revision()
                    || at.sheet() != self.caller_sheet
                    || !Arc::ptr_eq(inputs.definitions(), self.compiled_inputs.definitions())
                    || self
                        .plan
                        .callable_bindings()
                        .iter()
                        .any(|proof| !state.callables.validates(proof))))
            || !crate::contract::name_bindings_valid(inputs, self.plan.name_bindings())
        {
            return Err(NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "a detached formula with expired model, caller, name or callable bindings",
                owner: "dc-hsa5.6",
            }));
        }
        Ok(())
    }

    fn evaluate_recording(
        &self,
        context: &DetachedEvaluationContext<'_>,
        at: CellKey,
        effects: Option<&mut calc_fml::UserFunctionEffects>,
        random: Option<&mut dyn calc_fml::RandomSource>,
    ) -> Result<CalcValue, NotModeled> {
        self.validate_context(context, at)?;
        let (state, inputs) = (context.state, context.inputs);
        let instance = instantiate(&self.plan, FormulaAnchor::new(at));
        let view = context.view();
        let uncomputed = Cell::new(None);
        let mut reader = WorkbookReader::new(view, &uncomputed);
        let mut host = WorkbookReader::new(view, &uncomputed);
        let catalog = SnapshotCatalog::new(inputs);
        let mut scratch = EvalScratch::new();
        let outcome = {
            let mut services = FormulaEvaluationContext::new(
                &context.numeric,
                &mut reader,
                state.environment(),
                Some(&mut host),
                calc_fml::ReadRequest::DIAGNOSTIC,
            )
            .with_catalog(&catalog);
            if let Some(effects) = effects {
                services = services.with_user_function_effects(effects);
            }
            if let Some(random) = random {
                services = services.with_random(random);
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
        if context.policy == DetachedReadPolicy::CurrentValues
            && (reader.read_external_links() || host.read_external_links())
        {
            return Err(NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "a detached current-value evaluation that consumes unresolved external-link cache values",
                owner: "dc-hsa5.6",
            }));
        }
        outcome.map_err(NotModeled::Evaluation)
    }
}

fn name_pass(state: &EngineState, definition: calc_fml::reference::BoundName, at: CellKey)
    -> (crate::contract::NameInvocationId, crate::random::RecordedRandom) {
    use crate::contract::{CalcCommandId, CalcPassId, InvocationId, NameEvaluationLocus,
        PassReason, ReevaluationReason};
    let workbook = state.workbook_instance();
    let sequence = state.allocate_tick();
    (InvocationId {
        pass: CalcPassId { command: CalcCommandId { domain_incarnation: workbook.document,
            sequence }, ordinal: 0, reason: PassReason::Initial },
        workbook, locus: NameEvaluationLocus { definition, caller: at },
        evaluation: 0, call_site: 0, occurrence: Vec::new(),
        reason: ReevaluationReason::NewCommandDemand,
    }, crate::random::RecordedRandom::new(crate::random::RandomStream::new(
        state.random_seed, sequence, at)))
}
