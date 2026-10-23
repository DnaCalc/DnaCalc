//! Recalculation (plan 6): [`begin_recalc`] binds each unbound node's formula through CalcFml and
//! orders the dirty closure, [`step`] evaluates the ordered nodes through CalcFml a budget at a
//! time, and [`finish`] publishes the staged results as one generation. Nothing is published
//! before `finish` succeeds; a recalculation that fails or is refused leaves the published
//! generation as it was, and its pending nodes pending.
//!
//! Work package L2 (dc-0k9) adds what the order of the graph's edges alone cannot decide:
//!
//! - **Dynamic dependencies.** An evaluation may read cells no placed reference names (INDIRECT's
//!   text, OFFSET's result, a spilled range `#` names). When it reads a formula cell this
//!   recalculation has yet to evaluate, its result is discarded, never staged, and the formula
//!   waits for that cell and is evaluated again once it is evaluated. A formula that would wait
//!   on itself, directly or through others waiting, is on a circular reference and is refused
//!   (cycles are L3's, dc-m3u). Every area an evaluation read beyond its placed references is
//!   filed in the one graph as a dynamic prerequisite when the recalculation publishes, so a
//!   change to such a cell makes the formula stale and the next recalculation orders it after
//!   the formulas it read.
//! - **Spills.** A dynamic-array formula's result spills from its anchor (`spill.rs`); the cells
//!   its spill fills or would fill are dynamic prerequisites of the anchor, so a cell entered
//!   into them blocks the spill and a blocking cell cleared releases it. When an anchor's filled
//!   area changes, the formulas that read the cells it gained or lost and were already evaluated,
//!   or are not calculated by this recalculation, are calculated again in a further wave, with
//!   their dirty closure, until the areas settle.
//! - **Volatile functions.** A formula that calls one is calculated by every recalculation.
//! - **Random numbers.** RAND, RANDBETWEEN and RANDARRAY draw from a stream keyed by the state's
//!   seed, the recalculation's tick and the formula's cell (`random.rs`).
//!
//! The waiting formulas, the ready queue and the waves follow OxCalc's recalculation tracker
//! (`src/oxcalc-core/src/recalc.rs`, `Stage1RecalcTracker`: a node reads only published or
//! verified values, and one whose dependency shape changed re-enters the schedule) and its spill
//! invalidation (`src/oxcalc-core/src/grid/machine.rs`, `grid_vacated_spill_extent_dirty_seeds`),
//! at OxCalc commit 1bd514ef, rewritten on this engine's one graph; OxCalc's tree-reference
//! language and consumer layer are left behind.

use crate::graph::{AxisInterval, Prerequisite};
use crate::random::RandomStream;
use crate::read::{
    NodeStanding, ObservedEvaluation, ReadObserver, RecalcView, SnapshotCatalog, WorkbookReader,
};
use crate::result::{
    CalcTick, Calculation, FormulaResult, NotModeled, SpillChange, ValueChange, ValueDelta,
};
use crate::spill::{self, Spill, SpillIndex, SpillOutcome};
use crate::state::{ArrayRole, Binding, EngineState};
use calc_doc_model::{ArrayKind, AuthoredRevision, Axis, CellInput, FormulaText, ModelRoot};
use calc_fml::reference::{
    CellRun, FormulaAnchor, ReferenceNotation, ReferenceReader, ReferenceShape, template_key,
};
use calc_fml::{
    EvalFault, EvalFrame, EvalScratch, EvaluationObserver, EvaluationReport, EvaluationServices,
    ExecutedCall, FormulaForm, FormulaInstance, ResolvedReference, compile_loaded_with, evaluate,
    instantiate, parse_formula,
};
use calc_value::{CalcValue, CellKey, GridArea, NodeId, NumericMode};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::ops::ControlFlow;
use std::sync::Arc;

/// How many waves one recalculation runs before it refuses the formulas whose spills have not
/// settled.
const MAX_WAVES: u32 = 32;

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
    /// The evaluation of the formula at `formula` read the formula cell `cell`, which one of
    /// the formula's own placed references names and which this recalculation calculates, had
    /// not yet evaluated and was not waiting for another cell: the order missed an edge of the
    /// graph. Its old value was not handed out. A cell read through a reference the evaluation
    /// computed is waited for instead ([`step`]).
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

