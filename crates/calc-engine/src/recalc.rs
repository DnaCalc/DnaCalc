//! Recalculation (plan 6): [`begin_recalc`] binds each unbound node's formula through CalcFml and
//! orders the dirty closure, [`step`] evaluates the ordered nodes through CalcFml a budget at a
//! time. [`publish_ready`] can install certified interested scalar units during pure work;
//! [`finish`] installs the remaining results and each complete spill. Cancellation or failure
//! leaves already installed units in place and unfinished targets pending.
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
//!   that value, and one it never calculated keeps the value the file stored, or an empty text
//!   when the file stored none (dc-y2o.1, Excel 16.0.20527 2026-10-30, tests/fixtures/dc_y2o).
//!   Each is published as the circular-reference rule's value, marked so
//!   ([`Calculation::held_by_circular_rule`]) that a comparison counts it apart from a calculated
//!   pass. Every recalculation calculates them again, as Excel does (it saves each with
//!   `ca="1"`). The other formulas are calculated.
//! - **Iteration on.** The formulas of the order are evaluated in passes, each formula on a cycle
//!   reading the value the other formulas on it had at the end of the last pass, or, once
//!   evaluated in this pass, their new value (the members of a cycle in cell order: Excel
//!   evaluates A1 before B1, A5 before A6 and B5 before C5, and the sheets by name, letter case
//!   ignored, whatever the order the formulas were entered in, the saved calc chain, the tab order
//!   or the sheet IDs; dc-86f, Excel 16.0.20527 2026-10-30, tests/fixtures/dc_86f). The current
//!   stopping policy compares evaluated scalar values, including downstream readers, after a
//!   complete sweep and stops when the largest change is strictly below MaxChange, or
//!   MaxIterations sweeps have run. P14's partial 2026-11-03 recordings confirm strict equality,
//!   downstream amplifier influence and the recorded text/error outputs. Observer counts,
//!   intermediate caches and the remaining command/topology variants remain open. Excel
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

use crate::contract::*;
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
    ArrayKind, AuthoredRevision, Axis, CellInput, FormulaText, IterationSettings, ModelRoot,
};
use calc_fml::reference::{
    CellRun, FormulaAnchor, ReferenceNotation, ReferenceReader, ReferenceShape,
    template_key_of_chars,
};
use calc_fml::{
    EvalFault, EvalScratch, EvaluationObserver, EvaluationReport, ExecutedCall,
    FormulaEvaluationContext, FormulaForm, FormulaInstance, ResolvedReference,
    compile_loaded_with_bindings, instantiate, parse_formula,
};
use calc_value::{CalcValue, CellKey, GridArea, NodeId, NumericMode, SheetId};
use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashSet, VecDeque};
use std::fmt;
use std::ops::ControlFlow;
use std::sync::Arc;

/// How many waves one recalculation runs before it refuses the formulas whose spills have not
/// settled.
const MAX_WAVES: u32 = 32;

/// P04, Excel 16.0.20527: a volatile dynamic array whose size keeps changing is
/// evaluated at most eight times. This is per anchor, separate from graph repair.
const MAX_SIZING_EVALUATIONS: u32 = 8;

/// Why [`begin_recalc`] refused to start. A refused recalculation evaluates and publishes
/// nothing; its pending nodes stay pending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartError {
    /// A scope whose Excel calculation policy is not implemented. No target
    /// result is produced; the named work package owns the missing profile.
    NotModeledScope {
        construct: &'static str,
        owner: &'static str,
    },
    /// An exact scope is empty or names a sheet outside the workbook.
    InvalidScope { sheet: Option<SheetId> },
    /// The caller must transform the admitted scope after moving worksheet addresses.
    ScopeTransformRequired,
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
            StartError::NotModeledScope { construct, owner } => {
                write!(f, "{construct} is not modelled ({owner})")
            }
            StartError::ScopeTransformRequired => {
                f.write_str("the scoped successor needs an address transform (dc-hsa5)")
            }
            StartError::InvalidScope { sheet: None } => {
                f.write_str("the calculation scope contains no areas")
            }
            StartError::InvalidScope { sheet: Some(sheet) } => {
                write!(f, "the calculation scope names an unknown sheet {sheet:?}")
            }
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
    WrongDestination,
    CallableBindingMismatch,
    ExternalInputMismatch,
    /// A scope interaction discovered during evaluation has no measured profile.
    NotModeledScope {
        construct: &'static str,
        owner: &'static str,
    },
    /// A read of `area` by the evaluation of the formula at `formula` did not complete, so the
    /// evaluation's result was discarded: the read observer stopped the recalculation there
    /// ([`ReadObserver::observe`]), or the reads broke CalcFml's reader contract, which the
    /// engine's own reader keeps. This is a failure of the recalculation, not a result of the
    /// formula.
    ReadFailed {
        formula: CellKey,
        area: GridArea,
    },
    /// The evaluation of the formula at `formula` read the formula cell `cell`, which one of
    /// the formula's own placed references names and which this recalculation calculates, had
    /// not yet evaluated and was not waiting for another cell: the order missed an edge of the
    /// graph. Its old value was not handed out. A cell read through a reference the evaluation
    /// computed is waited for instead ([`step`]).
    UnscheduledRead {
        formula: CellKey,
        cell: CellKey,
    },
    /// The engine state or the input snapshot is no longer at the revision the recalculation
    /// began on (`revision`), or another recalculation published after it began.
    Superseded {
        revision: AuthoredRevision,
    },
    /// [`finish`] was called while `remaining` nodes were still to be evaluated.
    Incomplete {
        remaining: usize,
    },
    InvalidCertificate {
        at: CellKey,
    },
    ScopeViolation {
        at: CellKey,
    },
}

impl fmt::Display for RecalcFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecalcFailure::NotModeledScope { construct, owner } => {
                write!(f, "{construct} is not modelled ({owner})")
            }
            RecalcFailure::WrongDestination => {
                f.write_str("the result destination differs from its admitted owner")
            }
            RecalcFailure::ReadFailed { formula, area } => write!(
                f,
                "a read of {area:?} by the formula at {formula:?} did not complete"
            ),
            RecalcFailure::ExternalInputMismatch => {
                write!(f, "the external input view differs from its admitted cut")
            }
            RecalcFailure::CallableBindingMismatch => {
                f.write_str("the executable callable table differs from its admitted bindings")
            }
            RecalcFailure::UnscheduledRead { formula, cell } => write!(
                f,
                "the formula at {formula:?} read {cell:?}, which this recalculation had not evaluated yet"
            ),
            RecalcFailure::Superseded { revision } => write!(
                f,
                "the recalculation began on revision {}, and the state or its inputs have changed since",
                revision.get()
            ),
            RecalcFailure::ScopeViolation { at } => write!(
                f,
                "the target {at:?} is outside the admitted calculation scope"
            ),
            RecalcFailure::InvalidCertificate { at } => {
                write!(f, "the read certificate for {at:?} no longer validates")
            }
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
    /// No runnable work remains before an observation fence awaiting provider input.
    Waiting { remaining: usize },
    /// Every node is evaluated; [`finish`] publishes.
    Complete,
    /// The recalculation failed; [`finish`] returns why and publishes nothing.
    Failed,
}

/// How many activation quanta one [`step`] may evaluate. Finite steps retain the root instruction
/// stream and cooperative kernel state inside a cell. Other nested calls remain non-preemptible.
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
    entries: crate::slots::Slots<(NodeId, T)>,
}

#[cfg(test)]
mod scheduling_slot_tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn scheduling_metadata_keeps_slot_order_and_rejects_a_replaced_generation() {
        for dense in [false, true] {
            let mut map = BySlot::new(1_000_003, 3);
            let low = NodeId::from_parts(2, 3);
            let high = NodeId::from_parts(1_000_002, 2);
            map.insert(high, 20);
            map.insert(low, 10);
            if dense {
                map.reserve_slots(1_000_003);
            }
            for (_, value) in map.iter_mut() {
                *value += 1;
            }
            assert_eq!(
                map.iter()
                    .map(|(node, value)| (*node, *value))
                    .collect::<Vec<_>>(),
                vec![(low, 11), (high, 21)]
            );
            let replacement = NodeId::from_parts(high.slot(), high.generation() + 1);
            map.insert(replacement, 30);
            assert_eq!(map.get(high), None);
            assert_eq!(map.get_mut(&high), None);
            *map.get_mut(&replacement).unwrap() += 1;
            assert_eq!(
                map.into_entries().collect::<Vec<_>>(),
                vec![(low, 11), (replacement, 31)]
            );
        }
    }
}

impl<T> Default for BySlot<T> {
    fn default() -> Self {
        BySlot {
            entries: crate::slots::Slots::default(),
        }
    }
}

impl<T> BySlot<T> {
    fn new(slots: usize, expected: usize) -> Self {
        Self {
            entries: crate::slots::Slots::new(slots, expected),
        }
    }
    fn get(&self, node: impl std::borrow::Borrow<NodeId>) -> Option<&T> {
        let node = *node.borrow();
        self.entries
            .get(node.slot())
            .filter(|(held, _)| *held == node)
            .map(|(_, value)| value)
    }
    fn get_mut(&mut self, node: &NodeId) -> Option<&mut T> {
        self.entries
            .get_mut(node.slot())
            .filter(|(held, _)| held == node)
            .map(|(_, value)| value)
    }
    fn iter(&self) -> impl Iterator<Item = (&NodeId, &T)> {
        self.entries.iter().map(|(node, value)| (node, value))
    }
    fn iter_mut(&mut self) -> impl Iterator<Item = (&NodeId, &mut T)> {
        self.entries.iter_mut().map(|(node, value)| (&*node, value))
    }
    fn reserve_slots(&mut self, slots: usize) {
        self.entries.reserve_dense(slots);
    }
    fn insert(&mut self, node: NodeId, value: T) {
        self.entries.insert(node.slot(), (node, value));
    }
    fn remove(&mut self, node: NodeId) {
        if self.get(node).is_some() {
            self.entries.remove(node.slot());
        }
    }
    fn into_entries(self) -> impl Iterator<Item = (NodeId, T)> {
        self.entries.into_values()
    }
}

/// One recalculation in progress: the nodes it evaluates in their order, the formulas waiting for
/// a cell it has yet to evaluate, and the results, spills and dynamic prerequisites it has
/// staged. It holds no borrow of the engine state, which [`step`] and [`finish`] check has not
/// moved on since [`begin_recalc`].
#[derive(Debug)]
pub struct RecalcWork {
    linked_workbooks: Arc<crate::linked::LinkedWorkbooks>,
    linked_input_version: u64,
    external_input_version: u64,
    external_inputs: Option<std::rc::Rc<dyn calc_fml::RtdCut>>,
    rtd_connector: Option<std::rc::Rc<dyn calc_fml::RtdConnector>>,
    /// The sheet and cell the person has active, which a VBA function's `ActiveSheet` and
    /// `Selection` answer (dc-hsa5.12); `None` for a host that models neither.
    user_view: Option<calc_fml::UserView>,
    async_waits: BTreeMap<NodeId, AsyncToken>,
    async_resumed: BTreeSet<NodeId>,
    evaluations: BTreeMap<NodeId, (u64, ReevaluationReason)>,
    /// One provisional reference callback before the first iterative sweep.
    preliminary: BTreeSet<NodeId>,
    semantic_waits: BTreeSet<NodeId>,
    functions: Option<calc_fml::CallableTable>,
    references: Option<ReferenceSource>,
    callable_bindings: calc_fml::CallableBindings,
    root_reason: ReevaluationReason,
    admission: AdmissionReceipt,
    pass: CalcPassId,
    requirements: RequirementCut,
    facts_version: u64,
    events: Vec<CalcEvent>,
    reused: usize,
    repairing_certificates: bool,
    invalidated_outside: BTreeSet<NodeId>,
    revision: AuthoredRevision,
    generation: u64,
    coordinates: u64,
    calculation: Calculation,
    environment: Option<calc_fml::CalcEnvironment>,
    random_seed: u64,
    workbook_facts: Option<Arc<dyn calc_fml::WorkbookFacts>>,
    volatile_input_version: u64,
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
    blocked_spills: BTreeMap<CellKey, GridArea>,
    filled: BySlot<Option<GridArea>>,
    /// Each evaluated node's dynamic prerequisites.
    dynamic: BySlot<Vec<Prerequisite>>,
    /// Nodes to calculate again in the next wave.
    rewave: BTreeSet<NodeId>,
    waves: u32,
    sizing: BTreeMap<NodeId, Sizing>,
    /// The storage one evaluation works in, kept between evaluations ([`EvalLocal`]).
    pub(crate) local: EvalLocal,
    boundary_caches: crate::read::BoundarySnapshot,
    interest: Vec<GridArea>,
    preferred: BTreeSet<NodeId>,
    precedents: BTreeMap<NodeId, Vec<NodeId>>,
    reference_rank: BySlot<usize>,
    segments: BySlot<usize>,
    segment_scheduling: bool,
    parallel_scheduling: bool,
    has_fences: bool,
    preferred_quanta: usize,
    urgent_interest: bool,
    service_clock: u64,
    last_service: BySlot<u64>,
    suspended: BTreeMap<NodeId, EvalLocal>,
    early_published: BTreeMap<NodeId, Option<crate::contract::ResultStamp>>,
    publication_fences: bool,
    fence_flush: Option<usize>,
    publication_sequence: u64,
    failure: Option<RecalcFailure>,
    /// The workbook's iteration settings.
    iteration: IterationSettings,
    /// The formulas on a cycle.
    cyclic: BTreeSet<NodeId>,
    /// Manual iterative Range targets whose calculation is not established.
    /// They retain their old results and unmet requirements, outside Progress.
    range_refused: BTreeSet<NodeId>,
    /// With iteration on, the value each formula on a cycle holds as the recalculation stands:
    /// its value at the end of the last pass, or its new value once evaluated in this pass;
    /// `None` when its result is no value.
    iterates: BTreeMap<NodeId, Option<CalcValue>>,
    /// With iteration on, the evaluated formulas' values when the current pass began.
    /// Downstream formulas participate in stopping without becoming cyclic read seeds.
    pass_start: BTreeMap<NodeId, Option<CalcValue>>,
    /// How many passes over the order have ended.
    passes: u32,
    /// The final sweep's stopping decision has run. A yield after its last cell must still
    /// retain that decision as pending work; staged cells alone cannot complete the session.
    iteration_complete: bool,
    /// With iteration off, the formulas a circular reference left uncalculated (dc-10y): those on
    /// a cycle that their evaluations met, and those whose evaluation read one of them.
    kept: BTreeSet<NodeId>,
}

#[derive(Debug, Default)]
struct Sizing {
    evaluations: u32,
    changing: bool,
    exhausted: bool,
    previous: Option<GridArea>,
}

/// The storage one evaluation works in (dc-2m0: each thread of a parallel recalculation has its
/// own; a one-thread recalculation keeps one in its work): the evaluator's scratch, the areas the
/// evaluation read, the reader's gathering storage, and whether it read a formula of `kept`.
#[derive(Debug, Default)]
pub(crate) struct EvalLocal {
    external_cache: bool,
    effects: calc_fml::UserFunctionEffects,
    semantic_prerequisite: Option<(CellKey, NodeId)>,
    references: Option<std::rc::Rc<calc_fml::reference::ReferenceView>>,
    rtd_connector: Option<std::rc::Rc<dyn calc_fml::RtdConnector>>,
    observation: Option<crate::CachedReadLease>,
    observation_reads: Vec<(calc_fml::ReadRequest, GridArea)>,
    invocation: Option<calc_fml::UserInvocationFrame>,
    scratch: EvalScratch,
    activation: Option<calc_fml::EvalContinuation>,
    placement: Option<(CalcValue, Vec<Prerequisite>, Calculation)>,
    random: Option<RandomStream>,
    quantum: usize,
    read_stamps: Vec<ReadSpan>,
    name_reads: Vec<calc_fml::reference::NameBindingProof>,
    footprint: Footprint,
    fact_reads: Vec<calc_fml::FactRead>,
    value_reads: Vec<(calc_fml::ReadRequest, GridArea)>,
    blank_reads: Vec<(GridArea, BlankRun)>,
    current_node: Cell<Option<NodeId>>,
    boundary_read: Cell<Option<calc_fml::ReadDenial>>,
    boundary_used: Cell<bool>,
    read_batch: Vec<CalcValue>,
    kept_read: Cell<bool>,
}

/// What every evaluation of one recalculation reads of its work, and nothing it writes (dc-2m0):
/// the results it staged, its spills, the values of an iterating cycle, the formulas a cycle left
/// uncalculated, the formulas waiting, those on a cycle, and its calculation. Built from the work
/// for each evaluation of a one-thread recalculation, and for each level of a parallel one, when
/// all of it is only read.
#[derive(Debug)]
enum ReferenceSource {
    Live(std::rc::Rc<calc_fml::reference::ReferenceTracker>),
    Pinned(std::rc::Rc<calc_fml::reference::ReferenceView>),
}
impl ReferenceSource {
    fn pin(&self) -> std::rc::Rc<calc_fml::reference::ReferenceView> {
        match self {
            Self::Live(tracker) => tracker.pin(),
            Self::Pinned(view) => view.clone(),
        }
    }
}

pub(crate) struct EvalShared<'a> {
    linked_workbooks: &'a Arc<crate::linked::LinkedWorkbooks>,
    linked_input_version: u64,
    selection: EvalSelection,
    parallel_scheduling: bool,
    reference_rank: &'a BySlot<usize>,
    destination: ResultDestination,
    pass: CalcPassId,
    root_reason: ReevaluationReason,
    evaluations: &'a BTreeMap<NodeId, (u64, ReevaluationReason)>,
    preliminary: &'a BTreeSet<NodeId>,
    requirements: &'a RequirementCut,
    facts_version: u64,
    volatile_inputs: u64,
    boundary_caches: &'a crate::read::BoundarySnapshot,
    scope: &'a CalcScope,
    range_refused: &'a BTreeSet<NodeId>,
    pub(crate) calculation: &'a Calculation,
    pub(crate) environment: Option<calc_fml::CalcEnvironment>,
    pub(crate) random_seed: u64,
    external_inputs: Option<&'a dyn calc_fml::RtdCut>,
    user_view: Option<calc_fml::UserView>,
    pub(crate) workbook_facts: Option<&'a Arc<dyn calc_fml::WorkbookFacts>>,
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
            linked_workbooks: self.linked_workbooks,
            cached_observation: false,
            inputs,
            state,
            progress: self.progress,
            spills: self.spills,
            workbook_facts: self.workbook_facts,
            caller: local.current_node.get(),
            scope: Some(self.scope),
            range_refused: Some(self.range_refused),
            boundary: Some(&local.boundary_read),
            boundary_caches: Some(self.boundary_caches),
            boundary_used: Some(&local.boundary_used),
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
            linked_workbooks: &$work.linked_workbooks,
            linked_input_version: $work.linked_input_version,
            selection: $work.admission.selection,
            parallel_scheduling: $work.parallel_scheduling && $work.waves == 0,
            reference_rank: &$work.reference_rank,
            destination: $work.admission.destination,
            pass: $work.pass,
            root_reason: $work.root_reason,
            evaluations: &$work.evaluations,
            preliminary: &$work.preliminary,
            requirements: &$work.requirements,
            facts_version: $work.facts_version,
            volatile_inputs: $work.volatile_input_version,
            scope: &$work.admission.scope,
            range_refused: &$work.range_refused,
            boundary_caches: &$work.boundary_caches,
            calculation: &$work.calculation,
            environment: $work.environment,
            random_seed: $work.random_seed,
            external_inputs: $work.external_inputs.as_deref(),
            user_view: $work.user_view,
            workbook_facts: $work.workbook_facts.as_ref(),
            progress: &$work.progress,
            spills: &$work.spills,
            iterates: $work.iterating().then_some(&$work.iterates),
            kept: (!$work.iteration.enabled()).then_some(&$work.kept),
            waits_on: &$work.waits_on,
            cyclic: &$work.cyclic,
        }
    };
}

