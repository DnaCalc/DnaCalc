//! Recalculation (plan 6): [`begin_recalc`] binds each unbound node's formula through CalcFml and
//! orders the dirty closure, [`step`] evaluates the ordered nodes through CalcFml a budget at a
//! time, and [`finish`] publishes the staged results as one generation. Nothing is published
//! before `finish` succeeds; a recalculation that fails or is refused leaves the published
//! generation as it was, and its pending nodes pending.

use crate::graph::{AxisInterval, Prerequisite};
use crate::read::{ObservedEvaluation, ReadObserver, SnapshotCatalog, WorkbookReader};
use crate::result::{CalcTick, Calculation, FormulaResult, NotModeled, ValueChange, ValueDelta};
use crate::state::{Binding, EngineState};
use calc_doc_model::{ArrayKind, AuthoredRevision, Axis, CellInput, FormulaText, ModelRoot};
use calc_fml::reference::{
    CellRun, FormulaAnchor, ReferenceNotation, ReferenceReader, ReferenceShape,
};
use calc_fml::{
    EvalFault, EvalFrame, EvalScratch, EvaluationObserver, EvaluationReport, EvaluationServices,
    ExecutedCall, FormulaForm, FormulaInstance, compile_loaded_with, evaluate, instantiate,
    parse_formula,
};
use calc_value::{CalcValue, CellKey, GridArea, NodeId, NumericMode};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::ControlFlow;

/// Why [`begin_recalc`] refused to start. A refused recalculation evaluates and publishes
/// nothing; its pending nodes stay pending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartError {
    /// The input snapshot is not at the engine state's revision.
    StaleInputs {
        engine: AuthoredRevision,
        inputs: AuthoredRevision,
    },
    /// The input snapshot holds no formula at the anchor of `node`: it is not the snapshot of
    /// the state's revision.
    ForeignInputs { node: NodeId, at: CellKey },
    /// The dirty closure has a cycle. `cells` are the formula cells that cannot be ordered, those
    /// on a cycle and those that read one, in the order of the cells (sheet ID, then row, then
    /// column). The nodes stay pending, so every later recalculation is refused the same way
    /// until an edit breaks the cycle: strongly connected components are ceng-m5.1's and
    /// iterative calculation ceng-m5.2's.
    Cycle { cells: Vec<CellKey> },
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartError::StaleInputs { engine, inputs } => write!(
                f,
                "the input snapshot is at revision {}, the engine state at {}",
                inputs.get(),
                engine.get()
            ),
            StartError::ForeignInputs { node, at } => write!(
                f,
                "the input snapshot holds no formula at {at:?}, the anchor of {node:?}"
            ),
            StartError::Cycle { cells } => write!(
                f,
                "{} formula cells are on or read a circular reference",
                cells.len()
            ),
        }
    }
}

impl std::error::Error for StartError {}

/// Why a recalculation published nothing. Its pending nodes stay pending and the published
/// generation is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecalcFailure {
    /// A read of `area` by the evaluation of the formula at `formula` did not complete, so the
    /// evaluation's result was discarded: the read observer stopped the recalculation there
    /// ([`ReadObserver::observe`]), or the reads broke CalcFml's reader contract, which the
    /// engine's own reader keeps. This is a failure of the recalculation, not a result of the
    /// formula.
    ReadFailed { formula: CellKey, area: GridArea },
    /// The evaluation of the formula at `formula` read the formula cell `cell`, which this
    /// recalculation calculates and had not yet evaluated. Its old value was not handed out.
    UnscheduledRead { formula: CellKey, cell: CellKey },
    /// The engine state or the input snapshot is no longer at the revision the recalculation
    /// began on (`revision`), or another recalculation published after it began.
    Superseded { revision: AuthoredRevision },
    /// [`finish`] was called while `remaining` nodes were still to be evaluated.
    Incomplete { remaining: usize },
}