/// One recalculation in progress: the nodes it evaluates in their order, the formulas waiting for
/// a cell it has yet to evaluate, and the results, spills and dynamic prerequisites it has
/// staged. It holds no borrow of the engine state, which [`step`] and [`finish`] check has not
/// moved on since [`begin_recalc`].
#[derive(Debug)]
pub struct RecalcWork {
    revision: AuthoredRevision,
    generation: u64,
    calculation: Calculation,
    /// The current wave's nodes in their order, and the next to take.
    order: Vec<NodeId>,
    next: usize,
    /// Nodes whose awaited cell has been evaluated, taken before the order's next.
    ready: VecDeque<NodeId>,
    /// The waiting nodes by the node they wait for, and each waiting node's awaited node.
    waiting: BTreeMap<NodeId, Vec<NodeId>>,
    waits_on: BTreeMap<NodeId, NodeId>,
    scheduled: BTreeSet<NodeId>,
    evaluated: BTreeSet<NodeId>,
    staged: BTreeMap<NodeId, FormulaResult>,
    /// The active spills of the anchors evaluated, and each evaluated anchor's filled area.
    spills: SpillIndex,
    filled: BTreeMap<NodeId, Option<GridArea>>,
    /// Each evaluated node's dynamic prerequisites.
    dynamic: BTreeMap<NodeId, Vec<Prerequisite>>,
    /// Nodes to calculate again in the next wave.
    rewave: BTreeSet<NodeId>,
    waves: u32,
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

    /// How many nodes it evaluates: those its first ordering scheduled, and those a later wave
    /// added.
    pub fn scheduled(&self) -> usize {
        self.scheduled.len()
    }

    fn remaining(&self) -> usize {
        (self.order.len() - self.next) + self.ready.len() + self.waits_on.len() + self.rewave.len()
    }

    fn progress(&self) -> RecalcProgress {
        if self.failure.is_some() {
            RecalcProgress::Failed
        } else if self.remaining() > 0 {
            RecalcProgress::Pending {
                remaining: self.remaining(),
            }
        } else {
            RecalcProgress::Complete
        }
    }

    fn view<'a>(&'a self, state: &'a EngineState, inputs: &'a ModelRoot) -> RecalcView<'a> {
        RecalcView {
            inputs,
            state,
            staged: &self.staged,
            scheduled: &self.scheduled,
            evaluated: &self.evaluated,
            spills: &self.spills,
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
///   CalcFml refuses to compile reads nothing and is published as not modelled. An array formula
///   compiles in CalcFml's dynamic-array form; any other formula in the legacy form.
/// - The dirty closure is the pending nodes, the nodes whose formulas call a volatile function,
///   and every node that reads, directly or through other nodes, the anchor of one of them or a
///   cell its spill fills (or, before the engine has calculated it, the file's area of its array
///   formula). When `mode` differs from the numeric mode of the published generation, the
///   closure is every formula node instead: a published generation is calculated under one mode,
///   and a result calculated under another is never read as an input (plan 3.4).
/// - The closure is ordered by a ready queue with maintained prerequisite counts: a node is ready
///   once every node of the closure whose anchor or spill it reads is ordered, and of the ready
///   nodes the one whose anchor comes first (sheet ID, then row, then column) is ordered next.
///   When the dynamic prerequisites close a cycle that the placed references do not, the closure
///   is ordered by the placed references alone, and a formula that then reads a cell evaluated
///   later waits for it ([`step`]). A closure the placed references alone cannot order has a
///   cycle, and the recalculation is refused.
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
        _ => {
            let mut roots = state.pending.clone();
            roots.extend(state.volatile.iter().copied());
            roots
        }
    };
    let order = schedule(state, inputs, roots, &|node| {
        published_areas(state, inputs, node)
    })
    .map_err(|cells| StartError::Cycle { cells })?;
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
        ready: VecDeque::new(),
        waiting: BTreeMap::new(),
        waits_on: BTreeMap::new(),
        evaluated: BTreeSet::new(),
        staged: BTreeMap::new(),
        spills: SpillIndex::default(),
        filled: BTreeMap::new(),
        dynamic: BTreeMap::new(),
        rewave: BTreeSet::new(),
        waves: 0,
        scratch: EvalScratch::new(),
        failure: None,
    })
}

/// Evaluates up to `budget` of the recalculation's remaining nodes, each through CalcFml's
/// evaluator reading through the engine's reader, with `observer` shown every run the evaluation
/// receives and what the evaluator reports of each call it runs, each reference it resolves from
/// text and the outcome it returns (see [`ReadObserver`]), and stages each result:
///
/// - a value a cell holds (a finite number, a text, a logical or an error) is staged as
///   calculated, by this recalculation's tick, at its revision and under its mode; an array
///   formula's anchor is staged with the value it shows, and its spill beside it;
/// - a formula CalcFml refused to compile, a typed refusal of the evaluation (a function fault,
///   an input the reader cannot supply, a range where one value is needed, a final sum near zero)
///   and any other value are staged as not modelled;
/// - an evaluation that read a cell this recalculation has yet to calculate (a formula cell, or a
///   cell the spill of a formula not yet evaluated may fill) is discarded, and the formula waits
///   for that cell ([`ReadObserver::deferred`]); one that would wait on itself is staged as not
///   modelled;
/// - a read the observer stops, a read that breaks the reader contract, or a read of a formula
///   cell one of the formula's own placed references names, which the order should have
///   evaluated first, fails the recalculation.
///
/// A literal cell's value is read from the snapshot's page. A formula cell's value is read from
/// what this recalculation staged for it, or else from the published result when that was
/// calculated; a file cache or a result not modelled is read as a cell the reader cannot supply.
/// A cell that holds no input reads as the value a staged or published spill fills it with,
/// and otherwise as blank, or as not available when it is an unloaded cell or lies in the
/// file's area of an array formula whose result is not calculated. The observer cannot change
/// any of these (see [`ReadObserver`]). A step after a failure, or against a state or snapshot
/// that has moved on, evaluates nothing.
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
    let mut evaluations = 0;
    while work.failure.is_none() && evaluations < budget.evaluations {
        let Some(node) = next_node(state, inputs, work) else {
            break;
        };
        evaluations += 1;
        match evaluate_node(state, inputs, work, node, &mut *observer) {
            Ok(Attempt::Done(done)) => stage(state, inputs, work, node, done),
            Ok(Attempt::Waits { on, cell }) => {
                observer.deferred(live_anchor(state, node), cell);
                defer(state, inputs, work, node, on);
            }
            Err(failure) => work.failure = Some(failure),
        }
    }
    work.progress()
}