impl RecalcWork {
    /// The owner admitted, after a step, only the values connections returned to reads of this
    /// work. Those reads already saw them (the connection's value is what the add-in's RTD
    /// call returned), so the work continues under the new cut instead of being superseded:
    /// a suspended activation is not thrown away and its user functions are not entered again,
    /// as Excel does not enter a function again because ConnectData's value was returned to it.
    /// The caller checks the work was current before the admission.
    pub fn adopt_connected_inputs(
        &mut self,
        state: &EngineState,
        inputs: std::rc::Rc<dyn calc_fml::RtdCut>,
    ) {
        self.external_input_version = state.contract.external_inputs;
        self.volatile_input_version = state.volatile_input_version;
        if inputs.version() != self.external_input_version {
            self.failure = Some(RecalcFailure::ExternalInputMismatch);
        }
        self.external_inputs = Some(inputs);
    }
    pub fn with_external_inputs(mut self, inputs: std::rc::Rc<dyn calc_fml::RtdCut>) -> Self {
        if inputs.version() != self.external_input_version {
            self.failure = Some(RecalcFailure::ExternalInputMismatch);
        }
        self.external_inputs = Some(inputs);
        self
    }
    /// Live hosts admit a successor for a new clock/random/input cut. The raw engine can
    /// finish a historical clock-pinned pass, whose unmet current requirements remain stale.
    pub fn live_inputs_current(&self, state: &EngineState) -> bool {
        self.inputs_current(state)
            && self.volatile_input_version == state.volatile_input_version
            && state.environment == self.environment.map(Into::into)
    }
    pub fn inputs_current(&self, state: &EngineState) -> bool {
        self.external_input_version == state.contract.external_inputs
            && self.linked_input_version == state.linked_input_version
    }
    pub fn with_reference_tracker(
        mut self,
        references: std::rc::Rc<calc_fml::reference::ReferenceTracker>,
    ) -> Self {
        self.references = Some(ReferenceSource::Live(references));
        self
    }
    pub fn with_reference_view(
        mut self,
        references: std::rc::Rc<calc_fml::reference::ReferenceView>,
    ) -> Self {
        self.references = Some(ReferenceSource::Pinned(references));
        self
    }

    /// The owner's capability to join or start RTD topics while this work evaluates.
    pub fn with_rtd_connector(
        mut self,
        connector: std::rc::Rc<dyn calc_fml::RtdConnector>,
    ) -> Self {
        self.rtd_connector = Some(connector);
        self
    }
    /// The sheet and cell the person has active while this work evaluates (VBA's `ActiveSheet`
    /// and `Selection`).
    pub fn with_user_view(mut self, view: Option<calc_fml::UserView>) -> Self {
        self.user_view = view;
        self
    }
    pub fn with_functions(mut self, functions: &calc_fml::CallableTable) -> Self {
        if self.callable_bindings != functions.bindings() {
            self.failure = Some(RecalcFailure::CallableBindingMismatch);
        } else {
            self.functions = Some(functions.clone());
        }
        self
    }
    pub fn callables_current(&self, state: &EngineState) -> bool {
        self.callable_bindings == state.callables
    }
    pub fn with_reevaluation_reason(mut self, reason: ReevaluationReason) -> Self {
        self.root_reason = reason;
        self
    }

    /// Request installation handoffs before crossing observation fences. A caller
    /// using this route must drain `publish_ready` between `step` calls.
    pub fn enable_publication_fences(&mut self) {
        self.publication_fences = true;
    }
    /// Select the measured Manual ForceAll-range profile before admission is delivered.
    pub fn with_manual_range_poststate(mut self) -> Self {
        assert!(
            self.admission_pending()
                && matches!(self.admission.scope, CalcScope::Range(_))
                && self.admission.selection.forces_all(),
            "Manual range post-state is fixed at range admission"
        );
        self.admission.range_post_state = crate::contract::RangePostState::StillRequired;
        for event in &mut self.events {
            if let CalcEvent::Admitted(admission) = event {
                admission.range_post_state = self.admission.range_post_state;
            }
        }
        self
    }
    /// Prioritize existing targets and their prerequisite cone inside each pure segment.
    /// Callback-bearing bindings and unresolved macro calls separate the segments. Pure volatile scalars
    /// keep their command/pass draw identity when interest changes.
    pub fn set_interest(&mut self, state: &EngineState, inputs: &ModelRoot, areas: &[GridArea]) {
        if self.interest == areas {
            return;
        }
        self.urgent_interest =
            self.next != 0 || !self.ready.is_empty() || !self.suspended.is_empty();
        self.interest = areas.to_vec();
        self.preferred.clear();
        if !self.segment_scheduling || !self.cyclic.is_empty() {
            return;
        }
        let producers = scheduling_precedents(state, inputs, &self.order);
        for &node in &self.order {
            let footprint = published_areas(state, inputs, node);
            if areas.iter().any(|area| {
                area.contains(live_anchor(state, node))
                    || footprint
                        .iter()
                        .any(|filled| crate::contract::meets(*area, *filled))
            }) {
                self.preferred.insert(node);
            }
        }
        let mut stack: Vec<_> = self.preferred.iter().copied().collect();
        while let Some(reader) = stack.pop() {
            for &producer in producers.get(&reader).into_iter().flatten() {
                if self.preferred.insert(producer) {
                    stack.push(producer);
                }
            }
        }
        self.precedents = producers;
    }