impl fmt::Display for RecalcFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecalcFailure::ReadFailed { formula, area } => write!(
                f,
                "a read of {area:?} by the formula at {formula:?} did not complete"
            ),
            RecalcFailure::UnscheduledRead { formula, cell } => write!(
                f,
                "the formula at {formula:?} read {cell:?}, which this recalculation had not evaluated yet"
            ),
            RecalcFailure::Superseded { revision } => write!(
                f,
                "the recalculation began on revision {}, and the state or its inputs have changed since",
                revision.get()
            ),
            RecalcFailure::Incomplete { remaining } => {
                write!(f, "{remaining} nodes were still to be evaluated")
            }
        }
    }
}

impl std::error::Error for RecalcFailure {}

/// Where a recalculation stands after a [`step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecalcProgress {
    /// `remaining` nodes are still to be evaluated.
    Pending { remaining: usize },
    /// Every node is evaluated; [`finish`] publishes.
    Complete,
    /// The recalculation failed; [`finish`] returns why and publishes nothing.
    Failed,
}

/// How many nodes one [`step`] may evaluate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkBudget {
    evaluations: usize,
}

impl WorkBudget {
    /// No limit: the step evaluates every remaining node.
    pub const UNLIMITED: WorkBudget = WorkBudget {
        evaluations: usize::MAX,
    };

    pub const fn evaluations(count: usize) -> Self {
        WorkBudget { evaluations: count }
    }
}

/// One recalculation in progress: the nodes it evaluates in their order, and the results it has
/// staged. It holds no borrow of the engine state, which [`step`] and [`finish`] check has not
/// moved on since [`begin_recalc`].
#[derive(Debug)]
pub struct RecalcWork {
    revision: AuthoredRevision,
    generation: u64,
    calculation: Calculation,
    order: Vec<NodeId>,
    next: usize,
    scheduled: BTreeSet<NodeId>,
    staged: BTreeMap<NodeId, FormulaResult>,
    scratch: EvalScratch,
    failure: Option<RecalcFailure>,
}

impl RecalcWork {
    /// The authored revision the recalculation calculates.
    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    pub fn tick(&self) -> CalcTick {
        self.calculation.tick()
    }

    /// How many nodes it evaluates.
    pub fn scheduled(&self) -> usize {
        self.order.len()
    }

    fn progress(&self) -> RecalcProgress {
        if self.failure.is_some() {
            RecalcProgress::Failed
        } else if self.next < self.order.len() {
            RecalcProgress::Pending {
                remaining: self.order.len() - self.next,
            }
        } else {
            RecalcProgress::Complete
        }
    }
}

/// Begins a recalculation of the snapshot `inputs`, at the state's revision, under `mode`.
///
/// - Each pending node without a plan is bound: its formula text, read from the snapshot, is
///   parsed and compiled once through CalcFml (the plan shared through the state's plan cache by
///   its normal form) and placed at its anchor; its prerequisites, filed with
///   [`EngineState::set_prerequisites`], are the cells, rectangles and whole rows or columns its
///   placed references name. A reference that cannot be placed (`#REF!`) reads no cell. A formula
///   CalcFml refuses to compile reads nothing and is published as not modelled.
/// - The dirty closure is the pending nodes and every node that reads, directly or through other
///   nodes, the anchor of one of them ([`EngineState::stale`] lists it). When `mode` differs from
///   the numeric mode of the published generation, the closure is every formula node instead: a
///   published generation is calculated under one mode, and a result calculated under another is
///   never read as an input (plan 3.4).
/// - The closure is ordered by a ready queue with maintained prerequisite counts: a node is ready
///   once every node of the closure whose anchor it reads is ordered, and of the ready nodes the
///   one whose anchor comes first (sheet ID, then row, then column) is ordered next. A closure
///   that cannot be ordered has a cycle, and the recalculation is refused.
///
/// The recalculation takes the state's next tick.
pub fn begin_recalc(
    state: &mut EngineState,
    inputs: &ModelRoot,
    mode: NumericMode,
) -> Result<RecalcWork, StartError> {
    let revision = state.revision();
    if inputs.revision() != revision {
        return Err(StartError::StaleInputs {
            engine: revision,
            inputs: inputs.revision(),
        });
    }
    let mut unbound: Vec<(NodeId, CellKey, &FormulaText)> = Vec::new();
    for &node in &state.pending {
        if state.bindings.contains_key(&node) {
            continue;
        }
        let at = live_anchor(state, node);
        match inputs.cell(at) {
            Some(CellInput::Formula(text)) => unbound.push((node, at, text)),
            _ => return Err(StartError::ForeignInputs { node, at }),
        }
    }
    for (node, at, text) in unbound {
        bind(state, inputs, node, at, text);
    }
    let roots = match &state.published_mode {
        Some(published) if *published != mode => state.by_anchor.values().copied().collect(),
        _ => state.pending.clone(),
    };
    let order = schedule(state, inputs, roots)?;
    state.last_tick = state
        .last_tick
        .checked_add(1)
        .expect("fewer than 2^64 recalculations begin in one engine state");
    Ok(RecalcWork {
        revision,
        generation: state.generation,
        calculation: Calculation::new(revision, CalcTick::new(state.last_tick), mode),
        scheduled: order.iter().copied().collect(),
        order,
        next: 0,
        staged: BTreeMap::new(),
        scratch: EvalScratch::new(),
        failure: None,
    })
}