/// Publishes a completed recalculation: each evaluated node's staged result replaces its published
/// one, with its spill and its dynamic prerequisites; the results and spills of formulas removed
/// since the last publication are removed, the evaluated nodes are no longer pending, and the
/// published generation's numeric mode is the recalculation's. Returns what changed. A
/// recalculation that failed, is incomplete, or was superseded publishes nothing.
pub fn finish(state: &mut EngineState, work: RecalcWork) -> Result<ValueDelta, RecalcFailure> {
    if let Some(failure) = work.failure {
        return Err(failure);
    }
    let remaining = work.remaining();
    if remaining > 0 {
        return Err(RecalcFailure::Incomplete { remaining });
    }
    if state.revision() != work.revision || state.generation != work.generation {
        return Err(RecalcFailure::Superseded {
            revision: work.revision,
        });
    }
    let mut changes: BTreeMap<CellKey, ValueChange> = BTreeMap::new();
    let mut spill_changes = Vec::new();
    for (node, result) in work.staged {
        let at = live_anchor(state, node);
        let before = state.spills.remove(at).map(|spill| spill.area());
        let after = work.spills.get(at).cloned();
        let after_area = after.as_ref().map(Spill::area);
        if let Some(spill) = after {
            state.spills.insert(spill);
        }
        if before != after_area {
            spill_changes.push(SpillChange {
                anchor: at,
                before,
                after: after_area,
            });
        }
        state.results.insert(at, (node, result.clone()));
        state.pending.remove(&node);
        changes.insert(at, ValueChange::Published { at, node, result });
    }
    for (node, dynamic) in work.dynamic {
        state.set_dynamic(node, dynamic);
    }
    for at in std::mem::take(&mut state.retired) {
        if let Some(&(node, _)) = state.results.get(&at)
            && state.node_at(at) != Some(node)
        {
            state.results.remove(&at);
            if let Some(spill) = state.spills.remove(at) {
                spill_changes.push(SpillChange {
                    anchor: at,
                    before: Some(spill.area()),
                    after: None,
                });
            }
            changes.insert(at, ValueChange::Removed { at, node });
        }
    }
    state.generation += 1;
    state.published_mode = Some(work.calculation.mode().clone());
    spill_changes.sort_by_key(|change| change.anchor);
    Ok(ValueDelta::new(
        work.revision,
        work.calculation.tick(),
        changes.into_values().collect(),
    )
    .with_spill_changes(spill_changes))
}

pub(crate) fn live_anchor(state: &EngineState, node: NodeId) -> CellKey {
    state
        .node(node)
        .expect("a pending or scheduled node is live at the work's revision")
        .anchor()
}