    /// Bring only an admitted pure prerequisite cone to a waiting activation's segment.
    /// Its pinned observations stay unchanged; another callback never crosses the fence.
    fn admit_waiting_prerequisites(
        &mut self,
        state: &EngineState,
        inputs: &ModelRoot,
        node: NodeId,
        on: NodeId,
    ) {
        let Some(&segment) = self.segments.get(&node) else {
            return;
        };
        if self.segments.get(&on).is_none_or(|own| *own <= segment) {
            return;
        }
        if self.precedents.is_empty() {
            let members: BTreeSet<_> = self.order.iter().copied().collect();
            let mut readers = Vec::new();
            for &producer in &self.order {
                readers_into(
                    state,
                    inputs,
                    producer,
                    &published_areas(state, inputs, producer),
                    &mut readers,
                );
                for &reader in &readers {
                    if members.contains(&reader) {
                        self.precedents.entry(reader).or_default().push(producer);
                    }
                }
            }
        }
        let mut cone = BTreeSet::new();
        let mut stack = vec![on];
        while let Some(producer) = stack.pop() {
            if self.progress.is_evaluated(producer) || !cone.insert(producer) {
                continue;
            }
            if producer == node
                || !matches!(state.bindings.get(&producer),
                Some(Binding::Placed(instance, _)) if !instance.plan().has_user_calls())
            {
                return;
            }
            stack.extend(
                self.precedents
                    .get(&producer)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        for producer in cone {
            if let Some(own) = self.segments.get_mut(&producer) {
                *own = (*own).min(segment);
            }
        }
    }

    pub fn continuation_progress(&self) -> Option<calc_fml::ContinuationProgress> {
        self.local
            .activation
            .as_ref()
            .map(calc_fml::EvalContinuation::progress)
            .or_else(|| {
                self.suspended.values().find_map(|local| {
                    local
                        .activation
                        .as_ref()
                        .map(calc_fml::EvalContinuation::progress)
                })
            })
    }

    pub fn admission_pending(&self) -> bool {
        self.events.iter().any(|event|matches!(event,CalcEvent::Admitted(receipt) if receipt.command==self.admission.command))
    }
    /// Correlates admission with the accepted host command before it is emitted.
    pub fn with_source(mut self, source: CommandId) -> Self {
        assert!(
            self.admission_pending(),
            "an emitted admission is immutable"
        );
        self.admission.source = source;
        for event in &mut self.events {
            if let CalcEvent::Admitted(receipt) = event {
                if receipt.command == self.admission.command {
                    receipt.source = source;
                }
            }
        }
        self
    }
    pub fn admission(&self) -> &AdmissionReceipt {
        &self.admission
    }
    pub fn pass(&self) -> CalcPassId {
        self.pass
    }
    pub fn reused(&self) -> usize {
        self.reused
    }
    pub fn take_events(&mut self) -> Vec<CalcEvent> {
        std::mem::take(&mut self.events)
    }
    /// A candidate is unpublished work; reading it does not turn it into an application value.
    pub fn candidate(&self, node: NodeId) -> Option<&FormulaResult> {
        self.progress.staged(node)
    }

    fn open_pass(&mut self, reason: PassReason) {
        self.pass = CalcPassId {
            command: self.pass.command,
            ordinal: self
                .pass
                .ordinal
                .checked_add(1)
                .expect("pass ordinal exhausted"),
            reason,
        };
    }
    fn coverage(&self) -> ScopeCoverage {
        let mut coverage = ScopeCoverage {
            required: self.progress.scheduled() + self.range_refused.len(),
            refused: self.range_refused.len(),
            ..ScopeCoverage::default()
        };
        for (_, result) in self.progress.staged_results() {
            match result {
                FormulaResult::Calculated { calculation, .. }
                    if calculation.held_by_circular_rule() =>
                {
                    coverage.held += 1
                }
                FormulaResult::Calculated { .. } => {
                    coverage.calculated += 1;
                    if let Some(stamp) = result.stamp() {
                        coverage.boundary_cached += usize::from(stamp.reads.uses_boundary_cache);
                        coverage.imported_cached += usize::from(stamp.reads.uses_imported_cache);
                    }
                    if result.stamp().is_none_or(|stamp| {
                        stamp.reads.legacy_boundary || stamp.reads.external_cache
                    }) {
                        coverage.uncertified += 1;
                    }
                }
                _ => coverage.refused += 1,
            }
        }
        coverage
    }
    pub fn completion(
        &self,
        outcome: CompletionOutcome,
        results: ReadViewId,
        after_publication: Option<PublicationUnitId>,
    ) -> CompletionReceipt {
        CompletionReceipt {
            destination: self.admission.destination,
            command: self.admission.command,
            scope: self.admission.scope.clone(),
            admitted: self.admission.admitted,
            outcome,
            coverage: self.coverage(),
            results,
            after_publication: after_publication.or_else(|| {
                (self.publication_sequence != 0).then_some(PublicationUnitId {
                    command: self.admission.command,
                    sequence: self.publication_sequence,
                })
            }),
        }
    }

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

    pub(crate) fn remaining(&self) -> usize {
        (self.order.len() - self.next)
            + self.ready.len()
            + self.waits_on.len()
            + self.rewave.len()
            + self.async_waits.len()
            + usize::from(self.iterating() && !self.iteration_complete)
    }

    fn progress(&self) -> RecalcProgress {
        if self.failure.is_some() {
            RecalcProgress::Failed
        } else if self.is_waiting_external() {
            RecalcProgress::Waiting {
                remaining: self.remaining(),
            }
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
        self.iteration.enabled() && !self.cyclic.is_empty() && !self.iterative_range()
    }

    fn iterative_range(&self) -> bool {
        self.iteration.enabled()
            && matches!(self.admission.scope, CalcScope::Range(_))
            && self.admission.selection != EvalSelection::FormulaEntry
    }

    /// Makes `nodes` formulas on a cycle, each starting from its last calculated value in `state`
    /// (0 when it has none) when iteration is on.
    fn add_cyclic(&mut self, state: &EngineState, nodes: impl IntoIterator<Item = NodeId>) {
        for node in nodes {
            if self.cyclic.insert(node) && self.iteration.enabled() && !self.iterative_range() {
                self.iteration_complete = false;
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
    begin_with_scope(
        state,
        inputs,
        mode,
        CalcScope::Workbook,
        EvalSelection::DirtyAndVolatile,
        false,
        RangePostState::Satisfied,
    )
}

/// Exact SliceV1 scope. Ordinary scalar boundary caches are pinned and qualified; imported
/// caches never acquire producing invocations or freshness. Arrays require complete cached topology.
pub fn begin_recalc_scoped(
    state: &mut EngineState,
    inputs: &ModelRoot,
    mode: NumericMode,
    scope: CalcScope,
    selection: EvalSelection,
) -> Result<RecalcWork, StartError> {
    begin_recalc_scoped_with_poststate(
        state,
        inputs,
        mode,
        scope,
        selection,
        RangePostState::Satisfied,
    )
}

/// The owner supplies the admitted range post-state before scheduling. StillRequired
/// selects the measured Manual range profile; it cannot be inferred from file caches.
pub fn begin_recalc_scoped_with_poststate(
    state: &mut EngineState,
    inputs: &ModelRoot,
    mode: NumericMode,
    scope: CalcScope,
    selection: EvalSelection,
    poststate: RangePostState,
) -> Result<RecalcWork, StartError> {
    assert!(
        poststate == RangePostState::Satisfied
            || (matches!(scope, CalcScope::Range(_)) && selection.forces_all()),
        "StillRequired belongs to an admitted forced range request"
    );
    begin_with_scope(state, inputs, mode, scope, selection, false, poststate)
}

fn begin_with_scope(
    state: &mut EngineState,
    inputs: &ModelRoot,
    mode: NumericMode,
    scope: CalcScope,
    requested_selection: EvalSelection,
    empty_successor: bool,
    poststate: RangePostState,
) -> Result<RecalcWork, StartError> {
    if requested_selection == EvalSelection::FormulaEntry
        && !matches!(&scope, CalcScope::Range(areas) if areas.len() == 1 && areas[0].cell_count() == 1)
    {
        return Err(StartError::InvalidScope { sheet: None });
    }
    let sheets: Vec<_> = match &scope {
        CalcScope::Workbook => Vec::new(),
        CalcScope::Sheet(sheet) => vec![*sheet],
        CalcScope::Range(areas) if areas.is_empty() && !empty_successor => {
            return Err(StartError::InvalidScope { sheet: None });
        }
        CalcScope::Range(areas) => areas.iter().map(|area| area.sheet()).collect(),
    };
    if let Some(sheet) = sheets
        .into_iter()
        .find(|sheet| inputs.sheet_by_id(*sheet).is_none())
    {
        return Err(StartError::InvalidScope { sheet: Some(sheet) });
    }
    let revision = state.revision();
    if inputs.revision() != revision {
        return Err(StartError::StaleInputs {
            engine: revision,
            inputs: inputs.revision(),
        });
    }
    if state.name_templates_dirty {
        state
            .templates
            .retain(|_, plan| crate::contract::name_bindings_valid(inputs, plan.name_bindings()));
        state.plans.evict_expired();
        state.name_templates_dirty = false;
    }
    // Manual CalcRange keeps a separate obligation for the next authorized command. Admit
    // it now, only inside this scope; unrelated clean targets remain untouched (P02).
    let redemand: BTreeSet<_> = state
        .range_redemand
        .iter()
        .copied()
        .filter(|node| {
            state
                .node(*node)
                .is_some_and(|node| scope.contains(node.anchor()))
        })
        .collect();
    if !redemand.is_empty() {
        state.range_redemand.retain(|node| !redemand.contains(node));
        if !requested_selection.forces_all() && !state.contract.force_all {
            let affected = dirty_closure(state, inputs, redemand, &|node| {
                invalidation_areas(state, inputs, node)
            })
            .members()
            .to_vec();
            state.contract.invalidate(affected.iter().copied());
            state.pending.extend(affected);
        }
    }
    // Required versions are authoritative demand, including transitive invalidation that
    // arrived before the next scheduler cut. A stale producer cannot remain unscheduled.
    let empty = Progress::new(0);
    let context = (state.contract.environment, state.contract.bindings);
    let mut candidates = std::mem::take(&mut state.certificate_candidates);
    candidates.extend(state.certificate_watch.iter().copied());
    if !state.contract.facility_roots.is_empty() {
        candidates.extend(
            dirty_closure(
                state,
                inputs,
                state.contract.facility_roots.clone(),
                &|node| invalidation_areas(state, inputs, node),
            )
            .members()
            .iter()
            .copied(),
        );
    }
    if state.certificate_context != Some(context) {
        candidates.extend(state.nodes().map(|node| node.id()));
    }
    let invalid: Vec<_> = candidates
        .into_iter()
        .filter(|node| {
            let requirement = state.contract.requirement(*node);
            requirement.required == requirement.satisfied
                && state.node(*node).is_some()
                && state
                    .results
                    .of(*node)
                    .and_then(FormulaResult::stamp)
                    .is_some_and(|stamp| {
                        !certificate_valid(state, inputs, &empty, stamp, state.contract.facts)
                    })
        })
        .collect();
    #[cfg(debug_assertions)]
    {
        let expected: Vec<_> = state
            .nodes()
            .filter_map(|node| {
                let requirement = state.contract.requirement(node.id());
                (requirement.required == requirement.satisfied
                    && state
                        .results
                        .of(node.id())
                        .and_then(FormulaResult::stamp)
                        .is_some_and(|stamp| {
                            !certificate_valid(state, inputs, &empty, stamp, state.contract.facts)
                        }))
                .then_some(node.id())
            })
            .collect();
        debug_assert_eq!(
            invalid.iter().copied().collect::<BTreeSet<_>>(),
            expected.into_iter().collect(),
            "certificate candidates cover every invalid clean result"
        );
    }
    state.certificate_context = Some(context);
    if !invalid.is_empty() {
        state.contract.invalidate(invalid);
    }
    let demanded = state.contract.demanded.clone();
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
    let facility_roots = std::mem::take(&mut state.contract.facility_roots);
    for root in facility_roots {
        let version = state.contract.requirement(root).required;
        let members = dirty_closure(state, inputs, BTreeSet::from([root]), &|node| {
            invalidation_areas(state, inputs, node)
        })
        .members()
        .to_vec();
        state.contract.require_at(members, version);
    }
    let volatile_roots: BTreeSet<_> = state
        .volatile
        .iter()
        .copied()
        .filter(|node| scope.contains(live_anchor(state, *node)))
        .collect();
    if !volatile_roots.is_empty() {
        let members = dirty_closure(state, inputs, volatile_roots.clone(), &|node| {
            invalidation_areas(state, inputs, node)
        })
        .members()
        .to_vec();
        state.contract.invalidate(members.iter().copied());
        state.pending.extend(members);
    }
    if state
        .published_mode
        .as_ref()
        .is_some_and(|published| *published != mode)
    {
        state.contract.invalidate(state.by_anchor.values().copied());
        state.pending.extend(state.by_anchor.values().copied());
    }
    let selection = if requested_selection == EvalSelection::FormulaEntry {
        // Entry cannot consume a pending workbook-wide rebuild demand.
        requested_selection
    } else if std::mem::take(&mut state.contract.force_all) {
        EvalSelection::ForceAll
    } else {
        requested_selection
    };
    let force_roots: BTreeSet<NodeId> = if selection.forces_all() {
        state
            .nodes()
            .filter(|node| scope.contains(node.anchor()))
            .map(|node| node.id())
            .collect()
    } else {
        BTreeSet::new()
    };
    if !force_roots.is_empty() {
        let affected = if force_roots.len() == state.node_count() {
            // Every live participant is already a root; expansion adds no member.
            force_roots.iter().copied().collect()
        } else {
            dirty_closure(state, inputs, force_roots.clone(), &|node| {
                invalidation_areas(state, inputs, node)
            })
            .members()
            .to_vec()
        };
        state.contract.invalidate(affected.iter().copied());
        state.pending.extend(affected);
    }
    let iteration = inputs.definitions().iteration();
    let mut roots = match &state.published_mode {
        Some(published) if *published != mode => state.by_anchor.values().copied().collect(),
        _ => {
            let mut roots = state.pending.clone();
            roots.extend(demanded.iter().copied());
            roots.extend(volatile_roots.iter().copied());
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
    roots.extend(force_roots);
    if roots.len() != state.node_count() || roots.iter().any(|node| state.node(*node).is_none()) {
        roots = dirty_closure(state, inputs, roots, &|node| {
            invalidation_areas(state, inputs, node)
        })
        .members()
        .iter()
        .copied()
        .collect();
    }
    let mut cache = std::mem::take(&mut state.readers_cache);
    cache.validate(state, inputs);
    let Scheduled {
        mut order,
        mut cyclic,
    } = schedule(state, inputs, roots, Some(&mut cache), &|node| {
        published_areas(state, inputs, node)
    });
    state.readers_cache = cache;
    order.retain(|node| scope.contains(live_anchor(state, *node)));
    cyclic.retain(|node| scope.contains(live_anchor(state, *node)));
    let mut range_refused = BTreeSet::new();
    if iteration.enabled()
        && !cyclic.is_empty()
        && matches!(&scope, CalcScope::Range(_))
        && selection != EvalSelection::FormulaEntry
    {
        if poststate != RangePostState::StillRequired {
            return Err(iterative_range_refusal());
        }
        // The measured scalar Manual profile includes cached callback readers.
        // A reader is refused only if its executed path requests a refused value.
        if order
            .iter()
            .any(|node| !measured_range_scalar(state, inputs, *node))
        {
            return Err(iterative_range_refusal());
        }
        range_refused.extend(cyclic.iter().copied());
        order.retain(|node| !range_refused.contains(node));
    }
    if cyclic.is_empty()
        && selection.forces_all()
        && let CalcScope::Range(areas) = &scope
    {
        order = range_reference_order(state, inputs, order, areas);
    }
    if cyclic.is_empty()
        && selection.forces_all()
        && scope == CalcScope::Workbook
        && !inputs.definitions().iteration().enabled()
        && inputs.definitions().data_tables().is_empty()
    {
        order = full_observation_order(state, inputs, order);
    }
    let mut progress = Progress::with_capacity(state.nodes.len(), order.len());
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
    state.begin_tick();
    let command = CalcCommandId {
        domain_incarnation: state.contract.workbook.document,
        sequence: state.last_tick,
    };
    let pass = CalcPassId {
        command,
        ordinal: 0,
        reason: PassReason::Initial,
    };
    let admission = AdmissionReceipt {
        destination: state.destination,
        command,
        source: CommandId {
            client_session: command.domain_incarnation,
            sequence: command.sequence,
        },
        pass,
        scope,
        range_post_state: poststate,
        selection,
        admitted: AdmissionFrontier {
            model: revision,
            external_through: state.volatile_input_version,
        },
        predecessor: None,
    };
    let boundary_caches =
        crate::read::BoundarySnapshot::capture(state, inputs, &admission.scope, &range_refused);
    let mut reference_rank = BySlot::new(state.nodes.len(), order.len());
    let segment_scheduling = selection != EvalSelection::FormulaEntry
        && !iteration.enabled()
        && inputs.definitions().data_tables().is_empty()
        && !order.iter().any(|node| {
            state.volatile.contains(node)
                && matches!(
                    state.bindings.get(node),
                    Some(Binding::Placed(_, ArrayRole::Spill))
                )
        });
    let mut segment = 0;
    let mut parallel = false;
    let mut segments = BySlot::new(state.nodes.len(), order.len());
    for (rank, &node) in order.iter().enumerate() {
        reference_rank.insert(node, rank);
        let fence = match state.bindings.get(&node) {
            Some(Binding::Placed(instance, role)) => {
                let allowed = segment_scheduling
                    && matches!(role, ArrayRole::Cell | ArrayRole::Spill)
                    && instance
                        .plan()
                        .parallel_call_limit(&state.callables)
                        .is_some();
                parallel |= allowed && instance.plan().has_user_calls();
                instance.plan().has_user_calls() && !allowed
            }
            Some(Binding::Refused(NotModeled::Compile(calc_fml::CompileError::NotModelled {
                gap: calc_fml::Gap::UnknownFunction { .. },
                ..
            }))) => inputs.definitions().macro_enabled(),
            _ => false,
        };
        if fence {
            segment += 1;
        }
        let own = segment;
        if fence {
            segment += 1;
        }
        segments.insert(node, own);
    }
    Ok(RecalcWork {
        interest: Vec::new(),
        preferred: BTreeSet::new(),
        precedents: if parallel {
            scheduling_precedents(state, inputs, &order)
        } else {
            BTreeMap::new()
        },
        reference_rank,
        segments,
        has_fences: segment != 0,
        segment_scheduling,
        parallel_scheduling: parallel && segment_scheduling,
        preferred_quanta: 0,
        urgent_interest: false,
        service_clock: 0,
        last_service: BySlot::new(state.nodes.len(), order.len()),
        linked_workbooks: state.linked_workbooks.clone(),
        linked_input_version: state.linked_input_version,
        external_input_version: state.contract.external_inputs,
        external_inputs: None,
        async_waits: BTreeMap::new(),
        async_resumed: BTreeSet::new(),
        evaluations: BTreeMap::new(),
        preliminary: BTreeSet::new(),
        semantic_waits: BTreeSet::new(),
        suspended: BTreeMap::new(),
        early_published: BTreeMap::new(),
        publication_fences: false,
        fence_flush: None,
        publication_sequence: 0,
        functions: None,
        references: None,
        rtd_connector: None,
        user_view: None,
        callable_bindings: state.callables.clone(),
        root_reason: if selection == EvalSelection::FormulaEntry {
            ReevaluationReason::FormulaEntry
        } else {
            ReevaluationReason::NewCommandDemand
        },
        boundary_caches,
        admission: admission.clone(),
        pass,
        requirements: state.contract.snapshot(),
        facts_version: state.contract.facts,
        events: vec![CalcEvent::Admitted(admission)],
        reused: 0,
        repairing_certificates: false,
        invalidated_outside: BTreeSet::new(),
        revision,
        generation: state.generation,
        coordinates: state.contract.coordinates,
        calculation: Calculation::new(revision, CalcTick::new(state.last_tick), mode),
        environment: state.environment(),
        random_seed: state.random_seed,
        workbook_facts: state.workbook_facts.clone(),
        volatile_input_version: state.volatile_input_version,
        progress,
        order,
        next: 0,
        ready: VecDeque::new(),
        waiting: BTreeMap::new(),
        waits_on: BTreeMap::new(),
        spills: SpillIndex::default(),
        blocked_spills: BTreeMap::new(),
        filled,
        dynamic,
        rewave: BTreeSet::new(),
        waves: 0,
        sizing: BTreeMap::new(),
        local: EvalLocal::default(),
        failure: None,
        iteration,
        cyclic: BTreeSet::new(),
        range_refused,
        iterates: BTreeMap::new(),
        pass_start: BTreeMap::new(),
        passes: 0,
        iteration_complete: false,
        kept: BTreeSet::new(),
    })
    .map(|mut work| {
        work.add_cyclic(state, cyclic);
        if work.iterating()
            && selection.forces_all()
            && work
                .cyclic
                .iter()
                .all(|node| measured_range_scalar(state, inputs, *node))
        {
            // P14 eng16: direct reference callbacks get a provisional entry before
            // the scalar SCC sweeps, even when the file holds warm seed values.
            for &node in &work.order {
                if work.cyclic.contains(&node) {
                    continue;
                }
                let Some(Binding::Placed(instance, _)) = state.bindings.get(&node) else {
                    continue;
                };
                if !instance.plan().only_cached_user_calls(&state.callables)
                    || instance
                        .plan()
                        .name_bindings()
                        .iter()
                        .any(|proof| proof.resolved.is_some())
                {
                    continue;
                }
                let references = instance.plan().provisional_references(&state.callables);
                if references.iter().any(|&index| {
                    instance.areas()[index].is_ok_and(|area| {
                        work.cyclic
                            .iter()
                            .any(|producer| area.contains(live_anchor(state, *producer)))
                    })
                }) {
                    work.preliminary.insert(node);
                }
            }
            let prefix: Vec<_> = work
                .order
                .iter()
                .copied()
                .filter(|node| work.preliminary.contains(node))
                .collect();
            work.order.splice(0..0, prefix);
        }
        if !work.range_refused.is_empty() {
            work.events.push(CalcEvent::TargetsRefused {
                command: work.admission.command,
                targets: work
                    .range_refused
                    .iter()
                    .map(|node| live_anchor(state, *node))
                    .collect(),
                construct: "cyclic targets in a Manual range calculation",
                owner: "dc-hsa5.11",
            });
        }
        work
    })
}

fn iterative_range_refusal() -> StartError {
    StartError::NotModeledScope {
        construct: "iterative range calculation with circular formulas",
        owner: "dc-hsa5.11",
    }
}

fn dynamic_range_failure() -> RecalcFailure {
    RecalcFailure::NotModeledScope {
        construct: "a dynamic circular reference in an iterative range calculation",
        owner: "dc-hsa5.11",
    }
}

fn mixed_range_spill_failure() -> RecalcFailure {
    RecalcFailure::NotModeledScope {
        construct: "multi-cell spill placement in an iterative mixed range calculation",
        owner: "dc-hsa5.11",
    }
}

/// An edit opens a successor. Candidate reads are checked against the new requirements, not
/// merely against cached values. Effects, volatiles and the legacy session boundaries are not
/// eligible for this pure-result reuse path.
///
/// This entry keeps an exact scope's coordinates only across ordinary cell edits. After an
/// address move, use `supersede_recalc_scoped` with the explicitly transformed scope. A caller
/// expressing cut/move as replacement cell edits must also supply that transformed scope.
pub fn supersede_recalc(
    state: &mut EngineState,
    inputs: &ModelRoot,
    previous: RecalcWork,
    mode: NumericMode,
) -> Result<RecalcWork, StartError> {
    if !matches!(previous.admission.scope, CalcScope::Workbook)
        && previous.coordinates != state.contract.coordinates
    {
        return Err(StartError::ScopeTransformRequired);
    }
    let scope = previous.admission.scope.clone();
    let selection = previous.admission.selection;
    supersede_recalc_scoped(state, inputs, previous, mode, scope, selection)
}

/// Replaces work with an explicitly requested scope/selection. SliceV1 salvages only dirty
/// successors: an inherited or newly requested ForceAll evaluates its pure targets again.
pub fn supersede_recalc_scoped(
    state: &mut EngineState,
    inputs: &ModelRoot,
    mut previous: RecalcWork,
    mode: NumericMode,
    scope: CalcScope,
    selection: EvalSelection,
) -> Result<RecalcWork, StartError> {
    let predecessor = previous.admission.command;
    let mut events = previous.take_events();
    // A newly discovered dynamic read may not have been published into the graph yet.
    // Its certificate still invalidates that candidate and the graph's existing readers.
    let changed: BTreeSet<NodeId> = previous
        .progress
        .staged_results()
        .filter_map(|(node, result)| {
            let stamp = result.stamp()?;
            (!certificate_valid(
                state,
                inputs,
                &previous.progress,
                stamp,
                state.contract.facts,
            ))
            .then_some(node)
        })
        .filter(|node| state.node(*node).is_some())
        .collect();
    if !changed.is_empty() {
        let affected = dirty_closure(state, inputs, changed, &|node| {
            invalidation_areas(state, inputs, node)
        })
        .members()
        .to_vec();
        state.contract.invalidate(affected);
    }
    let poststate = if matches!(scope, CalcScope::Range(_)) && selection.forces_all() {
        previous.admission.range_post_state
    } else {
        RangePostState::Satisfied
    };
    let mut next = begin_with_scope(state, inputs, mode, scope, selection, true, poststate)?;
    if previous.admission.range_post_state == crate::contract::RangePostState::StillRequired
        && matches!(next.admission.scope, CalcScope::Range(_))
        && next.admission.selection.forces_all()
    {
        next = next.with_manual_range_poststate();
    }
    next.admission.predecessor = Some(predecessor);
    let terminal = previous.completion(
        CompletionOutcome::Superseded {
            by: next.admission.command,
        },
        ReadViewId(state.generation),
        None,
    );
    events.push(CalcEvent::Superseded(terminal));
    events.push(CalcEvent::Admitted(next.admission.clone()));
    events.extend(
        next.events
            .drain(..)
            .filter(|event| !matches!(event, CalcEvent::Admitted(_))),
    );
    next.events = events;
    let force = next.admission.selection.forces_all();
    let mut candidates: BTreeMap<NodeId, FormulaResult> = std::mem::take(&mut previous.progress)
        .into_staged()
        .collect();
    let mut ordered = previous.order.clone();
    ordered.extend(
        candidates
            .keys()
            .filter(|node| !previous.order.contains(node))
            .copied(),
    );
    for node in ordered {
        let Some(mut result) = candidates.remove(&node) else {
            continue;
        };
        let Some(stamp) = result.stamp().cloned() else {
            continue;
        };
        let at = stamp.target.locus;
        let eligible = !force
            && previous.failure.is_none()
            && !previous.cyclic.contains(&node)
            && next.progress.is_scheduled(node)
            && !state.volatile.contains(&node)
            && !stamp.reads.legacy_boundary
            && !stamp.reads.uses_boundary_cache
            && !stamp.reads.uses_imported_cache
            && result.calculated().is_some_and(|(_, calculation)| {
                !calculation.held_by_circular_rule()
                    && !calculation.reads_external_links()
                    && calculation.mode() == next.calculation.mode()
            })
            && matches!(state.bindings.get(&node), Some(Binding::Placed(instance, _)) if !instance.plan().has_user_calls());
        let output_region = previous
            .spills
            .get(at)
            .map(Spill::area)
            .or_else(|| previous.blocked_spills.get(&at).copied());
        let output_ok = output_region.is_none_or(|region| {
            // Reuse does not choose a winner against another unresolved spill producer.
            !state.bindings.spill_iter().any(|(&other, binding)| {
                other != node
                    && next.progress.is_scheduled(other)
                    && !next.progress.is_evaluated(other)
                    && matches!(binding, Binding::Placed(_, ArrayRole::Spill))
                    && state.node(other).is_some_and(|record| {
                        record.anchor().sheet() == region.sheet()
                            && record.anchor().row() <= region.last().row()
                            && record.anchor().column() <= region.last().column()
                    })
            })
        });
        if !eligible
            || !output_ok
            || !certificate_valid(state, inputs, &next.progress, &stamp, next.facts_version)
        {
            next.events.push(CalcEvent::Rejected { at });
            continue;
        }
        let computed_by = match &stamp.computed_by {
            ComputationOrigin::Evaluated { root } => root.pass.command,
            _ => {
                next.events.push(CalcEvent::Rejected { at });
                continue;
            }
        };
        let mut certified = stamp;
        certified.certification = FreshnessClaim {
            command: next.admission.command,
            pass: next.pass,
            frontier: next.admission.admitted,
            required: state.contract.requirement(node).required,
        };
        result.set_stamp(certified);
        let done = Done {
            result,
            spill: previous.spills.get(at).cloned(),
            blocked: previous.blocked_spills.get(&at).copied(),
            dynamic: previous.dynamic.get(node).cloned().unwrap_or_default(),
        };
        stage(state, inputs, &mut next, node, done);
        next.reused += 1;
        next.events.push(CalcEvent::Reused {
            at,
            computed_by,
            certified_for: next.admission.command,
        });
    }
    Ok(next)
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
    if work.admission.destination != state.destination {
        work.failure = Some(RecalcFailure::WrongDestination);
    }
    if work.failure.is_none()
        && (state.revision() != work.revision
            || inputs.revision() != work.revision
            || state.generation != work.generation
            || !work.inputs_current(state)
            || !work.callables_current(state)
            || state.environment != work.environment.map(Into::into))
    {
        work.failure = Some(RecalcFailure::Superseded {
            revision: work.revision,
        });
    }
    work.local.quantum = if budget == WorkBudget::UNLIMITED {
        usize::MAX
    } else {
        1024
    };
    #[cfg(not(target_arch = "wasm32"))]
    if budget == WorkBudget::UNLIMITED && parallel::eligible(state, inputs, work, &*observer) {
        parallel::evaluate_levels(state, inputs, work, &mut *observer);
    }
    let mut evaluations = 0;
    while work.failure.is_none() && evaluations < budget.evaluations {
        if work.publication_fences
            && work.has_fences
            && work.segment_scheduling
            && work.waves == 0
            && let Some(segment) = active_segment(work)
            && work.progress.staged_results().any(|(node, result)| {
                work.segments.get(&node).is_some_and(|own| *own < segment)
                    && work.early_published.get(&node) != Some(&result.stamp().cloned())
            })
        {
            work.fence_flush = Some(segment);
            break;
        }
        let Some(node) = next_node(state, inputs, work) else {
            if queue_certificate_repairs(state, inputs, work) {
                continue;
            }
            break;
        };
        evaluations += 1;
        process(state, inputs, work, node, &mut *observer);
        // A newly promoted scalar gets a publication opportunity before another off-interest
        // continuation resumes, even when the caller supplied a larger finite budget.
        if budget != WorkBudget::UNLIMITED
            && work.urgent_interest
            && work.preferred.contains(&node)
            && work.progress.is_evaluated(node)
            && matches!(
                state.bindings.get(&node),
                Some(Binding::Placed(_, ArrayRole::Cell))
            )
        {
            break;
        }
    }
    if work.failure.is_none() && work.remaining() == 0 {
        queue_certificate_repairs(state, inputs, work);
    }
    let progress = work.progress();
    if let RecalcProgress::Pending { remaining } = progress {
        work.events.push(CalcEvent::Yielded {
            pass: work.pass,
            remaining,
        });
    }
    progress
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
    work.service_clock = work
        .service_clock
        .checked_add(1)
        .expect("service age exhausted");
    work.last_service.insert(node, work.service_clock);
    let quantum = work.local.quantum;
    if (work.local.activation.is_some() || work.local.placement.is_some())
        && work.local.current_node.get() != Some(node)
    {
        let previous = work
            .local
            .current_node
            .get()
            .expect("a suspended activation has a target");
        work.suspended
            .insert(previous, std::mem::take(&mut work.local));
    }
    if let Some(local) = work.suspended.remove(&node) {
        work.local = local;
    }
    work.local.quantum = quantum;
    if work.local.placement.is_none()
        && let Some((limit, held)) = parallel_reservations(state, work)
        && !held.contains(&node)
        && matches!(state.bindings.get(&node), Some(Binding::Placed(instance, _)) if instance.plan().has_user_calls())
        && parallel_limit(state, node).is_none_or(|own| held.len() >= limit.min(own))
    {
        let done = refused(
            work,
            NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "a scalar callback admission without an available pool slot",
                owner: "dc-snz.23",
            }),
        );
        stage(state, inputs, work, node, done);
        return;
    }
    let attempt = evaluate_node(state, inputs, work, node, &mut *observer);
    apply(state, inputs, work, node, attempt, observer);
}

/// Retain the finished invocation before admitting a semantic reevaluation.
fn record_reevaluation(
    state: &EngineState,
    work: &mut RecalcWork,
    node: NodeId,
    reason: ReevaluationReason,
) {
    let previous = root_invocation(state, &shared_of!(work), node);
    let invocations = work
        .local
        .effects
        .invocations()
        .iter()
        .map(|record| {
            let mut identity = previous.clone();
            identity.call_site = record.call_site;
            identity.occurrence = record.occurrence.clone();
            CallInvocation {
                identity,
                runtime: record.id,
                parent: record.parent,
                callable: record.callable,
                completed: record.completed,
                non_preemptible: record.non_preemptible,
            }
        })
        .collect();
    work.evaluations.insert(
        node,
        (
            previous
                .evaluation
                .checked_add(1)
                .expect("evaluation ordinal exhausted"),
            reason,
        ),
    );
    let next = root_invocation(state, &shared_of!(work), node);
    work.events.push(CalcEvent::Reevaluated {
        previous,
        next,
        invocations,
    });
    work.local = EvalLocal::default();
}

fn finish_preliminary(work: &mut RecalcWork, node: NodeId) {
    if !work.preliminary.remove(&node) {
        return;
    }
    let position = work.order[..work.next]
        .iter()
        .position(|entry| *entry == node)
        .expect("provisional prefix was selected");
    work.order.remove(position);
    work.next -= 1;
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
        Ok(Attempt::Suspended) => work.ready.push_front(node),
        Ok(Attempt::NeedsOwner) => unreachable!("only a level worker returns NeedsOwner"),
        Ok(Attempt::AwaitExternal) => {
            let local = std::mem::take(&mut work.local);
            let (runtime, request) = local
                .activation
                .as_ref()
                .and_then(calc_fml::EvalContinuation::pending_external)
                .expect("external wait owns a callable frame");
            let record = local
                .effects
                .invocations()
                .iter()
                .find(|r| r.id == runtime)
                .expect("admitted callable");
            let mut invocation = root_invocation(state, &shared_of!(work), node);
            invocation.call_site = record.call_site;
            invocation.occurrence = record.occurrence.clone();
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            let id = NEXT
                .fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |n| n.checked_add(1),
                )
                .expect("async token identity exhausted");
            let token = AsyncToken {
                destination: work.admission.destination,
                id,
                invocation,
                runtime,
                request,
                provider: record.callable,
                target: EvaluationTarget {
                    workbook: state.contract.workbook,
                    origin_revision: work.revision,
                    formula_generation: state.contract.generation(node),
                    node,
                    locus: live_anchor(state, node),
                },
                required: work.requirements.get(&node).map_or(1, |r| r.required),
            };
            work.events.push(CalcEvent::AwaitingAsync(token.clone()));
            work.async_waits.insert(node, token);
            work.suspended.insert(node, local);
        }
        Ok(Attempt::Done(done)) if work.preliminary.contains(&node) => {
            // The provisional entry ends without publishing its temporary value.
            // Its ordinary position remains in the first sweep and has a new identity.
            finish_preliminary(work, node);
            record_reevaluation(state, work, node, ReevaluationReason::ProvisionalArguments);
            if !matches!(done.result, FormulaResult::Calculated { .. }) || done.spill.is_some() {
                work.failure = Some(RecalcFailure::NotModeledScope {
                    construct: "an iterative provisional callback without a scalar result",
                    owner: "dc-hsa5.11",
                });
            }
        }
        Ok(Attempt::Done(done)) => {
            stage(state, inputs, work, node, done);
            if work.async_resumed.remove(&node) {
                work.open_pass(PassReason::AsyncDependents);
            }
        }
        Ok(Attempt::Reevaluate { on, cell, reason }) => {
            finish_preliminary(work, node);
            record_reevaluation(state, work, node, reason);
            work.semantic_waits.insert(node);
            // The previous invocation has ended. Its next evaluation and known consumers
            // follow the unresolved producer and the surviving observation fences. Install
            // that prefix before admitting the new cached view; never certify staged inputs
            // against an older installed frontier.
            let moved: BTreeSet<_> = dirty_closure(state, inputs, [node].into(), &|n| {
                invalidation_areas(state, inputs, n)
            })
            .members()
            .iter()
            .copied()
            .collect();
            let mut ranks: BTreeMap<_, usize> = work
                .segments
                .iter()
                .map(|(node, segment)| ((moved.contains(node), *segment), 0))
                .collect();
            for (rank, value) in ranks.values_mut().enumerate() {
                *value = rank;
            }
            for (node, segment) in work.segments.iter_mut() {
                *segment = ranks[&(moved.contains(node), *segment)];
            }
            observer.deferred(live_anchor(state, node), cell);
            defer(state, inputs, work, node, on);
        }
        Ok(Attempt::Kept) => keep(state, inputs, work, node),
        Ok(Attempt::RangeRefused) => exclude_range_reader(state, work, node),
        Ok(Attempt::Waits { on, cell }) => {
            // A prerequisite before callback entry retires the provisional
            // position too: dynamically discovered SCCs require unique members.
            finish_preliminary(work, node);
            if work.parallel_scheduling && parallel_limit(state, node).is_some() {
                // Static prerequisites were checked before entry. A callback's
                // additional prerequisite cannot consume an unreserved worker,
                // be retried as the same invocation, or be labelled circular.
                work.local.activation.take();
                let done = refused(
                    work,
                    NotModeled::Evaluation(EvalFault::NotModelled {
                        construct: "a dynamic prerequisite inside a concurrent scalar callback",
                        owner: "dc-snz.23",
                    }),
                );
                stage(state, inputs, work, node, done);
                return;
            }
            observer.deferred(live_anchor(state, node), cell);
            defer(state, inputs, work, node, on);
        }
        Err(failure) => work.failure = Some(failure),
    }
}

/// Retains an executed reader's historical result and releases its pure waiters.
/// Refusal does not establish SCC membership or satisfy a target requirement.
fn exclude_range_reader(state: &EngineState, work: &mut RecalcWork, node: NodeId) {
    exclude_range_target(
        state,
        work,
        node,
        "an executed reader of a refused cyclic value in a Manual range calculation",
    );
}

fn exclude_range_target(
    state: &EngineState,
    work: &mut RecalcWork,
    node: NodeId,
    construct: &'static str,
) {
    if !work.range_refused.insert(node) {
        return;
    }
    work.progress.exclude(node);
    work.local = EvalLocal::default();
    work.suspended.remove(&node);
    work.dynamic.remove(node);
    work.filled.remove(node);
    let at = live_anchor(state, node);
    work.spills.remove(at);
    work.blocked_spills.remove(&at);
    work.sizing.remove(&node);
    work.rewave.remove(&node);
    work.semantic_waits.remove(&node);
    work.ready.retain(|held| *held != node);
    if let Some(on) = work.waits_on.remove(&node)
        && let Some(waiters) = work.waiting.get_mut(&on)
    {
        waiters.retain(|held| *held != node);
    }
    if let Some(waiters) = work.waiting.remove(&node) {
        for waiter in waiters {
            if work.waits_on.remove(&waiter) == Some(node)
                && !work.range_refused.contains(&waiter)
                && !work.ready.contains(&waiter)
            {
                work.ready.push_back(waiter);
            }
        }
    }
    work.events.push(CalcEvent::TargetsRefused {
        command: work.admission.command,
        targets: vec![at],
        construct,
        owner: "dc-hsa5.11",
    });
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
            && work.interest.is_empty()
            && !work.has_fences
            && work.suspended.is_empty()
            && work.local.activation.is_none()
            && work.local.placement.is_none()
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
            let readers = state.readers_cache.get(*node, live_anchor(state, *node))?;
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
        let mut locals: Vec<EvalLocal> = (0..state.threads).map(|_| EvalLocal::default()).collect();
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
        let worker = |local: &mut EvalLocal| {
            let mut silent = Unobserved;
            loop {
                let from = cursor.fetch_add(CHUNK, Ordering::Relaxed);
                if from >= positions.len() {
                    break;
                }
                for index in from..(from + CHUNK).min(positions.len()) {
                    let node = order[positions[index]];
                    // A worker has no callable table: user calls run on the owner.
                    let attempt = if matches!(
                        state.bindings.get(&node),
                        Some(Binding::Placed(instance, _)) if instance.plan().has_user_calls()
                    ) {
                        Ok(Attempt::NeedsOwner)
                    } else {
                        evaluate_with(
                            state,
                            inputs,
                            &shared,
                            local,
                            node,
                            None,
                            None,
                            None,
                            &mut silent,
                        )
                    };
                    // Unfinished speculative work is replayed by process on the owner.
                    // Its continuation belongs to this node, never the worker's next one.
                    if !matches!(&attempt, Ok(Attempt::Done(_))) {
                        *local = EvalLocal::default();
                    } else {
                        debug_assert!(local.activation.is_none() && local.placement.is_none());
                    }
                    if attempts[index].set(attempt).is_err() {
                        unreachable!("each formula of a level is taken once");
                    }
                }
            }
        };
        std::thread::scope(|scope| {
            // Thread-bound callable activations stay on their owner. Pure workers create
            // their own scratch on entry; no activation is transferred across threads.
            for _ in 1..locals.len() {
                let worker = &worker;
                scope.spawn(move || {
                    let mut local = EvalLocal::default();
                    worker(&mut local);
                });
            }
            let own = locals.first_mut().expect("at least one thread");
            worker(own);
        });
        attempts
    }
}

/// Install interested scalars, or the completed prefix before an observation fence.
/// Prefix spills install values, ownership and removals in the same publication unit.
/// An unprovable prefix stops the command before any later callback can enter.
pub fn publish_ready(
    state: &mut EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
) -> Option<ValueDelta> {
    if work.admission.destination != state.destination {
        work.failure = Some(RecalcFailure::WrongDestination);
        return None;
    }
    let flush = work.fence_flush;
    if work.failure.is_some()
        || work.preferred.is_empty() && flush.is_none()
        || work.waves != 0
        || !work.rewave.is_empty()
        || work.iterating()
        || !work.cyclic.is_empty()
        || state.revision() != work.revision
        || inputs.revision() != work.revision
        || state.generation != work.generation
        || !work.inputs_current(state)
        || !work.callables_current(state)
        || state.environment != work.environment.map(Into::into)
    {
        return None;
    }
    let in_prefix = |node| {
        flush.is_some_and(|segment| work.segments.get(&node).is_some_and(|own| *own < segment))
    };
    let mut candidates: Vec<_> = work
        .progress
        .staged_results()
        .filter(|(node, _)| work.preferred.contains(node) || in_prefix(*node))
        .map(|(node, _)| node)
        .collect();
    candidates.sort_by_key(|node| work.reference_rank.get(node).copied().unwrap_or(usize::MAX));
    let empty = Progress::new(0);
    let mut changes = Vec::new();
    let mut spill_changes = Vec::new();
    let mut installed = Vec::new();
    let mut blocked_prefix = None;
    for node in candidates {
        let prefix = in_prefix(node);
        let at = live_anchor(state, node);
        if !prefix
            && (inputs.definitions().array(at).is_some()
                || state.spills.get(at).is_some()
                || work.spills.get(at).is_some())
        {
            continue;
        }
        let result = work
            .progress
            .staged(node)
            .expect("candidate came from staged results")
            .clone();
        let stamp = result.stamp();
        if work.early_published.get(&node) == Some(&stamp.cloned()) {
            continue;
        }
        let valid = stamp.is_some_and(|stamp| {
            certificate_valid_at(
                state,
                inputs,
                &empty,
                stamp,
                work.facts_version,
                Some(&work.requirements),
                work.volatile_input_version,
                &|_| false,
            )
        });
        let permitted = if prefix {
            valid || stamp.is_none() && matches!(result, FormulaResult::NotModeled { .. })
        } else {
            valid
                && stamp.is_some_and(|stamp| {
                    !stamp.reads.legacy_boundary
                        && !stamp.reads.external_cache
                        && !stamp.reads.uses_boundary_cache
                        && !stamp.reads.uses_imported_cache
                        && stamp.reads.absences.is_empty()
                })
                && matches!(result, FormulaResult::Calculated { ref calculation, .. } if !calculation.held_by_circular_rule())
        };
        if !permitted {
            if prefix {
                blocked_prefix = Some(at);
            }
            continue;
        }
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
        state.blocked_spills.remove(&at);
        if let Some(extent) = work.blocked_spills.get(&at) {
            state.blocked_spills.insert(at, *extent);
        }
        if let Some(stamp) = stamp {
            state
                .index
                .set_name_bindings(node, stamp.reads.names.clone());
        }
        state.contract.satisfy(node, stamp, work.pass);
        work.early_published.insert(node, stamp.cloned());
        state.cached_arrays.remove(&node);
        if let Some(volatile) = result
            .stamp()
            .and_then(|stamp| stamp.reads.udf_volatility)
            .map(|request| formula_volatile(state, node, request))
        {
            if volatile {
                state.volatile.insert(node);
            } else {
                state.volatile.remove(&node);
            }
        }
        state.publish_result(node, at, result);
        state.pending.remove(&node);
        state.entered.remove(&node);
        if work.admission.range_post_state == crate::contract::RangePostState::StillRequired {
            state.range_redemand.insert(node);
        } else {
            state.range_redemand.remove(&node);
        }
        if let Some(dynamic) = work.dynamic.get(node) {
            state.set_dynamic(node, dynamic.clone());
        }
        installed.push(node);
        changes.push(ValueChange::Published { at, node });
    }
    if changes.is_empty() {
        if let Some(at) = blocked_prefix {
            work.failure = Some(RecalcFailure::InvalidCertificate { at });
        }
        return None;
    }
    if !spill_changes.is_empty() {
        state.spill_epoch = state
            .spill_epoch
            .checked_add(1)
            .expect("occupancy version exhausted");
        for change in &spill_changes {
            for area in change.before.iter().chain(change.after.iter()) {
                state.contract.record_occupancy(*area, state.spill_epoch);
            }
        }
    }
    // Published certificates describe the installed topology. Keep their staged copies
    // identical, so finish neither installs them twice nor certifies old occupancy versions.
    for node in installed {
        if let Some(result) = state.results.of_mut(node) {
            result.rebase_occupancy(|region| state.contract.occupancy(region));
            work.early_published.insert(node, result.stamp().cloned());
            work.progress.stage(node, result.clone());
        }
    }
    if !spill_changes.is_empty() && !work.invalidated_outside.is_empty() {
        let invalidated = std::mem::take(&mut work.invalidated_outside);
        state.contract.invalidate(invalidated.iter().copied());
        state.pending.extend(invalidated);
    }
    if blocked_prefix.is_none() {
        work.fence_flush = None;
    }
    state.advance_view();
    work.generation = state.generation;
    state.published_mode = Some(work.calculation.mode().clone());
    work.publication_sequence = work
        .publication_sequence
        .checked_add(1)
        .expect("publication identity exhausted");
    let unit = PublicationUnitId {
        command: work.admission.command,
        sequence: work.publication_sequence,
    };
    changes.sort_by_key(|change| match change {
        ValueChange::Published { at, .. } | ValueChange::Removed { at, .. } => *at,
    });
    spill_changes.sort_by_key(|change| change.anchor);
    Some(
        ValueDelta::new(work.revision, work.calculation.tick(), changes)
            .with_spill_changes(spill_changes)
            .with_events(vec![CalcEvent::Published(unit)]),
    )
}

/// Publishes a completed recalculation: each evaluated node's staged result replaces its published
/// one, with its spill and its dynamic prerequisites; the results and spills of formulas removed
/// since the last publication are removed, the evaluated nodes are no longer pending, and the
/// published generation's numeric mode is the recalculation's. Returns what changed. A
/// failed, incomplete or superseded work installs no further unit; earlier units stay installed.
pub fn finish(
    state: &mut EngineState,
    inputs: &ModelRoot,
    mut work: RecalcWork,
) -> Result<ValueDelta, RecalcFailure> {
    if work.admission.destination != state.destination {
        return Err(RecalcFailure::WrongDestination);
    }
    if let Some(failure) = work.failure {
        return Err(failure);
    }
    let remaining = work.remaining();
    if remaining > 0 {
        return Err(RecalcFailure::Incomplete { remaining });
    }
    if state.revision() != work.revision
        || inputs.revision() != work.revision
        || state.generation != work.generation
        || !work.inputs_current(state)
        || !work.callables_current(state)
        || state.environment != work.environment.map(Into::into)
    {
        return Err(RecalcFailure::Superseded {
            revision: work.revision,
        });
    }
    for (_, result) in work.progress.staged_results() {
        if let Some(stamp) = result.stamp() {
            if !certificate_valid_at(
                state,
                inputs,
                &work.progress,
                stamp,
                work.facts_version,
                Some(&work.requirements),
                work.volatile_input_version,
                &|_| false,
            ) {
                return Err(RecalcFailure::InvalidCertificate {
                    at: stamp.target.locus,
                });
            }
        }
    }
    let coverage = work.coverage();
    let unit = PublicationUnitId {
        command: work.admission.command,
        sequence: work
            .publication_sequence
            .checked_add(1)
            .expect("publication identity exhausted"),
    };
    // Validation is complete. Reserve the same globally unique view for the installed
    // results and their receipt, even when another destination advanced the sequence.
    state.advance_view();
    let completion = work.completion(
        if coverage.refused == 0 && coverage.held == 0 && coverage.uncertified == 0 {
            CompletionOutcome::Complete
        } else {
            CompletionOutcome::Incomplete
        },
        state.result_view(),
        Some(unit),
    );
    // Each change is of a distinct cell: a published node's anchor, or a retired anchor whose
    // published result no live node there replaced. They are sorted by cell at the end.
    let mut changes: Vec<(CellKey, ValueChange)> = Vec::with_capacity(work.progress.scheduled());
    let mut spill_changes = Vec::new();
    let mut installed = Vec::new();
    let mut published = crate::slots::Slots::new(state.nodes.len(), work.progress.scheduled());
    for (node, result) in std::mem::take(&mut work.progress).into_staged() {
        published.insert(node.slot(), ());
        let at = live_anchor(state, node);
        installed.push(node);
        if work.early_published.get(&node) == Some(&result.stamp().cloned()) {
            continue;
        }
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
        state.blocked_spills.remove(&at);
        if let Some(extent) = work.blocked_spills.get(&at) {
            state.blocked_spills.insert(at, *extent);
        }
        if let Some(stamp) = result.stamp() {
            state
                .index
                .set_name_bindings(node, stamp.reads.names.clone());
        }
        state.contract.satisfy(node, result.stamp(), work.pass);
        state.cached_arrays.remove(&node);
        if let Some(volatile) = result
            .stamp()
            .and_then(|stamp| stamp.reads.udf_volatility)
            .map(|request| formula_volatile(state, node, request))
        {
            if volatile {
                state.volatile.insert(node);
            } else {
                state.volatile.remove(&node);
            }
        }
        state.publish_result(node, at, result);
        changes.push((at, ValueChange::Published { at, node }));
    }
    state
        .range_redemand
        .retain(|node| published.get(node.slot()).is_none());
    if work.admission.range_post_state == crate::contract::RangePostState::StillRequired {
        state.range_redemand.extend(installed.iter().copied());
    }
    // The published nodes are no longer pending, nor entered since the last publication: each set
    // is built anew from the nodes that stay, in node order, which builds it in one pass (dc-dxm),
    // rather than by removing the published nodes one at a time.
    let stays = |node: &NodeId| {
        state.contract.requirement(*node).required != state.contract.requirement(*node).satisfied
            || published.get(node.slot()).is_none()
    };
    state.pending = std::mem::take(&mut state.pending)
        .into_iter()
        .filter(stays)
        .collect();
    if state.volatile_input_version != work.volatile_input_version {
        state.pending.extend(state.volatile.iter().copied());
    }
    state.entered = std::mem::take(&mut state.entered)
        .into_iter()
        .filter(stays)
        .collect();
    for (node, dynamic) in std::mem::take(&mut work.dynamic).into_entries() {
        state.set_dynamic(node, dynamic);
    }
    for at in std::mem::take(&mut state.retired) {
        state.blocked_spills.remove(&at);
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
        state.spill_epoch = state
            .spill_epoch
            .checked_add(1)
            .expect("occupancy version exhausted");
    }
    for change in &spill_changes {
        for area in change.before.iter().chain(change.after.iter()) {
            state.contract.record_occupancy(*area, state.spill_epoch);
        }
    }
    for node in installed {
        if let Some(result) = state.results.of_mut(node) {
            result.rebase_occupancy(|region| state.contract.occupancy(region));
        }
    }
    if !work.invalidated_outside.is_empty() {
        state
            .contract
            .invalidate(work.invalidated_outside.iter().copied());
        state.pending.extend(work.invalidated_outside);
    }
    if matches!(work.admission.scope, CalcScope::Workbook) {
        state.cycle_members = work.cyclic;
    } else {
        // A narrow command cannot discharge the known cyclic demand elsewhere.
        let outside = state
            .cycle_members
            .iter()
            .copied()
            .filter(|node| {
                state.node(*node).is_some()
                    && !work.admission.scope.contains(live_anchor(state, *node))
            })
            .collect::<BTreeSet<_>>();
        state.cycle_members = outside;
        state.cycle_members.extend(work.cyclic);
    }
    state.published_mode = Some(work.calculation.mode().clone());
    spill_changes.sort_by_key(|change| change.anchor);
    changes.sort_by_key(|(at, _)| *at);
    debug_assert!(
        changes.windows(2).all(|pair| pair[0].0 != pair[1].0),
        "one change per cell"
    );
    work.events.push(CalcEvent::Published(unit));
    work.events.push(CalcEvent::Completed(completion.clone()));
    Ok(ValueDelta::new(
        work.revision,
        work.calculation.tick(),
        changes.into_iter().map(|(_, change)| change).collect(),
    )
    .with_spill_changes(spill_changes)
    .with_completion(completion)
    .with_events(work.events))
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
        None => (
            text.form(),
            match text.form() {
                FormulaForm::Legacy => ArrayRole::Cell,
                FormulaForm::Dynamic => ArrayRole::Spill,
            },
        ),
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
    let (source, source_anchor) = match text.block() {
        Some(block) if text.member_at() == Some(at) => (
            block.source().source(),
            FormulaAnchor::new(block.source().anchor()),
        ),
        _ => (text.text(), anchor),
    };
    let decoded: Result<Vec<char>, _> =
        char::decode_utf16(source.utf16_code_units().iter().copied()).collect();
    let name_trace = RefCell::new(Vec::new());
    let binding = match decoded {
        Err(_) => Binding::Refused(NotModeled::FormulaText),
        Ok(chars) => {
            let key = template_key_of_chars(&chars, source_anchor).map(|key| {
                (
                    at.sheet(),
                    calc_doc_model::SemanticTemplateKey {
                        normalized_ir: key.into(),
                        placement: calc_doc_model::TemplatePlacement::Cell,
                        evaluation: form,
                    },
                )
            });
            match key
                .as_ref()
                .and_then(|key| state.templates.get(key))
                .filter(|plan| {
                    crate::contract::name_bindings_valid(inputs, plan.name_bindings())
                        && plan
                            .callable_bindings()
                            .iter()
                            .all(|proof| state.callables.validates(proof))
                }) {
                Some(plan) => Binding::Placed(instantiate(plan, anchor), role),
                None => match compile_loaded_with_bindings(
                    &parse_formula(&chars.iter().collect::<String>()),
                    ReferenceNotation::A1,
                    source_anchor,
                    &SnapshotCatalog::traced(inputs, &name_trace),
                    &mut state.plans,
                    form,
                    &state.callables,
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
    let mut prerequisites = match &binding {
        Binding::Placed(instance, _) => {
            let plan = instance.plan();
            if plan.volatile() {
                state.volatile.insert(node);
            }
            if plan.calls().any(|target| target.traits().is_subtotal) {
                state.subtotal_cells.insert(at);
            }
            prerequisites(instance)
        }
        Binding::Refused(_) | Binding::DataTable(_) => Vec::new(),
    };
    let proofs = match &binding {
        Binding::Placed(instance, _) => instance.plan().name_bindings().to_vec(),
        _ => name_trace.into_inner(),
    };
    for bound in proofs.iter().filter_map(|proof| proof.resolved) {
        let edge = Prerequisite::Name(bound.id);
        if !prerequisites.contains(&edge) {
            prerequisites.push(edge);
        }
    }
    state.index.set_name_bindings(node, proofs);
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

/// What a placed formula reads: distinct placed references and bound definition identities.
fn prerequisites(instance: &FormulaInstance) -> Vec<Prerequisite> {
    let mut out: Vec<Prerequisite> = Vec::with_capacity(instance.areas().len());
    for proof in instance.plan().name_bindings() {
        if let Some(bound) = proof.resolved {
            let edge = Prerequisite::Name(bound.id);
            if !out.contains(&edge) {
                out.push(edge);
            }
        }
    }
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
    slots: usize,
    /// The members, in ID order.
    members: Vec<NodeId>,
    /// Where each member's readers lie in `edges` (start and count), at the member's slot; no
    /// readers at any other slot.
    readers: crate::slots::Slots<(u32, u32)>,
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
        let &(start, count) = self.readers.get(node.slot()).expect("a closure member");
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
/// Unknown dynamic-array shapes may reach any currently blank cell southeast of the anchor.
/// This widens invalidation/demand only; dependency ordering still uses actual graph edges.
pub(crate) fn invalidation_areas(
    state: &EngineState,
    inputs: &ModelRoot,
    node: NodeId,
) -> Vec<GridArea> {
    let mut areas = published_areas(state, inputs, node);
    let at = live_anchor(state, node);
    if inputs
        .definitions()
        .array(at)
        .is_some_and(|array| matches!(array.kind(), ArrayKind::Dynamic { .. }))
    {
        let end = CellKey::new(at.sheet(), calc_value::MAX_ROWS, calc_value::MAX_COLUMNS)
            .expect("the grid corner");
        let reach = GridArea::new(at, end).expect("a southeast area");
        if !areas.contains(&reach) {
            areas.push(reach);
        }
    }
    areas
}

pub(crate) fn dirty_closure(
    state: &EngineState,
    inputs: &ModelRoot,
    roots: BTreeSet<NodeId>,
    areas: &dyn Fn(NodeId) -> Vec<GridArea>,
) -> Closure {
    closure_with(state, inputs, &roots, None, areas)
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
    roots: &BTreeSet<NodeId>,
    mut cache: Option<&mut ReadersCache>,
    areas: &dyn Fn(NodeId) -> Vec<GridArea>,
) -> Closure {
    let slots = state.nodes.len();
    let mut member = crate::slots::Slots::new(slots, roots.len());
    let mut readers = crate::slots::Slots::new(slots, roots.len());
    let mut edges: Vec<NodeId> = Vec::new();
    let mut members: Vec<NodeId> = Vec::with_capacity(roots.len());
    for node in roots {
        member.insert(node.slot(), ());
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
            let slot = reader.slot();
            if member.get(slot).is_none() {
                member.insert(slot, ());
                members.push(reader);
                stack.push(reader);
            }
        }
        let start = u32::try_from(edges.len()).expect("fewer than 2^32 edges in a closure");
        let count = u32::try_from(found.len()).expect("fewer than 2^32 readers of a node");
        readers.insert(node.slot(), (start, count));
        edges.extend_from_slice(&found);
    }
    members.sort_unstable();
    Closure {
        slots,
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
                    let closure = dirty_closure(state, inputs, roots, areas);
                    let fresh = order(
                        &closure,
                        |node| live_anchor(state, node),
                        |node| live_anchor(state, node),
                    );
                    debug_assert_eq!(fresh.as_ref(), Ok(kept), "the kept order stands");
                }
                return acyclic(kept.clone());
            }
            let closure = closure_with(state, inputs, &roots, Some(&mut *cache), areas);
            let ordered = order(
                &closure,
                |node| cache.rank(node),
                |node| live_anchor(state, node),
            );
            if every && let Ok(kept) = &ordered {
                cache.full_order = Some(kept.clone());
                cache.full_levels = std::sync::OnceLock::new();
            }
            (closure, ordered)
        }
        None => {
            let closure = dirty_closure(state, inputs, roots, areas);
            let ordered = order(
                &closure,
                |node| live_anchor(state, node),
                |node| live_anchor(state, node),
            );
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
                Err(_) => order_with_cycles(state, inputs, &placed),
            }
        }
    }
}

/// The same graph's temporary integer indices for Tarjan, used by initial
/// ordering and by a dynamic edge that joins an existing cyclic component.
fn indexed_readers(readers_of: &BTreeMap<NodeId, Vec<NodeId>>) -> (Vec<NodeId>, Vec<Vec<usize>>) {
    let nodes: Vec<NodeId> = readers_of.keys().copied().collect();
    let index_of: BTreeMap<NodeId, usize> =
        nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let edges: Vec<Vec<usize>> = nodes
        .iter()
        .map(|node| readers_of[node].iter().map(|r| index_of[r]).collect())
        .collect();
    (nodes, edges)
}

/// The nodes of `readers_of` with its cycles: each strongly connected component (found by an
/// iterative Tarjan, ported from OxCalc's `cycle_groups`) in the order of the ready queue over
/// the components, a component ready once every component it reads is ordered and the one whose
/// first anchor comes first ordered next, its members in cell order; and the members of the
/// components that are cycles (more than one node, or a node that reads itself).
fn order_with_cycles(
    state: &EngineState,
    inputs: &ModelRoot,
    readers_of: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Scheduled {
    let (nodes, edges) = indexed_readers(readers_of);
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
        if held.len() > 1 {
            sort_members(state, inputs, &mut held);
        }
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

/// Sorts the members of a cycle into the order Excel evaluates them in (dc-86f, Excel 16.0.20527,
/// 2026-10-30, tests/fixtures/dc_86f): their sheets by name, letter case ignored (a sheet named
/// b before one named C, and A before b, whatever their tab order, sheet IDs or the order they
/// were made in), then by row, then by column. The order the formulas were entered in and the
/// order of the file's saved calc chain play no part. Names that fold equal keep their workbook
/// order.
fn sort_members(state: &EngineState, inputs: &ModelRoot, members: &mut [NodeId]) {
    let mut sheets: Vec<(String, usize, calc_value::SheetId)> = inputs
        .sheets()
        .iter()
        .enumerate()
        .map(|(position, sheet)| {
            let name = sheet.name().text().to_string_lossy();
            (calc_value::fold_workbook_name(&name), position, sheet.id())
        })
        .collect();
    sheets.sort();
    let rank: BTreeMap<calc_value::SheetId, usize> = sheets
        .iter()
        .enumerate()
        .map(|(rank, (_, _, id))| (*id, rank))
        .collect();
    members.sort_by_key(|member| {
        let at = live_anchor(state, *member);
        (
            rank.get(&at.sheet()).copied().unwrap_or(usize::MAX),
            at.row(),
            at.column(),
        )
    });
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

/// P08's seeded Full profile: independent pure work precedes cached observers.
/// Callback order stays fixed; other callbacks remain barriers to moving pure work.
fn full_observation_order(
    state: &EngineState,
    inputs: &ModelRoot,
    previous: Vec<NodeId>,
) -> Vec<NodeId> {
    if !previous.iter().any(|node| {
        matches!(state.bindings.get(node), Some(Binding::Placed(instance, _))
        if instance.plan().only_cached_user_calls(&state.callables))
    }) {
        return previous;
    }
    let mut segment = 0usize;
    let mut observers = false;
    let keys: BTreeMap<_, _> = previous
        .iter()
        .enumerate()
        .map(|(rank, node)| {
            let (observer, barrier) = match state.bindings.get(node) {
                Some(Binding::Placed(instance, _)) => {
                    let observer = instance.plan().only_cached_user_calls(&state.callables);
                    (
                        observer,
                        instance.plan().has_user_calls()
                            && !observer
                            && !instance.plan().permits_early_execution(&state.callables),
                    )
                }
                Some(Binding::Refused(NotModeled::Compile(
                    calc_fml::CompileError::NotModelled {
                        gap: calc_fml::Gap::UnknownFunction { .. },
                        ..
                    },
                ))) => (false, inputs.definitions().macro_enabled()),
                _ => (false, false),
            };
            observers |= observer;
            if barrier {
                segment += 1;
            }
            let key = (segment, observer, rank);
            if barrier {
                segment += 1;
            }
            (*node, key)
        })
        .collect();
    if !observers {
        return previous;
    }
    let closure = dirty_closure(state, inputs, previous.iter().copied().collect(), &|node| {
        published_areas(state, inputs, node)
    });
    order(
        &closure,
        |node| keys[&node],
        |node| live_anchor(state, node),
    )
    .unwrap_or(previous)
}

/// P01/P03: requested areas order independent work and eligible provisional callbacks.
/// Pure dependencies and expression arguments still precede their consumers.
fn range_reference_order(
    state: &EngineState,
    inputs: &ModelRoot,
    previous: Vec<NodeId>,
    areas: &[GridArea],
) -> Vec<NodeId> {
    let members: BTreeSet<_> = previous.iter().copied().collect();
    let mut readers = crate::slots::Slots::new(state.nodes.len(), members.len());
    let mut edges = Vec::new();
    let mut found = Vec::new();
    let provisional: BTreeMap<_, _> = previous
        .iter()
        .filter_map(|node| {
            let Binding::Placed(instance, _) = state.bindings.get(node)? else {
                return None;
            };
            instance
                .plan()
                .name_bindings()
                .iter()
                .all(|proof| proof.resolved.is_none())
                .then(|| {
                    (
                        *node,
                        instance.plan().provisional_references(&state.callables),
                    )
                })
        })
        .collect();
    for &producer in &previous {
        let footprint = published_areas(state, inputs, producer);
        readers_into(state, inputs, producer, &footprint, &mut found);
        let first = edges.len();
        edges.extend(
            found
                .iter()
                .copied()
                .filter(|reader| members.contains(reader))
                .filter(|reader| {
                    let Some(indices) = provisional
                        .get(reader)
                        .filter(|indices| !indices.is_empty())
                    else {
                        return true;
                    };
                    let Some(Binding::Placed(instance, _)) = state.bindings.get(reader) else {
                        return true;
                    };
                    let mut direct = false;
                    for (index, area) in instance.areas().iter().enumerate() {
                        if area.is_ok_and(|area| {
                            area.contains(live_anchor(state, producer))
                                || footprint
                                    .iter()
                                    .any(|filled| crate::contract::meets(area, *filled))
                        }) {
                            if !indices.contains(&index) {
                                return true;
                            }
                            direct = true;
                        }
                    }
                    !direct
                }),
        );
        readers.insert(
            producer.slot(),
            (
                u32::try_from(first).expect("bounded edges"),
                u32::try_from(edges.len() - first).expect("bounded edges"),
            ),
        );
    }
    let closure = Closure {
        slots: state.nodes.len(),
        members: previous.clone(),
        readers,
        edges,
    };
    order(
        &closure,
        |node| {
            let at = live_anchor(state, node);
            (
                areas
                    .iter()
                    .position(|area| area.contains(at))
                    .unwrap_or(usize::MAX),
                at,
            )
        },
        |node| live_anchor(state, node),
    )
    .unwrap_or(previous)
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
    let mut waiting = crate::slots::Slots::new(closure.slots, closure.members().len());
    for node in closure.members() {
        waiting.insert(node.slot(), 0u32);
    }
    for node in closure.members() {
        for reader in closure.readers(*node) {
            *waiting.get_mut(reader.slot()).expect("a closure reader") += 1;
        }
    }
    let mut ready: BinaryHeap<Reverse<(K, NodeId)>> = closure
        .members()
        .iter()
        .filter(|node| waiting.get(node.slot()) == Some(&0))
        .map(|node| Reverse((key(*node), *node)))
        .collect();
    let mut order = Vec::with_capacity(closure.members().len());
    while let Some(Reverse((_, node))) = ready.pop() {
        order.push(node);
        for &reader in closure.readers(node) {
            let count = waiting.get_mut(reader.slot()).expect("a closure reader");
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
            .filter(|node| waiting.get(node.slot()).is_some_and(|count| *count > 0))
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
        if work.is_waiting_external() {
            return None;
        }
        if let Some(node) = interested_node(state, work) {
            return Some(node);
        }
        if work.segment_scheduling
            && (work.has_fences || !work.preferred.is_empty())
            && work.waves == 0
            && work.cyclic.is_empty()
            && let Some(segment) = active_segment(work)
        {
            let blocked: Vec<_> = work
                .waits_on
                .iter()
                .filter_map(|(&node, &on)| {
                    (work.segments.get(&node) == Some(&segment)
                        && work.segments.get(&on).is_some_and(|later| *later > segment))
                    .then_some(node)
                })
                .collect();
            if !blocked.is_empty() {
                for node in blocked {
                    if let Some(on) = work.waits_on.remove(&node)
                        && let Some(waiters) = work.waiting.get_mut(&on)
                    {
                        waiters.retain(|n| *n != node);
                    }
                    let done = refused(
                        work,
                        NotModeled::Evaluation(EvalFault::NotModelled {
                            construct: "a dynamic prerequisite across an observation fence",
                            owner: "dc-hsa5.4",
                        }),
                    );
                    stage(state, inputs, work, node, done);
                }
                continue;
            }
            let mut stuck: Vec<_> = work
                .waits_on
                .keys()
                .copied()
                .filter(|node| work.segments.get(node) == Some(&segment))
                .collect();
            if !stuck.is_empty() {
                sort_members(state, inputs, &mut stuck);
                for node in stuck {
                    if let Some(on) = work.waits_on.remove(&node)
                        && let Some(waiters) = work.waiting.get_mut(&on)
                    {
                        waiters.retain(|n| *n != node);
                    }
                    keep(state, inputs, work, node);
                }
                continue;
            }
        }
        if let Some(node) = work.ready.pop_front() {
            if work.range_refused.contains(&node) {
                continue;
            }
            return Some(node);
        }
        if work.next < work.order.len() {
            let node = work.order[work.next];
            work.next += 1;
            if work.range_refused.contains(&node)
                || work.progress.is_evaluated(node)
                || work.waits_on.contains_key(&node)
            {
                continue;
            }
            return Some(node);
        }
        if !work.waits_on.is_empty() {
            // Each waits on a formula that waits too: a cycle only the evaluations found.
            let stuck: Vec<NodeId> = work.waits_on.keys().copied().collect();
            work.waits_on.clear();
            work.waiting.clear();
            if work.iterative_range() {
                // Preserve the historical values needed by a later ordinary
                // calculation; a refusal result would overwrite those seeds.
                work.failure = Some(dynamic_range_failure());
                return None;
            } else if work.iteration.enabled() {
                let mut stuck = stuck;
                sort_members(state, inputs, &mut stuck);
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

/// Static prerequisites shared by viewport priority and scalar pool admission.
fn scheduling_precedents(
    state: &EngineState,
    inputs: &ModelRoot,
    order: &[NodeId],
) -> BTreeMap<NodeId, Vec<NodeId>> {
    let members: BTreeSet<_> = order.iter().copied().collect();
    let mut producers: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    let mut readers = Vec::new();
    for &node in order {
        let footprint = published_areas(state, inputs, node);
        readers_into(state, inputs, node, &footprint, &mut readers);
        for &reader in &readers {
            if members.contains(&reader) {
                producers.entry(reader).or_default().push(node);
            }
        }
    }
    producers
}

fn parallel_limit(state: &EngineState, node: NodeId) -> Option<usize> {
    match state.bindings.get(&node)? {
        Binding::Placed(instance, ArrayRole::Cell | ArrayRole::Spill) => {
            instance.plan().parallel_call_limit(&state.callables)
        }
        _ => None,
    }
}

/// Reserve a place from the cell's first suspended instruction through its
/// completion, including pure prefixes before a native call. Resuming a prefix
/// cannot start an additional call outside the admitted capacity.
fn parallel_reservations(
    state: &EngineState,
    work: &RecalcWork,
) -> Option<(usize, BTreeSet<NodeId>)> {
    if !work.parallel_scheduling {
        return None;
    }
    let mut held = BTreeMap::new();
    for (node, local) in work
        .suspended
        .iter()
        .map(|(node, local)| (Some(*node), local))
        .chain(std::iter::once((
            work.local.current_node.get(),
            &work.local,
        )))
    {
        if local.activation.is_some()
            && let Some(node) = node
            && !work.progress.is_evaluated(node)
            && let Some(limit) = parallel_limit(state, node)
        {
            held.insert(node, limit);
        }
    }
    Some((*held.values().min()?, held.into_keys().collect()))
}

/// Choose one quantum without moving admitted reference ranks. Off-interest
/// work gets the eighth contested quantum; pool cells share by service age.
fn interested_node(state: &EngineState, work: &mut RecalcWork) -> Option<NodeId> {
    let parallel = parallel_reservations(state, work);
    let completed_placement = work.parallel_scheduling
        && (work.local.placement.is_some()
            || work
                .suspended
                .values()
                .any(|local| local.placement.is_some()));
    if !work.segment_scheduling
        || (!work.has_fences
            && work.preferred.is_empty()
            && parallel.is_none()
            && !completed_placement)
        || work.waves != 0
        || !work.cyclic.is_empty()
    {
        return None;
    }
    let segment = active_segment(work)?;
    // Pure native computation may overlap. Placement still waits for earlier
    // array reservations in place_value, without repeating the completed call.
    let is_array = |node: NodeId| {
        matches!(
            state.bindings.get(&node),
            Some(Binding::Placed(
                _,
                ArrayRole::LegacyFill(_) | ArrayRole::Spill
            ))
        )
    };
    let first_array = work
        .order
        .iter()
        .copied()
        .filter(|node| is_array(*node) && !work.progress.is_evaluated(*node))
        .filter(|node| !work.waits_on.contains_key(node))
        .min_by_key(|node| work.reference_rank.get(node).copied().unwrap_or(usize::MAX));
    let candidates = work
        .ready
        .iter()
        .copied()
        .chain(work.order[work.next..].iter().copied())
        .filter(|node| !work.progress.is_evaluated(*node) && !work.waits_on.contains_key(node))
        .filter(|node| {
            work.precedents
                .get(node)
                .into_iter()
                .flatten()
                .all(|producer| work.progress.is_evaluated(*producer))
        })
        .filter(|node| work.segments.get(node) == Some(&segment))
        .filter(|node| {
            let Some((limit, held)) = &parallel else {
                return true;
            };
            if held.contains(node)
                || work
                    .suspended
                    .get(node)
                    .is_some_and(|local| local.placement.is_some())
                || work.local.current_node.get() == Some(*node) && work.local.placement.is_some()
            {
                return true;
            }
            match state.bindings.get(node) {
                Some(Binding::Placed(instance, _)) if instance.plan().has_user_calls() => {
                    parallel_limit(state, *node).is_some_and(|own| held.len() < (*limit).min(own))
                }
                _ => true,
            }
        })
        .filter(|node| {
            !is_array(*node)
                || first_array == Some(*node)
                || work.parallel_scheduling && parallel_limit(state, *node).is_some()
        });
    let mut preferred = None;
    let mut other = None;
    for node in candidates {
        let rank = work
            .reference_rank
            .get(&node)
            .copied()
            .unwrap_or(usize::MAX);
        if work.preferred.contains(&node) {
            let age = if parallel.is_some() || completed_placement {
                work.last_service.get(&node).copied().unwrap_or(0)
            } else {
                0
            };
            if preferred.is_none_or(|held: (u64, usize, NodeId)| (age, rank, node) < held) {
                preferred = Some((age, rank, node));
            }
        } else {
            let age = if work.preferred.is_empty() && parallel.is_none() && !completed_placement {
                0
            } else {
                work.last_service.get(&node).copied().unwrap_or(0)
            };
            if other.is_none_or(|held: (u64, usize, NodeId)| (age, rank, node) < held) {
                other = Some((age, rank, node));
            }
        }
    }
    let node = match (preferred, other) {
        (Some(_), Some((_, _, node))) if work.preferred_quanta >= 7 => {
            work.preferred_quanta = 0;
            node
        }
        (Some((_, _, node)), Some(_)) => {
            work.preferred_quanta += 1;
            node
        }
        (Some((_, _, node)), None) => {
            work.preferred_quanta = 0;
            node
        }
        (None, Some((_, _, node))) => {
            work.preferred_quanta = 0;
            node
        }
        (None, None) => return None,
    };
    if let Some(position) = work.ready.iter().position(|held| *held == node) {
        work.ready.remove(position);
    } else {
        let position = work.order[work.next..]
            .iter()
            .position(|held| *held == node)?
            + work.next;
        work.order.swap(work.next, position);
        work.next += 1;
    }
    Some(node)
}

fn active_segment(work: &RecalcWork) -> Option<usize> {
    work.segments
        .iter()
        .filter(|(node, _)| {
            !work.progress.is_evaluated(**node)
                && !(work.semantic_waits.contains(node) && work.waits_on.contains_key(node))
        })
        .map(|(_, segment)| *segment)
        .min()
}

/// Ends a pass of an iterating recalculation: when it changed an evaluated formula's value
/// by the maximum change or more and fewer passes than the maximum have run, begins the next
/// pass, every node of the order no longer evaluated (its staged result stands until it is
/// evaluated again), and returns true.
fn next_pass(work: &mut RecalcWork) -> bool {
    if work.iteration_complete {
        return false;
    }
    work.passes += 1;
    let change = work
        .order
        .iter()
        .filter_map(|node| {
            let before = work.pass_start.get(node).and_then(Option::as_ref);
            let after = work
                .progress
                .staged(*node)
                .and_then(FormulaResult::calculated)
                .map(|(value, _)| value);
            // P14 measures scalar downstream changes. Array/spill repair keeps
            // its separate wave boundary rather than extending the scalar sweep.
            if !work.cyclic.contains(node)
                && (work.filled.get(*node).is_some_and(Option::is_some)
                    || after.is_none_or(|value| {
                        value.as_array().is_some() || value.as_reference().is_some()
                    }))
            {
                // Preserve the existing non-scalar boundary at MaxChange zero.
                Some(0.0)
            } else {
                change_between(before, after)
            }
        })
        .reduce(f64::max);
    if work.passes >= work.iteration.max_iterations()
        || change.is_none_or(|change| change < work.iteration.max_change())
    {
        work.iteration_complete = true;
        return false;
    }
    work.open_pass(PassReason::IterationSweep(work.passes));
    work.pass_start = work
        .progress
        .staged_results()
        .map(|(node, result)| (node, result.calculated().map(|(value, _)| value.clone())))
        .collect();
    for node in &work.order {
        work.progress.schedule(*node);
    }
    work.next = 0;
    true
}

/// P14's text, error and logical outputs supply no numeric stopping sample.
/// A numeric output uses its prior number, or zero for a prior text/error/logical.
/// Eng17's positive/negative eighth controls distinguish this from TRUE-to-one coercion.
/// No samples stops even at MaxChange zero; a numeric zero change still uses the
/// strict threshold comparison. Array and missing outcomes retain their prior rule.
/// Other result shapes retain the conservative unequal-value rule.
fn change_between(before: Option<&CalcValue>, after: Option<&CalcValue>) -> Option<f64> {
    Some(match (before, after) {
        (_, Some(b))
            if b.as_text().is_some() || b.as_error().is_some() || b.as_logical().is_some() =>
        {
            return None;
        }
        (Some(a), Some(b)) => match (a.as_number(), b.as_number()) {
            (Some(x), Some(y)) => (x - y).abs(),
            (None, Some(y))
                if a.as_text().is_some() || a.as_error().is_some() || a.as_logical().is_some() =>
            {
                y.abs()
            }
            _ if a == b => 0.0,
            _ => f64::INFINITY,
        },
        (None, None) => 0.0,
        _ => f64::INFINITY,
    })
}

/// Begins the next wave: the formulas to calculate again and their dirty closure, ordered, each
/// no longer evaluated (its staged result and spill stand until it is evaluated again).
fn next_wave(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork) {
    // A repaired footprint/topology can change a previously converged value. Keep the
    // existing repair-wave policy, including another stopping decision, across owner yields.
    work.iteration_complete = false;
    let roots = std::mem::take(&mut work.rewave);
    work.waves += 1;
    let reason = if std::mem::take(&mut work.repairing_certificates) {
        PassReason::DependencyRepair
    } else {
        PassReason::SpillShape
    };
    work.open_pass(reason);
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
        Ok(Scheduled {
            mut order,
            mut cyclic,
        }) => {
            work.invalidated_outside.extend(
                order
                    .iter()
                    .copied()
                    .filter(|node| !work.admission.scope.contains(live_anchor(state, *node))),
            );
            order.retain(|node| {
                work.admission.scope.contains(live_anchor(state, *node))
                    && !work.range_refused.contains(node)
            });
            cyclic.retain(|node| work.admission.scope.contains(live_anchor(state, *node)));
            if work.iterative_range()
                && cyclic.iter().any(|node| !work.range_refused.contains(node))
            {
                work.failure = Some(dynamic_range_failure());
                return;
            }
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
            for node in roots
                .into_iter()
                .filter(|node| !work.range_refused.contains(node))
                .collect::<Vec<_>>()
            {
                work.progress.schedule(node);
                let done = refused(work, refusal.clone());
                stage(state, inputs, work, node, done);
            }
            work.rewave.clear();
        }
    }
}

/// Leaves `node` uncalculated, as Excel leaves a formula on a circular reference, or one that reads
/// such a formula, while iterative calculation is off (dc-10y, dc-y2o.1; see the module
/// documentation): it joins the formulas every recalculation calculates again, and its cell keeps
/// what it holds, published as the circular-reference rule's value
/// ([`Calculation::held_by_circular_rule`]) by this recalculation. A formula entered since its last
/// result shows 0; one this engine calculated before keeps that value; one it never calculated
/// keeps the value the file stored for it, and an empty text when the file stored none (Excel
/// 16.0.20527, 2026-10-30, tests/fixtures/dc_y2o: stored 77 and 55, 999 in a reader, "abc", #N/A
/// stay after a full rebuild, and a formula stored without a value holds an empty text, which
/// Excel saves as `t="str"`). A formula whose result spilled, or the anchor of a legacy array
/// formula over several cells that this engine has not calculated, is refused as not modelled.
fn keep(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork, node: NodeId) {
    work.kept.insert(node);
    work.add_cyclic(state, [node]);
    let at = live_anchor(state, node);
    let rule = work.calculation.clone().with_circular_rule();
    let held = if state.entered.contains(&node) {
        Some((CalcValue::number(0.0), rule))
    } else if state.spills.get(at).is_some() {
        None
    } else {
        match state.result(at) {
            Some(FormulaResult::Calculated { value, calculation }) => {
                let rule = if calculation.reads_external_links() {
                    rule.with_external_links()
                } else {
                    rule
                };
                Some((value.clone(), rule))
            }
            Some(FormulaResult::NotModeled { .. }) => None,
            _ if inputs
                .definitions()
                .array(at)
                .is_some_and(|array| array.area().cell_count() > 1) =>
            {
                None
            }
            Some(FormulaResult::FileCached(cache)) => Some((cache.value().to_value(), rule)),
            None => Some((
                CalcValue::text(calc_value::ExcelText::from_utf16_code_units(Vec::new())),
                rule,
            )),
        }
    };
    let done = match held {
        Some((value, calculation)) => Done {
            result: FormulaResult::Calculated { value, calculation },
            spill: None,
            blocked: None,
            dynamic: Vec::new(),
        },
        None => refused(work, circular()),
    };
    stage(state, inputs, work, node, done);
}

/// The refusal of a formula on a circular reference while iterative calculation is off whose held
/// value this engine does not model: one whose result spilled, or a legacy array formula over
/// several cells it never calculated.
fn circular() -> NotModeled {
    NotModeled::Evaluation(EvalFault::NotModelled {
        construct: "a spilled or multi-cell array formula on a circular reference with iterative calculation off",
        owner: "dc-y2o",
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
        blocked: None,
        dynamic: Vec::new(),
    }
}

/// Makes `node` wait for `on`, or, when `on` waits, directly or through others, for `node`, a
/// cycle only the evaluations found: with iteration on its formulas join the formulas on a cycle
/// and are evaluated again at once, and with iteration off `node` is staged as not modelled.
fn defer(state: &EngineState, inputs: &ModelRoot, work: &mut RecalcWork, node: NodeId, on: NodeId) {
    if work.iteration.enabled() && work.cyclic.contains(&node) {
        // An existing cyclic seed can let a static reader finish before the
        // reverse dynamic edge exposes their larger SCC (P14 B7). Find that SCC
        // using the same graph and Tarjan as ordinary ordering, with this pending
        // edge included. Do not admit participants outside the current scope.
        let closure = dirty_closure(state, inputs, BTreeSet::from([node]), &|member| {
            published_areas(state, inputs, member)
        });
        if closure.members().binary_search(&on).is_ok() {
            let mut readers = closure.to_map();
            readers
                .get_mut(&on)
                .expect("the awaited node is in the closure")
                .push(node);
            let (nodes, edges) = indexed_readers(&readers);
            let components = tarjan(&edges);
            let group = components[nodes.binary_search(&node).expect("the closure root")];
            let chain: Vec<_> = nodes
                .iter()
                .enumerate()
                .filter_map(|(i, node)| (components[i] == group).then_some(*node))
                .collect();
            if chain.iter().all(|node| work.progress.is_scheduled(*node)) {
                resume_cycle(state, inputs, work, chain);
                return;
            }
        }
    }
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
            if refuse_closed_range_chain(state, inputs, work, &chain) {
                return;
            }
            resume_cycle(state, inputs, work, chain);
            return;
        }
        match work.waits_on.get(&cursor) {
            Some(next) => cursor = *next,
            None => break,
        }
    }
    if work.local.activation.is_some() || work.local.placement.is_some() {
        work.admit_waiting_prerequisites(state, inputs, node, on);
    }
    if work.preferred.contains(&node) && work.progress.is_scheduled(on) {
        work.preferred.insert(on);
    }
    work.waits_on.insert(node, on);
    work.waiting.entry(on).or_default().push(node);
}

/// Existing scalar Range admission, shared by static and closed-wait discovery.
fn pure_range_scalar(state: &EngineState, inputs: &ModelRoot, node: NodeId) -> bool {
    measured_range_scalar(state, inputs, node)
        && !matches!(state.bindings.get(&node), Some(Binding::Placed(instance, _)) if instance.plan().has_user_calls())
}

fn measured_range_scalar(state: &EngineState, inputs: &ModelRoot, node: NodeId) -> bool {
    let at = live_anchor(state, node);
    !matches!(state.bindings.get(&node), Some(Binding::DataTable(_)))
        && !matches!(state.bindings.get(&node), Some(Binding::Placed(instance, _))
            if instance.plan().has_user_calls() && !instance.plan().only_cached_user_calls(&state.callables))
        && !inputs
            .definitions()
            .array(at)
            .is_some_and(|array| array.area().cell_count() > 1)
        && !state
            .spills
            .get(at)
            .is_some_and(|spill| spill.area().cell_count() > 1)
        && !state
            .cached_arrays
            .get(&node)
            .is_some_and(|spill| spill.area().cell_count() > 1)
}

/// An actual closed wait chain proves these producers cyclic. For the measured
/// pure first-wave Manual Range profile, retain them and let independent work
/// continue. Graph-based guesses and later repair waves keep their typed refusal.
fn refuse_closed_range_chain(
    state: &EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
    chain: &[NodeId],
) -> bool {
    if !work.iterative_range()
        || work.admission.range_post_state != RangePostState::StillRequired
        || work.waves != 0
        || work.passes != 0
        || work.has_fences
        || !work.early_published.is_empty()
        || !work.async_waits.is_empty()
        || !work.semantic_waits.is_empty()
        || chain.is_empty()
        || chain.iter().copied().collect::<BTreeSet<_>>().len() != chain.len()
        || chain.iter().any(|node| {
            state.node(*node).is_none()
                || !work.admission.scope.contains(live_anchor(state, *node))
                || !work.progress.is_scheduled(*node)
                || work.progress.is_evaluated(*node)
                || work.range_refused.contains(node)
        })
    {
        return false;
    }
    let safe_local = |local: &EvalLocal| {
        !local.external_cache
            && !local.effects.has_observable_effects()
            && local.placement.is_none()
            && local.invocation.is_none()
            && local.observation.is_none()
            && local.observation_reads.is_empty()
            && local.semantic_prerequisite.is_none()
    };
    if !safe_local(&work.local) || work.suspended.values().any(|local| !safe_local(local)) {
        return false;
    }
    let participants = work
        .order
        .iter()
        .chain(work.range_refused.iter())
        .chain(work.suspended.keys())
        .copied()
        .chain(work.progress.staged_results().map(|(node, _)| node));
    if participants.into_iter().any(|node| !pure_range_scalar(state, inputs, node)
        || !matches!(state.bindings.get(&node), Some(Binding::Placed(instance, _)) if instance.plan().external_references().is_empty())) {
        return false;
    }
    if work.progress.staged_results().any(|(node, result)| {
        let at = live_anchor(state, node);
        work.spills.get(at).is_some_and(|spill| spill.area().cell_count() > 1)
            || work.blocked_spills.get(&at).is_some_and(|area| area.cell_count() > 1)
            || !matches!(result, FormulaResult::Calculated { value, calculation }
                if value.as_array().is_none() && !calculation.held_by_circular_rule() && !calculation.reads_external_links()
                    && calculation.stamp().is_some_and(|stamp|
                        matches!(stamp.computed_by, ComputationOrigin::Evaluated { .. })
                        && !stamp.reads.uses_imported_cache && !stamp.reads.uses_boundary_cache
                        && !stamp.reads.external_cache && !stamp.reads.legacy_boundary))
    }) {
        return false;
    }
    work.add_cyclic(state, chain.iter().copied());
    for node in chain {
        exclude_range_target(
            state,
            work,
            *node,
            "a dynamically proven cyclic value in a Manual range calculation",
        );
    }
    true
}

/// Resume a discovered component in the same cell order as a statically found
/// component. Keep that order on subsequent sweeps, without evaluating a member
/// twice in the discovery sweep if its existing seed already let it finish.
fn resume_cycle(
    state: &EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
    mut chain: Vec<NodeId>,
) {
    if work.iterative_range() {
        work.failure = Some(dynamic_range_failure());
        return;
    }
    for member in &chain {
        if let Some(awaited) = work.waits_on.remove(member)
            && let Some(waiters) = work.waiting.get_mut(&awaited)
        {
            waiters.retain(|waiter| waiter != member);
        }
    }
    sort_members(state, inputs, &mut chain);
    if work.iteration.enabled() {
        work.add_cyclic(state, chain.iter().copied());
        let members: BTreeSet<_> = chain.iter().copied().collect();
        let placed: BTreeSet<_> = work
            .order
            .iter()
            .copied()
            .filter(|node| members.contains(node))
            .collect();
        let mut ordered = chain.iter().filter(|node| placed.contains(node));
        for node in &mut work.order {
            if members.contains(node) {
                *node = *ordered.next().expect("one place for each component member");
            }
        }
        let progress = &work.progress;
        let queued: BTreeSet<_> = work.ready.iter().copied().collect();
        work.ready.extend(
            chain
                .into_iter()
                .filter(|node| !progress.is_evaluated(*node) && !queued.contains(node)),
        );
    } else {
        // With iteration off no formula of the cycle is calculated (dc-10y).
        for member in chain {
            keep(state, inputs, work, member);
        }
    }
}

/// Stages an evaluated node's result, spill and dynamic prerequisites, releases the nodes that
/// waited for it, and, when its filled area changed, marks for the next wave the readers of the
/// cells it gained or lost that are already evaluated or not calculated by this recalculation.
fn stage(
    state: &EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
    node: NodeId,
    mut done: Done,
) {
    if work.range_refused.contains(&node) {
        work.failure = Some(RecalcFailure::ScopeViolation {
            at: live_anchor(state, node),
        });
        return;
    }
    if !work.range_refused.is_empty() && matches!(&done.result, FormulaResult::NotModeled { .. }) {
        // A refusal can stop a reader before it reveals its own cycle. Installing
        // that refusal would discard the warm seed needed by a later command.
        work.failure = Some(RecalcFailure::NotModeledScope {
            construct: "a typed refusal before dependency discovery completes in an iterative mixed range",
            owner: "dc-hsa5.11",
        });
        return;
    }
    if !work.range_refused.is_empty()
        && (done
            .spill
            .as_ref()
            .is_some_and(|spill| spill.area().cell_count() > 1)
            || done.blocked.is_some_and(|area| area.cell_count() > 1))
    {
        work.failure = Some(mixed_range_spill_failure());
        return;
    }
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
    let user_volatile = done
        .result
        .stamp()
        .and_then(|stamp| stamp.reads.udf_volatility)
        .map(|request| formula_volatile(state, node, request));
    let volatile = user_volatile.unwrap_or_else(|| state.volatile.contains(&node));
    if volatile
        && matches!(
            state.bindings.get(&node),
            Some(Binding::Placed(_, ArrayRole::Spill))
        )
        && matches!(done.result, FormulaResult::Calculated { .. })
        && done.blocked.is_none()
    {
        let sizing = work.sizing.entry(node).or_insert_with(|| Sizing {
            previous: before.or_else(|| {
                let error = state
                    .permitted_cache(at)
                    .and_then(FormulaResult::calculated)
                    .is_some_and(|(value, _)| value.as_error().is_some());
                (!error).then_some(GridArea::cell(at))
            }),
            ..Sizing::default()
        });
        sizing.evaluations += 1;
        let after = done.spill.as_ref().map(Spill::area).or_else(|| {
            done.result
                .calculated()
                .filter(|(value, _)| value.as_error().is_none())
                .map(|_| GridArea::cell(at))
        });
        // A held footprint or two consecutive equal sizes ends sizing. A transient
        // error after resizing began does not establish a size (P04's third-call error).
        let changed =
            after.is_some() && sizing.previous != after || sizing.changing && after.is_none();
        sizing.changing |= after.is_some() && sizing.previous != after;
        sizing.previous = after;
        if sizing.exhausted || changed {
            if !sizing.exhausted && sizing.evaluations < MAX_SIZING_EVALUATIONS {
                work.rewave.insert(node);
            } else {
                sizing.exhausted = true;
                if let FormulaResult::Calculated { value, .. } = &mut done.result {
                    *value = CalcValue::error(calc_value::WorksheetErrorCode::Spill);
                }
                done.spill = None;
                if let Some(mut stamp) = done.result.stamp().cloned() {
                    stamp.completion = TargetCompletion::WorksheetError;
                    stamp
                        .reads
                        .absences
                        .retain(|proof| proof.kind != AbsenceKind::SpillReservation);
                    done.result.set_stamp(stamp);
                }
            }
        }
    }
    let after = done.spill.as_ref().map(Spill::area);
    work.blocked_spills.remove(&at);
    if let Some(extent) = done.blocked {
        work.blocked_spills.insert(at, extent);
    }
    work.spills.remove(at);
    if let Some(spill) = done.spill {
        work.spills.insert(spill);
    }
    work.filled.insert(node, after);
    work.progress.footprints.remove(&node);
    if let Some(area) = after {
        work.progress.footprints.insert(node, area);
    }
    if work.iteration.enabled() {
        work.pass_start
            .entry(node)
            .or_insert_with(|| Some(starting_value(state, node)));
        if work.cyclic.contains(&node) {
            let value = done.result.calculated().map(|(value, _)| value.clone());
            work.iterates.insert(node, value);
        }
    }
    work.progress.stage(node, done.result);
    work.dynamic.insert(node, done.dynamic);
    if before != after {
        let mut readers = BTreeSet::new();
        for area in before.iter().chain(after.iter()) {
            state.index.dependents_of_area(inputs, *area, &mut readers);
        }
        readers.remove(&node);
        let outside: BTreeSet<_> = readers
            .iter()
            .copied()
            .filter(|reader| !work.admission.scope.contains(live_anchor(state, *reader)))
            .collect();
        if !outside.is_empty() {
            let affected = dirty_closure(state, inputs, outside, &|node| {
                published_areas(state, inputs, node)
            });
            work.invalidated_outside.extend(
                affected
                    .members()
                    .iter()
                    .copied()
                    .filter(|reader| !work.admission.scope.contains(live_anchor(state, *reader))),
            );
        }
        for reader in readers {
            if !work.admission.scope.contains(live_anchor(state, reader)) {
                continue;
            }
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
pub(crate) type SharedObserver<'l, 'o> = &'l RefCell<&'o mut dyn ReadObserver>;

/// The engine's reader, with the caller's observer shown each run before the evaluation receives
/// it, for one formula's evaluation. It keeps every area the evaluation read.
pub(crate) struct Observed<'l, 'o> {
    pub(crate) observer: SharedObserver<'l, 'o>,
    /// Whether the observer observes nothing ([`ReadObserver::is_silent`]): each run then goes
    /// to the evaluation as the engine's reader gives it, without the observer being shown it
    /// (dc-dxm), which is what showing it would come to.
    pub(crate) silent: bool,
    pub(crate) formula: CellKey,
    pub(crate) cells: &'l mut dyn ReferenceReader,
    pub(crate) footprint: &'l RefCell<Footprint>,
    pub(crate) facts: &'l RefCell<Vec<calc_fml::FactRead>>,
    pub(crate) value_reads: &'l RefCell<Vec<(calc_fml::ReadRequest, GridArea)>>,
    pub(crate) blanks: &'l RefCell<Vec<(GridArea, BlankRun)>>,
    /// The area of the read the observer stopped.
    pub(crate) stopped: Option<GridArea>,
}

impl ReferenceReader for Observed<'_, '_> {
    fn pending_producer(&self, at: CellKey) -> bool {
        self.cells.pending_producer(at)
    }

    fn provisional_argument_pending(&self, reference: &calc_fml::value::ReferenceLike) -> bool {
        self.cells.provisional_argument_pending(reference)
    }

    fn provisional_observation_pending(&self, reference: &calc_fml::value::ReferenceLike) -> bool {
        self.cells.provisional_observation_pending(reference)
    }

    fn read_denial(&self) -> Option<calc_fml::ReadDenial> {
        self.cells.read_denial()
    }

    fn read_fact(&mut self, fact: calc_fml::FactRead) {
        if let calc_fml::FactSubject::Cell(at) = fact.subject {
            self.footprint.borrow_mut().push(GridArea::cell(at));
        }
        // A fact is read once as evidence: an instruction that runs again after a wait asks the
        // same fact again, and the certificate holds one stamp for it (dc-hsa5.19).
        {
            let mut facts = self.facts.borrow_mut();
            if !facts.contains(&fact) {
                facts.push(fact);
            }
        }
        self.cells.read_fact(fact);
    }

    fn read_cell(
        &mut self,
        request: calc_fml::ReadRequest,
        at: CellKey,
    ) -> Result<calc_fml::reference::CellRead, calc_fml::reference::ReadContractError> {
        if self.silent && self.stopped.is_none() {
            self.footprint.borrow_mut().push(GridArea::cell(at));
            let read = self.cells.read_cell(request, at);
            if !matches!(read, Ok(calc_fml::reference::CellRead::Missing)) {
                self.value_reads
                    .borrow_mut()
                    .push((request, GridArea::cell(at)));
            }
            if matches!(read, Ok(calc_fml::reference::CellRead::Blank)) {
                self.blanks
                    .borrow_mut()
                    .push((GridArea::cell(at), BlankRun { offset: 0, len: 1 }));
            }
            return read;
        }
        calc_fml::reference::read_cell_by_area(self, request, at)
    }

    fn read_area(
        &mut self,
        request: calc_fml::ReadRequest,
        area: GridArea,
        sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
    ) {
        let total = area.cell_count();
        if self.stopped.is_some() {
            let _ = sink(CellRun::Missing(total));
            return;
        }
        self.footprint.borrow_mut().push(area);
        let before_blanks = self.blanks.borrow().len();
        let mut missing = false;
        if self.silent {
            let blanks = &self.blanks;
            let mut offset = 0;
            self.cells.read_area(request, area, &mut |run| {
                missing |= matches!(run, CellRun::Missing(_));
                if matches!(run, CellRun::Blank(_)) {
                    blanks.borrow_mut().push((
                        area,
                        BlankRun {
                            offset,
                            len: run.len(),
                        },
                    ));
                }
                offset += run.len();
                sink(run)
            });
            if missing {
                self.blanks.borrow_mut().truncate(before_blanks);
            } else {
                self.value_reads.borrow_mut().push((request, area));
            }
            return;
        }
        let mut observer = self.observer.borrow_mut();
        let formula = self.formula;
        let mut offset = 0u64;
        let mut stopped = false;
        self.cells.read_area(request, area, &mut |run| {
            missing |= matches!(run, CellRun::Missing(_));
            if matches!(run, CellRun::Blank(_)) {
                self.blanks.borrow_mut().push((
                    area,
                    BlankRun {
                        offset,
                        len: run.len(),
                    },
                ));
            }
            if observer.observe(formula, area, offset, run).is_break() {
                stopped = true;
                return ControlFlow::Break(());
            }
            offset += run.len();
            sink(run)
        });
        drop(observer);
        if missing || stopped {
            self.blanks.borrow_mut().truncate(before_blanks);
        } else {
            self.value_reads.borrow_mut().push((request, area));
        }
        if stopped {
            // The withheld run and the rest of the area reach the evaluation as cells the reader
            // cannot supply, so it reads no value the engine's reader did not report; the
            // recalculation fails whatever the evaluation returns.
            self.stopped = Some(area);
            let _ = sink(CellRun::Missing(total - offset));
        }
    }

    fn spill_extent(&mut self, anchor: CellKey) -> calc_fml::reference::SpillExtent {
        let region = GridArea::cell(anchor);
        self.footprint.borrow_mut().push(region);
        let extent = self.cells.spill_extent(anchor);
        if !matches!(extent, calc_fml::reference::SpillExtent::NotModelled) {
            self.value_reads.borrow_mut().push((
                calc_fml::ReadRequest {
                    purpose: calc_fml::ReadPurpose::ReferenceShape,
                    facets: calc_fml::ObservableFacets::Shape,
                    ..calc_fml::ReadRequest::DEPENDENCY
                },
                region,
            ));
        }
        extent
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
    /// Whether the observer observes nothing: it is then not shown calls or outcomes.
    silent: bool,
    formula: CellKey,
    calculation: &'l Calculation,
    footprint: &'l RefCell<Footprint>,
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
    /// An executed pure reader requested a refused selected cyclic value.
    /// Retain its historical result without promoting it to a cyclic producer.
    RangeRefused,
    /// The same semantic activation retains its stack, instruction pointer and kernel buffer.
    Suspended,
    /// A level worker did not evaluate the node: its formula calls user code, whose callable
    /// table only the owner holds. `evaluate_levels` evaluates it again on the owner, in order.
    NeedsOwner,
    AwaitExternal,
    Reevaluate {
        on: NodeId,
        cell: CellKey,
        reason: ReevaluationReason,
    },
    /// The result to stage.
    Done(Done),
    /// The evaluation read `cell`, which `on` calculates and this recalculation has yet to
    /// evaluate: it is discarded, and the node waits for `on`.
    Waits {
        on: NodeId,
        cell: CellKey,
    },
    /// With iteration off, the evaluation read a formula a circular reference left uncalculated:
    /// it is discarded, and the node is left uncalculated too (dc-10y).
    Kept,
}

/// A node's staged result, with its spill when its result fills cells beside its anchor and the
/// dynamic prerequisites its evaluation found.
struct Done {
    result: FormulaResult,
    spill: Option<Spill>,
    blocked: Option<GridArea>,
    dynamic: Vec<Prerequisite>,
}

/// Only pure candidates may be semantically evaluated again after a producer/footprint
/// changes. An effectful cone needs a continuation; it never re-enters through this path.
fn queue_certificate_repairs(
    state: &EngineState,
    inputs: &ModelRoot,
    work: &mut RecalcWork,
) -> bool {
    if work.failure.is_some() {
        return false;
    }
    let invalid: Vec<_> = work
        .progress
        .staged_results()
        .filter_map(|(node, result)| {
            let stamp = result.stamp()?;
            (!certificate_valid_at(
                state,
                inputs,
                &work.progress,
                stamp,
                work.facts_version,
                Some(&work.requirements),
                work.volatile_input_version,
                &|read| {
                    work.rtd_connector.as_deref().is_some_and(|connector| {
                        connector.provisional_pending(&read.topic, read.lease, read.sequence)
                    })
                },
            ))
            .then_some(node)
        })
        .collect();
    if invalid.is_empty() {
        return false;
    }
    let pure = invalid.iter().all(|node| {
        matches!(state.bindings.get(node),
        Some(Binding::Placed(instance, _)) if !instance.plan().has_user_calls())
    });
    if !pure || work.waves >= MAX_WAVES {
        // A bounded unsupported topology is local to its targets. It cannot publish a
        // provisional value, and cannot erase independently certified results in the command.
        let refusal = NotModeled::Evaluation(EvalFault::NotModelled {
            construct: "dependency or occupancy certificates that did not stabilize within the bounded recalculation",
            owner: "dc-hsa5",
        });
        for node in invalid {
            let done = refused(work, refusal.clone());
            stage(state, inputs, work, node, done);
        }
        return !work.rewave.is_empty();
    }
    work.repairing_certificates = true;
    work.rewave.extend(invalid);
    true
}

/// Whether the formula at `node` is volatile after an evaluation that ended with `request`:
/// the last request when one ran, else the formula's own token (VOLATILITY.md F2).
fn formula_volatile(state: &EngineState, node: NodeId, request: calc_fml::UdfVolatility) -> bool {
    let token = matches!(
        state.bindings.get(&node),
        Some(Binding::Placed(instance, _)) if instance.plan().volatile()
    );
    request.decide(token)
}

fn root_invocation(state: &EngineState, shared: &EvalShared<'_>, node: NodeId) -> InvocationId {
    InvocationId {
        pass: shared.pass,
        workbook: state.contract.workbook,
        locus: live_anchor(state, node),
        evaluation: shared.evaluations.get(&node).map_or(0, |entry| entry.0),
        call_site: 0,
        occurrence: Vec::new(),
        reason: shared.evaluations.get(&node).map_or_else(
            || match shared.pass.reason {
                PassReason::SpillShape => ReevaluationReason::SpillShapePass,
                PassReason::IterationSweep(_) => ReevaluationReason::IterationSweep,
                PassReason::Scenario(_) => ReevaluationReason::TableScenario,
                PassReason::DependencyRepair => ReevaluationReason::DependencyRepair,
                _ => shared.root_reason,
            },
            |entry| entry.1,
        ),
    }
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
    evaluate_with(
        state,
        inputs,
        &shared,
        &mut work.local,
        node,
        work.functions.as_ref(),
        work.references.as_ref(),
        work.rtd_connector.as_ref(),
        observer,
    )
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
    functions: Option<&calc_fml::CallableTable>,
    references: Option<&ReferenceSource>,
    rtd_connector: Option<&std::rc::Rc<dyn calc_fml::RtdConnector>>,
    observer: &mut dyn ReadObserver,
) -> Result<Attempt, RecalcFailure> {
    if !shared.scope.contains(live_anchor(state, node)) {
        return Err(RecalcFailure::ScopeViolation {
            at: live_anchor(state, node),
        });
    }
    if local.activation.is_none() && local.placement.is_none() {
        local.external_cache = false;
        local.effects = calc_fml::UserFunctionEffects::default();
        local.references = if matches!(state.bindings.get(&node),Some(Binding::Placed(instance,_)) if instance.plan().has_user_calls())
        {
            references.map(|references| references.pin())
        } else {
            None
        };
        local.rtd_connector = if matches!(state.bindings.get(&node),Some(Binding::Placed(instance,_)) if instance.plan().has_user_calls())
        {
            rtd_connector.cloned()
        } else {
            None
        };
        local.observation = None;
        local.observation_reads.clear();
        local.invocation = None;
        local.fact_reads.clear();
        local.value_reads.clear();
        local.read_stamps.clear();
        local.name_reads.clear();
        local.blank_reads.clear();
        local.footprint.clear();
        local.boundary_read.set(None);
        local.boundary_used.set(false);
        local.kept_read.set(false);
        local.current_node.set(Some(node));
    } else {
        debug_assert_eq!(local.current_node.get(), Some(node));
    }
    let attempt = evaluate_unstamped(state, inputs, shared, local, node, functions, observer)?;
    // Stamp at the slice that actually performed the read. A later slice must never certify
    // an old operand against a newer staged value after the activation resumes.
    for &(request, region) in &local.value_reads {
        if !local
            .read_stamps
            .iter()
            .any(|read| read.region == region && read.request == request)
        {
            let versions =
                span_versions_at(state, shared.progress, region, Some(shared.requirements));
            let mut boundary_caches = if versions.boundary_cache > 0 || versions.imported_cache > 0
            {
                crate::contract::boundary_ancestry_at(state, shared.progress, region)
            } else {
                Vec::new()
            };
            if local.boundary_used.get()
                && let Some(span) = shared.boundary_caches.span(region)
            {
                if !boundary_caches.contains(&span) {
                    boundary_caches.push(span);
                }
            }
            local.read_stamps.push(ReadSpan {
                boundary_caches,
                region,
                request,
                versions,
            });
        }
    }
    let Attempt::Done(mut done) = attempt else {
        return Ok(attempt);
    };
    // Existing iteration and data-table execution keeps its own bounded legacy boundary.
    // P14/P15 decide the remaining session certificates; no FreshFor claim is attached here.
    if shared.iterates.is_some() || matches!(state.bindings.get(&node), Some(Binding::DataTable(_)))
    {
        return Ok(Attempt::Done(done));
    }
    let at = live_anchor(state, node);
    let required = shared.requirements.get(&node).map_or(1, |r| r.required);
    let mut reads = ReadCertificate {
        external_cache: match &done.result {
            FormulaResult::Calculated { calculation, .. }
            | FormulaResult::NotModeled { calculation, .. } => calculation.reads_external_links(),
            FormulaResult::FileCached(_) => false,
        },
        semantic_environment: state.contract.environment,
        bindings: state.contract.bindings,
        ..ReadCertificate::default()
    };
    if !shared.linked_workbooks.is_empty() {
        reads.linked_inputs = Some(crate::LinkedInputStamp {
            version: shared.linked_input_version,
            sources: shared.linked_workbooks.values().cloned().collect(),
        });
    }
    if !local.effects.external_inputs().is_empty() {
        reads.external_inputs = local.effects.external_inputs().to_vec();
    }
    let callables = match state.bindings.get(&node) {
        Some(Binding::Placed(instance, _)) => instance.plan().callable_bindings().to_vec(),
        _ => Vec::new(),
    };
    if !callables.is_empty() {
        reads.callables = callables;
    }
    for proof in local.effects.callable_reads() {
        if !reads.callables.contains(proof) {
            reads.callables.push(proof.clone());
        }
    }
    if let Some(pin) = &local.observation {
        for (request, region) in &local.observation_reads {
            let read = pin.observation(*request, *region);
            if !reads.observations.contains(&read) {
                reads.observations.push(read);
            }
        }
    }
    let invocations: Vec<_> = local
        .effects
        .invocations()
        .iter()
        .map(|record| {
            let mut identity = root_invocation(state, shared, node);
            identity.call_site = record.call_site;
            identity.occurrence = record.occurrence.clone();
            crate::contract::CallInvocation {
                identity,
                runtime: record.id,
                parent: record.parent,
                callable: record.callable,
                completed: record.completed,
                non_preemptible: record.non_preemptible,
            }
        })
        .collect();
    if !invocations.is_empty() {
        reads.invocations = invocations;
    }
    if !local.effects.name_invocations().is_empty() {
        reads.name_invocations = local.effects.name_invocations().to_vec();
    }
    if local.effects.omitted_name_invocations() != 0 {
        reads.omitted_name_invocations = local.effects.omitted_name_invocations();
    }
    if matches!(state.bindings.get(&node),Some(Binding::Placed(instance,_)) if instance.plan().has_user_calls() || instance.plan().has_names())
    {
        reads.udf_volatility = Some(local.effects.volatility());
    }
    let names = match state.bindings.get(&node) {
        Some(Binding::Placed(instance, _)) => instance.plan().name_bindings().to_vec(),
        _ => state.index.name_bindings(node).to_vec(),
    };
    if !names.is_empty() {
        reads.names = names;
    }
    for proof in &local.name_reads {
        if !reads.names.contains(proof) {
            reads.names.push(proof.clone());
        }
        if let Some(bound) = proof.resolved {
            let edge = Prerequisite::Name(bound.id);
            if !done.dynamic.contains(&edge)
                && !state
                    .node(node)
                    .is_some_and(|n| n.prerequisites().is_some_and(|p| p.contains(&edge)))
            {
                done.dynamic.push(edge);
            }
        }
    }
    reads.dependencies = local.read_stamps.clone();
    // Walk the same retained spans while filling the certificate's optional details.
    for read in &local.read_stamps {
        for span in &read.boundary_caches {
            if !reads.boundary_caches.contains(span) {
                reads.boundary_caches.push(span.clone());
            }
        }
    }
    for dependency in &local.read_stamps {
        for observation in
            crate::contract::observation_ancestry_at(state, shared.progress, dependency.region)
        {
            if !reads.observations.contains(&observation) {
                reads.observations.push(observation);
            }
        }
    }
    if reads.observations.iter().any(|read| read.external_cache) {
        reads.external_cache = true;
        match &mut done.result {
            FormulaResult::Calculated { calculation, .. }
            | FormulaResult::NotModeled { calculation, .. } => {
                *calculation = calculation.clone().with_external_links()
            }
            _ => {}
        }
    }
    reads.uses_boundary_cache = !reads.observations.is_empty()
        || !reads.boundary_caches.is_empty()
        || reads
            .dependencies
            .iter()
            .any(|read| read.versions.boundary_cache > 0);
    reads.uses_imported_cache = reads
        .observations
        .iter()
        .any(|read| read.lineage.as_ref().is_some_and(|span| span.imported > 0))
        || reads.boundary_caches.iter().any(|span| span.imported > 0)
        || reads
            .dependencies
            .iter()
            .any(|read| read.versions.imported_cache > 0);
    reads.legacy_boundary = reads
        .dependencies
        .iter()
        .any(|read| read.versions.uncertified > 0);
    for &read in &local.fact_reads {
        reads.facts.push(FactStamp {
            read,
            version: fact_version(state, read, shared.facts_version),
        });
    }
    if done.spill.as_ref().is_some_and(|spill| {
        local
            .blank_reads
            .iter()
            .any(|(read, blank)| blank.first_intersection(*read, spill.area()).is_some())
    }) {
        // Shape is now known: this output would occupy a blank its own evaluation consumed.
        // P05 decides the Excel outcome; do not turn a prospective self-spill into a cached cycle.
        if let FormulaResult::Calculated { calculation, .. } = &done.result {
            done.result = FormulaResult::NotModeled {
                calculation: calculation.clone(),
                refusal: NotModeled::Evaluation(EvalFault::NotModelled {
                    construct: "a spill occupying a blank it read (waiting for Excel P05)",
                    owner: "dc-hsa5",
                }),
            };
            done.spill = None;
        }
    }
    for &(region, blank) in local
        .blank_reads
        .iter()
        .filter(|_| matches!(done.result, FormulaResult::Calculated { .. }))
    {
        if let Some(proof) = reads
            .absences
            .iter_mut()
            .find(|proof| proof.region == region && proof.kind == AbsenceKind::ReadBlank)
        {
            if !proof.blanks.contains(&blank) {
                proof.blanks.push(blank);
            }
            continue;
        }
        reads.absences.push(AbsenceProof {
            region,
            kind: AbsenceKind::ReadBlank,
            authored: state.contract.authored(region),
            occupancy: state.contract.occupancy(region),
            corridor: if producer_version_at(state, region, node, Some(shared.requirements)) == 0 {
                0
            } else {
                state.contract.authored(absence_corridor(region))
            },
            producers: producer_version_at(state, region, node, Some(shared.requirements)),
            pending_producers: producer_pending_at(
                state,
                inputs,
                shared.progress,
                region,
                &[blank],
                node,
                Some(shared.requirements),
            ),
            cached: None,
            blanks: vec![blank],
        });
    }
    if !reads.absences.is_empty() {
        let mut uses_cached_absence = false;
        for proof in &mut reads.absences {
            proof.pending_producers = producer_pending_at(
                state,
                inputs,
                shared.progress,
                proof.region,
                &proof.blanks,
                node,
                Some(shared.requirements),
            );
            if proof.pending_producers != 0
                && crate::contract::producer_pending_in(
                    state,
                    inputs,
                    shared.progress,
                    proof.region,
                    &proof.blanks,
                    node,
                    Some(shared.requirements),
                    &|at| shared.scope.contains(at),
                ) == 0
            {
                proof.cached = Some(shared.boundary_caches.view());
                uses_cached_absence = true;
            }
        }
        reads.uses_boundary_cache |= uses_cached_absence;
    }
    if let Some(region) = done.spill.as_ref().map(Spill::area).or(done.blocked) {
        reads.absences.push(AbsenceProof {
            region,
            kind: AbsenceKind::SpillReservation,
            authored: state.contract.authored(region),
            occupancy: state.contract.occupancy(region),
            corridor: state.contract.authored(region),
            producers: producer_version_at(state, region, node, Some(shared.requirements)),
            pending_producers: 0,
            cached: None,
            blanks: Vec::new(),
        });
    }
    let binding = state.bindings.get(&node);
    let origin = if matches!(binding, Some(Binding::Refused(_))) {
        ComputationOrigin::EngineRefusal
    } else {
        ComputationOrigin::Evaluated {
            root: root_invocation(state, shared, node),
        }
    };
    if reads
        .udf_volatility
        .is_some_and(calc_fml::UdfVolatility::requests_volatile)
        || matches!(state.bindings.get(&node),Some(Binding::Placed(instance,_)) if instance.plan().reads_volatile_inputs())
    {
        reads.volatile_inputs = Some(shared.volatile_inputs);
    }
    let completion = match &done.result {
        FormulaResult::Calculated { value, .. } if value.as_error().is_some() => {
            TargetCompletion::WorksheetError
        }
        FormulaResult::Calculated { .. } => TargetCompletion::Value,
        _ => TargetCompletion::EngineRefusal,
    };
    done.result.set_stamp(ResultStamp {
        destination: shared.destination,
        target: EvaluationTarget {
            workbook: state.contract.workbook,
            origin_revision: shared.calculation.revision(),
            formula_generation: state.contract.generation(node),
            node,
            locus: at,
        },
        observed: ObservationFrontier {
            model: shared.calculation.revision(),
            results: ReadViewId(state.generation),
            inputs: shared.volatile_inputs,
        },
        computed_by: origin,
        reads,
        certification: FreshnessClaim {
            command: shared.pass.command,
            pass: shared.pass,
            frontier: AdmissionFrontier {
                model: shared.calculation.revision(),
                external_through: shared.volatile_inputs,
            },
            required,
        },
        completion,
    });
    Ok(Attempt::Done(done))
}

fn evaluate_unstamped(
    state: &EngineState,
    inputs: &ModelRoot,
    shared: &EvalShared<'_>,
    local: &mut EvalLocal,
    node: NodeId,
    functions: Option<&calc_fml::CallableTable>,
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
                shared.environment,
                table,
                formula,
                Some((&calculation, &mut *observer)),
            );
            if local.kept_read.get() {
                return Ok(Attempt::Kept);
            }
            return Ok(match outcome {
                crate::data_table::TableOutcome::Value(value) => {
                    match NotModeled::publishable(value) {
                        Ok(value) => Attempt::Done(Done {
                            result: FormulaResult::Calculated { value, calculation },
                            spill: None,
                            blocked: None,
                            dynamic: Vec::new(),
                        }),
                        Err(refusal) => Attempt::Done(refused_by(&calculation, refusal)),
                    }
                }
                crate::data_table::TableOutcome::Refused(refusal) => {
                    Attempt::Done(refused_by(&calculation, refusal))
                }
                crate::data_table::TableOutcome::Waits { on, cell } => Attempt::Waits { on, cell },
            });
        }
    };
    if let Some((value, dynamic, calculation)) = local.placement.take() {
        return place_value(
            state,
            inputs,
            shared,
            local,
            node,
            formula,
            role,
            value,
            dynamic,
            calculation,
        );
    }
    if instance.plan().has_user_calls() && functions.is_none() {
        return Ok(Attempt::Done(refused_by(
            &calculation,
            NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "a callable provider missing from its admitted command",
                owner: "dc-hsa5.4",
            }),
        )));
    }
    if instance.plan().has_user_calls() && local.observation.is_none() {
        let excluded = if shared.preliminary.contains(&node) {
            shared.cyclic
        } else {
            shared.range_refused
        };
        let pin = crate::CachedReadLease::capture(
            state,
            inputs,
            shared.linked_workbooks.clone(),
            shared.progress,
            shared.spills,
            shared.volatile_inputs,
            (matches!(shared.scope, CalcScope::Range(_))
                && shared.selection.forces_all()
                && functions
                    .is_some_and(|functions| instance.plan().has_provisional_calls(functions)))
            .then_some(node),
            shared.iterates,
            excluded,
        );
        local.invocation = Some(
            calc_fml::UserInvocationFrame::new(
                None,
                std::rc::Rc::new(crate::contract::InvocationReadView {
                    root: root_invocation(state, shared, node),
                    view: pin.clone(),
                }),
            )
            .with_formula_entry(shared.selection == EvalSelection::FormulaEntry)
            // Range.Calculate runs on one thread in Excel (x17: every `$` entry on main).
            .with_single_threaded(matches!(shared.scope, CalcScope::Range(_))),
        );
        local.observation = Some(pin);
    }
    let reference_view = local.references.clone();
    let rtd_connector = local.rtd_connector.clone();
    let observation_pin = local.observation.clone();
    let invocation = local.invocation.clone();
    let mut user_effects = std::mem::take(&mut local.effects);
    let observation_reads = RefCell::new(std::mem::take(&mut local.observation_reads));
    let observation_footprint = RefCell::new(Footprint::default());
    let observation_facts = RefCell::new(Vec::new());
    let observation_blanks = RefCell::new(Vec::new());
    let observation_uncomputed = Cell::new(None);
    let uncomputed = Cell::new(None);
    let footprint = RefCell::new(std::mem::take(&mut local.footprint));
    let facts = RefCell::new(std::mem::take(&mut local.fact_reads));
    let value_reads = RefCell::new(std::mem::take(&mut local.value_reads));
    let blanks = RefCell::new(std::mem::take(&mut local.blank_reads));
    let batch = std::mem::take(&mut local.read_batch);
    let mut activation = local.activation.take().unwrap_or_else(|| {
        calc_fml::EvalContinuation::new(instance, std::mem::take(&mut local.scratch))
    });
    let mut random = local.random.take().unwrap_or_else(|| {
        RandomStream::new(
            shared.random_seed
                ^ (u64::from(shared.pass.ordinal) << 32)
                ^ root_invocation(state, shared, node)
                    .evaluation
                    .rotate_left(17),
            calculation.tick().get(),
            formula,
        )
    });
    let name_reads = RefCell::new(std::mem::take(&mut local.name_reads));
    let view = shared.view(state, inputs, &*local);
    let mut cells = WorkbookReader::new(view, &uncomputed).with_batch(batch);
    let catalog = SnapshotCatalog::traced(inputs, &name_reads);
    let silent = observer.is_silent();
    let observer = RefCell::new(observer);
    let (outcome, stopped) = {
        let mut observed = Observed {
            observer: &observer,
            silent,
            formula,
            cells: &mut cells,
            footprint: &footprint,
            facts: &facts,
            value_reads: &value_reads,
            blanks: &blanks,
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
        let (outcome, cached_stopped) = {
            let mut observation_cells = observation_pin.as_ref().map(|pin| {
                let mut view = pin.view(state, shared.workbook_facts);
                view.caller = (!shared
                    .iterates
                    .is_some_and(|values| values.contains_key(&node)))
                .then_some(node);
                WorkbookReader::new(view, &observation_uncomputed)
            });
            let mut cached_observed = observation_cells.as_mut().map(|cells| Observed {
                observer: &observer,
                silent,
                formula,
                cells,
                footprint: &observation_footprint,
                facts: &observation_facts,
                value_reads: &observation_reads,
                blanks: &observation_blanks,
                stopped: None,
            });
            let mut host = WorkbookReader::new(view, &uncomputed);
            let mut services = FormulaEvaluationContext::new(
                calculation.mode(),
                &mut observed,
                shared.environment,
                Some(&mut host),
                calc_fml::ReadRequest::DEPENDENCY,
            )
            .with_catalog(&catalog)
            .with_random(&mut random)
            .with_user_function_effects(&mut user_effects)
            .observed_by(instrument);
            if let Some(inputs) = shared.external_inputs {
                services = services.with_external_inputs(inputs);
            }
            if let Some(functions) = functions {
                services = services.with_functions(functions);
            }
            if let Some(references) = reference_view.as_deref() {
                services = services.with_reference_handles(references);
            }
            if let Some(connector) = rtd_connector.as_deref() {
                services = services.with_rtd_connector(connector);
            }
            if let Some(view) = shared.user_view {
                services = services.with_user_view(view);
            }
            if let Some(invocation) = invocation {
                services = services.with_invocation(invocation);
            }
            if let Some(cached) = cached_observed.as_mut() {
                services = services.with_observation_reader(cached);
            }
            let step = activation.advance(
                &mut services,
                if local.quantum == 0 {
                    usize::MAX
                } else {
                    local.quantum
                },
            );
            drop(services);
            (
                step,
                cached_observed.as_ref().and_then(|reader| reader.stopped),
            )
        };
        (outcome, observed.stopped.or(cached_stopped))
    };
    let unresolved_links = instance.plan().external_references().iter().any(|operand| {
        let number = match &operand.book {
            calc_fml::reference::ExternalBookRef::Number(number) => Some(*number),
            calc_fml::reference::ExternalBookRef::File(name) => {
                inputs.definitions().external_book_named(name)
            }
        };
        number.is_none_or(|number| !shared.linked_workbooks.contains_key(&number))
    });
    let external_cache = unresolved_links || cells.read_external_links();
    local.read_batch = cells.into_batch();
    local.external_cache |= external_cache;
    let calculation = if local.external_cache {
        calculation.with_external_links()
    } else {
        calculation
    };
    local.effects = user_effects;
    local.observation_reads = observation_reads.into_inner();
    local.name_reads = name_reads.into_inner();
    local.fact_reads = facts.into_inner();
    local.value_reads = value_reads.into_inner();
    local.blank_reads = blanks.into_inner();
    let kept_read = local.kept_read.get();
    if let Some(area) = stopped {
        return Err(RecalcFailure::ReadFailed { formula, area });
    }
    if local.boundary_read.get() == Some(crate::read::iterative_range_denial()) {
        return Ok(Attempt::RangeRefused);
    }
    // With iteration off, a formula that read one a circular reference left uncalculated is not
    // calculated either (dc-10y).
    if kept_read {
        return Ok(Attempt::Kept);
    }
    if local.boundary_read.get().is_none()
        && let Some((cell, on)) = uncomputed.get().or(local.semantic_prerequisite)
    {
        if let Some(reason) = local.effects.reevaluation() {
            if matches!(outcome, calc_fml::EvalStep::Suspended(_)) {
                if activation.pending_external().is_some() {
                    return Ok(Attempt::Done(refused_by(
                        &calculation,
                        NotModeled::Evaluation(EvalFault::NotModelled {
                            construct: "an async wait inside a provisional semantic evaluation",
                            owner: "dc-hsa5.4",
                        }),
                    )));
                }
                local.semantic_prerequisite = Some((cell, on));
                local.footprint = footprint.into_inner();
                local.activation = Some(activation);
                local.random = Some(random);
                return Ok(Attempt::Suspended);
            }
            let reason = match reason {
                calc_fml::UserReevaluationReason::XllUncalcedUnwind => {
                    ReevaluationReason::XllUncalcedUnwind
                }
                calc_fml::UserReevaluationReason::ProvisionalArguments => {
                    ReevaluationReason::ProvisionalArguments
                }
            };
            return Ok(Attempt::Reevaluate { on, cell, reason });
        }
        let named = instance
            .areas()
            .iter()
            .any(|area| area.is_ok_and(|area| area.contains(cell)));
        let formula_cell = matches!(inputs.cell(cell), Some(CellInput::Formula(_)));
        if named
            && formula_cell
            && !shared.waits_on.contains_key(&on)
            && !shared.cyclic.contains(&on)
        {
            return Err(RecalcFailure::UnscheduledRead { formula, cell });
        }
        if matches!(outcome, calc_fml::EvalStep::Suspended(_)) {
            local.footprint = footprint.into_inner();
            local.activation = Some(activation);
            local.random = Some(random);
            return Ok(Attempt::Waits { on, cell });
        }
        if local.effects.calls() > 0 {
            return Ok(Attempt::Done(refused_by(
                &calculation,
                NotModeled::Evaluation(EvalFault::NotModelled {
                    construct: "an effectful callable requiring a dependency continuation",
                    owner: "dc-hsa5.4",
                }),
            )));
        }
        return Ok(Attempt::Waits { on, cell });
    }
    if local.effects.reevaluation().is_some() {
        return Ok(Attempt::Done(refused_by(
            &calculation,
            NotModeled::Evaluation(EvalFault::NotModelled {
                construct: "semantic reevaluation without an unresolved prerequisite",
                owner: "dc-hsa5.4",
            }),
        )));
    }
    let outcome = if matches!(outcome, calc_fml::EvalStep::Suspended(_))
        && let Some(cell) = activation.waiting_input()
    {
        // No admitted unresolved producer reached the wait branch above. A permanently
        // unavailable input cannot become runnable merely by scheduling another quantum.
        calc_fml::EvalStep::Complete(Err(EvalFault::MissingInput { cell }))
    } else {
        outcome
    };
    let outcome = match outcome {
        calc_fml::EvalStep::Suspended(_) => {
            local.footprint = footprint.into_inner();
            local.activation = Some(activation);
            local.random = Some(random);
            return Ok(
                if local
                    .activation
                    .as_ref()
                    .is_some_and(|a| a.pending_external().is_some())
                {
                    Attempt::AwaitExternal
                } else {
                    Attempt::Suspended
                },
            );
        }
        calc_fml::EvalStep::Complete(outcome) => {
            local.scratch = activation.into_scratch();
            outcome
        }
    };
    let mut areas = footprint.into_inner();
    let dynamic = dynamic_prerequisites(state, node, instance, areas.areas());
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
                blocked: None,
                dynamic,
            }));
        }
    };
    place_value(
        state,
        inputs,
        shared,
        local,
        node,
        formula,
        role,
        value,
        dynamic,
        calculation,
    )
}

fn place_value(
    state: &EngineState,
    inputs: &ModelRoot,
    shared: &EvalShared<'_>,
    local: &mut EvalLocal,
    node: NodeId,
    formula: CellKey,
    role: ArrayRole,
    value: CalcValue,
    mut dynamic: Vec<Prerequisite>,
    calculation: Calculation,
) -> Result<Attempt, RecalcFailure> {
    if !shared.range_refused.is_empty()
        && role == ArrayRole::Spill
        && value.callable_value().is_none()
        && value
            .as_array()
            .is_some_and(|array| !array.is_boxed() && array.cell_count() > 1)
    {
        // Refuse the attempted extent before collision handling, including an
        // unknown outside array whose children overlap this new spill.
        return Err(mixed_range_spill_failure());
    }
    if shared.parallel_scheduling
        && matches!(role, ArrayRole::Spill)
        && parallel_limit(state, node).is_some()
    {
        let rank = shared
            .reference_rank
            .get(&node)
            .copied()
            .unwrap_or(usize::MAX);
        let earlier = shared
            .reference_rank
            .iter()
            .filter(|(other, own_rank)| {
                **own_rank < rank
                    && shared.progress.is_scheduled(**other)
                    && !shared.progress.is_evaluated(**other)
                    && !shared.waits_on.contains_key(other)
                    && matches!(
                        state.bindings.get(other),
                        Some(Binding::Placed(
                            _,
                            ArrayRole::Spill | ArrayRole::LegacyFill(_)
                        ))
                    )
            })
            .min_by_key(|(_, own_rank)| **own_rank)
            .map(|(other, _)| *other);
        if earlier.is_some() {
            // This is an output reservation, not a formula read dependency.
            // It consumes no native pool slot and must not create a wait edge.
            local.placement = Some((value, dynamic, calculation));
            return Ok(Attempt::Suspended);
        }
    }
    let outcome = match array_outcome(
        shared.view(state, inputs, local),
        node,
        formula,
        role,
        value.clone(),
    ) {
        Ok(outcome) => outcome,
        Err(Placed::Waits { on, cell }) => {
            local.placement = Some((value, dynamic, calculation));
            return Ok(Attempt::Waits { on, cell });
        }
        Err(Placed::Refused(refusal)) => {
            return Ok(Attempt::Done(Done {
                result: FormulaResult::NotModeled {
                    refusal,
                    calculation,
                },
                spill: None,
                blocked: None,
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
    let blocked = match &outcome {
        SpillOutcome::Blocked { extent } => Some(*extent),
        _ => None,
    };
    let published = outcome.publishable_value();
    let spill = match outcome {
        SpillOutcome::Fills(spill) => Some(spill),
        SpillOutcome::Scalar(_) | SpillOutcome::Blocked { .. } => None,
    };
    let result = match published {
        Ok(value) => FormulaResult::Calculated { value, calculation },
        Err(refusal) => {
            return Ok(Attempt::Done(Done {
                result: FormulaResult::NotModeled {
                    refusal,
                    calculation,
                },
                spill: None,
                blocked: None,
                dynamic,
            }));
        }
    };
    Ok(Attempt::Done(Done {
        result,
        spill,
        blocked,
        dynamic,
    }))
}

/// The areas an evaluation read, each once, in the order they were first read (dc-hsa5.19, D6).
///
/// A user function that reads cell by cell, a loop that reads one cell again and again, and an
/// evaluation that waits and runs a kernel again all read the same areas many times over. The
/// footprint keeps one entry per distinct area, so its size is bounded by the areas read rather
/// than the number of reads, and turning it into prerequisites takes time linear in that size.
#[derive(Debug, Default)]
pub(crate) struct Footprint {
    areas: Vec<GridArea>,
    seen: HashSet<GridArea>,
}

impl Footprint {
    pub(crate) fn push(&mut self, area: GridArea) {
        if self.seen.insert(area) {
            self.areas.push(area);
        }
    }

    pub(crate) fn areas(&self) -> &[GridArea] {
        &self.areas
    }

    pub(crate) fn clear(&mut self) {
        self.areas.clear();
        self.seen.clear();
    }
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
    let mut found: HashSet<Prerequisite> = HashSet::new();
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
        if !held && found.insert(prerequisite) {
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
    // A boxed array (dc-h9u: INDEX of a GROUPBY result's cell) shows its first element and spills
    // nothing (Excel 16.0.20527: `=INDEX(GROUPBY(k,v,LAMBDA(x,x)),1,2)` shows the group's first
    // value).
    if let Some(boxed) = array.as_ref().filter(|array| array.is_boxed()) {
        return Ok(SpillOutcome::Scalar(shown(
            boxed.get(0, 0).expect("a non-empty array"),
        )));
    }
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

/// A value as a cell shows it ([`spill::shown`]).
fn shown(value: &CalcValue) -> CalcValue {
    spill::shown(value)
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
        .definitions()
        .pivots()
        .tables()
        .any(|table| table.area.intersection(extent).is_some())
    {
        return Collision::Blocked;
    }
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

impl RecalcWork {
    pub fn pending_async(&self) -> impl Iterator<Item = &AsyncToken> {
        self.async_waits.values()
    }
    pub fn is_waiting_external(&self) -> bool {
        if self.async_waits.is_empty() {
            return false;
        }
        // Every user call is an observation fence. Its state lane remains held while parked.
        // No later segment may enter or publish through this fence.
        let segment = active_segment(self);
        self.async_waits
            .keys()
            .any(|node| self.segments.get(node).copied() == segment)
            || self.next == self.order.len() && self.ready.is_empty()
    }
    /// Validate the admitted target and every argument read before delivering to its existing
    /// activation. This executes no provider code and never starts a replacement evaluation.
    pub fn accept_async(
        &mut self,
        state: &EngineState,
        inputs: &ModelRoot,
        token: &AsyncToken,
        value: CalcValue,
    ) -> Result<(), AsyncRejection> {
        let node = token.target.node;
        let held = self
            .async_waits
            .get(&node)
            .ok_or(AsyncRejection::UnknownToken)?;
        if held != token {
            return Err(AsyncRejection::IdentityMismatch);
        }
        if self.failure.is_some()
            || !self.live_inputs_current(state)
            || self.generation != state.generation
            || self.revision != inputs.revision()
            || self.revision != state.revision()
            || token.invocation.pass.command != self.admission.command
        {
            return Err(AsyncRejection::Revoked);
        }
        if token.target.workbook != state.contract.workbook
            || token.target.formula_generation != state.contract.generation(node)
            || state
                .node(node)
                .is_none_or(|n| n.anchor() != token.target.locus)
        {
            return Err(AsyncRejection::TargetChanged);
        }
        if !self.callables_current(state)
            || self
                .functions
                .as_ref()
                .and_then(|f| f.resolve(token.provider))
                .is_none()
        {
            return Err(AsyncRejection::ProviderChanged);
        }
        if state.contract.requirement(node).required != token.required
            || self
                .requirements
                .get(&node)
                .is_none_or(|r| r.required != token.required)
        {
            return Err(AsyncRejection::RequirementsChanged);
        }
        let local = self
            .suspended
            .get_mut(&node)
            .ok_or(AsyncRejection::ContinuationUnavailable)?;
        if state.environment != self.environment.map(Into::into)
            || local.read_stamps.iter().any(|read| {
                span_versions_at(state, &self.progress, read.region, Some(&self.requirements))
                    != read.versions
            })
            || local.fact_reads.iter().any(|read| {
                fact_version(state, *read, self.facts_version)
                    != fact_version(state, *read, state.contract.facts)
            })
            || !crate::contract::name_bindings_valid(inputs, &local.name_reads)
        {
            return Err(AsyncRejection::ArgumentsChanged);
        }
        if value.is_missing() {
            return Err(AsyncRejection::InvalidValue);
        }
        if !local
            .activation
            .as_mut()
            .is_some_and(|a| a.accept_external(token.runtime, token.request, value))
        {
            return Err(AsyncRejection::ContinuationUnavailable);
        }
        self.async_waits.remove(&node);
        self.async_resumed.insert(node);
        self.ready.push_front(node);
        Ok(())
    }
}

#[cfg(test)]
mod footprint_tests {
    use super::*;
    use calc_value::IdAllocator;

    /// dc-hsa5.19 (Fable D6): reading cells one by one, again and again, files each distinct area
    /// once and in the order it was first read, so the footprint is bounded by the areas read and
    /// not by the number of reads.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_footprint_keeps_each_distinct_area_once_in_first_read_order() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        let cell = |row| GridArea::cell(CellKey::new(sheet, row, 1).unwrap());
        let column = GridArea::new(
            CellKey::new(sheet, 1, 1).unwrap(),
            CellKey::new(sheet, 3, 1).unwrap(),
        )
        .unwrap();
        let mut footprint = Footprint::default();
        for round in 0..100_000u32 {
            for row in [3, 1, 2] {
                footprint.push(cell(row));
            }
            if round % 1000 == 0 {
                footprint.push(column);
            }
        }
        assert_eq!(footprint.areas(), [cell(3), cell(1), cell(2), column]);
        footprint.clear();
        assert!(footprint.areas().is_empty());
        footprint.push(cell(2));
        assert_eq!(
            footprint.areas(),
            [cell(2)],
            "a cleared footprint files an area again"
        );
    }
}