/// Evaluates up to `budget` of the recalculation's remaining nodes, in order, each through
/// CalcFml's evaluator reading through the engine's reader, with `observer` shown every run the
/// evaluation receives and what the evaluator reports of each call it runs and of the outcome it
/// returns (see [`ReadObserver`]), and stages each result:
///
/// - a value a cell holds (a finite number, a text, a logical or an error) is staged as
///   calculated, by this recalculation's tick, at its revision and under its mode;
/// - a formula CalcFml refused to compile, a typed refusal of the evaluation (a function fault,
///   an input the reader cannot supply, a range where one value is needed, a final sum near zero)
///   and any other value are staged as not modelled;
/// - a read the observer stops, a read that breaks the reader contract, or a read of a formula
///   cell this recalculation has yet to evaluate fails the recalculation.
///
/// A literal cell's value is read from the snapshot's page. A formula cell's value is read from
/// what this recalculation staged for it, or else from the published result when that was
/// calculated; a file cache or a result not modelled is read as a cell the reader cannot supply.
/// The observer cannot change any of these (see [`ReadObserver`]). A step after a failure, or
/// against a state or snapshot that has moved on, evaluates nothing.
pub fn step(
    state: &EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
    budget: WorkBudget,
    observer: &mut dyn ReadObserver,
) -> RecalcProgress {
    if work.failure.is_none()
        && (state.revision() != work.revision
            || inputs.revision() != work.revision
            || state.generation != work.generation)
    {
        work.failure = Some(RecalcFailure::Superseded {
            revision: work.revision,
        });
    }
    let mut evaluated = 0;
    while work.failure.is_none() && evaluated < budget.evaluations && work.next < work.order.len() {
        let node = work.order[work.next];
        match evaluate_node(state, inputs, work, node, &mut *observer) {
            Ok(result) => {
                work.staged.insert(node, result);
                work.next += 1;
                evaluated += 1;
            }
            Err(failure) => work.failure = Some(failure),
        }
    }
    work.progress()
}

/// Publishes a completed recalculation: each evaluated node's staged result replaces its published
/// one, the results of formulas removed since the last publication are removed, the evaluated
/// nodes are no longer pending, and the published generation's numeric mode is the
/// recalculation's. Returns what changed. A recalculation that failed, is incomplete, or was
/// superseded publishes nothing.
pub fn finish(state: &mut EngineState, work: RecalcWork) -> Result<ValueDelta, RecalcFailure> {
    if let Some(failure) = work.failure {
        return Err(failure);
    }
    if work.next < work.order.len() {
        return Err(RecalcFailure::Incomplete {
            remaining: work.order.len() - work.next,
        });
    }
    if state.revision() != work.revision || state.generation != work.generation {
        return Err(RecalcFailure::Superseded {
            revision: work.revision,
        });
    }
    let mut changes: BTreeMap<CellKey, ValueChange> = BTreeMap::new();
    for (node, result) in work.staged {
        let at = live_anchor(state, node);
        state.results.insert(at, (node, result.clone()));
        state.pending.remove(&node);
        changes.insert(at, ValueChange::Published { at, node, result });
    }
    for at in std::mem::take(&mut state.retired) {
        if let Some(&(node, _)) = state.results.get(&at)
            && state.node_at(at) != Some(node)
        {
            state.results.remove(&at);
            changes.insert(at, ValueChange::Removed { at, node });
        }
    }
    state.generation += 1;
    state.published_mode = Some(work.calculation.mode().clone());
    Ok(ValueDelta::new(
        work.revision,
        work.calculation.tick(),
        changes.into_values().collect(),
    ))
}

