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
//!   on itself, directly or through others waiting, is on a circular reference (below). Every
//!   area an evaluation read beyond its placed references is
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
//! Work package L3 (dc-m3u) adds circular references. The formulas on a cycle of placed
//! references are found when the order is made (strongly connected components, by OxCalc's
//! iterative Tarjan of `src/oxcalc-core/src/grid/machine/workbook_coordinator.rs`,
//! `cycle_groups`, at commit 1bd514ef, rewritten on this engine's graph); those on a cycle that
//! only an evaluation finds (INDIRECTs naming each other) are found when they would wait on each
//! other. The workbook's iteration settings ([`calc_doc_model::IterationSettings`]) decide what
//! happens to them:
//!
//! - **Iteration off.** Excel calculates no formula whose evaluation meets the circular reference,
//!   nor one that reads such a formula: each keeps the value its cell held, and a formula entered
//!   (or entered again) shows 0 (Excel 16.0, 2026-10-23 and 2026-10-25: with A1 `=B1+1` and B1
//!   `=A1+1` entered one after the other, A1 shows 1 and B1 0, and G1 `=A1*10` 0, through
//!   `CalculateFull`, `CalculateFullRebuild` and edits of inputs the readers also read; a file's
//!   stored values stay as stored; `=IF(TRUE,"x",F5)` in F5, which never reads F5, is calculated).
//!   So the formulas on a cycle the order finds are evaluated like any other, and those whose
//!   evaluations wait on each other, or read one of them, are left uncalculated (dc-10y, `keep`):
//!   one entered since its last result is published as 0, one this engine calculated before keeps
//!   that value, and any other (a value only the file stored, which this engine never calculated)
//!   is published as a typed refusal. Every recalculation calculates them again, as Excel does
//!   (it saves each with `ca="1"`). The other formulas are calculated.
//! - **Iteration on.** The formulas of the order are evaluated in passes, each formula on a cycle
//!   reading the value the other formulas on it had at the end of the last pass, or, once
//!   evaluated in this pass, their new value (the members of a cycle in cell order: Excel
//!   evaluates A1 before B1, A5 before A6 and B5 before C5). After each pass the change of every
//!   formula on a cycle is measured: the recalculation stops after the first pass in which no
//!   value changed by the maximum change or more, or after the maximum number of passes. Excel
//!   16.0 (2026-10-23, 100 iterations, maximum change 0.001) added 200 to A1 `=B1+1` and B1
//!   `=A1+1` and 100 to C1 `=C1+1` on each recalculation, and converged D3 `=E3/2+1` with E3
//!   `=D3` to 2 and F3 `=F3*0.5+5` to 10. Every recalculation calculates the formulas on a cycle
//!   again, as Excel's did each time a formula elsewhere was entered. A formula on a cycle starts
//!   from the value its cell holds, as Excel's iteration does: its last calculated value in this
//!   engine, before that the value the file stored (Excel 16.0.20430, 2026-10-25: A1 `=B1+1` and
//!   B1 `=A1+1` stored as 77 and 55 and opened without calculating read 254 and 255 after one full
//!   recalculation of 100 iterations, from B1's 55), else 0 (dc-10y). The stored value only seeds
//!   the first pass; what is published is what the passes calculate.
//!
//! The waiting formulas, the ready queue and the waves follow OxCalc's recalculation tracker
//! (`src/oxcalc-core/src/recalc.rs`, `Stage1RecalcTracker`: a node reads only published or
//! verified values, and one whose dependency shape changed re-enters the schedule) and its spill
//! invalidation (`src/oxcalc-core/src/grid/machine.rs`, `grid_vacated_spill_extent_dirty_seeds`),
//! at OxCalc commit 1bd514ef, rewritten on this engine's one graph; OxCalc's own reference
//! language and consumer layer are left behind.

use crate::graph::{AxisInterval, Prerequisite};
use crate::random::RandomStream;
use crate::read::{
    NodeStanding, ObservedEvaluation, Progress, ReadObserver, RecalcView, SnapshotCatalog,
    WorkbookReader,
};
use crate::result::{
    CalcTick, Calculation, FormulaResult, NotModeled, SpillChange, ValueChange, ValueDelta,
};
use crate::spill::{self, Spill, SpillIndex, SpillOutcome};
use crate::state::{ArrayRole, Binding, EngineState};
use calc_doc_model::{
    ArrayKind, AuthoredRevision, Axis, CellInput, FormulaText, IterationSettings,
    ModelRoot,
};
use calc_fml::reference::{
    CellRun, FormulaAnchor, ReferenceNotation, ReferenceReader, ReferenceShape,
    template_key_of_chars,
};
use calc_fml::{
    EvalFault, EvalFrame, EvalScratch, EvaluationObserver, EvaluationReport, EvaluationServices,
    ExecutedCall, FormulaForm, FormulaInstance, ResolvedReference, compile_loaded_with, evaluate,
    instantiate, parse_formula,
};
use calc_value::{CalcValue, CellKey, GridArea, NodeId, NumericMode};
use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};
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
    /// column). [`begin_recalc`] no longer gives it: since L3 (dc-m3u) the formulas on a cycle
    /// are refused one by one or calculated iteratively, as the workbook's iteration settings
    /// say, and the others are calculated.
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

/// Values by node, kept at the node's slot (dc-10d9: in place of a BTreeMap from node, which each
/// evaluation of a recalculation inserted into twice). No two live nodes share a slot while a
/// recalculation runs, since a commit cancels it.
#[derive(Debug)]
struct BySlot<T> {
    entries: Vec<Option<(NodeId, T)>>,
}

impl<T> Default for BySlot<T> {
    fn default() -> Self {
        BySlot {
            entries: Vec::new(),
        }
    }
}

impl<T> BySlot<T> {
    fn get(&self, node: NodeId) -> Option<&T> {
        match self.entries.get(node.slot() as usize) {
            Some(Some((held, value))) if *held == node => Some(value),
            _ => None,
        }
    }

    fn reserve_slots(&mut self, slots: usize) {
        if self.entries.len() < slots {
            self.entries.resize_with(slots, || None);
        }
    }

    fn insert(&mut self, node: NodeId, value: T) {
        let slot = node.slot() as usize;
        if slot >= self.entries.len() {
            self.entries.resize_with(slot + 1, || None);
        }
        self.entries[slot] = Some((node, value));
    }

    /// Every entry, in slot order (the order of the node IDs).
    fn into_entries(self) -> impl Iterator<Item = (NodeId, T)> {
        self.entries.into_iter().flatten()
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
    /// Which nodes it calculates and has evaluated, and what it staged.
    progress: Progress,
    /// The active spills of the anchors evaluated, and each evaluated anchor's filled area.
    spills: SpillIndex,
    filled: BySlot<Option<GridArea>>,
    /// Each evaluated node's dynamic prerequisites.
    dynamic: BySlot<Vec<Prerequisite>>,
    /// Nodes to calculate again in the next wave.
    rewave: BTreeSet<NodeId>,
    waves: u32,
    /// The storage one evaluation works in, kept between evaluations ([`EvalLocal`]).
    pub(crate) local: EvalLocal,
    failure: Option<RecalcFailure>,
    /// The workbook's iteration settings.
    iteration: IterationSettings,
    /// The formulas on a cycle.
    cyclic: BTreeSet<NodeId>,
    /// With iteration on, the value each formula on a cycle holds as the recalculation stands:
    /// its value at the end of the last pass, or its new value once evaluated in this pass;
    /// `None` when its result is no value.
    iterates: BTreeMap<NodeId, Option<CalcValue>>,
    /// With iteration on, the values of `iterates` when the current pass began.
    pass_start: BTreeMap<NodeId, Option<CalcValue>>,
    /// How many passes over the order have ended.
    passes: u32,
    /// With iteration off, the formulas a circular reference left uncalculated (dc-10y): those on
    /// a cycle that their evaluations met, and those whose evaluation read one of them.
    kept: BTreeSet<NodeId>,
}

/// The storage one evaluation works in (dc-2m0: each thread of a parallel recalculation has its
/// own; a one-thread recalculation keeps one in its work): the evaluator's scratch, the areas the
/// evaluation read, the reader's gathering storage, and whether it read a formula of `kept`.
#[derive(Debug, Default)]
pub(crate) struct EvalLocal {
    scratch: EvalScratch,
    footprint: Vec<GridArea>,
    read_batch: Vec<CalcValue>,
    kept_read: Cell<bool>,
}

/// What every evaluation of one recalculation reads of its work, and nothing it writes (dc-2m0):
/// the results it staged, its spills, the values of an iterating cycle, the formulas a cycle left
/// uncalculated, the formulas waiting, those on a cycle, and its calculation. Built from the work
/// for each evaluation of a one-thread recalculation, and for each level of a parallel one, when
/// all of it is only read.
pub(crate) struct EvalShared<'a> {
    pub(crate) calculation: &'a Calculation,
    pub(crate) progress: &'a Progress,
    pub(crate) spills: &'a SpillIndex,
    pub(crate) iterates: Option<&'a BTreeMap<NodeId, Option<CalcValue>>>,
    pub(crate) kept: Option<&'a BTreeSet<NodeId>>,
    pub(crate) waits_on: &'a BTreeMap<NodeId, NodeId>,
    pub(crate) cyclic: &'a BTreeSet<NodeId>,
}