/// Compiles and places the formula of `node`, anchored at `at`, and files its prerequisites. The
/// document holds a formula's text as a file stores it ([`FormulaText`]), so it is compiled as
/// such ([`compile_loaded_with`]). An array formula compiles in the dynamic-array form, which
/// lifts: a legacy (Ctrl+Shift+Enter) one fills its fixed area, a dynamic one spills.
fn bind(
    state: &mut EngineState,
    inputs: &ModelRoot,
    node: NodeId,
    at: CellKey,
    text: &FormulaText,
) {
    let anchor = FormulaAnchor::new(at);
    let (form, role) = match inputs.definitions().array(at) {
        None => (FormulaForm::Legacy, ArrayRole::Cell),
        Some(array) => match array.kind() {
            ArrayKind::Legacy if array.area().cell_count() == 1 => {
                (FormulaForm::Dynamic, ArrayRole::LegacyCell)
            }
            ArrayKind::Legacy => (FormulaForm::Dynamic, ArrayRole::LegacyFill(array.area())),
            ArrayKind::Dynamic { .. } => (FormulaForm::Dynamic, ArrayRole::Spill),
        },
    };
    // A formula whose template was compiled before (a copy of a filled or shared formula) is
    // placed from that plan without being parsed; the plan is the one compiling it would give.
    let binding = match String::from_utf16(text.text().utf16_code_units()) {
        Err(_) => Binding::Refused(NotModeled::FormulaText),
        Ok(source) => {
            let key = template_key(&source, anchor).map(|key| (at.sheet(), form, key));
            match key.as_ref().and_then(|key| state.templates.get(key)) {
                Some(plan) => Binding::Placed(instantiate(plan, anchor), role),
                None => match compile_loaded_with(
                    &parse_formula(&source),
                    ReferenceNotation::A1,
                    anchor,
                    &SnapshotCatalog(inputs),
                    &mut state.plans,
                    form,
                ) {
                    Ok(plan) => {
                        if let Some(key) = key {
                            state.templates.insert(key, Arc::clone(&plan));
                        }
                        Binding::Placed(instantiate(&plan, anchor), role)
                    }
                    Err(error) => Binding::Refused(NotModeled::Compile(error)),
                },
            }
        }
    };
    let prerequisites = match &binding {
        Binding::Placed(instance, _) => {
            let plan = instance.plan();
            if plan.volatile() {
                state.volatile.insert(node);
            }
            if plan.calls().any(|target| {
                matches!(
                    target.spelling(),
                    "SUBTOTAL" | "AGGREGATE" | "_XLFN.AGGREGATE"
                )
            }) {
                state.subtotal_cells.insert(at);
            }
            prerequisites(instance)
        }
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
                area_prerequisite(*area)
            }
        };
        if !out.contains(&prerequisite) {
            out.push(prerequisite);
        }
    }
    out
}

/// One cell's prerequisite, or a rectangle's.
fn area_prerequisite(area: GridArea) -> Prerequisite {
    if area.cell_count() == 1 {
        Prerequisite::Cell(area.first())
    } else {
        Prerequisite::Area(area)
    }
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

/// The areas beside a node's anchor whose readers read the node's result: the area its published
/// spill fills, and the file's area of its array formula, which the engine may not have
/// calculated yet.
pub(crate) fn published_areas(
    state: &EngineState,
    inputs: &ModelRoot,
    node: NodeId,
) -> Vec<GridArea> {
    let at = live_anchor(state, node);
    let mut areas = Vec::new();
    if let Some(spill) = state.spills.get(at) {
        areas.push(spill.area());
    }
    if let Some(array) = inputs.definitions().array(at)
        && array.area().cell_count() > 1
        && !areas.contains(&array.area())
    {
        areas.push(array.area());
    }
    areas
}

/// The nodes that read the anchor of `node` or a cell of one of `areas`. A formula that reads its
/// own anchor is among them, a cycle; one that reads only the cells its own result fills is not
/// (its spill's own dynamic prerequisites are those cells), and an evaluation that reads them
/// finds the circular reference itself.
fn readers(
    state: &EngineState,
    inputs: &ModelRoot,
    node: NodeId,
    areas: &[GridArea],
) -> BTreeSet<NodeId> {
    let mut readers = BTreeSet::new();
    let mut area_readers = BTreeSet::new();
    for area in areas {
        state
            .index
            .dependents_of_area(inputs, *area, &mut area_readers);
    }
    area_readers.remove(&node);
    state
        .index
        .dependents_of_cell(inputs, live_anchor(state, node), &mut readers);
    readers.append(&mut area_readers);
    readers
}

/// The dirty closure of `roots`: the roots and every node that reads the anchor of one, or a cell
/// of one of the areas `areas` gives for it, directly or through other nodes, each with the
/// nodes that read it. `inputs` is the snapshot at the state's revision.
pub(crate) fn dirty_closure(
    state: &EngineState,
    inputs: &ModelRoot,
    roots: BTreeSet<NodeId>,
    areas: &dyn Fn(NodeId) -> Vec<GridArea>,
) -> BTreeMap<NodeId, Vec<NodeId>> {
    let mut stack: Vec<NodeId> = roots.iter().copied().collect();
    let mut closure = roots;
    let mut readers_of: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    while let Some(node) = stack.pop() {
        let readers = readers(state, inputs, node, &areas(node));
        for &reader in &readers {
            if closure.insert(reader) {
                stack.push(reader);
            }
        }
        readers_of.insert(node, readers.into_iter().collect());
    }
    readers_of
}

/// The dirty closure of `roots`, in the order of the ready queue; when the dynamic prerequisites
/// close a cycle, in the order of the placed references alone. `Err` with the anchors of the
/// nodes on or after a cycle of the placed references, in cell order.
fn schedule(
    state: &EngineState,
    inputs: &ModelRoot,
    roots: BTreeSet<NodeId>,
    areas: &dyn Fn(NodeId) -> Vec<GridArea>,
) -> Result<Vec<NodeId>, Vec<CellKey>> {
    let readers_of = dirty_closure(state, inputs, roots, areas);
    match order(state, &readers_of) {
        Ok(order) => Ok(order),
        Err(_) => {
            // Keep only the edges a placed reference makes: a dynamic prerequisite may be stale
            // (it is what the last evaluation read), and a formula that reads a cell evaluated
            // later waits for it at evaluation.
            let placed: BTreeMap<NodeId, Vec<NodeId>> = readers_of
                .iter()
                .map(|(node, readers)| {
                    let anchor = live_anchor(state, *node);
                    let node_areas = areas(*node);
                    let kept = readers
                        .iter()
                        .copied()
                        .filter(|reader| {
                            state
                                .node(*reader)
                                .and_then(|record| record.prerequisites())
                                .is_some_and(|placed| {
                                    placed.iter().any(|prerequisite| {
                                        prerequisite.covers(anchor)
                                            || node_areas
                                                .iter()
                                                .any(|area| prerequisite.meets_area(*area))
                                    })
                                })
                        })
                        .collect();
                    (*node, kept)
                })
                .collect();
            order(state, &placed)
        }
    }
}

/// The nodes of `readers_of` in the order of the ready queue, or the anchors of those a cycle
/// leaves unordered.
fn order(
    state: &EngineState,
    readers_of: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Result<Vec<NodeId>, Vec<CellKey>> {
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
        return Err(cells);
    }
    Ok(order)
}

/// The next node to evaluate: a node whose awaited cell is now evaluated, else the order's next.
/// When the order is done and nodes still wait, each waits on a node that waits too, which a
/// circular reference makes, and each is staged as not modelled. When everything is done and
/// formulas must be calculated again because a spill's area changed, the next wave begins.
fn next_node(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork) -> Option<NodeId> {
    loop {
        if let Some(node) = work.ready.pop_front() {
            return Some(node);
        }
        if work.next < work.order.len() {
            let node = work.order[work.next];
            work.next += 1;
            if work.evaluated.contains(&node) || work.waits_on.contains_key(&node) {
                continue;
            }
            return Some(node);
        }
        if !work.waits_on.is_empty() {
            let stuck: Vec<NodeId> = work.waits_on.keys().copied().collect();
            work.waits_on.clear();
            work.waiting.clear();
            for node in stuck {
                let done = refused(work, circular());
                stage(state, inputs, work, node, done);
            }
            continue;
        }
        if !work.rewave.is_empty() {
            next_wave(state, inputs, work);
            continue;
        }
        return None;
    }
}

/// Begins the next wave: the formulas to calculate again and their dirty closure, ordered, each
/// no longer evaluated (its staged result and spill stand until it is evaluated again).
fn next_wave(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork) {
    let roots = std::mem::take(&mut work.rewave);
    work.waves += 1;
    let areas = |node: NodeId| -> Vec<GridArea> {
        match work.filled.get(&node) {
            Some(filled) => filled.iter().copied().collect(),
            None => published_areas(state, inputs, node),
        }
    };
    let ordered = if work.waves > MAX_WAVES {
        Err(())
    } else {
        schedule(state, inputs, roots.clone(), &areas).map_err(|_| ())
    };
    match ordered {
        Ok(order) => {
            for node in &order {
                work.scheduled.insert(*node);
                if work.evaluated.remove(node) {
                    let at = live_anchor(state, *node);
                    work.spills.remove(at);
                }
            }
            work.order = order;
            work.next = 0;
        }
        Err(()) => {
            let refusal = NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "spill ranges that kept changing size within one recalculation",
                owner: "dc-0k9",
            });
            for node in roots {
                work.scheduled.insert(node);
                let done = refused(work, refusal.clone());
                stage(state, inputs, work, node, done);
            }
            work.rewave.clear();
        }
    }
}