pub(crate) fn live_anchor(state: &EngineState, node: NodeId) -> CellKey {
    state
        .node(node)
        .expect("a pending or scheduled node is live at the work's revision")
        .anchor()
}

/// Compiles and places the formula of `node`, anchored at `at`, and files its prerequisites. The
/// document holds a formula's text as a file stores it ([`FormulaText`]), so it is compiled as
/// such ([`compile_loaded`]).
fn bind(
    state: &mut EngineState,
    inputs: &ModelRoot,
    node: NodeId,
    at: CellKey,
    text: &FormulaText,
) {
    let anchor = FormulaAnchor::new(at);
    let array = inputs.definitions().array(at);
    // An array formula of one cell evaluates as an array formula (CalcFml's dynamic-array form,
    // which lifts); one that fills more cells is a spill, which is L2's (dc-0k9).
    let form = match array {
        Some(array) if array.area().cell_count() > 1 => None,
        Some(_) => Some(FormulaForm::Dynamic),
        None => Some(FormulaForm::Legacy),
    };
    let binding = match String::from_utf16(text.text().utf16_code_units()) {
        _ if form.is_none() => Binding::Refused(NotModeled::ArrayFormula(
            array.expect("an array formula").kind(),
        )),
        Err(_) => Binding::Refused(NotModeled::FormulaText),
        Ok(source) => match compile_loaded_with(
            &parse_formula(&source),
            ReferenceNotation::A1,
            anchor,
            &SnapshotCatalog(inputs),
            &mut state.plans,
            form.expect("a form"),
        ) {
            Ok(plan) => Binding::Placed(instantiate(&plan, anchor)),
            Err(error) => Binding::Refused(NotModeled::Compile(error)),
        },
    };
    let prerequisites = match &binding {
        Binding::Placed(instance) => prerequisites(instance),
        Binding::Refused(_) => Vec::new(),
    };
    state
        .set_prerequisites(inputs, node, prerequisites)
        .expect("the snapshot is at the state's revision, the node is live, and every sheet came from the snapshot");
    state.bindings.insert(node, binding);
}

/// What a placed formula reads: one prerequisite per distinct placed reference.
fn prerequisites(instance: &FormulaInstance) -> Vec<Prerequisite> {
    let mut out: Vec<Prerequisite> = Vec::new();
    for (operand, placed) in instance.plan().references().iter().zip(instance.areas()) {
        let Ok(area) = placed else {
            continue;
        };
        let prerequisite = match operand.shape {
            ReferenceShape::Rows { .. } => Prerequisite::AxisValue(axis_of(*area, Axis::Rows)),
            ReferenceShape::Columns { .. } => {
                Prerequisite::AxisValue(axis_of(*area, Axis::Columns))
            }
            ReferenceShape::Cell(_) | ReferenceShape::Area { .. } | ReferenceShape::Deleted => {
                if area.cell_count() == 1 {
                    Prerequisite::Cell(area.first())
                } else {
                    Prerequisite::Area(*area)
                }
            }
        };
        if !out.contains(&prerequisite) {
            out.push(prerequisite);
        }
    }
    out
}

/// The whole rows or columns of a placed area that spans every column or every row.
fn axis_of(area: GridArea, axis: Axis) -> AxisInterval {
    let (first, last) = match axis {
        Axis::Rows => (area.first().row(), area.last().row()),
        Axis::Columns => (area.first().column(), area.last().column()),
    };
    AxisInterval::new(area.sheet(), axis, first, last)
        .expect("a placed area's rows and columns lie on the grid")
}