impl<'a> EvalShared<'a> {
    /// The view an evaluation's reader reads through, with `local`'s flag for a read of a kept
    /// formula.
    fn view(
        &self,
        state: &'a EngineState,
        inputs: &'a ModelRoot,
        local: &'a EvalLocal,
    ) -> RecalcView<'a> {
        RecalcView {
            inputs,
            state,
            progress: self.progress,
            spills: self.spills,
            iterates: self.iterates,
            kept: self.kept.map(|kept| (kept, &local.kept_read)),
        }
    }
}

/// [`EvalShared`] of `work`, borrowing only the fields it names, so that `work.local` can be
/// borrowed beside it.
macro_rules! shared_of {
    ($work:expr) => {
        EvalShared {
            calculation: &$work.calculation,
            progress: &$work.progress,
            spills: &$work.spills,
            iterates: ($work.iteration.enabled() && !$work.cyclic.is_empty())
                .then_some(&$work.iterates),
            kept: (!$work.iteration.enabled()).then_some(&$work.kept),
            waits_on: &$work.waits_on,
            cyclic: &$work.cyclic,
        }
    };
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
        self.progress.scheduled()
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

    /// Whether the formulas on a cycle are calculated iteratively.
    fn iterating(&self) -> bool {
        self.iteration.enabled() && !self.cyclic.is_empty()
    }

    /// Makes `nodes` formulas on a cycle, each starting from its last calculated value in `state`
    /// (0 when it has none) when iteration is on.
    fn add_cyclic(&mut self, state: &EngineState, nodes: impl IntoIterator<Item = NodeId>) {
        for node in nodes {
            if self.cyclic.insert(node) && self.iteration.enabled() {
                let start = starting_value(state, node);
                self.iterates.insert(node, Some(start.clone()));
                self.pass_start.insert(node, Some(start));
            }
        }
    }
}

