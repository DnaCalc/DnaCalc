//! An explicitly admitted selected-name activation. It owns the ordinary CalcFml
//! continuation and reads one pinned current-value frontier without publishing cells.
use super::{
    DetachedEvaluationContext, DetachedFormula, DetachedNameEvaluation, DetachedReadPolicy,
};
use crate::contract::{CallInvocation, NameEvaluationLocus, NameInvocationId, ReadViewId};
use crate::read::{Progress, RecalcView, SnapshotCatalog, WorkbookReader};
use crate::spill::SpillIndex;
use crate::{CachedReadLease, EngineState, NotModeled};
use calc_doc_model::ModelRoot;
use calc_fml::{EvalContinuation, EvalFault, EvalScratch, EvalStep, FormulaEvaluationContext};
use calc_value::{CalcValue, CellKey, GridArea, NumericMode};
use std::cell::{Cell, RefCell};

/// This is execution evidence, not a published cell's freshness certificate.
#[derive(Clone, Debug, PartialEq)]
pub struct NameCalculationResult {
    pub revision: calc_doc_model::AuthoredRevision,
    pub results: ReadViewId,
    pub evaluation: DetachedNameEvaluation,
    pub calls: Vec<CallInvocation<NameEvaluationLocus>>,
    pub runtime_root: Option<u64>,
    pub observations: Vec<crate::contract::CachedObservation>,
    pub value_reads: Vec<(calc_fml::ReadRequest, GridArea)>,
    pub facts: Vec<calc_fml::FactRead>,
    pub external_inputs: Vec<calc_fml::RtdRead>,
    /// What the user functions the evaluation ran asked of volatility ([`calc_fml::UdfVolatility`]).
    pub udf_volatility: calc_fml::UdfVolatility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameCalculationProgress {
    Calculating,
    /// A terminal value or refusal is available, including cancellation.
    Ready,
}

/// A retained non-cell activation. Edits, publication, input changes, reload or a
/// different numeric mode expire it before further user code can enter.
pub struct NameCalculationWork {
    formula: DetachedFormula,
    at: CellKey,
    root: NameInvocationId,
    numeric: NumericMode,
    inputs: crate::CalculationInputVersion,
    results: ReadViewId,
    semantic: (u64, u64, u64, u64),
    clock: Option<f64>,
    progress: Progress,
    spills: SpillIndex,
    observation: Option<CachedReadLease>,
    invocation: Option<calc_fml::UserInvocationFrame>,
    activation: Option<EvalContinuation>,
    random: Option<crate::random::RecordedRandom>,
    effects: calc_fml::UserFunctionEffects,
    value_reads: Vec<(calc_fml::ReadRequest, GridArea)>,
    observation_reads: Vec<(calc_fml::ReadRequest, GridArea)>,
    facts: Vec<calc_fml::FactRead>,
    result: Option<NameCalculationResult>,
}

fn refused(construct: &'static str) -> NotModeled {
    NotModeled::Evaluation(EvalFault::NotModelled {
        construct,
        owner: "dc-hsa5.6",
    })
}

impl DetachedFormula {
    /// Admission only: validation and pinning run before reserving a pass. No
    /// instruction, callback, producer, graph edit or publication runs here.
    pub fn begin_name_calculation(
        &self,
        state: &EngineState,
        inputs: &ModelRoot,
        at: CellKey,
        numeric: &NumericMode,
    ) -> Result<NameCalculationWork, NotModeled> {
        let context = DetachedEvaluationContext::new_with_numeric_mode(
            state,
            inputs,
            DetachedReadPolicy::CurrentValues,
            numeric,
        )?;
        self.validate_context(&context, at)?;
        let definition = self.selected_name.ok_or_else(|| {
            refused("effect-capable calculation without a selected name identity")
        })?;
        let (root, random) = super::name_pass(state, definition, at);
        let observation = self.plan.has_user_calls().then(|| {
            CachedReadLease::capture(
                state,
                inputs,
                state.linked_workbooks.clone(),
                &context.progress,
                &context.spills,
                state.volatile_input_version,
                None,
                None,
                &Default::default(),
            )
        });
        let invocation = observation.as_ref().map(|observation| {
            calc_fml::UserInvocationFrame::new(
                None,
                std::rc::Rc::new(crate::contract::InvocationReadView {
                    root: root.clone(),
                    view: observation.clone(),
                }),
            )
        });
        let instance =
            calc_fml::instantiate(&self.plan, calc_fml::reference::FormulaAnchor::new(at));
        Ok(NameCalculationWork {
            formula: self.clone(),
            at,
            root,
            numeric: numeric.clone(),
            inputs: state.input_version(),
            results: state.result_view(),
            semantic: (
                state.contract.environment,
                state.contract.bindings,
                state.contract.coordinates,
                state.contract.facts,
            ),
            clock: state.environment().and_then(|env| env.now),
            progress: context.progress,
            spills: context.spills,
            observation,
            invocation,
            activation: Some(EvalContinuation::new(&instance, EvalScratch::new())),
            random: Some(random),
            effects: Default::default(),
            value_reads: Vec::new(),
            observation_reads: Vec::new(),
            facts: Vec::new(),
            result: None,
        })
    }
}

impl NameCalculationWork {
    pub fn root(&self) -> &NameInvocationId {
        &self.root
    }
    pub fn result(&self) -> Option<&NameCalculationResult> {
        self.result.as_ref()
    }
    pub fn into_result(mut self) -> Result<NameCalculationResult, Self> {
        match self.result.take() {
            Some(result) => Ok(result),
            None => Err(self),
        }
    }
    pub fn progress(&self) -> NameCalculationProgress {
        if self.result.is_some() {
            NameCalculationProgress::Ready
        } else {
            NameCalculationProgress::Calculating
        }
    }
    pub fn cancel(&mut self) {
        if self.result.is_none() {
            self.complete(Err(refused("a cancelled selected-name calculation")));
        }
    }
    pub fn expire(&mut self) {
        if self.result.is_none() {
            self.complete(Err(refused(
                "a selected-name calculation with expired owner authority",
            )));
        }
    }
    fn complete(&mut self, outcome: Result<CalcValue, NotModeled>) {
        self.activation = None;
        let calls = self
            .effects
            .invocations()
            .iter()
            .map(|call| {
                let mut identity = self.root.clone();
                identity.call_site = call.call_site;
                identity.occurrence = call.occurrence.clone();
                CallInvocation {
                    identity,
                    runtime: call.id,
                    parent: call.parent,
                    callable: call.callable,
                    completed: call.completed,
                    non_preemptible: call.non_preemptible,
                }
            })
            .collect();
        let mut observations = Vec::new();
        if let Some(pin) = &self.observation {
            for &(request, area) in &self.observation_reads {
                let read = pin.observation(request, area);
                if !observations.contains(&read) {
                    observations.push(read);
                }
            }
        }
        self.result = Some(NameCalculationResult {
            revision: self.formula.compiled_inputs.revision(),
            results: self.results,
            evaluation: DetachedNameEvaluation {
                outcome,
                root: Some(self.root.clone()),
                numeric: self.numeric.clone(),
                inputs: self.inputs,
                clock: self.clock,
                random_draws: self
                    .random
                    .take()
                    .expect("one terminal result")
                    .into_evidence(),
                invocations: self.effects.name_invocations().to_vec(),
                omitted: self.effects.omitted_name_invocations(),
            },
            calls,
            runtime_root: self.invocation.as_ref().map(|frame| frame.id),
            observations,
            value_reads: std::mem::take(&mut self.value_reads),
            facts: std::mem::take(&mut self.facts),
            external_inputs: self.effects.external_inputs().to_vec(),
            udf_volatility: self.effects.volatility(),
        });
    }