/// The dirty closure of `roots`: the roots and every node that reads the anchor of one, directly
/// or through other nodes, each with the nodes that read its anchor. `inputs` is the snapshot at
/// the state's revision.
pub(crate) fn dirty_closure(
    state: &EngineState,
    inputs: &ModelRoot,
    roots: BTreeSet<NodeId>,
) -> BTreeMap<NodeId, Vec<NodeId>> {
    let mut stack: Vec<NodeId> = roots.iter().copied().collect();
    let mut closure = roots;
    let mut readers_of: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    while let Some(node) = stack.pop() {
        let mut readers = BTreeSet::new();
        state
            .index
            .dependents_of_cell(inputs, live_anchor(state, node), &mut readers);
        for &reader in &readers {
            if closure.insert(reader) {
                stack.push(reader);
            }
        }
        readers_of.insert(node, readers.into_iter().collect());
    }
    readers_of
}

/// The dirty closure of `roots`, in the order of the ready queue.
fn schedule(
    state: &EngineState,
    inputs: &ModelRoot,
    roots: BTreeSet<NodeId>,
) -> Result<Vec<NodeId>, StartError> {
    let readers_of = dirty_closure(state, inputs, roots);
    let mut waiting: BTreeMap<NodeId, usize> = readers_of.keys().map(|node| (*node, 0)).collect();
    for readers in readers_of.values() {
        for reader in readers {
            *waiting
                .get_mut(reader)
                .expect("a reader joined the closure") += 1;
        }
    }
    let mut ready: BTreeSet<(CellKey, NodeId)> = waiting
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(node, _)| (live_anchor(state, *node), *node))
        .collect();
    let mut order = Vec::with_capacity(readers_of.len());
    while let Some((_, node)) = ready.pop_first() {
        order.push(node);
        for &reader in &readers_of[&node] {
            let count = waiting
                .get_mut(&reader)
                .expect("a reader joined the closure");
            *count -= 1;
            if *count == 0 {
                ready.insert((live_anchor(state, reader), reader));
            }
        }
    }
    if order.len() < readers_of.len() {
        let mut cells: Vec<CellKey> = waiting
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(node, _)| live_anchor(state, *node))
            .collect();
        cells.sort();
        return Err(StartError::Cycle { cells });
    }
    Ok(order)
}

/// The caller's observer during one formula's evaluation, shared by the engine's reader, which
/// shows it each run, and the instrument CalcFml's evaluator reports to. Neither borrows it across
/// the other: a read borrows it while the read runs, and the evaluator reports a call only after
/// the call returns and its outcome only after the evaluation ends, never inside a read.
type SharedObserver<'l, 'o> = &'l RefCell<&'o mut dyn ReadObserver>;

/// The engine's reader, with the caller's observer shown each run before the evaluation receives
/// it, for one formula's evaluation.
struct Observed<'l, 'o> {
    observer: SharedObserver<'l, 'o>,
    formula: CellKey,
    cells: &'l mut dyn ReferenceReader,
    /// The area of the read the observer stopped.
    stopped: Option<GridArea>,
}

impl ReferenceReader for Observed<'_, '_> {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        let total = area.cell_count();
        if self.stopped.is_some() {
            let _ = sink(CellRun::Missing(total));
            return;
        }
        let mut observer = self.observer.borrow_mut();
        let formula = self.formula;
        let mut offset = 0u64;
        let mut stopped = false;
        self.cells.read_area(area, &mut |run| {
            if observer.observe(formula, area, offset, run).is_break() {
                stopped = true;
                return ControlFlow::Break(());
            }
            offset += run.len();
            sink(run)
        });
        drop(observer);
        if stopped {
            // The withheld run and the rest of the area reach the evaluation as cells the reader
            // cannot supply, so it reads no value the engine's reader did not report; the
            // recalculation fails whatever the evaluation returns.
            self.stopped = Some(area);
            let _ = sink(CellRun::Missing(total - offset));
        }
    }
}

/// The instrument on the evaluation route: CalcFml's evaluator reports each call it runs and the
/// outcome it returns to this, and it passes them on to the caller's observer, each call with the
/// formula and each outcome's report, which only the evaluator makes, unchanged with the
/// recalculation's calculation. It holds nothing the engine stages or publishes.
struct Instrument<'l, 'o> {
    observer: SharedObserver<'l, 'o>,
    formula: CellKey,
    calculation: &'l Calculation,
}