/// The refusal of a formula on a circular reference that only an evaluation found.
fn circular() -> NotModeled {
    NotModeled::Evaluation(EvalFault::NotModelled {
        construct: "a circular reference through a reference computed at evaluation (INDIRECT, OFFSET, a spill)",
        owner: "dc-m3u",
    })
}

/// A staged refusal, with no spill and no dynamic prerequisite.
fn refused(work: &RecalcWork, refusal: NotModeled) -> Done {
    Done {
        result: FormulaResult::NotModeled {
            refusal,
            calculation: work.calculation.clone(),
        },
        spill: None,
        dynamic: Vec::new(),
    }
}

/// Makes `node` wait for `on`, or stages it as not modelled when `on` waits, directly or through
/// others, for `node`.
fn defer(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork, node: NodeId, on: NodeId) {
    let mut cursor = on;
    loop {
        if cursor == node {
            let done = refused(work, circular());
            stage(state, inputs, work, node, done);
            return;
        }
        match work.waits_on.get(&cursor) {
            Some(next) => cursor = *next,
            None => break,
        }
    }
    work.waits_on.insert(node, on);
    work.waiting.entry(on).or_default().push(node);
}

/// Stages an evaluated node's result, spill and dynamic prerequisites, releases the nodes that
/// waited for it, and, when its filled area changed, marks for the next wave the readers of the
/// cells it gained or lost that are already evaluated or not calculated by this recalculation.
fn stage(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork, node: NodeId, done: Done) {
    let at = live_anchor(state, node);
    let before = match work.filled.get(&node) {
        Some(filled) => *filled,
        None => match state.spills.get(at) {
            Some(spill) => Some(spill.area()),
            None => match state.result(at) {
                Some(FormulaResult::Calculated { .. } | FormulaResult::NotModeled { .. }) => None,
                _ => inputs
                    .definitions()
                    .array(at)
                    .map(|array| array.area())
                    .filter(|area| area.cell_count() > 1),
            },
        },
    };
    let after = done.spill.as_ref().map(Spill::area);
    work.spills.remove(at);
    if let Some(spill) = done.spill {
        work.spills.insert(spill);
    }
    work.filled.insert(node, after);
    work.staged.insert(node, done.result);
    work.evaluated.insert(node);
    work.dynamic.insert(node, done.dynamic);
    if before != after {
        let mut readers = BTreeSet::new();
        for area in before.iter().chain(after.iter()) {
            state.index.dependents_of_area(inputs, *area, &mut readers);
        }
        readers.remove(&node);
        for reader in readers {
            if work.evaluated.contains(&reader) || !work.scheduled.contains(&reader) {
                work.rewave.insert(reader);
            }
        }
    }
    if let Some(waiters) = work.waiting.remove(&node) {
        for waiter in waiters {
            work.waits_on.remove(&waiter);
            work.ready.push_back(waiter);
        }
    }
}