/// The value a formula on a cycle starts iterating from: the value the cell holds, as Excel's
/// iteration starts (dc-10y, see the module documentation). That is its last calculated value;
/// before this engine has calculated it, the value the file stored for it; else 0. A formula
/// entered (or entered again) since its last result starts from 0, whatever its cell held before:
/// the result of a formula the cell no longer holds is no seed (Excel 16.0.20430, 2026-10-25,
/// iteration on, at most 1 iteration: C1 `=C1+1` stored as 100, entered as `=C1+2`, gives 2, and
/// E1 `=E1+1` stored as 100 and entered again gives 1, while D1 `=D1+1` gives 101). The stored
/// value only seeds the first pass: what is published is what the passes calculate from it.
fn starting_value(state: &EngineState, node: NodeId) -> CalcValue {
    if state.entered.contains(&node) {
        return CalcValue::number(0.0);
    }
    let at = live_anchor(state, node);
    match state.result(at) {
        Some(FormulaResult::Calculated { value, .. }) => value.clone(),
        Some(FormulaResult::FileCached(cache)) => cache.value().to_value(),
        _ => CalcValue::number(0.0),
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
    let mut unbound: Vec<(NodeId, CellKey, Option<&FormulaText>)> = Vec::new();
    for &node in &state.pending {
        if state.bindings.contains_key(&node) {
            continue;
        }
        let at = live_anchor(state, node);
        match inputs.cell(at) {
            Some(CellInput::Formula(text)) => unbound.push((node, at, Some(text))),
            None if crate::data_table::is_table_cell(inputs, at) => unbound.push((node, at, None)),
            _ => return Err(StartError::ForeignInputs { node, at }),
        }
    }
    for (node, at, text) in unbound {
        match text {
            Some(text) => bind(state, inputs, node, at, text),
            None => bind_table_cell(state, inputs, node, at),
        }
    }
    let iteration = inputs.definitions().iteration();
    let roots = match &state.published_mode {
        Some(published) if *published != mode => state.by_anchor.values().copied().collect(),
        _ => {
            let mut roots = state.pending.clone();
            roots.extend(state.volatile.iter().copied());
            if state.tables_deferred {
                // The table cells the recalculation would reach stay pending, for the
                // recalculation that calculates tables.
                state.tables_deferred = false;
                let reached: Vec<NodeId> = dirty_closure(state, inputs, roots.clone(), &|node| {
                    published_areas(state, inputs, node)
                })
                .members()
                .iter()
                .copied()
                .filter(|node| crate::data_table::is_table_cell(inputs, live_anchor(state, *node)))
                .collect();
                state.tables_deferred = true;
                state.pending.extend(reached);
                roots.retain(|node| {
                    !crate::data_table::is_table_cell(inputs, live_anchor(state, *node))
                });
            }
            // The formulas on a cycle, and with iteration off those that read one, are calculated
            // by every recalculation.
            roots.extend(
                state
                    .cycle_members
                    .iter()
                    .copied()
                    .filter(|node| state.node(*node).is_some()),
            );
            roots
        }
    };
    let mut cache = std::mem::take(&mut state.readers_cache);
    cache.validate(state, inputs);
    let Scheduled { order, cyclic } = schedule(state, inputs, roots, Some(&mut cache), &|node| {
        published_areas(state, inputs, node)
    });
    state.readers_cache = cache;
    let mut progress = Progress::new(state.nodes.len());
    for node in &order {
        progress.schedule(*node);
    }
    // The storage of what the first wave stages, made at once when the wave holds a good share
    // of the nodes (dc-dxm); a small wave grows it as it stages.
    let (mut filled, mut dynamic) = (BySlot::default(), BySlot::default());
    if order.len() * 4 >= state.nodes.len() {
        let slots = state.nodes.len();
        progress.reserve_slots(slots);
        filled.reserve_slots(slots);
        dynamic.reserve_slots(slots);
    }
    state.last_tick = state
        .last_tick
        .checked_add(1)
        .expect("fewer than 2^64 recalculations begin in one engine state");
    Ok(RecalcWork {
        revision,
        generation: state.generation,
        calculation: Calculation::new(revision, CalcTick::new(state.last_tick), mode),
        progress,
        order,
        next: 0,
        ready: VecDeque::new(),
        waiting: BTreeMap::new(),
        waits_on: BTreeMap::new(),
        spills: SpillIndex::default(),
        filled,
        dynamic,
        rewave: BTreeSet::new(),
        waves: 0,
        local: EvalLocal::default(),
        failure: None,
        iteration,
        cyclic: BTreeSet::new(),
        iterates: BTreeMap::new(),
        pass_start: BTreeMap::new(),
        passes: 0,
        kept: BTreeSet::new(),
    })
    .map(|mut work| {
        work.add_cyclic(state, cyclic);
        work
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
    #[cfg(not(target_arch = "wasm32"))]
    if budget == WorkBudget::UNLIMITED && parallel::eligible(state, inputs, work, &*observer) {
        parallel::evaluate_levels(state, inputs, work, &mut *observer);
    }
    let mut evaluations = 0;
    while work.failure.is_none() && evaluations < budget.evaluations {
        let Some(node) = next_node(state, inputs, work) else {
            break;
        };
        evaluations += 1;
        process(state, inputs, work, node, &mut *observer);
    }
    work.progress()
}

/// Evaluates `node` on this thread and stages what it gives: a result, a formula a cycle leaves
/// uncalculated, a wait, or the recalculation's failure.
fn process(
    state: &EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
    node: NodeId,
    observer: &mut dyn ReadObserver,
) {
    let attempt = evaluate_node(state, inputs, work, node, &mut *observer);
    apply(state, inputs, work, node, attempt, observer);
}

/// Stages what an evaluation of `node` gave.
fn apply(
    state: &EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
    node: NodeId,
    attempt: Result<Attempt, RecalcFailure>,
    observer: &mut dyn ReadObserver,
) {
    match attempt {
        Ok(Attempt::Done(done)) => stage(state, inputs, work, node, done),
        Ok(Attempt::Kept) => keep(state, inputs, work, node),
        Ok(Attempt::Waits { on, cell }) => {
            observer.deferred(live_anchor(state, node), cell);
            defer(state, inputs, work, node, on);
        }
        Err(failure) => work.failure = Some(failure),
    }
}

/// Parallel evaluation of a recalculation's first wave on native platforms (dc-2m0).
///
/// The order the recalculation was given (the ready queue's) is cut into levels: a node's level is
/// one more than the highest level of the nodes of the closure it reads, by the readers the
/// ordering found (the readers cache), so no node reads, through an edge of the graph, a node of
/// its own level or a later one. Each level is evaluated on several threads through the one
/// evaluator ([`evaluate_with`]), every evaluation reading only what earlier levels staged
/// ([`EvalShared`], which no thread writes while a level runs) and writing only its own thread's
/// storage ([`EvalLocal`]); then this thread stages the results in the level's order of the
/// recalculation, which is where every write to the recalculation is made (`stage`, as the
/// one-thread route does). An evaluation that did not finish (it read a formula cell this
/// recalculation has yet to calculate, through a reference no edge knew of, or met a formula a
/// cycle left uncalculated) is dropped, and the formula is evaluated again on this thread in its
/// place in that order, with everything staged before it, as the one-thread route would at that
/// point; formulas released from waiting are evaluated before the next level, as the one-thread
/// route takes them before the order's next. A level of fewer formulas than the state's parallel
/// minimum (64 by default) is evaluated on this thread, and so is a first wave of fewer than
/// 1,024.
///
/// Array formulas (dc-dxm): the area an array formula fills or spills into decides what other
/// formulas read there, and which of two competing spills fills its area depends on which is
/// evaluated first. So each array formula of the order (a legacy array filling its area, or a
/// dynamic array that spills) is evaluated on this thread at its own place in the order, and the
/// levels are cut there: the formulas of the order before it, in their levels, are all staged
/// before it, those after it after it, and with them the formulas they released from waiting.
/// Each array formula is so evaluated with exactly the formulas before it in the order staged, as
/// on the one-thread route, and every other formula with every array formula before it staged and
/// none after it. When an array formula has to wait (its spill area holds a formula cell, or a
/// spill whose anchor, not yet evaluated, may move), the rest of the recalculation runs on one
/// thread from the formula after it.
///
/// A recalculation is evaluated this way only when nothing else in it depends on the order of
/// evaluation beyond its values: no circular reference and no iteration, no what-if data table, an
/// observer that observes nothing ([`ReadObserver::is_silent`]: an observer is shown evaluations
/// in order), and every node's readers known. Any other recalculation, and every one on wasm32,
/// runs on one thread. The values published are those of the one-thread route: each formula's
/// value depends only on the values it reads, and every value it reads was staged before its
/// level, or it is evaluated again on this thread.
#[cfg(not(target_arch = "wasm32"))]
mod parallel {
    use super::*;
    use crate::Unobserved;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// How many formulas a thread takes from a level at a time.
    const CHUNK: usize = 32;

    pub(super) fn eligible(
        state: &EngineState,
        inputs: &ModelRoot,
        work: &RecalcWork,
        observer: &dyn ReadObserver,
    ) -> bool {
        state.threads > 1
            && observer.is_silent()
            && work.failure.is_none()
            && work.next == 0
            && work.waves == 0
            && work.ready.is_empty()
            && work.waits_on.is_empty()
            && work.order.len() >= state.parallel_minimum.wave
            && !work.iteration.enabled()
            && work.cyclic.is_empty()
            && work.spills.is_empty()
            && inputs.definitions().data_tables().is_empty()
    }

    /// The positions of `order` grouped by level, or `None` when a node's readers are not known.
    fn levels(state: &EngineState, order: &[NodeId]) -> Option<Levels> {
        const NONE: u32 = u32::MAX;
        let mut position = vec![NONE; state.nodes.len()];
        for (at, node) in order.iter().enumerate() {
            position[node.slot() as usize] = at as u32;
        }
        let mut level = vec![0u32; order.len()];
        for (at, node) in order.iter().enumerate() {
            let readers = state
                .readers_cache
                .get(*node, live_anchor(state, *node))?;
            let next = level[at] + 1;
            for reader in readers {
                let to = position[reader.slot() as usize];
                // A reader the order puts first reads the node through an edge the order left
                // out (an order around a cycle keeps only the edges placed references make): it
                // waits for the node if it reads it before the node is staged.
                if to != NONE && to as usize > at {
                    let held = &mut level[to as usize];
                    *held = (*held).max(next);
                }
            }
        }
        let count = level.iter().max().map_or(0, |max| *max as usize + 1);
        let mut starts = vec![0usize; count + 1];
        for held in &level {
            starts[*held as usize + 1] += 1;
        }
        for at in 1..=count {
            starts[at] += starts[at - 1];
        }
        let mut grouped = vec![0usize; level.len()];
        let mut next = starts.clone();
        for (at, held) in level.iter().enumerate() {
            grouped[next[*held as usize]] = at;
            next[*held as usize] += 1;
        }
        Some(Levels {
            level,
            starts,
            grouped,
        })
    }

    /// Whether `node` is an array formula: a legacy one filling its area, or a dynamic one that
    /// spills.
    fn is_array(state: &EngineState, node: NodeId) -> bool {
        matches!(
            state.bindings.get(&node),
            Some(Binding::Placed(
                _,
                ArrayRole::LegacyFill(_) | ArrayRole::Spill
            ))
        )
    }

    pub(super) fn evaluate_levels(
        state: &EngineState,
        inputs: &ModelRoot,
        work: &mut RecalcWork,
        observer: &mut dyn ReadObserver,
    ) {
        // The levels of the order of every formula are kept with it (dc-dxm).
        let found;
        let kept = &state.readers_cache;
        let levels = if kept.full_order.as_ref() == Some(&work.order) {
            kept.full_levels
                .get_or_init(|| levels(state, &work.order))
                .as_ref()
        } else {
            found = levels(state, &work.order);
            found.as_ref()
        };
        let Some(levels) = levels else {
            return;
        };
        let mut locals: Vec<EvalLocal> =
            (0..state.threads).map(|_| EvalLocal::default()).collect();
        let arrays: Vec<usize> = if inputs.definitions().arrays().next().is_none() {
            Vec::new()
        } else {
            (0..work.order.len())
                .filter(|at| is_array(state, work.order[*at]))
                .collect()
        };
        if arrays.is_empty() {
            for held in 0..levels.starts.len() - 1 {
                let positions = &levels.grouped[levels.starts[held]..levels.starts[held + 1]];
                run_level(state, inputs, work, positions, &mut locals, &mut *observer);
            }
        } else {
            let mut from = 0;
            let mut positions = Vec::new();
            for &array in arrays.iter().chain(std::iter::once(&work.order.len())) {
                // The formulas between two array formulas, by level, each level in the order's
                // order.
                let between = &levels.level[from..array];
                if let (Some(low), Some(high)) = (between.iter().min(), between.iter().max()) {
                    let mut starts = vec![0usize; (high - low) as usize + 2];
                    for held in between {
                        starts[(held - low) as usize + 1] += 1;
                    }
                    for at in 1..starts.len() {
                        starts[at] += starts[at - 1];
                    }
                    positions.clear();
                    positions.resize(between.len(), 0);
                    let mut next = starts.clone();
                    for (offset, held) in between.iter().enumerate() {
                        let bucket = (held - low) as usize;
                        positions[next[bucket]] = from + offset;
                        next[bucket] += 1;
                    }
                    for bucket in 0..starts.len() - 1 {
                        let level = &positions[starts[bucket]..starts[bucket + 1]];
                        run_level(state, inputs, work, level, &mut locals, &mut *observer);
                    }
                }
                if work.failure.is_some() || array == work.order.len() {
                    break;
                }
                let node = work.order[array];
                if !work.progress.is_evaluated(node) && !work.waits_on.contains_key(&node) {
                    process(state, inputs, work, node, &mut *observer);
                    drain(state, inputs, work, &mut *observer);
                }
                if work.waits_on.contains_key(&node) {
                    // The one-thread route goes on from the formula after it.
                    work.next = array + 1;
                    return;
                }
                from = array + 1;
            }
        }
        // Every node of the order was evaluated or waits; the one-thread route takes what is
        // left (the waits, the formulas a cycle left waiting).
        work.next = work.order.len();
    }

    /// Evaluates one level's formulas (`positions` of the order, in its order), on several threads
    /// when there are enough of them, stages each in that order, and then evaluates the formulas
    /// they released from waiting.
    fn run_level(
        state: &EngineState,
        inputs: &ModelRoot,
        work: &mut RecalcWork,
        positions: &[usize],
        locals: &mut [EvalLocal],
        observer: &mut dyn ReadObserver,
    ) {
        if work.failure.is_some() {
            return;
        }
        if positions.len() < state.parallel_minimum.level {
            for &at in positions {
                if work.failure.is_some() {
                    break;
                }
                let node = work.order[at];
                process(state, inputs, work, node, &mut *observer);
            }
        } else {
            let attempts = evaluate_level(state, inputs, work, positions, locals);
            for (at, attempt) in positions.iter().zip(attempts) {
                let attempt = attempt
                    .into_inner()
                    .expect("every formula of the level was evaluated");
                if work.failure.is_some() {
                    break;
                }
                let node = work.order[*at];
                match attempt {
                    Ok(Attempt::Done(done)) => stage(state, inputs, work, node, done),
                    // Evaluated again here, in its place in the order.
                    _ => process(state, inputs, work, node, &mut *observer),
                }
            }
        }
        drain(state, inputs, work, observer);
    }

    /// Evaluates the formulas released from waiting, as the one-thread route takes them before
    /// the order's next.
    fn drain(
        state: &EngineState,
        inputs: &ModelRoot,
        work: &mut RecalcWork,
        observer: &mut dyn ReadObserver,
    ) {
        while work.failure.is_none()
            && let Some(node) = work.ready.pop_front()
        {
            process(state, inputs, work, node, &mut *observer);
        }
    }

    /// The attempts of a level's formulas, in `positions`' order, evaluated on `locals.len()`
    /// threads against what earlier levels staged: each thread sets the attempts of the formulas
    /// it takes, at their places.
    fn evaluate_level(
        state: &EngineState,
        inputs: &ModelRoot,
        work: &RecalcWork,
        positions: &[usize],
        locals: &mut [EvalLocal],
    ) -> Vec<OnceLock<Result<Attempt, RecalcFailure>>> {
        let shared = shared_of!(work);
        let order = &work.order;
        let cursor = AtomicUsize::new(0);
        let attempts: Vec<OnceLock<Result<Attempt, RecalcFailure>>> =
            positions.iter().map(|_| OnceLock::new()).collect();
        std::thread::scope(|scope| {
            let worker = |local: &mut EvalLocal| {
                let mut silent = Unobserved;
                loop {
                    let from = cursor.fetch_add(CHUNK, Ordering::Relaxed);
                    if from >= positions.len() {
                        break;
                    }
                    for index in from..(from + CHUNK).min(positions.len()) {
                        let node = order[positions[index]];
                        let attempt =
                            evaluate_with(state, inputs, &shared, local, node, &mut silent);
                        if attempts[index].set(attempt).is_err() {
                            unreachable!("each formula of a level is taken once");
                        }
                    }
                }
            };
            let (own, others) = locals.split_first_mut().expect("at least one thread");
            for local in others {
                scope.spawn(move || worker(local));
            }
            worker(own);
        });
        attempts
    }
}

/// Publishes a completed recalculation: each evaluated node's staged result replaces its published
/// one, with its spill and its dynamic prerequisites; the results and spills of formulas removed
/// since the last publication are removed, the evaluated nodes are no longer pending, and the
/// published generation's numeric mode is the recalculation's. Returns what changed. A
/// recalculation that failed, is incomplete, or was superseded publishes nothing.
pub fn finish(state: &mut EngineState, mut work: RecalcWork) -> Result<ValueDelta, RecalcFailure> {
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
    // Each change is of a distinct cell: a published node's anchor, or a retired anchor whose
    // published result no live node there replaced. They are sorted by cell at the end.
    let mut changes: Vec<(CellKey, ValueChange)> = Vec::with_capacity(work.progress.scheduled());
    let mut spill_changes = Vec::new();
    let mut published = vec![false; state.nodes.len()];
    for (node, result) in std::mem::take(&mut work.progress).into_staged() {
        published[node.slot() as usize] = true;
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
        state.results.publish(node, at, result);
        changes.push((at, ValueChange::Published { at, node }));
    }
    // The published nodes are no longer pending, nor entered since the last publication: each set
    // is built anew from the nodes that stay, in node order, which builds it in one pass (dc-dxm),
    // rather than by removing the published nodes one at a time.
    let stays = |node: &NodeId| !published.get(node.slot() as usize).copied().unwrap_or(false);
    state.pending = std::mem::take(&mut state.pending)
        .into_iter()
        .filter(stays)
        .collect();
    state.entered = std::mem::take(&mut state.entered)
        .into_iter()
        .filter(stays)
        .collect();
    for (node, dynamic) in std::mem::take(&mut work.dynamic).into_entries() {
        state.set_dynamic(node, dynamic);
    }
    for at in std::mem::take(&mut state.retired) {
        if let Some((node, _)) = state.results.take_orphan(at) {
            if let Some(spill) = state.spills.remove(at) {
                spill_changes.push(SpillChange {
                    anchor: at,
                    before: Some(spill.area()),
                    after: None,
                });
            }
            changes.push((at, ValueChange::Removed { at, node }));
        }
    }
    if !spill_changes.is_empty() {
        state.spill_epoch += 1;
    }
    state.cycle_members = work.cyclic;
    state.generation += 1;
    state.published_mode = Some(work.calculation.mode().clone());
    spill_changes.sort_by_key(|change| change.anchor);
    changes.sort_by_key(|(at, _)| *at);
    debug_assert!(
        changes.windows(2).all(|pair| pair[0].0 != pair[1].0),
        "one change per cell"
    );
    Ok(ValueDelta::new(
        work.revision,
        work.calculation.tick(),
        changes.into_iter().map(|(_, change)| change).collect(),
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
    // The stored UTF-16 text decoded into characters once (dc-dxm), for the template and, when
    // the template is new, the compilation; an unpaired surrogate refuses it.
    let decoded: Result<Vec<char>, _> =
        char::decode_utf16(text.text().utf16_code_units().iter().copied()).collect();
    let binding = match decoded {
        Err(_) => Binding::Refused(NotModeled::FormulaText),
        Ok(chars) => {
            let key = template_key_of_chars(&chars, anchor).map(|key| (at.sheet(), form, key));
            match key.as_ref().and_then(|key| state.templates.get(key)) {
                Some(plan) => Binding::Placed(instantiate(plan, anchor), role),
                None => match compile_loaded_with(
                    &parse_formula(&chars.iter().collect::<String>()),
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
        Binding::Refused(_) | Binding::DataTable(_) => Vec::new(),
    };
    state
        .set_prerequisites(inputs, node, prerequisites)
        .expect("the snapshot is at the state's revision, the node is live, and every sheet came from the snapshot");
    state.bindings.insert(node, binding);
}

/// Binds the cell `at` of a what-if data table: it reads its formula cell, the cells its row and
/// column values stand in, and its table's input cells.
fn bind_table_cell(state: &mut EngineState, inputs: &ModelRoot, node: NodeId, at: CellKey) {
    let table = *inputs
        .definitions()
        .data_table_containing(at)
        .expect("a table cell lies in a table");
    let mut prerequisites = Vec::new();
    if let Some((formula, pairs)) = table.what_if(at) {
        let mut cells = vec![formula];
        for (input, from) in pairs {
            cells.push(input);
            cells.push(from);
        }
        for cell in cells {
            let prerequisite = Prerequisite::Cell(cell);
            if !prerequisites.contains(&prerequisite) {
                prerequisites.push(prerequisite);
            }
        }
    }
    state
        .set_prerequisites(inputs, node, prerequisites)
        .expect("the snapshot is at the state's revision and the node is live");
    state.bindings.insert(node, Binding::DataTable(table));
}

/// What a placed formula reads: one prerequisite per distinct placed reference.
fn prerequisites(instance: &FormulaInstance) -> Vec<Prerequisite> {
    let mut out: Vec<Prerequisite> = Vec::with_capacity(instance.areas().len());
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

/// Puts into `out`, in ID order and each once, the nodes that read the anchor of `node` or a cell
/// of one of `areas`. A formula that reads its own anchor is among them, a cycle; one that reads
/// only the cells its own result fills is not (its spill's own dynamic prerequisites are those
/// cells), and an evaluation that reads them finds the circular reference itself.
fn readers_into(
    state: &EngineState,
    inputs: &ModelRoot,
    node: NodeId,
    areas: &[GridArea],
    out: &mut Vec<NodeId>,
) {
    out.clear();
    for area in areas {
        state.index.dependents_of_area(inputs, *area, out);
    }
    out.retain(|reader| *reader != node);
    state
        .index
        .dependents_of_cell(inputs, live_anchor(state, node), out);
    if state.tables_deferred {
        out.retain(|reader| !crate::data_table::is_table_cell(inputs, live_anchor(state, *reader)));
    }
    out.sort_unstable();
    out.dedup();
}

/// The dirty closure of a set of roots: its members, and each member's readers, kept by the
/// member's slot (dc-10d9: in place of a map from node to readers, which ordering a closure of
/// every formula built anew for every recalculation).
pub(crate) struct Closure {
    /// The members, in ID order.
    members: Vec<NodeId>,
    /// Where each member's readers lie in `edges` (start and count), at the member's slot; no
    /// readers at any other slot.
    readers: Vec<(u32, u32)>,
    /// Every member's readers, each member's run in ID order, one after the other.
    edges: Vec<NodeId>,
}

impl Closure {
    /// The members, in ID order.
    pub(crate) fn members(&self) -> &[NodeId] {
        &self.members
    }

    /// The readers of the member `node`, in ID order.
    fn readers(&self, node: NodeId) -> &[NodeId] {
        let (start, count) = self.readers[node.slot() as usize];
        &self.edges[start as usize..(start + count) as usize]
    }

    /// The members with their readers, as a map.
    fn to_map(&self) -> BTreeMap<NodeId, Vec<NodeId>> {
        self.members
            .iter()
            .map(|node| (*node, self.readers(*node).to_vec()))
            .collect()
    }
}

/// The dirty closure of `roots`: the roots and every node that reads the anchor of one, or a cell
/// of one of the areas `areas` gives for it, directly or through other nodes, each with the
/// nodes that read it. `inputs` is the snapshot at the state's revision.
pub(crate) fn dirty_closure(
    state: &EngineState,
    inputs: &ModelRoot,
    roots: BTreeSet<NodeId>,
    areas: &dyn Fn(NodeId) -> Vec<GridArea>,
) -> Closure {
    closure_with(state, inputs, roots, None, areas)
}

/// What the readers of every node depend on beside the node itself: the graph's edges, the
/// published spills' areas and the nodes' anchors, the snapshot's array formulas (the
/// definitions, shared unchanged from one revision to the next until an edit changes them), the
/// sheet order (a span's sheets) and whether data tables wait.
#[derive(Debug, Clone)]
struct ReadersKey {
    index_epoch: u64,
    spill_epoch: u64,
    /// Counts the nodes added and removed (dc-dxm): a node with no edges (a formula of
    /// constants) changes no edge.
    anchor_epoch: u64,
    definitions: Arc<calc_doc_model::Definitions>,
    sheets: Vec<calc_value::SheetId>,
    tables_deferred: bool,
}

impl PartialEq for ReadersKey {
    fn eq(&self, other: &Self) -> bool {
        self.index_epoch == other.index_epoch
            && self.spill_epoch == other.spill_epoch
            && self.anchor_epoch == other.anchor_epoch
            && Arc::ptr_eq(&self.definitions, &other.definitions)
            && self.sheets == other.sheets
            && self.tables_deferred == other.tables_deferred
    }
}

/// Each node's readers, as [`readers_into`] found them with the published areas of the node, by
/// the node's slot, with the node and its anchor (dc-10d9). Ordering a closure of every formula
/// asked the reverse index for every formula's readers anew each recalculation; while the key
/// (everything the readers depend on beside the node: [`ReadersKey`]) is unchanged, the readers
/// found before stand, and a recalculation after a change of literal inputs, or a full one,
/// reads them from here. Any change of the key empties it. A debug build checks every reader list
/// it hands out against the reverse index.
#[derive(Debug, Default)]
pub(crate) struct ReadersCache {
    key: Option<ReadersKey>,
    entries: Vec<Option<(NodeId, CellKey, Box<[NodeId]>)>>,
    /// Each live node's place in cell order (sheet ID, then row, then column), by slot: the ready
    /// queue orders by it (dc-dxm: a u32 compares faster than a cell, and the order is the same).
    ranks: Vec<u32>,
    /// The order of the last recalculation of every formula, when it found no cycle (dc-dxm):
    /// with the key unchanged, the next one of every formula has the same closure and the same
    /// order.
    full_order: Option<Vec<NodeId>>,
    /// The levels of `full_order` (dc-dxm), found by the first parallel recalculation of every
    /// formula that needs them, and kept with the order.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    full_levels: std::sync::OnceLock<Option<Levels>>,
}

/// The positions of an order grouped by level (dc-2m0): a node's level is one more than the
/// highest level of the nodes of the order it reads. `grouped[starts[l]..starts[l + 1]]` are the
/// positions of level `l`, in the order's order.
#[derive(Debug)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct Levels {
    /// Each position's level.
    level: Vec<u32>,
    starts: Vec<usize>,
    grouped: Vec<usize>,
}

impl ReadersCache {
    /// Empties the cache unless it was filled under what `state` and `inputs` now hold.
    pub(crate) fn validate(&mut self, state: &EngineState, inputs: &ModelRoot) {
        let key = ReadersKey {
            index_epoch: state.index.epoch(),
            spill_epoch: state.spill_epoch,
            definitions: Arc::clone(inputs.definitions()),
            sheets: inputs.sheets().iter().map(|sheet| sheet.id()).collect(),
            tables_deferred: state.tables_deferred,
            anchor_epoch: state.anchor_epoch,
        };
        if self.key.as_ref() != Some(&key) {
            self.entries.clear();
            self.full_order = None;
            self.full_levels = std::sync::OnceLock::new();
            self.ranks.clear();
            self.ranks.resize(state.nodes.len(), u32::MAX);
            for (rank, node) in state.by_anchor.values().enumerate() {
                self.ranks[node.slot() as usize] =
                    u32::try_from(rank).expect("fewer than 2^32 formula nodes");
            }
            self.key = Some(key);
        }
    }

    /// The place of the live node `node` in cell order.
    fn rank(&self, node: NodeId) -> u32 {
        self.ranks[node.slot() as usize]
    }

    fn get(&self, node: NodeId, anchor: CellKey) -> Option<&[NodeId]> {
        match self.entries.get(node.slot() as usize) {
            Some(Some((held, at, readers))) if *held == node && *at == anchor => Some(readers),
            _ => None,
        }
    }

    fn insert(&mut self, node: NodeId, anchor: CellKey, readers: &[NodeId]) {
        let slot = node.slot() as usize;
        if slot >= self.entries.len() {
            self.entries.resize_with(slot + 1, || None);
        }
        self.entries[slot] = Some((node, anchor, readers.into()));
    }
}

/// [`dirty_closure`], reading and filling `cache` when one is given, which only the ordering of a
/// recalculation's first wave does, with the published areas as `areas`.
fn closure_with(
    state: &EngineState,
    inputs: &ModelRoot,
    roots: BTreeSet<NodeId>,
    mut cache: Option<&mut ReadersCache>,
    areas: &dyn Fn(NodeId) -> Vec<GridArea>,
) -> Closure {
    let slots = state.nodes.len();
    let mut member = vec![false; slots];
    let mut readers: Vec<(u32, u32)> = vec![(0, 0); slots];
    let mut edges: Vec<NodeId> = Vec::new();
    let mut members: Vec<NodeId> = Vec::with_capacity(roots.len());
    for node in &roots {
        member[node.slot() as usize] = true;
        members.push(*node);
    }
    let mut stack = members.clone();
    let mut found = Vec::new();
    while let Some(node) = stack.pop() {
        let anchor = live_anchor(state, node);
        match cache.as_deref().and_then(|cache| cache.get(node, anchor)) {
            Some(cached) => {
                found.clear();
                found.extend_from_slice(cached);
                #[cfg(debug_assertions)]
                {
                    let mut fresh = Vec::new();
                    readers_into(state, inputs, node, &areas(node), &mut fresh);
                    debug_assert_eq!(found, fresh, "the cached readers of {node:?} stand");
                }
            }
            None => {
                readers_into(state, inputs, node, &areas(node), &mut found);
                if let Some(cache) = cache.as_deref_mut() {
                    cache.insert(node, anchor, &found);
                }
            }
        }
        for &reader in &found {
            let slot = reader.slot() as usize;
            if !member[slot] {
                member[slot] = true;
                members.push(reader);
                stack.push(reader);
            }
        }
        let start = u32::try_from(edges.len()).expect("fewer than 2^32 edges in a closure");
        let count = u32::try_from(found.len()).expect("fewer than 2^32 readers of a node");
        readers[node.slot() as usize] = (start, count);
        edges.extend_from_slice(&found);
    }
    members.sort_unstable();
    Closure {
        members,
        readers,
        edges,
    }
}

/// The order a recalculation evaluates its nodes in, and the nodes on a cycle.
struct Scheduled {
    order: Vec<NodeId>,
    cyclic: BTreeSet<NodeId>,
}

/// The dirty closure of `roots`, in the order of the ready queue; when the dynamic prerequisites
/// close a cycle, in the order of the placed references alone; and when the placed references
/// close one too, the strongly connected components in the order of the ready queue, each one's
/// members in cell order, with the members of each cycle.
fn schedule(
    state: &EngineState,
    inputs: &ModelRoot,
    roots: BTreeSet<NodeId>,
    cache: Option<&mut ReadersCache>,
    areas: &dyn Fn(NodeId) -> Vec<GridArea>,
) -> Scheduled {
    let acyclic = |order| Scheduled {
        order,
        cyclic: BTreeSet::new(),
    };
    let every = roots.len() == state.node_count();
    let (closure, ordered) = match cache {
        Some(cache) => {
            if every && let Some(kept) = &cache.full_order {
                #[cfg(debug_assertions)]
                {
                    let closure = dirty_closure(state, inputs, roots.clone(), areas);
                    let fresh = order(&closure, |node| live_anchor(state, node), |node| {
                        live_anchor(state, node)
                    });
                    debug_assert_eq!(fresh.as_ref(), Ok(kept), "the kept order stands");
                }
                return acyclic(kept.clone());
            }
            let closure = closure_with(state, inputs, roots, Some(&mut *cache), areas);
            let ordered = order(&closure, |node| cache.rank(node), |node| {
                live_anchor(state, node)
            });
            if every && let Ok(kept) = &ordered {
                cache.full_order = Some(kept.clone());
                cache.full_levels = std::sync::OnceLock::new();
            }
            (closure, ordered)
        }
        None => {
            let closure = dirty_closure(state, inputs, roots, areas);
            let ordered = order(&closure, |node| live_anchor(state, node), |node| {
                live_anchor(state, node)
            });
            (closure, ordered)
        }
    };
    match ordered {
        Ok(order) => acyclic(order),
        Err(_) => {
            let readers_of = closure.to_map();
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
            match order_map(state, &placed) {
                Ok(order) => acyclic(order),
                Err(_) => order_with_cycles(state, &placed),
            }
        }
    }
}

/// The nodes of `readers_of` with its cycles: each strongly connected component (found by an
/// iterative Tarjan, ported from OxCalc's `cycle_groups`) in the order of the ready queue over
/// the components, a component ready once every component it reads is ordered and the one whose
/// first anchor comes first ordered next, its members in cell order; and the members of the
/// components that are cycles (more than one node, or a node that reads itself).
fn order_with_cycles(
    state: &EngineState,
    readers_of: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Scheduled {
    let nodes: Vec<NodeId> = readers_of.keys().copied().collect();
    let index_of: BTreeMap<NodeId, usize> =
        nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let edges: Vec<Vec<usize>> = nodes
        .iter()
        .map(|node| readers_of[node].iter().map(|r| index_of[r]).collect())
        .collect();
    let component = tarjan(&edges);
    let count = component.iter().copied().max().map_or(0, |max| max + 1);
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (index, comp) in component.iter().enumerate() {
        members[*comp].push(index);
    }
    let mut cyclic = BTreeSet::new();
    for held in &members {
        let self_loop = held.len() == 1 && edges[held[0]].contains(&held[0]);
        if held.len() > 1 || self_loop {
            cyclic.extend(held.iter().map(|index| nodes[*index]));
        }
    }
    let mut readers: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); count];
    let mut waiting = vec![0usize; count];
    for (from, targets) in edges.iter().enumerate() {
        for to in targets {
            let (a, b) = (component[from], component[*to]);
            if a != b && readers[a].insert(b) {
                waiting[b] += 1;
            }
        }
    }
    let first_anchor = |comp: usize| {
        members[comp]
            .iter()
            .map(|index| live_anchor(state, nodes[*index]))
            .min()
            .expect("a component has a member")
    };
    let mut ready: BTreeSet<(CellKey, usize)> = (0..count)
        .filter(|comp| waiting[*comp] == 0)
        .map(|comp| (first_anchor(comp), comp))
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some((_, comp)) = ready.pop_first() {
        let mut held: Vec<NodeId> = members[comp].iter().map(|index| nodes[*index]).collect();
        held.sort_by_key(|node| live_anchor(state, *node));
        order.extend(held);
        for reader in &readers[comp] {
            waiting[*reader] -= 1;
            if waiting[*reader] == 0 {
                ready.insert((first_anchor(*reader), *reader));
            }
        }
    }
    debug_assert_eq!(order.len(), nodes.len(), "the components form no cycle");
    Scheduled { order, cyclic }
}

/// The strongly connected component of each vertex of `edges` (vertex `i` reaches each of
/// `edges[i]`), numbered from 0, by Tarjan's algorithm on an explicit stack.
fn tarjan(edges: &[Vec<usize>]) -> Vec<usize> {
    const UNSEEN: usize = usize::MAX;
    let n = edges.len();
    let mut index = vec![UNSEEN; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut component = vec![UNSEEN; n];
    let mut next_index = 0;
    let mut next_component = 0;
    for root in 0..n {
        if index[root] != UNSEEN {
            continue;
        }
        // Each frame: a vertex and the position of its next edge.
        let mut frames: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (vertex, ref mut edge)) = frames.last_mut() {
            if let Some(&to) = edges[vertex].get(*edge) {
                *edge += 1;
                if index[to] == UNSEEN {
                    index[to] = next_index;
                    low[to] = next_index;
                    next_index += 1;
                    stack.push(to);
                    on_stack[to] = true;
                    frames.push((to, 0));
                } else if on_stack[to] {
                    low[vertex] = low[vertex].min(index[to]);
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                low[parent] = low[parent].min(low[vertex]);
            }
            if low[vertex] == index[vertex] {
                loop {
                    let member = stack.pop().expect("the component's vertices are stacked");
                    on_stack[member] = false;
                    component[member] = next_component;
                    if member == vertex {
                        break;
                    }
                }
                next_component += 1;
            }
        }
    }
    component
}

/// The members of `closure` in the order of the ready queue (a node is ready once every member
/// it reads is ordered, and of the ready nodes the one whose anchor comes first is ordered next),
/// or the anchors of those a cycle leaves unordered, in cell order. The counts are kept by slot
/// and the ready queue is a heap (dc-10d9); the order is the one a map and an ordered set gave.
fn order<K: Ord + Copy>(
    closure: &Closure,
    key: impl Fn(NodeId) -> K,
    anchor: impl Fn(NodeId) -> CellKey,
) -> Result<Vec<NodeId>, Vec<CellKey>> {
    let mut waiting = vec![0u32; closure.readers.len()];
    for node in closure.members() {
        for reader in closure.readers(*node) {
            waiting[reader.slot() as usize] += 1;
        }
    }
    let mut ready: BinaryHeap<Reverse<(K, NodeId)>> = closure
        .members()
        .iter()
        .filter(|node| waiting[node.slot() as usize] == 0)
        .map(|node| Reverse((key(*node), *node)))
        .collect();
    let mut order = Vec::with_capacity(closure.members().len());
    while let Some(Reverse((_, node))) = ready.pop() {
        order.push(node);
        for &reader in closure.readers(node) {
            let count = &mut waiting[reader.slot() as usize];
            *count -= 1;
            if *count == 0 {
                ready.push(Reverse((key(reader), reader)));
            }
        }
    }
    if order.len() < closure.members().len() {
        let mut cells: Vec<CellKey> = closure
            .members()
            .iter()
            .filter(|node| waiting[node.slot() as usize] > 0)
            .map(|node| anchor(*node))
            .collect();
        cells.sort();
        return Err(cells);
    }
    Ok(order)
}

/// The nodes of `readers_of` in the order of the ready queue, or the anchors of those a cycle
/// leaves unordered: [`order`] over a closure given as a map, as the placed references alone give
/// it when the dynamic prerequisites close a cycle.
fn order_map(
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
            if work.progress.is_evaluated(node) || work.waits_on.contains_key(&node) {
                continue;
            }
            return Some(node);
        }
        if !work.waits_on.is_empty() {
            // Each waits on a formula that waits too: a cycle only the evaluations found.
            let stuck: Vec<NodeId> = work.waits_on.keys().copied().collect();
            work.waits_on.clear();
            work.waiting.clear();
            if work.iteration.enabled() {
                let mut stuck = stuck;
                stuck.sort_by_key(|member| live_anchor(state, *member));
                work.add_cyclic(state, stuck.iter().copied());
                work.ready.extend(stuck);
            } else {
                for node in stuck {
                    keep(state, inputs, work, node);
                }
            }
            continue;
        }
        if work.iterating() && next_pass(work) {
            continue;
        }
        if !work.rewave.is_empty() {
            next_wave(state, inputs, work);
            continue;
        }
        return None;
    }
}

/// Ends a pass of an iterating recalculation: when it changed the value of a formula on a cycle
/// by the maximum change or more and fewer passes than the maximum have run, begins the next
/// pass, every node of the order no longer evaluated (its staged result stands until it is
/// evaluated again), and returns true.
fn next_pass(work: &mut RecalcWork) -> bool {
    work.passes += 1;
    let change = work
        .cyclic
        .iter()
        .map(|node| {
            let before = work.pass_start.get(node).cloned().flatten();
            let after = work.iterates.get(node).cloned().flatten();
            change_between(before.as_ref(), after.as_ref())
        })
        .fold(0.0f64, f64::max);
    if work.passes >= work.iteration.max_iterations() || change < work.iteration.max_change() {
        return false;
    }
    work.pass_start = work.iterates.clone();
    for node in &work.order {
        work.progress.schedule(*node);
    }
    work.next = 0;
    true
}

/// How much a formula's value changed in a pass: the difference of two numbers, nothing for two
/// equal values, and without bound for any other change.
fn change_between(before: Option<&CalcValue>, after: Option<&CalcValue>) -> f64 {
    match (before, after) {
        (Some(a), Some(b)) => match (a.as_number(), b.as_number()) {
            (Some(x), Some(y)) => (x - y).abs(),
            _ if a == b => 0.0,
            _ => f64::INFINITY,
        },
        (None, None) => 0.0,
        _ => f64::INFINITY,
    }
}

/// Begins the next wave: the formulas to calculate again and their dirty closure, ordered, each
/// no longer evaluated (its staged result and spill stand until it is evaluated again).
fn next_wave(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork) {
    let roots = std::mem::take(&mut work.rewave);
    work.waves += 1;
    let areas = |node: NodeId| -> Vec<GridArea> {
        match work.filled.get(node) {
            Some(filled) => filled.iter().copied().collect(),
            None => published_areas(state, inputs, node),
        }
    };
    let ordered = if work.waves > MAX_WAVES {
        Err(())
    } else {
        Ok(schedule(state, inputs, roots.clone(), None, &areas))
    };
    match ordered {
        Ok(Scheduled { order, cyclic }) => {
            work.add_cyclic(state, cyclic);
            for node in &order {
                let evaluated = work.progress.is_evaluated(*node);
                work.progress.schedule(*node);
                if evaluated {
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
                owner: "dc-10y",
            });
            for node in roots {
                work.progress.schedule(node);
                let done = refused(work, refusal.clone());
                stage(state, inputs, work, node, done);
            }
            work.rewave.clear();
        }
    }
}

/// Leaves `node` uncalculated, as Excel leaves a formula on a circular reference, or one that reads
/// such a formula, while iterative calculation is off (dc-10y; see the module documentation): it
/// joins the formulas every recalculation calculates again, and its cell keeps what it holds. A
/// formula entered since its last result shows 0, published as this recalculation's; one this
/// engine calculated before keeps that result, with the calculation that gave it; any other (a
/// value the file stored, which this engine never calculated, a refusal, a result that spilled)
/// is refused as not modelled.
fn keep(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork, node: NodeId) {
    work.kept.insert(node);
    work.add_cyclic(state, [node]);
    let at = live_anchor(state, node);
    let held = if state.entered.contains(&node) {
        Some(FormulaResult::Calculated {
            value: CalcValue::number(0.0),
            calculation: work.calculation.clone(),
        })
    } else if state.spills.get(at).is_some() {
        None
    } else {
        match state.result(at) {
            Some(result @ FormulaResult::Calculated { .. }) => Some(result.clone()),
            _ => None,
        }
    };
    let done = match held {
        Some(result) => Done {
            result,
            spill: None,
            dynamic: Vec::new(),
        },
        None => refused(work, circular()),
    };
    stage(state, inputs, work, node, done);
}

/// The refusal of a formula on a circular reference while iterative calculation is off: Excel
/// leaves each such cell at its previous value, which this engine did not calculate.
fn circular() -> NotModeled {
    NotModeled::Evaluation(EvalFault::NotModelled {
        construct: "a circular reference with iterative calculation off (Excel keeps each cell's previous value)",
        owner: "dc-10y",
    })
}

/// A staged refusal, with no spill and no dynamic prerequisite.
fn refused(work: &RecalcWork, refusal: NotModeled) -> Done {
    refused_by(&work.calculation, refusal)
}

/// A staged refusal by `calculation`, with no spill and no dynamic prerequisite.
fn refused_by(calculation: &Calculation, refusal: NotModeled) -> Done {
    Done {
        result: FormulaResult::NotModeled {
            refusal,
            calculation: calculation.clone(),
        },
        spill: None,
        dynamic: Vec::new(),
    }
}

/// Makes `node` wait for `on`, or, when `on` waits, directly or through others, for `node`, a
/// cycle only the evaluations found: with iteration on its formulas join the formulas on a cycle
/// and are evaluated again at once, and with iteration off `node` is staged as not modelled.
fn defer(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork, node: NodeId, on: NodeId) {
    let mut cursor = on;
    loop {
        if cursor == node {
            let mut chain = vec![node];
            let mut link = on;
            while link != node {
                chain.push(link);
                link = match work.waits_on.get(&link) {
                    Some(next) => *next,
                    None => break,
                };
            }
            for member in &chain {
                if let Some(awaited) = work.waits_on.remove(member)
                    && let Some(waiters) = work.waiting.get_mut(&awaited)
                {
                    waiters.retain(|waiter| waiter != member);
                }
            }
            chain.sort_by_key(|member| live_anchor(state, *member));
            if work.iteration.enabled() {
                // Evaluated again in cell order, as the members of a cycle the order found are.
                work.add_cyclic(state, chain.iter().copied());
                work.ready.extend(chain);
            } else {
                // With iteration off no formula of the cycle is calculated (dc-10y).
                for member in chain {
                    keep(state, inputs, work, member);
                }
            }
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
    let before = match work.filled.get(node) {
        Some(filled) => *filled,
        None => match state.spills.get(at) {
            Some(spill) => Some(spill.area()),
            // The file's area of an array formula whose result is not calculated (the array is
            // looked up first: most formulas are not array formulas, and need no result read).
            None => inputs
                .definitions()
                .array(at)
                .map(|array| array.area())
                .filter(|area| area.cell_count() > 1)
                .filter(|_| {
                    !matches!(
                        state.result(at),
                        Some(FormulaResult::Calculated { .. } | FormulaResult::NotModeled { .. })
                    )
                }),
        },
    };
    let after = done.spill.as_ref().map(Spill::area);
    work.spills.remove(at);
    if let Some(spill) = done.spill {
        work.spills.insert(spill);
    }
    work.filled.insert(node, after);
    if work.iteration.enabled() && work.cyclic.contains(&node) {
        let value = done.result.calculated().map(|(value, _)| value.clone());
        work.iterates.insert(node, value);
    }
    work.progress.stage(node, done.result);
    work.dynamic.insert(node, done.dynamic);
    if before != after {
        let mut readers = BTreeSet::new();
        for area in before.iter().chain(after.iter()) {
            state.index.dependents_of_area(inputs, *area, &mut readers);
        }
        readers.remove(&node);
        for reader in readers {
            if work.progress.is_evaluated(reader) || !work.progress.is_scheduled(reader) {
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
    /// Whether the observer observes nothing ([`ReadObserver::is_silent`]): each run then goes
    /// to the evaluation as the engine's reader gives it, without the observer being shown it
    /// (dc-dxm), which is what showing it would come to.
    silent: bool,
    formula: CellKey,
    cells: &'l mut dyn ReferenceReader,
    footprint: &'l RefCell<Vec<GridArea>>,
    /// The area of the read the observer stopped.
    stopped: Option<GridArea>,
}

impl ReferenceReader for Observed<'_, '_> {
    fn read_cell(
        &mut self,
        at: CellKey,
    ) -> Result<calc_fml::reference::CellRead, calc_fml::reference::ReadContractError> {
        if self.silent && self.stopped.is_none() {
            self.footprint.borrow_mut().push(GridArea::cell(at));
            return self.cells.read_cell(at);
        }
        calc_fml::reference::read_cell_by_area(self, at)
    }

    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        let total = area.cell_count();
        if self.stopped.is_some() {
            let _ = sink(CellRun::Missing(total));
            return;
        }
        self.footprint.borrow_mut().push(area);
        if self.silent {
            self.cells.read_area(area, sink);
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

    fn spill_extent(&mut self, anchor: CellKey) -> calc_fml::reference::SpillExtent {
        self.cells.spill_extent(anchor)
    }

    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<calc_fml::SubtotalExclusions> {
        self.cells.subtotal_exclusions(area)
    }

    fn environment(&self) -> Option<calc_fml::CalcEnvironment> {
        self.cells.environment()
    }

    fn host_facts(&mut self) -> Option<&mut dyn calc_fml::HostFacts> {
        Some(self)
    }
}

/// The engine reader's host facts, with the formula being evaluated as the caller.
impl calc_fml::HostFacts for Observed<'_, '_> {
    fn caller(&mut self) -> Option<CellKey> {
        Some(self.formula)
    }

    fn is_formula(&mut self, cell: CellKey) -> Option<bool> {
        self.cells.host_facts()?.is_formula(cell)
    }

    fn formula_text(&mut self, cell: CellKey) -> Option<Option<String>> {
        self.cells.host_facts()?.formula_text(cell)
    }

    fn sheet_index(&mut self, sheet: calc_value::SheetId) -> Option<u32> {
        self.cells.host_facts()?.sheet_index(sheet)
    }

    fn sheet_named(&mut self, name: &str) -> Option<calc_value::SheetId> {
        self.cells.host_facts()?.sheet_named(name)
    }

    fn sheet_name(&mut self, sheet: calc_value::SheetId) -> Option<String> {
        self.cells.host_facts()?.sheet_name(sheet)
    }

    fn sheet_count(&mut self) -> Option<u32> {
        self.cells.host_facts()?.sheet_count()
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
    /// Whether the observer observes nothing: it is then not shown calls or outcomes.
    silent: bool,
    formula: CellKey,
    calculation: &'l Calculation,
    footprint: &'l RefCell<Vec<GridArea>>,
    uncomputed: &'l Cell<Option<(CellKey, NodeId)>>,
    kept_read: &'l Cell<bool>,
}

impl EvaluationObserver for Instrument<'_, '_> {
    fn called(&mut self, call: ExecutedCall) {
        if !self.silent {
            self.observer.borrow_mut().called(self.formula, call);
        }
    }

    fn resolved(&mut self, reference: ResolvedReference) {
        self.footprint.borrow_mut().push(reference.area());
        if !self.silent {
            self.observer.borrow_mut().resolved(self.formula, reference);
        }
    }

    fn returned(&mut self, report: EvaluationReport<'_>) {
        if self.silent || self.uncomputed.get().is_some() || self.kept_read.get() {
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
    /// With iteration off, the evaluation read a formula a circular reference left uncalculated:
    /// it is discarded, and the node is left uncalculated too (dc-10y).
    Kept,
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
    let shared = shared_of!(work);
    evaluate_with(state, inputs, &shared, &mut work.local, node, observer)
}

/// Evaluates `node` against what `shared` holds of the recalculation, in `local`'s storage: the one
/// evaluation route, whether one thread runs the recalculation or several (dc-2m0). It writes
/// nothing but `local`; staging what it gives is the caller's, in order.
fn evaluate_with(
    state: &EngineState,
    inputs: &ModelRoot,
    shared: &EvalShared<'_>,
    local: &mut EvalLocal,
    node: NodeId,
    observer: &mut dyn ReadObserver,
) -> Result<Attempt, RecalcFailure> {
    let formula = live_anchor(state, node);
    let calculation = shared.calculation.clone();
    let (instance, role) = match state
        .bindings
        .get(&node)
        .expect("begin_recalc bound every node it scheduled")
    {
        Binding::Placed(instance, role) => (instance, *role),
        Binding::Refused(refusal) => {
            return Ok(Attempt::Done(refused_by(&calculation, refusal.clone())));
        }
        Binding::DataTable(table) => {
            // A table cell whose substitution reads a formula a circular reference left
            // uncalculated is not calculated either (dc-10y; Excel 16.0.20430, 2026-10-25,
            // iteration off: a table over `=A1+C1`, A1 on a cycle, shows 0 in each cell, as
            // a formula that reads the cycle does).
            local.kept_read.set(false);
            let view = shared.view(state, inputs, local);
            let outcome = crate::data_table::evaluate_cell(
                state,
                inputs,
                view,
                calculation.mode(),
                table,
                formula,
                Some((&calculation, &mut *observer)),
            );
            if local.kept_read.get() {
                return Ok(Attempt::Kept);
            }
            return Ok(
                match outcome {
                    crate::data_table::TableOutcome::Value(value) => {
                        match NotModeled::publishable(value) {
                            Ok(value) => Attempt::Done(Done {
                                result: FormulaResult::Calculated { value, calculation },
                                spill: None,
                                dynamic: Vec::new(),
                            }),
                            Err(refusal) => Attempt::Done(refused_by(&calculation, refusal)),
                        }
                    }
                    crate::data_table::TableOutcome::Refused(refusal) => {
                        Attempt::Done(refused_by(&calculation, refusal))
                    }
                    crate::data_table::TableOutcome::Waits { on, cell } => {
                        Attempt::Waits { on, cell }
                    }
                },
            );
        }
    };
    let uncomputed = Cell::new(None);
    let footprint = RefCell::new(std::mem::take(&mut local.footprint));
    local.kept_read.set(false);
    let batch = std::mem::take(&mut local.read_batch);
    let mut scratch = std::mem::take(&mut local.scratch);
    let view = shared.view(state, inputs, &*local);
    let mut cells = WorkbookReader::new(view, &uncomputed).with_batch(batch);
    let catalog = SnapshotCatalog(inputs);
    let mut random = RandomStream::new(state.random_seed, calculation.tick().get(), formula);
    let silent = observer.is_silent();
    let observer = RefCell::new(observer);
    let (outcome, stopped) = {
        let mut observed = Observed {
            observer: &observer,
            silent,
            formula,
            cells: &mut cells,
            footprint: &footprint,
            stopped: None,
        };
        let instrument: &mut dyn EvaluationObserver = &mut Instrument {
            observer: &observer,
            silent,
            formula,
            calculation: &calculation,
            footprint: &footprint,
            uncomputed: &uncomputed,
            kept_read: &local.kept_read,
        };
        let outcome = {
            let mut services = EvaluationServices::new(calculation.mode(), &mut observed)
                .with_catalog(&catalog)
                .with_random(&mut random)
                .observed_by(instrument);
            evaluate(EvalFrame::new(instance, &mut scratch), &mut services)
        };
        (outcome, observed.stopped)
    };
    local.read_batch = cells.into_batch();
    local.scratch = scratch;
    let kept_read = local.kept_read.get();
    if let Some(area) = stopped {
        return Err(RecalcFailure::ReadFailed { formula, area });
    }
    // With iteration off, a formula that read one a circular reference left uncalculated is not
    // calculated either (dc-10y).
    if kept_read {
        return Ok(Attempt::Kept);
    }
    if let Some((cell, on)) = uncomputed.get() {
        // A formula cell one of the formula's own placed references names was ordered before it,
        // unless it waits itself or is on a cycle: the graph missed an edge.
        let named = instance
            .areas()
            .iter()
            .any(|area| area.is_ok_and(|area| area.contains(cell)));
        let formula_cell = matches!(inputs.cell(cell), Some(CellInput::Formula(_)));
        if named && formula_cell && !shared.waits_on.contains_key(&on) && !shared.cyclic.contains(&on)
        {
            return Err(RecalcFailure::UnscheduledRead { formula, cell });
        }
        return Ok(Attempt::Waits { on, cell });
    }
    let mut areas = footprint.into_inner();
    let mut dynamic = dynamic_prerequisites(state, node, instance, &areas);
    areas.clear();
    local.footprint = areas;
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
    let outcome = match array_outcome(shared.view(state, inputs, local), node, formula, role, value)
    {
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


/// The areas an evaluation read that none of the formula's placed references holds, each once,
/// as prerequisites.
fn dynamic_prerequisites(
    state: &EngineState,
    node: NodeId,
    instance: &FormulaInstance,
    footprint: &[GridArea],
) -> Vec<Prerequisite> {
    let placed = state
        .node(node)
        .and_then(|record| record.prerequisites())
        .unwrap_or(&[]);
    let mut out: Vec<Prerequisite> = Vec::new();
    for &area in footprint {
        // Most reads are of a placed reference's own area, found first (dc-dxm).
        let held = instance.areas().iter().any(|placed| *placed == Ok(area))
            || placed
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
                        owner: "dc-10y",
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