    /// Resume the retained activation once. Inputs and callbacks belong to the same
    /// owner as state. Producer demand and provider waits remain explicit refusals.
    pub fn advance(
        &mut self,
        state: &EngineState,
        inputs: &ModelRoot,
        numeric: &NumericMode,
        functions: &calc_fml::CallableTable,
        external: &dyn calc_fml::RtdCut,
        references: &dyn calc_fml::ReferenceHandles,
        quantum: usize,
    ) -> NameCalculationProgress {
        if self.result.is_some() {
            return NameCalculationProgress::Ready;
        }
        if state.workbook_instance() != self.root.workbook
            || state.input_version() != self.inputs
            || state.revision() != self.formula.compiled_inputs.revision()
            || state.result_view() != self.results
            || numeric != &self.numeric
            || (
                state.contract.environment,
                state.contract.bindings,
                state.contract.coordinates,
                state.contract.facts,
            ) != self.semantic
            || self
                .formula
                .validate_inputs(state, inputs, self.at, true)
                .is_err()
            || self
                .formula
                .plan
                .callable_bindings()
                .iter()
                .chain(self.effects.callable_reads())
                .any(|p| !state.callables.validates(p) || !functions.bindings().validates(p))
        {
            self.complete(Err(refused(
                "a selected-name continuation with expired owner, inputs or bindings",
            )));
            return NameCalculationProgress::Ready;
        }
        if quantum == 0 {
            return NameCalculationProgress::Calculating;
        }
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
            progress: &self.progress,
            spills: &self.spills,
            iterates: None,
            kept: None,
        };
        let uncomputed = Cell::new(None);
        let observation_uncomputed = Cell::new(None);
        let mut cells = WorkbookReader::new(view, &uncomputed);
        let mut host = WorkbookReader::new(view, &uncomputed);
        let mut cached = self.observation.as_ref().map(|pin| {
            WorkbookReader::new(
                pin.view(state, state.workbook_facts.as_ref()),
                &observation_uncomputed,
            )
        });
        let footprint = RefCell::new(crate::recalc::Footprint::default());
        let facts = RefCell::new(std::mem::take(&mut self.facts));
        let value_reads = RefCell::new(std::mem::take(&mut self.value_reads));
        let blanks = RefCell::new(Vec::new());
        let observation_reads = RefCell::new(std::mem::take(&mut self.observation_reads));
        let observation_facts = RefCell::new(Vec::new());
        let observation_blanks = RefCell::new(Vec::new());
        let mut silent = crate::Unobserved;
        let observer: RefCell<&mut dyn crate::ReadObserver> = RefCell::new(&mut silent);
        let mut observed = crate::recalc::Observed {
            observer: &observer,
            silent: true,
            formula: self.at,
            cells: &mut cells,
            footprint: &footprint,
            facts: &facts,
            value_reads: &value_reads,
            blanks: &blanks,
            stopped: None,
        };
        let mut cached_observed = cached.as_mut().map(|cached| crate::recalc::Observed {
            observer: &observer,
            silent: true,
            formula: self.at,
            cells: cached,
            footprint: &footprint,
            facts: &observation_facts,
            value_reads: &observation_reads,
            blanks: &observation_blanks,
            stopped: None,
        });
        let catalog = SnapshotCatalog::new(inputs);
        let mut services = FormulaEvaluationContext::new(
            &self.numeric,
            &mut observed,
            state.environment(),
            Some(&mut host),
            calc_fml::ReadRequest::DEPENDENCY,
        )
        .with_catalog(&catalog)
        .with_functions(functions)
        .with_external_inputs(external)
        .with_reference_handles(references)
        .with_user_function_effects(&mut self.effects)
        .with_random(self.random.as_mut().expect("an active random stream"));
        if let Some(frame) = &self.invocation {
            services = services.with_invocation(frame.clone());
        }
        if let Some(cached) = &mut cached_observed {
            services = services.with_observation_reader(cached);
        }
        if self.formula.reference_result {
            services = services.with_reference_result();
        }
        let activation = self.activation.as_mut().expect("an active continuation");
        let step = activation.advance(&mut services, quantum.max(1));
        drop(services);
        let waits = activation.waiting_input().is_some() || activation.pending_external().is_some();
        let external_cache = cells.read_external_links()
            || host.read_external_links()
            || cached
                .as_ref()
                .is_some_and(|cached| cached.read_external_links());
        self.facts = facts.into_inner();
        self.facts.extend(observation_facts.into_inner());
        self.value_reads = value_reads.into_inner();
        self.observation_reads = observation_reads.into_inner();
        if self.effects.reevaluation().is_some() {
            self.complete(Err(refused(
                "semantic reevaluation of an effect-capable selected-name calculation",
            )));
        } else if external_cache {
            self.complete(Err(refused(
                "a selected-name calculation consuming unresolved external-link caches",
            )));
        } else if uncomputed.get().is_some() || observation_uncomputed.get().is_some() || waits {
            self.complete(Err(refused(
                "a selected-name calculation requiring producer or asynchronous provider work",
            )));
        } else if let EvalStep::Complete(outcome) = step {
            self.complete(outcome.map_err(NotModeled::Evaluation));
        }
        self.progress()
    }
}