/// The caller's observer during one formula's evaluation, shared by the engine's reader, which
/// shows it each run, and the instrument CalcFml's evaluator reports to. Neither borrows it across
/// the other: a read borrows it while the read runs, and the evaluator reports a call only after
/// the call returns and its outcome only after the evaluation ends, never inside a read.
type SharedObserver<'l, 'o> = &'l RefCell<&'o mut dyn ReadObserver>;

/// The engine's reader, with the caller's observer shown each run before the evaluation receives
/// it, for one formula's evaluation. It keeps every area the evaluation read.
struct Observed<'l, 'o> {
    observer: SharedObserver<'l, 'o>,
    formula: CellKey,
    cells: &'l mut dyn ReferenceReader,
    footprint: &'l RefCell<Vec<GridArea>>,
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
        self.footprint.borrow_mut().push(area);
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

    fn spill_extent(&mut self, anchor: CellKey) -> calc_fml::reference::SpillExtent {
        self.cells.spill_extent(anchor)
    }

    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<calc_fml::SubtotalExclusions> {
        self.cells.subtotal_exclusions(area)
    }
}

/// The instrument on the evaluation route: CalcFml's evaluator reports each call it runs, each
/// reference it resolves from text and the outcome it returns to this, and it passes them on to
/// the caller's observer, each call and resolution with the formula and each outcome's report,
/// which only the evaluator makes, unchanged with the recalculation's calculation. The outcome
/// of an evaluation the engine discards, because it read a cell this recalculation has yet to
/// calculate, is not passed on. It holds nothing the engine stages or publishes.
struct Instrument<'l, 'o> {
    observer: SharedObserver<'l, 'o>,
    formula: CellKey,
    calculation: &'l Calculation,
    footprint: &'l RefCell<Vec<GridArea>>,
    uncomputed: &'l Cell<Option<(CellKey, NodeId)>>,
}

impl EvaluationObserver for Instrument<'_, '_> {
    fn called(&mut self, call: ExecutedCall) {
        self.observer.borrow_mut().called(self.formula, call);
    }

    fn resolved(&mut self, reference: ResolvedReference) {
        self.footprint.borrow_mut().push(reference.area());
        self.observer.borrow_mut().resolved(self.formula, reference);
    }

    fn returned(&mut self, report: EvaluationReport<'_>) {
        if self.uncomputed.get().is_some() {
            return;
        }
        self.observer
            .borrow_mut()
            .evaluated(ObservedEvaluation::new(self.calculation, report));
    }
}

/// What one evaluation of a node gives.
enum Attempt {
    /// The result to stage.
    Done(Done),
    /// The evaluation read `cell`, which `on` calculates and this recalculation has yet to
    /// evaluate: it is discarded, and the node waits for `on`.
    Waits { on: NodeId, cell: CellKey },
}

/// A node's staged result, with its spill when its result fills cells beside its anchor and the
/// dynamic prerequisites its evaluation found.
struct Done {
    result: FormulaResult,
    spill: Option<Spill>,
    dynamic: Vec<Prerequisite>,
}