impl EvaluationObserver for Instrument<'_, '_> {
    fn called(&mut self, call: ExecutedCall) {
        self.observer.borrow_mut().called(self.formula, call);
    }

    fn returned(&mut self, report: EvaluationReport<'_>) {
        self.observer
            .borrow_mut()
            .evaluated(ObservedEvaluation::new(self.calculation, report));
    }
}

/// Evaluates one scheduled node and gives the result to stage, or the failure that stops the
/// recalculation.
fn evaluate_node(
    state: &EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
    node: NodeId,
    observer: &mut dyn ReadObserver,
) -> Result<FormulaResult, RecalcFailure> {
    let formula = live_anchor(state, node);
    let calculation = work.calculation.clone();
    let instance = match state
        .bindings
        .get(&node)
        .expect("begin_recalc bound every node it scheduled")
    {
        Binding::Placed(instance) => instance,
        Binding::Refused(refusal) => {
            return Ok(FormulaResult::NotModeled {
                refusal: refusal.clone(),
                calculation,
            });
        }
    };
    let mut cells = WorkbookReader {
        inputs,
        state,
        staged: &work.staged,
        scheduled: &work.scheduled,
        unscheduled: None,
    };
    let observer = RefCell::new(observer);
    let (outcome, stopped) = {
        let mut observed = Observed {
            observer: &observer,
            formula,
            cells: &mut cells,
            stopped: None,
        };
        let instrument: &mut dyn EvaluationObserver = &mut Instrument {
            observer: &observer,
            formula,
            calculation: &calculation,
        };
        let outcome = {
            let mut services =
                EvaluationServices::new(calculation.mode(), &mut observed).observed_by(instrument);
            evaluate(EvalFrame::new(instance, &mut work.scratch), &mut services)
        };
        (outcome, observed.stopped)
    };
    if let Some(area) = stopped {
        return Err(RecalcFailure::ReadFailed { formula, area });
    }
    if let Some(cell) = cells.unscheduled {
        // A cell none of the formula's own references names was read through a reference the
        // evaluation computed (OFFSET's): the schedule cannot order it, and dynamic dependencies
        // are L2's (dc-0k9). The formula is refused rather than given the value read; a missed
        // edge of a reference the formula holds still fails the recalculation.
        let named = instance
            .areas()
            .iter()
            .any(|area| area.is_ok_and(|area| area.contains(cell)));
        if named {
            return Err(RecalcFailure::UnscheduledRead { formula, cell });
        }
        return Ok(FormulaResult::NotModeled {
            refusal: NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "a reference computed at evaluation to a formula cell this recalculation calculates later",
                owner: "dc-0k9",
            }),
            calculation,
        });
    }
    let outcome = outcome.map(|value| one_cell_array_value(inputs, formula, value));
    match outcome {
        Ok(value) => Ok(match NotModeled::publishable(value) {
            Ok(value) => FormulaResult::Calculated { value, calculation },
            Err(refusal) => FormulaResult::NotModeled {
                refusal,
                calculation,
            },
        }),
        Err(EvalFault::ReadContract(error)) => Err(RecalcFailure::ReadFailed {
            formula,
            area: error.area,
        }),
        Err(fault) => Ok(FormulaResult::NotModeled {
            refusal: NotModeled::Evaluation(fault),
            calculation,
        }),
    }
}

/// The value an array formula of one cell at `at` shows: the first element of an array result
/// when it is a legacy (Ctrl+Shift+Enter) array formula, which shows one element per cell of its
/// area, or when the array has one element; any other value as it is (a larger array of a
/// dynamic-array formula is a spill, which publishing refuses).
fn one_cell_array_value(inputs: &ModelRoot, at: CellKey, value: CalcValue) -> CalcValue {
    let Some(array) = inputs.definitions().array(at) else {
        return value;
    };
    let first = match value.as_array() {
        Some(elements) if array.kind() == ArrayKind::Legacy || elements.cell_count() == 1 => {
            elements.get(0, 0).cloned()
        }
        _ => None,
    };
    match first {
        Some(element) if element.is_empty() => CalcValue::number(0.0),
        Some(element) => element,
        None => value,
    }
}