/// Evaluates one scheduled node and gives what to stage, or the failure that stops the
/// recalculation.
fn evaluate_node(
    state: &EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
    node: NodeId,
    observer: &mut dyn ReadObserver,
) -> Result<Attempt, RecalcFailure> {
    let formula = live_anchor(state, node);
    let calculation = work.calculation.clone();
    let (instance, role) = match state
        .bindings
        .get(&node)
        .expect("begin_recalc bound every node it scheduled")
    {
        Binding::Placed(instance, role) => (instance, *role),
        Binding::Refused(refusal) => return Ok(Attempt::Done(refused(work, refusal.clone()))),
    };
    let uncomputed = Cell::new(None);
    let footprint = RefCell::new(Vec::new());
    let view = RecalcView {
        inputs,
        state,
        staged: &work.staged,
        scheduled: &work.scheduled,
        evaluated: &work.evaluated,
        spills: &work.spills,
    };
    let mut cells = WorkbookReader::new(view, &uncomputed);
    let catalog = SnapshotCatalog(inputs);
    let mut random = RandomStream::new(state.random_seed, calculation.tick().get(), formula);
    let observer = RefCell::new(observer);
    let (outcome, stopped) = {
        let mut observed = Observed {
            observer: &observer,
            formula,
            cells: &mut cells,
            footprint: &footprint,
            stopped: None,
        };
        let instrument: &mut dyn EvaluationObserver = &mut Instrument {
            observer: &observer,
            formula,
            calculation: &calculation,
            footprint: &footprint,
            uncomputed: &uncomputed,
        };
        let outcome = {
            let mut services = EvaluationServices::new(calculation.mode(), &mut observed)
                .with_catalog(&catalog)
                .with_random(&mut random)
                .observed_by(instrument);
            evaluate(EvalFrame::new(instance, &mut work.scratch), &mut services)
        };
        (outcome, observed.stopped)
    };
    if let Some(area) = stopped {
        return Err(RecalcFailure::ReadFailed { formula, area });
    }
    if let Some((cell, on)) = uncomputed.get() {
        // A formula cell one of the formula's own placed references names was ordered before it,
        // unless it waits itself: the graph missed an edge.
        let named = instance
            .areas()
            .iter()
            .any(|area| area.is_ok_and(|area| area.contains(cell)));
        let formula_cell = matches!(inputs.cell(cell), Some(CellInput::Formula(_)));
        if named && formula_cell && !work.waits_on.contains_key(&on) {
            return Err(RecalcFailure::UnscheduledRead { formula, cell });
        }
        return Ok(Attempt::Waits { on, cell });
    }
    let mut dynamic = dynamic_prerequisites(state, node, instance, footprint.into_inner());
    let value = match outcome {
        Ok(value) => value,
        Err(EvalFault::ReadContract(error)) => {
            return Err(RecalcFailure::ReadFailed {
                formula,
                area: error.area,
            });
        }
        Err(fault) => {
            return Ok(Attempt::Done(Done {
                result: FormulaResult::NotModeled {
                    refusal: NotModeled::Evaluation(fault),
                    calculation,
                },
                spill: None,
                dynamic,
            }));
        }
    };
    let outcome = match array_outcome(view_of(work, state, inputs), node, formula, role, value) {
        Ok(outcome) => outcome,
        Err(Placed::Waits { on, cell }) => return Ok(Attempt::Waits { on, cell }),
        Err(Placed::Refused(refusal)) => {
            return Ok(Attempt::Done(Done {
                result: FormulaResult::NotModeled {
                    refusal,
                    calculation,
                },
                spill: None,
                dynamic,
            }));
        }
    };
    if let Some(extent) = outcome.extent() {
        for beside in spill::beside_anchor(extent) {
            let prerequisite = Prerequisite::Area(beside);
            if !dynamic.contains(&prerequisite) {
                dynamic.push(prerequisite);
            }
        }
    }
    let shown = outcome.anchor_value();
    let spill = match outcome {
        SpillOutcome::Fills(spill) => Some(spill),
        SpillOutcome::Scalar(_) | SpillOutcome::Blocked { .. } => None,
    };
    let result = match NotModeled::publishable(shown) {
        Ok(value) => FormulaResult::Calculated { value, calculation },
        Err(refusal) => {
            return Ok(Attempt::Done(Done {
                result: FormulaResult::NotModeled {
                    refusal,
                    calculation,
                },
                spill: None,
                dynamic,
            }));
        }
    };
    Ok(Attempt::Done(Done {
        result,
        spill,
        dynamic,
    }))
}

fn view_of<'a>(
    work: &'a RecalcWork,
    state: &'a EngineState,
    inputs: &'a ModelRoot,
) -> RecalcView<'a> {
    work.view(state, inputs)
}

/// The areas an evaluation read that none of the formula's placed references holds, each once,
/// as prerequisites.
fn dynamic_prerequisites(
    state: &EngineState,
    node: NodeId,
    instance: &FormulaInstance,
    footprint: Vec<GridArea>,
) -> Vec<Prerequisite> {
    let placed = state
        .node(node)
        .and_then(|record| record.prerequisites())
        .unwrap_or(&[]);
    let mut out: Vec<Prerequisite> = Vec::new();
    for area in footprint {
        let held = placed
            .iter()
            .any(|prerequisite| prerequisite.contains_area(area))
            || instance.areas().iter().any(|placed| {
                placed.is_ok_and(|placed| {
                    placed.contains(area.first()) && placed.contains(area.last())
                })
            });
        let prerequisite = area_prerequisite(area);
        if !held && !out.contains(&prerequisite) {
            out.push(prerequisite);
        }
    }
    out
}

/// Why an array result could not be placed.
enum Placed {
    /// A cell of the area it would fill may be filled by `on`'s spill, which this recalculation
    /// has yet to evaluate.
    Waits { on: NodeId, cell: CellKey },
    /// The result is refused.
    Refused(NotModeled),
}

/// What the result `value` of the formula at `anchor` shows and fills, by its role.
fn array_outcome(
    view: RecalcView<'_>,
    node: NodeId,
    anchor: CellKey,
    role: ArrayRole,
    value: CalcValue,
) -> Result<SpillOutcome, Placed> {
    let array = value
        .as_array()
        .filter(|_| value.callable_value().is_none())
        .cloned();
    match role {
        ArrayRole::Cell => Ok(SpillOutcome::Scalar(value)),
        ArrayRole::LegacyCell => Ok(SpillOutcome::Scalar(match &array {
            Some(array) => shown(array.get(0, 0).expect("a non-empty array")),
            None => value,
        })),
        ArrayRole::LegacyFill(_) if value.callable_value().is_some() => {
            Ok(SpillOutcome::Scalar(value))
        }
        ArrayRole::LegacyFill(area) => Ok(SpillOutcome::Fills(spill::fill_of(area, &value))),
        ArrayRole::Spill => {
            let Some(array) = array else {
                return Ok(SpillOutcome::Scalar(value));
            };
            if array.cell_count() == 1 {
                return Ok(SpillOutcome::Scalar(shown(
                    array.get(0, 0).expect("a non-empty array"),
                )));
            }
            let (extent, fits) = spill::extent_of(anchor, array.shape());
            if !fits {
                return Ok(SpillOutcome::Blocked { extent });
            }
            match collision(view, node, anchor, extent) {
                Collision::Free => Ok(SpillOutcome::Fills(spill::spill_of(extent, &array))),
                Collision::Blocked => Ok(SpillOutcome::Blocked { extent }),
                Collision::Waits { on, cell } => Err(Placed::Waits { on, cell }),
                Collision::Unknown => Err(Placed::Refused(NotModeled::Evaluation(
                    EvalFault::NotModelled {
                        construct: "a spill over the file's area of another array formula whose result is not calculated",
                        owner: "dc-0k9",
                    },
                ))),
            }
        }
    }
}

/// A value as a cell shows it: a blank as 0.
fn shown(value: &CalcValue) -> CalcValue {
    if value.is_empty() || value.is_missing() {
        CalcValue::number(0.0)
    } else {
        value.clone()
    }
}

/// Whether a spill over `extent` from `anchor` collides.
enum Collision {
    Free,
    Blocked,
    Waits { on: NodeId, cell: CellKey },
    Unknown,
}

/// Whether the spill of `node`, anchored at `anchor`, may fill `extent`: a cell of it other than
/// the anchor that holds an input or is unloaded blocks it, and so does another anchor's spill or
/// legacy array area in place (staged, or published for an anchor this recalculation does not
/// calculate). A published spill or file area of an anchor this recalculation has yet to
/// evaluate makes it wait; the file area of a dynamic array formula whose result is not
/// calculated leaves it unknown.
fn collision(view: RecalcView<'_>, node: NodeId, anchor: CellKey, extent: GridArea) -> Collision {
    let inputs = view.inputs;
    if inputs
        .cells(extent)
        .into_iter()
        .flatten()
        .any(|(at, _)| at != anchor)
    {
        return Collision::Blocked;
    }
    let definitions = inputs.definitions();
    if definitions.unloaded_in(extent).next().is_some() {
        return Collision::Blocked;
    }
    if view
        .spills
        .meeting(extent)
        .iter()
        .any(|spill| spill.anchor() != anchor)
    {
        return Collision::Blocked;
    }
    for spill in view.state.spills.meeting(extent) {
        let other = spill.anchor();
        let Some(owner) = view.state.node_at(other).filter(|owner| *owner != node) else {
            continue;
        };
        match view.standing(owner) {
            NodeStanding::Pending if spill.is_fixed() => return Collision::Blocked,
            NodeStanding::Pending => {
                return Collision::Waits {
                    on: owner,
                    cell: other,
                };
            }
            NodeStanding::Staged(_) => {}
            NodeStanding::Published(_) => return Collision::Blocked,
        }
    }
    for array in definitions.arrays_meeting(extent) {
        let other = array.anchor();
        if other == anchor {
            continue;
        }
        let legacy = array.kind() == ArrayKind::Legacy;
        let Some(owner) = view.state.node_at(other) else {
            continue;
        };
        match view.standing(owner) {
            NodeStanding::Pending if legacy => return Collision::Blocked,
            NodeStanding::Pending => {
                return Collision::Waits {
                    on: owner,
                    cell: other,
                };
            }
            NodeStanding::Staged(result) | NodeStanding::Published(Some(result))
                if result.calculated().is_some() => {}
            _ if legacy => return Collision::Blocked,
            _ => return Collision::Unknown,
        }
    }
    Collision::Free
}
