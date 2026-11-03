//! The caller-owned engine state: interned node IDs, the formula-node records and the one
//! workbook graph.

use crate::graph::{BlockHasher, Prerequisite, ReverseIndex};
use crate::recalc::{ReadersCache, dirty_closure, live_anchor};
use crate::result::{CacheWithoutFormula, FormulaResult, NotModeled};
use crate::results::{ResultGroup, Results};
use crate::spill::{Spill, SpillIndex};
use calc_doc_model::{AuthoredRevision, CellInput, FileCached, ModelRoot};
use calc_fml::{CalcEnvironment, CompiledPlan, FormulaInstance, PlanCache};
use calc_value::{CellKey, GridArea, IdAllocator, NameId, NodeId, NumericMode, SheetId};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::hash::BuildHasherDefault;
use std::sync::Arc;

/// One formula participant of the graph: its interned ID, the cell it anchors, and what it reads
/// once its plan is bound. It holds no formula text and no value: the authored formula stays in
/// the document model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaNode {
    id: NodeId,
    anchor: CellKey,
    prerequisites: Option<Vec<Prerequisite>>,
    dynamic: Vec<Prerequisite>,
}

impl FormulaNode {
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// The cell whose formula this node is.
    pub fn anchor(&self) -> CellKey {
        self.anchor
    }

    /// What the node reads, or `None` while its formula's plan is not bound: a node adopted from
    /// a snapshot, or added or replaced by an edit, has no prerequisites until
    /// [`EngineState::set_prerequisites`] states them, which [`crate::begin_recalc`] does when it
    /// binds the node's formula. A node whose formula CalcFml refused to compile keeps any
    /// inspected definitions and namespace probes, so a name edit can make it bind again.
    pub fn prerequisites(&self) -> Option<&[Prerequisite]> {
        self.prerequisites.as_deref()
    }

    /// What the node's last calculated evaluation read beyond its placed references: the areas a
    /// reference computed as it ran (INDIRECT's text, OFFSET's result, a spilled range `#` names)
    /// read, and, for an array formula, the cells beside its anchor its result covers or would
    /// cover. They are edges of the one graph like the placed references, so a change to one of
    /// these cells makes the node stale. A node with no published calculation, and one whose
    /// formula an edit replaced, has none.
    pub fn dynamic_prerequisites(&self) -> &[Prerequisite] {
        &self.dynamic
    }
}

/// Why [`EngineState::set_prerequisites`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrerequisiteError {
    /// The input snapshot is not at the engine state's revision.
    StaleInputs {
        engine: AuthoredRevision,
        inputs: AuthoredRevision,
    },
    /// The node is not live in this state.
    UnknownNode(NodeId),
    /// A name prerequisite no longer exists in the input snapshot.
    UnknownName(NameId),
    /// A prerequisite names a sheet the input snapshot does not hold.
    UnknownSheet(SheetId),
}

impl fmt::Display for PrerequisiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrerequisiteError::StaleInputs { engine, inputs } => write!(
                f,
                "the input snapshot is at revision {}, the engine state at {}",
                inputs.get(),
                engine.get()
            ),
            PrerequisiteError::UnknownName(id) => {
                write!(f, "the input snapshot holds no name {id:?}")
            }
            PrerequisiteError::UnknownNode(node) => {
                write!(f, "node {node:?} is not live in this engine state")
            }
            PrerequisiteError::UnknownSheet(sheet) => {
                write!(f, "the input snapshot holds no sheet {sheet:?}")
            }
        }
    }
}

impl std::error::Error for PrerequisiteError {}

/// A formula node's bound plan: its compiled plan placed at its anchor, or CalcFml's refusal.
#[derive(Clone, Debug)]
pub(crate) enum Binding {
    Placed(FormulaInstance, ArrayRole),
    Refused(NotModeled),
    /// A cell of a what-if data table (L3, dc-m3u), calculated from the table.
    DataTable(calc_doc_model::DataTable),
}

/// The formula nodes by anchor (dc-dxm): for each sheet whose anchors fill enough of the rectangle
/// around them, a grid over that rectangle, indexed by row and column, so that a range of formula
/// cells is read from consecutive memory; every other anchor in a hash map. Built from the anchors
/// when the state is made and after a structural edit; an anchor added later outside its sheet's
/// grid goes to the map.
#[derive(Debug, Default)]
pub(crate) struct AnchorIndex {
    /// By the sheet's slot: the sheet and its grid.
    grids: Vec<Option<(SheetId, AnchorGrid)>>,
    other: HashMap<CellKey, NodeId, BuildHasherDefault<BlockHasher>>,
}

#[derive(Debug)]
struct AnchorGrid {
    top: u32,
    left: u32,
    rows: u32,
    columns: u32,
    cells: Vec<Option<NodeId>>,
}

impl AnchorGrid {
    fn position(&self, at: CellKey) -> Option<usize> {
        let (row, column) = (
            at.row().wrapping_sub(self.top),
            at.column().wrapping_sub(self.left),
        );
        (row < self.rows && column < self.columns)
            .then(|| row as usize * self.columns as usize + column as usize)
    }
}

impl AnchorIndex {
    /// The index of `anchors`, with a grid for each sheet whose anchors fill at least a quarter of
    /// their rectangle (plus 4,096 cells of slack) and whose rectangle holds at most 2^24 cells.
    pub(crate) fn build(anchors: &BTreeMap<CellKey, NodeId>) -> Self {
        let mut index = AnchorIndex::default();
        let mut bounds: BTreeMap<SheetId, (u32, u32, u32, u32, usize)> = BTreeMap::new();
        for at in anchors.keys() {
            let entry = bounds
                .entry(at.sheet())
                .or_insert((u32::MAX, u32::MAX, 0, 0, 0));
            entry.0 = entry.0.min(at.row());
            entry.1 = entry.1.min(at.column());
            entry.2 = entry.2.max(at.row());
            entry.3 = entry.3.max(at.column());
            entry.4 += 1;
        }
        for (sheet, (top, left, bottom, right, count)) in bounds {
            let (rows, columns) = (bottom - top + 1, right - left + 1);
            let area = u64::from(rows) * u64::from(columns);
            if area <= 1 << 24 && area <= 4 * count as u64 + 4096 {
                let slot = sheet.slot() as usize;
                if slot >= index.grids.len() {
                    index.grids.resize_with(slot + 1, || None);
                }
                let grid = AnchorGrid {
                    top,
                    left,
                    rows,
                    columns,
                    cells: vec![None; area as usize],
                };
                index.grids[slot] = Some((sheet, grid));
            }
        }
        for (at, node) in anchors {
            index.insert(*at, *node);
        }
        index
    }

    fn grid(&self, sheet: SheetId) -> Option<&AnchorGrid> {
        match self.grids.get(sheet.slot() as usize) {
            Some(Some((held, grid))) if *held == sheet => Some(grid),
            _ => None,
        }
    }

    fn grid_mut(&mut self, sheet: SheetId) -> Option<&mut AnchorGrid> {
        match self.grids.get_mut(sheet.slot() as usize) {
            Some(Some((held, grid))) if *held == sheet => Some(grid),
            _ => None,
        }
    }

    pub(crate) fn get(&self, at: CellKey) -> Option<NodeId> {
        if let Some(grid) = self.grid(at.sheet())
            && let Some(position) = grid.position(at)
        {
            return grid.cells[position];
        }
        self.other.get(&at).copied()
    }

    pub(crate) fn insert(&mut self, at: CellKey, node: NodeId) {
        if let Some(grid) = self.grid_mut(at.sheet())
            && let Some(position) = grid.position(at)
        {
            grid.cells[position] = Some(node);
            return;
        }
        self.other.insert(at, node);
    }

    pub(crate) fn remove(&mut self, at: CellKey) {
        if let Some(grid) = self.grid_mut(at.sheet())
            && let Some(position) = grid.position(at)
        {
            grid.cells[position] = None;
            return;
        }
        self.other.remove(&at);
    }
}

/// Each bound node's binding, at the node's slot (dc-dxm: every evaluation looked its node's
/// binding up in a BTreeMap). The methods are the map's that the engine uses.
#[derive(Clone, Debug, Default)]
pub(crate) struct Bindings {
    slots: Vec<Option<(NodeId, Binding)>>,
}

impl Bindings {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&NodeId, &Binding)> {
        self.slots
            .iter()
            .flatten()
            .map(|(node, binding)| (node, binding))
    }
    pub(crate) fn get(&self, node: &NodeId) -> Option<&Binding> {
        match self.slots.get(node.slot() as usize) {
            Some(Some((held, binding))) if held == node => Some(binding),
            _ => None,
        }
    }

    pub(crate) fn contains_key(&self, node: &NodeId) -> bool {
        self.get(node).is_some()
    }

    pub(crate) fn insert(&mut self, node: NodeId, binding: Binding) {
        let slot = node.slot() as usize;
        if slot >= self.slots.len() {
            self.slots.resize_with(slot + 1, || None);
        }
        self.slots[slot] = Some((node, binding));
    }

    pub(crate) fn remove(&mut self, node: &NodeId) {
        if let Some(entry) = self.slots.get_mut(node.slot() as usize)
            && entry.as_ref().is_some_and(|(held, _)| held == node)
        {
            *entry = None;
        }
    }
}

/// How a bound formula's result fills cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArrayRole {
    /// A formula that is not an array formula: its result is the one value of its cell.
    Cell,
    /// A legacy array formula of one cell: its cell shows the first element of its result.
    LegacyCell,
    /// A legacy array formula filling this fixed area.
    LegacyFill(GridArea),
    /// A dynamic-array formula: its result spills from its anchor.
    Spill,
}

/// The engine's derived state for one workbook, owned by its caller (plan 3.1): node IDs interned
/// from the cells whose formulas they are, the formula-node records, one graph whose edges may
/// cross sheets, each node's bound plan, and the published generation of formula results. It
/// holds no authored input, no source bytes and no formatting; literal inputs are read through
/// the pages of the immutable snapshot each call borrows.
///
/// A node's ID is issued by the state's own [`IdAllocator`] and keyed by its anchor [`CellKey`],
/// which carries the sheet, so equal rows and columns on two sheets are two nodes. A removed
/// node's ID is released and never equals a later one.
///
/// A node is pending when a change reached it directly: each node of a new state, each node an
/// edit adds or replaces, and each node that reads a cell an edit changes (the dependents of the
/// edit's seeds). A node that reads a pending node's cell, directly or through other nodes, is
/// not pending itself, but its published result is stale all the same: the pending nodes and
/// those readers are the dirty closure, which [`EngineState::stale`] lists and the next
/// recalculation calculates. Only a finished recalculation clears a node from pending, by
/// publishing its result; after a recalculation fails or is refused, the pending nodes stay
/// pending and [`EngineState::stale`] still lists every result it left stale.
#[derive(Debug)]
pub struct EngineState {
    pub(crate) linked_workbooks: Arc<crate::linked::LinkedWorkbooks>,
    pub(crate) linked_input_version: u64,
    pub(crate) contract: crate::contract::ContractState,
    revision: AuthoredRevision,
    pub(crate) callables: calc_fml::CallableBindings,
    pub(crate) ids: Option<IdAllocator<NodeId>>,
    pub(crate) destination: crate::contract::ResultDestination,
    tick_sequence: Arc<std::sync::atomic::AtomicU64>,
    view_sequence: Arc<std::sync::atomic::AtomicU64>,
    pub(crate) by_anchor: BTreeMap<CellKey, NodeId>,
    /// The same entries as `by_anchor`, for lookup ([`AnchorIndex`]): [`EngineState::node_at`],
    /// which every read of a formula cell asks, looks here; `by_anchor` keeps them in cell order
    /// for the callers that walk or range over them. Both change together, in
    /// [`EngineState::add_node`], [`EngineState::remove_node`] and [`EngineState::apply_moves`]
    /// alone.
    anchor_index: AnchorIndex,
    /// Node records by ID slot.
    pub(crate) nodes: Vec<Option<FormulaNode>>,
    pub(crate) index: ReverseIndex,
    /// The plans of every formula the state has compiled, one per normal form.
    pub(crate) plans: PlanCache,
    /// Each bound node's plan; a node without an entry is unbound, and pending.
    pub(crate) bindings: Bindings,
    /// The published generation: each formula cell's result ([`Results`]).
    pub(crate) results: Results,
    /// The nodes a change reached directly; the roots of the dirty closure.
    pub(crate) pending: BTreeSet<NodeId>,
    /// P02 Manual range calculation fulfils its own command but asks the next authorized
    /// calculation that includes these nodes to evaluate them again. This is demand, not an
    /// assertion that the just-published scoped value was computed from changed inputs.
    pub(crate) range_redemand: BTreeSet<NodeId>,
    /// The anchors of nodes removed since the last publication, whose results it removes.
    pub(crate) retired: BTreeSet<CellKey>,
    /// The last tick a recalculation began with; 0 before the first.
    pub(crate) last_tick: u64,
    /// How many recalculations have published.
    pub(crate) generation: u64,
    /// The numeric mode of the last recalculation that published; `None` before the first. Every
    /// node that is not pending was last calculated under it.
    pub(crate) published_mode: Option<NumericMode>,
    /// The bound nodes whose formulas call a volatile function; every recalculation calculates
    /// them.
    pub(crate) volatile: BTreeSet<NodeId>,
    /// The anchors of the bound formulas that call SUBTOTAL or AGGREGATE, which those functions
    /// leave out of the ranges they read.
    pub(crate) subtotal_cells: BTreeSet<CellKey>,
    /// The published spills: each array formula's filled cells, by anchor.
    pub(crate) spills: SpillIndex,
    /// Complete imported array images, keyed by the original formula instance.
    /// They are never calculated spills or producing invocations.
    pub(crate) cached_arrays: BTreeMap<NodeId, Spill>,
    pub(crate) blocked_spills: BTreeMap<CellKey, GridArea>,
    /// The seed RAND, RANDBETWEEN and RANDARRAY draw from, mixed with each recalculation's tick.
    pub(crate) random_seed: u64,
    /// The plan of each formula template compiled ([`calc_fml::reference::template_key`]), per
    /// sheet and form, so a formula whose template was compiled is placed without parsing. Like
    /// the plan cache's plans, they were compiled against the state's sheets and defined names:
    /// sheet edits are refused (dc-m3u), and an edit that sets a defined name's formula (dc-gf0)
    /// empties them when it commits.
    pub(crate) templates:
        HashMap<(SheetId, calc_doc_model::SemanticTemplateKey), Arc<CompiledPlan>>,
    /// Stable date and locale semantics, apart from the pass clock. `None` refuses the date, time and
    /// number-format functions.
    pub(crate) environment: Option<calc_fml::SemanticEnvironment>,
    pub(crate) clock: Option<f64>,
    pub(crate) volatile_input_version: u64,
    /// The host's facts about the workbook's file, formats and views and the application, which
    /// CELL and INFO read (dc-uar): one snapshot the host gives before each recalculation
    /// ([`EngineState::set_workbook_facts`]). `None` refuses what they would supply.
    pub(crate) workbook_facts: Option<Arc<dyn calc_fml::WorkbookFacts>>,
    /// The formulas the last published recalculation found on a cycle (L3, dc-m3u), and with
    /// iteration off those that read one (dc-10y); every recalculation calculates them again, as
    /// Excel calculates such a cell whenever it calculates (it saves each with `ca="1"`).
    pub(crate) cycle_members: BTreeSet<NodeId>,
    /// The formula nodes a cell edit added or replaced since their last published result
    /// (dc-10y): with iteration off, such a formula on a circular reference shows 0, where one
    /// that was not entered again keeps its value.
    pub(crate) entered: BTreeSet<NodeId>,
    /// Whether recalculations leave the cells of data tables, and the formulas that read only
    /// through them, uncalculated (L3, dc-m3u): Excel's "Automatic except for data tables".
    pub(crate) tables_deferred: bool,
    /// Counts the changes to the published spills' areas and to the nodes' anchors, which the
    /// readers of a node depend on beside the graph's edges (dc-10d9).
    pub(crate) spill_epoch: u64,
    /// Each node's readers as the last recalculation's ordering found them, kept while nothing
    /// they depend on has changed (dc-10d9, [`ReadersCache`]).
    pub(crate) readers_cache: ReadersCache,
    /// Counts the nodes added, removed and moved (dc-dxm), which the kept readers and order
    /// depend on beside the graph's edges.
    pub(crate) anchor_epoch: u64,
    /// How many threads a recalculation may evaluate on (dc-2m0); 1, the default, on one.
    pub(crate) threads: usize,
    /// The fewest formulas a recalculation, and a level of it, must hold to be evaluated on
    /// several threads (dc-2m0).
    pub(crate) parallel_minimum: ParallelMinimum,
}

/// Opaque input/semantic identity for owner operations spanning several tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalculationInputVersion {
    environment: u64,
    volatile: u64,
    external: u64,
    linked: u64,
}

/// The fewest formulas a recalculation's first wave, and one of its levels, must hold to be
/// evaluated on several threads (dc-2m0): below them, starting threads costs more than it saves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParallelMinimum {
    pub(crate) wave: usize,
    pub(crate) level: usize,
}

impl Default for ParallelMinimum {
    fn default() -> Self {
        ParallelMinimum {
            wave: 1024,
            level: 64,
        }
    }
}

impl EngineState {
    pub fn input_version(&self) -> CalculationInputVersion {
        CalculationInputVersion {
            environment: self.contract.environment,
            volatile: self.volatile_input_version,
            external: self.contract.external_inputs,
            linked: self.linked_input_version,
        }
    }
    fn result_group(&self, node: NodeId, at: CellKey) -> Option<ResultGroup> {
        let Binding::Placed(instance, role) = self.bindings.get(&node)? else {
            return None;
        };
        let role = match role {
            ArrayRole::Cell => 0,
            ArrayRole::Spill => 1,
            ArrayRole::LegacyCell | ArrayRole::LegacyFill(_) => return None,
        };
        Some(ResultGroup {
            sheet: at.sheet(),
            plan: Arc::as_ptr(instance.plan()) as usize,
            role,
        })
    }
    pub(crate) fn publish_result(&mut self, node: NodeId, at: CellKey, result: FormulaResult) {
        let group = self.result_group(node, at);
        self.results.publish(node, at, result, group);
    }
    /// Physical result rows, distinct from calculation coverage or freshness.
    pub fn result_storage_stats(&self) -> crate::ResultStorageStats {
        self.results.stats()
    }
    /// Regroup retained values by their existing bound plans and remove vacant rows.
    /// The budget bounds inspected index entries, anchors, columns and rows; an insufficient
    /// budget declines before mutation. This never evaluates, certifies or publishes a value.
    pub fn compact_result_storage(
        &mut self,
        max_slots: usize,
    ) -> Option<crate::ResultStorageStats> {
        if !self.results.can_compact(self.by_anchor.len(), max_slots) {
            return None;
        }
        let assignments = self
            .by_anchor
            .iter()
            .map(|(at, node)| (*node, (*at, self.result_group(*node, *at))))
            .collect();
        self.results.compact(assignments);
        Some(self.results.stats())
    }

    pub(crate) fn node_ids(&mut self) -> &mut IdAllocator<NodeId> {
        self.ids
            .as_mut()
            .expect("pinned calculations cannot edit formula membership")
    }
    pub(crate) fn begin_tick(&mut self) {
        self.last_tick = self
            .tick_sequence
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |tick| tick.checked_add(1),
            )
            .expect("calculation tick exhausted")
            + 1;
    }
    pub(crate) fn advance_view(&mut self) {
        self.generation = self
            .view_sequence
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |view| view.checked_add(1),
            )
            .expect("result view identity exhausted")
            + 1;
    }
    // Only PinnedCalculation can own this state. It exposes no authored mutation API,
    // so the one node allocator stays with the live owner. Immutable plans/values share
    // their payloads; derived graph/result indexes are copied for this explicit save.
    pub(crate) fn snapshot_for_save(&self, save: crate::contract::CommandId) -> Self {
        Self {
            linked_workbooks: self.linked_workbooks.clone(),
            linked_input_version: self.linked_input_version,
            contract: self.contract.clone(),
            revision: self.revision,
            callables: self.callables.clone(),
            ids: None,
            destination: crate::contract::ResultDestination::PinnedSave { save },
            tick_sequence: self.tick_sequence.clone(),
            view_sequence: self.view_sequence.clone(),
            by_anchor: self.by_anchor.clone(),
            anchor_index: AnchorIndex::build(&self.by_anchor),
            nodes: self.nodes.clone(),
            index: self.index.clone(),
            plans: self.plans.clone(),
            bindings: self.bindings.clone(),
            results: self.results.clone(),
            pending: self.pending.clone(),
            range_redemand: self.range_redemand.clone(),
            retired: self.retired.clone(),
            last_tick: self.last_tick,
            generation: self.generation,
            published_mode: self.published_mode.clone(),
            volatile: self.volatile.clone(),
            subtotal_cells: self.subtotal_cells.clone(),
            spills: self.spills.clone(),
            cached_arrays: self.cached_arrays.clone(),
            blocked_spills: self.blocked_spills.clone(),
            random_seed: self.random_seed,
            templates: self.templates.clone(),
            environment: self.environment,
            clock: self.clock,
            volatile_input_version: self.volatile_input_version,
            workbook_facts: self.workbook_facts.clone(),
            cycle_members: self.cycle_members.clone(),
            entered: self.entered.clone(),
            tables_deferred: self.tables_deferred,
            spill_epoch: self.spill_epoch,
            readers_cache: ReadersCache::default(),
            anchor_epoch: self.anchor_epoch,
            threads: self.threads,
            parallel_minimum: self.parallel_minimum,
        }
    }

    /// Publish binding metadata only. Executable owners remain on the serial host/command.
    /// Positive and negative callable watches rebind only consumers of changed entries.
    pub fn set_callable_bindings(&mut self, inputs: &ModelRoot, next: calc_fml::CallableBindings) {
        if self.callables == next {
            return;
        }
        let affected: BTreeSet<_> = self
            .bindings
            .iter()
            .filter_map(|(node, binding)| {
                let changed = match binding {
                    Binding::Placed(instance, _) => instance
                        .plan()
                        .callable_bindings()
                        .iter()
                        .any(|proof| !next.validates(proof)),
                    Binding::Refused(NotModeled::Compile(
                        calc_fml::CompileError::UnknownFunction { .. },
                    )) => true,
                    _ => false,
                };
                (changed
                    || self
                        .results
                        .of(*node)
                        .and_then(FormulaResult::stamp)
                        .is_some_and(|stamp| {
                            stamp
                                .reads
                                .callables
                                .iter()
                                .any(|proof| !next.validates(proof))
                        }))
                .then_some(*node)
            })
            .collect();
        let dirty = crate::recalc::dirty_closure(self, inputs, affected.clone(), &|node| {
            crate::recalc::invalidation_areas(self, inputs, node)
        })
        .members()
        .to_vec();
        for node in affected {
            self.rebind_names(node);
        }
        self.contract.invalidate(dirty);
        self.templates.retain(|_, plan| {
            plan.callable_bindings()
                .iter()
                .all(|proof| next.validates(proof))
        });
        self.callables = next;
    }

    /// An RTD/subscription event advances the next command's input cut. It is neither an
    /// authored cell edit nor an async return, and never supplies a formula result directly.
    pub fn invalidate_callable_input(
        &mut self,
        inputs: &ModelRoot,
        key: calc_fml::UserInputKey,
        sequence: u64,
    ) {
        let provider = key.callable;
        let roots:BTreeSet<_>=self.bindings.iter().filter_map(|(node,binding)| {
            let watched=matches!(binding,Binding::Placed(instance,_) if instance.plan().callable_bindings().iter().any(|proof| proof.bound==Some(provider)))
                || self.results.of(*node).and_then(FormulaResult::stamp).is_some_and(|stamp| stamp.reads.callables.iter().any(|proof| proof.bound==Some(provider)));
            watched.then_some(*node)
        }).collect();
        let dirty = crate::recalc::dirty_closure(self, inputs, roots.clone(), &|n| {
            crate::recalc::invalidation_areas(self, inputs, n)
        })
        .members()
        .to_vec();
        self.pending.extend(roots);
        self.contract.invalidate(dirty);
        self.contract.external_versions.insert(key, sequence);
        self.contract.external_inputs = self
            .contract
            .external_inputs
            .checked_add(1)
            .expect("external input epoch exhausted");
        self.volatile_input_version = self
            .volatile_input_version
            .checked_add(1)
            .expect("external input epoch exhausted");
    }

    /// The state of the snapshot `inputs`, at its revision: one unbound, pending node for each
    /// formula cell, its ID issued in sheet order and then in row-major order, read through the
    /// snapshot's pages. Literal cells get no node and nothing of them is copied. No result is
    /// published until the first recalculation, which calculates every node.
    pub fn new(inputs: &ModelRoot) -> Self {
        let mut state = EngineState {
            linked_workbooks: Default::default(),
            linked_input_version: 0,
            contract: crate::contract::ContractState::new(),
            callables: calc_fml::CallableBindings::default(),
            revision: inputs.revision(),
            ids: Some(IdAllocator::new()),
            destination: crate::contract::ResultDestination::Live,
            tick_sequence: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            view_sequence: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            by_anchor: BTreeMap::new(),
            anchor_index: AnchorIndex::default(),
            nodes: Vec::new(),
            index: ReverseIndex::default(),
            plans: PlanCache::new(),
            bindings: Bindings::default(),
            results: Results::default(),
            pending: BTreeSet::new(),
            range_redemand: BTreeSet::new(),
            retired: BTreeSet::new(),
            last_tick: 0,
            generation: 0,
            published_mode: None,
            entered: BTreeSet::new(),
            volatile: BTreeSet::new(),
            subtotal_cells: BTreeSet::new(),
            spills: SpillIndex::default(),
            cached_arrays: BTreeMap::new(),
            blocked_spills: BTreeMap::new(),
            random_seed: 0,
            templates: HashMap::new(),
            environment: None,
            clock: None,
            volatile_input_version: 0,
            workbook_facts: None,
            cycle_members: BTreeSet::new(),
            tables_deferred: false,
            spill_epoch: 0,
            readers_cache: ReadersCache::default(),
            anchor_epoch: 0,
            threads: 1,
            parallel_minimum: ParallelMinimum::default(),
        };
        for sheet in inputs.sheets() {
            for (at, input) in inputs.sheet_cells(sheet.id()).into_iter().flatten() {
                if let CellInput::Formula(_) = input {
                    let id = issue_node_id(state.node_ids());
                    state.record_node(id, at);
                    state.pending.insert(id);
                }
            }
        }
        // L3 (dc-m3u): each cell of a what-if data table is a node, calculated from its table.
        for table in inputs.definitions().data_tables() {
            let area = table.area();
            for row in area.first().row()..=area.last().row() {
                for column in area.first().column()..=area.last().column() {
                    let at = CellKey::new(area.sheet(), row, column)
                        .expect("a cell of the table's area");
                    if state.by_anchor.contains_key(&at)
                        || matches!(inputs.cell(at), Some(CellInput::Formula(_)))
                    {
                        continue;
                    }
                    let id = issue_node_id(state.node_ids());
                    state.record_node(id, at);
                    state.pending.insert(id);
                }
            }
        }
        state.anchor_index = AnchorIndex::build(&state.by_anchor);
        state
    }

    /// Whether recalculations leave the cells of what-if data tables, and the formulas that read
    /// only through them, uncalculated: Excel's "Automatic except for data tables", which a host
    /// sets until a recalculation is asked for (L3, dc-m3u). Those nodes stay pending.
    pub fn set_tables_deferred(&mut self, deferred: bool) {
        self.tables_deferred = deferred;
    }

    /// Whether a recalculation would calculate anything: a pending node, other than a data
    /// table's cell while tables are deferred ([`EngineState::set_tables_deferred`]).
    pub fn needs_recalc(&self, inputs: &ModelRoot) -> bool {
        self.pending.iter().chain(&self.range_redemand).any(|node| {
            !(self.tables_deferred
                && self.node(*node).is_some_and(|record| {
                    crate::data_table::is_table_cell(inputs, record.anchor())
                }))
        })
    }

    /// The state of an opened file's snapshot, as [`EngineState::new`] makes it, with the results
    /// the file cached for its formulas published as [`FormulaResult::FileCached`]. A cache is
    /// never counts as calculated: qualified boundary reads can consume it while every node
    /// stays pending. Array-child caches are accepted only inside their declared array; only
    /// complete images can supply boundary reads. Other caches without a formula are refused.
    pub fn with_file_caches(
        inputs: &ModelRoot,
        caches: impl IntoIterator<Item = (CellKey, FileCached)>,
    ) -> Result<Self, CacheWithoutFormula> {
        let mut state = EngineState::new(inputs);
        let mut children = BTreeMap::new();
        for (at, cache) in caches {
            if let Some(node) = state.node_at(at) {
                state
                    .results
                    .publish(node, at, FormulaResult::FileCached(cache), None);
            } else if inputs.cell(at).is_none()
                && inputs.definitions().unloaded(at).is_none()
                && inputs.definitions().array_containing(at).is_some()
            {
                children.insert(at, cache);
            } else {
                return Err(CacheWithoutFormula { at });
            }
        }
        for array in inputs.definitions().arrays() {
            let at = array.anchor();
            let Some(node) = state.node_at(at) else {
                continue;
            };
            let Some(FormulaResult::FileCached(anchor)) = state.results.of(node) else {
                continue;
            };
            let mut values = BTreeMap::new();
            values.insert(at, anchor.value().to_value());
            for (&cell, cache) in children.range(array.area().first()..=array.area().last()) {
                if array.area().contains(cell) && cache.source() == anchor.source() {
                    values.insert(cell, cache.value().to_value());
                }
            }
            if values.len() as u64 == array.area().cell_count() {
                let shape = calc_value::ArrayShape {
                    rows: array.area().row_count() as usize,
                    cols: array.area().column_count() as usize,
                };
                let values = calc_value::CalcArray::new(shape, values.into_values().collect())
                    .expect("one cached value per array cell");
                state.cached_arrays.insert(
                    node,
                    crate::spill::cached_spill(
                        array.area(),
                        values,
                        matches!(array.kind(), calc_doc_model::ArrayKind::Legacy),
                    ),
                );
            }
        }
        Ok(state)
    }

    /// How many threads a recalculation may evaluate its formulas on (dc-2m0): at least 1. On
    /// wasm32 every recalculation runs on one thread whatever this says; elsewhere a
    /// recalculation runs on several only when nothing in it depends on the order of evaluation
    /// beyond its values (see `recalc.rs`, module `parallel`), and publishes the values the
    /// one-thread route would.
    pub fn set_threads(&mut self, threads: usize) {
        self.threads = threads.max(1);
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Sets the fewest formulas a recalculation must hold to be evaluated on several threads, and
    /// the fewest a level of it must (default 1,024 and 64; at least 1). It changes no value, only
    /// where threads are used; the tests set it to 1 so that small workbooks take the parallel
    /// route.
    pub fn set_parallel_minimum(&mut self, wave: usize, level: usize) {
        self.parallel_minimum = ParallelMinimum {
            wave: wave.max(1),
            level: level.max(1),
        };
    }

    /// The authored revision the graph matches.
    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    /// The node whose formula is in the cell `at`.
    pub fn node_at(&self, at: CellKey) -> Option<NodeId> {
        self.anchor_index.get(at)
    }

    /// The record of a live node.
    pub fn node(&self, id: NodeId) -> Option<&FormulaNode> {
        self.nodes
            .get(id.slot() as usize)
            .and_then(Option::as_ref)
            .filter(|node| node.id == id)
    }

    /// Every node, in the order of their anchors (sheet ID, then row, then column).
    pub fn nodes(&self) -> impl Iterator<Item = &FormulaNode> {
        self.by_anchor.values().filter_map(|id| self.node(*id))
    }

    pub fn node_count(&self) -> usize {
        self.by_anchor.len()
    }

    /// The formula cells the last published recalculation found on a circular reference, in
    /// cell order (dc-y2o.1): those on a cycle of their references (`=IF(C1>5,I1+1,7)` in I1
    /// too, whose evaluation need not meet itself), those on a cycle only their evaluations met,
    /// and with iteration off those that read one, which keep their values by the
    /// circular-reference rule. A host warns of a circular reference while this is not empty.
    pub fn circular_cells(&self) -> Vec<CellKey> {
        let mut cells: Vec<CellKey> = self
            .cycle_members
            .iter()
            .filter(|node| self.node(**node).is_some())
            .map(|node| crate::recalc::live_anchor(self, *node))
            .collect();
        cells.sort();
        cells
    }

    /// The published result of the formula cell `at`.
    pub fn result(&self, at: CellKey) -> Option<&FormulaResult> {
        match self.node_at(at) {
            Some(node) => self.result_of(node, at),
            None => self.results.orphan(at),
        }
    }

    /// The published result of the live node `node`, whose anchor is `at`: its own, or else the
    /// result a removed node left at the cell.
    pub(crate) fn result_of(&self, node: NodeId, at: CellKey) -> Option<&FormulaResult> {
        self.results.of(node).or_else(|| self.results.orphan(at))
    }

    /// Every published result, in the order of the cells (sheet ID, then row, then column).
    pub fn results(&self) -> impl Iterator<Item = (CellKey, &FormulaResult)> {
        let mut all: Vec<(CellKey, &FormulaResult)> = self
            .results
            .live()
            .filter_map(|(node, result)| Some((self.node(node)?.anchor(), result)))
            .chain(self.results.orphans())
            .collect();
        all.sort_unstable_by_key(|(at, _)| *at);
        all.into_iter()
    }

    /// The nodes a change reached directly, in ID order: each node of a new state, each node an
    /// edit added or replaced, and each node that reads a cell an edit changed. The nodes that
    /// read them are not listed here; [`EngineState::stale`] lists the whole dirty closure.
    pub fn pending(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.pending.iter().copied()
    }

    /// The formula cells whose published result is stale, in the order of the cells (sheet ID,
    /// then row, then column): the anchors of the pending nodes and of every node that reads one
    /// of them (through a placed reference, a dynamic prerequisite or a cell of its spill),
    /// directly or through other nodes. The next recalculation under the published generation's
    /// numeric mode calculates these and also every formula that calls a volatile function, with
    /// its readers: a volatile formula's published result is not stale for that, it is only
    /// calculated again ([`EngineState::is_volatile`]). After a recalculation fails or
    /// is refused nothing of it was published, so this lists every result it left stale. `None`
    /// when `inputs` is not at the state's revision: a span's sheets are read from the
    /// snapshot's sheet order.
    pub fn stale(&self, inputs: &ModelRoot) -> Option<Vec<CellKey>> {
        if inputs.revision() != self.revision {
            return None;
        }
        // A pending dynamic array's next result may spill anywhere below and right of its anchor,
        // past the area it last filled, so every formula reading a cell there may read it: those
        // readers are stale too, until the recalculation settles the spill.
        let areas = |node| crate::recalc::invalidation_areas(self, inputs, node);
        let mut pending = self.pending.clone();
        let progress = crate::read::Progress::new(0);
        pending.extend(
            self.nodes()
                .filter(|node| {
                    self.results
                        .of(node.id())
                        .and_then(FormulaResult::stamp)
                        .is_some_and(|stamp| {
                            !crate::contract::certificate_valid(
                                self,
                                inputs,
                                &progress,
                                stamp,
                                self.contract.facts,
                            )
                        })
                })
                .map(|node| node.id()),
        );
        let mut cells: Vec<CellKey> = dirty_closure(self, inputs, pending, &areas)
            .members()
            .iter()
            .copied()
            .map(|node| live_anchor(self, node))
            .collect();
        // Removed anchors still own their previous spill until publication removes it. Their
        // followers must show stale provenance rather than an orphan's calculated value.
        cells.extend(self.retired.iter().copied());
        cells.sort();
        Some(cells)
    }

    /// The bound plan of a live node: its compiled plan, shared through the state's plan cache
    /// with every formula of the same normal form, placed at its anchor. `None` while the node is
    /// unbound, and when CalcFml refused to compile its formula. A node is bound once, by the
    /// first [`crate::begin_recalc`] that accepts the snapshot after the node was added or
    /// replaced, and keeps its plan until an edit replaces or removes its formula.
    pub fn instance(&self, node: NodeId) -> Option<&FormulaInstance> {
        self.node(node)?;
        match self.bindings.get(&node)? {
            Binding::Placed(instance, _) => Some(instance),
            Binding::Refused(_) | Binding::DataTable(_) => None,
        }
    }

    /// Whether a live, bound node's formula calls a volatile function, so that every
    /// recalculation calculates it (see [`calc_fml::CompiledPlan::volatile`]).
    pub fn is_volatile(&self, node: NodeId) -> bool {
        self.volatile.contains(&node)
    }

    /// The attempted extent of the last published blocked spill. Callers must check that its result is current.
    pub fn blocked_spill_area(&self, anchor: CellKey) -> Option<GridArea> {
        self.blocked_spills.get(&anchor).copied()
    }

    /// The published spill of the array formula anchored at `anchor`; absent for a scalar or blocked array.
    pub fn spill(&self, anchor: CellKey) -> Option<&Spill> {
        self.spills.get(anchor)
    }

    /// The published spill whose filled cells include `at`.
    pub fn spill_covering(&self, at: CellKey) -> Option<&Spill> {
        self.spills.covering(at)
    }

    /// A held cache owned by the current formula node, excluding re-entry and orphaned
    /// results. It can be stale or imported; this grants no calculation or freshness proof.
    pub fn permitted_cache(&self, at: CellKey) -> Option<&FormulaResult> {
        let node = self.node_at(at)?;
        if self.entered.contains(&node) {
            return None;
        }
        match self.results.of(node)? {
            result @ (FormulaResult::Calculated { .. } | FormulaResult::FileCached(_)) => {
                Some(result)
            }
            FormulaResult::NotModeled { .. } => None,
        }
    }

    /// A local obligation, including Manual range demand retained for the next command.
    /// Transitive stale readers are additionally reported by `stale`.
    pub fn calculation_required_at(&self, at: CellKey) -> bool {
        self.node_at(at).is_some_and(|node| {
            let requirement = self.contract.requirement(node);
            self.pending.contains(&node)
                || self.range_redemand.contains(&node)
                || requirement.required != requirement.satisfied
        })
    }

    pub fn result_view(&self) -> crate::contract::ReadViewId {
        crate::contract::ReadViewId(self.generation)
    }

    pub fn workbook_instance(&self) -> crate::contract::WorkbookInstanceId {
        self.contract.workbook
    }

    pub fn requirement_at(&self, at: CellKey) -> Option<crate::contract::RequirementState> {
        Some(self.contract.requirement(self.node_at(at)?))
    }

    pub fn validated_result(
        &self,
        inputs: &ModelRoot,
        at: CellKey,
    ) -> Option<&crate::contract::FreshnessClaim> {
        let stamp = self.result(at)?.stamp()?;
        if stamp.reads.legacy_boundary
            || stamp.reads.external_cache
            || stamp.reads.uses_boundary_cache
            || stamp.reads.uses_imported_cache
        {
            return None;
        }
        let current = self.contract.requirement(stamp.target.node);
        (current.required == current.satisfied
            && crate::contract::certificate_valid(
                self,
                inputs,
                &crate::read::Progress::new(0),
                stamp,
                self.contract.facts,
            ))
        .then_some(&stamp.certification)
    }

    /// Every published spill, in anchor order.
    pub fn spills(&self) -> impl Iterator<Item = &Spill> {
        self.spills.iter()
    }

    /// The seed RAND, RANDBETWEEN and RANDARRAY draw from. Each recalculation mixes it with its
    /// tick and each formula's cell, so the draws differ from one recalculation to the next and
    /// a recalculation's draws can be reproduced from the seed and the tick.
    pub fn random_seed(&self) -> u64 {
        self.random_seed
    }

    /// Sets the seed RAND, RANDBETWEEN and RANDARRAY draw from (see
    /// [`EngineState::random_seed`]).
    pub fn set_random_seed(&mut self, seed: u64) {
        if self.random_seed != seed {
            self.random_seed = seed;
            self.volatile_input_version = self
                .volatile_input_version
                .checked_add(1)
                .expect("volatile input version exhausted");
            self.pending.extend(self.volatile.iter().copied());
            self.contract.invalidate(self.volatile.iter().copied());
            self.contract
                .facility_roots
                .extend(self.volatile.iter().copied());
        }
    }

    /// The environment every evaluation runs in (see [`EngineState::set_environment`]).
    pub fn environment(&self) -> Option<CalcEnvironment> {
        self.environment.map(|e| e.at(self.clock))
    }

    /// Sets the environment every evaluation runs in: the workbook's date system, the host's
    /// locale profile and clock. Date or locale changes invalidate every formula; clock changes
    /// invalidate volatile formulas and their dependents.
    /// The state of `inputs` built afresh ([`EngineState::new`]: every formula a node, every node
    /// pending, nothing published), keeping this state's tick counter, environment and random
    /// seed, so the next recalculation's tick follows this state's last. A host commits it for an
    /// edit that brings back a whole earlier revision ([`calc_doc_model::ModelRoot::restored`]).
    pub fn rebuilt(&self, inputs: &ModelRoot) -> EngineState {
        let mut state = EngineState::new(inputs);
        state.linked_workbooks = self.linked_workbooks.clone();
        state.linked_input_version = self.linked_input_version;
        state.callables = self.callables.clone();
        state.last_tick = self.last_tick;
        state.tick_sequence = self.tick_sequence.clone();
        state.view_sequence = self.view_sequence.clone();
        state.advance_view();
        state.environment = self.environment;
        state.clock = self.clock;
        state.volatile_input_version = self.volatile_input_version;
        state.workbook_facts = self.workbook_facts.clone();
        state.random_seed = self.random_seed;
        state.tables_deferred = self.tables_deferred;
        let nodes = state.by_anchor.values().copied().collect();
        state.contract.inherit_identity(&self.contract, nodes);
        state
    }

    /// Rebuild derived bindings/edges before a public ForceAll command. Authored
    /// identities, held caches and spills survive; existing result proofs do not.
    /// The owner must have no active calculation using this mutable graph.
    pub fn rebuild_bindings(&mut self) {
        let nodes: Vec<_> = self.nodes().map(|node| node.id()).collect();
        for node in nodes {
            self.rebind_names(node);
        }
        self.templates.clear();
        self.plans = PlanCache::new();
        self.contract.bindings = self.contract.advance();
        self.mark_all_pending();
    }

    /// Sets the host's facts about the workbook's file, formats and views and the application
    /// that CELL and INFO read (dc-uar). Nothing is made pending: those functions are volatile,
    /// so every recalculation calculates them and reads the facts set last.
    pub fn set_workbook_facts(&mut self, facts: Option<Arc<dyn calc_fml::WorkbookFacts>>) {
        self.workbook_facts = facts;
        self.contract.facts = self.contract.advance();
    }

    pub fn set_environment(&mut self, environment: Option<CalcEnvironment>) {
        let semantics = environment.map(Into::into);
        let clock = environment.and_then(|e| e.now);
        if self.environment != semantics {
            self.environment = semantics;
            self.contract.environment = self.contract.advance();
            self.contract.invalidate(self.by_anchor.values().copied());
            self.pending.extend(self.by_anchor.values().copied());
        } else if self.clock != clock {
            self.pending.extend(self.volatile.iter().copied());
            self.contract.invalidate(self.volatile.iter().copied());
            self.contract
                .facility_roots
                .extend(self.volatile.iter().copied());
        }
        if self.clock != clock {
            self.volatile_input_version = self
                .volatile_input_version
                .checked_add(1)
                .expect("clock epoch exhausted");
        }
        self.clock = clock;
    }

    /// Advances only the pass clock. Its readers are volatile; date and locale semantics stay
    /// unchanged, so unrelated formulas keep their calculated results.
    pub fn set_clock(&mut self, clock: Option<f64>) {
        if self.clock != clock {
            self.clock = clock;
            self.volatile_input_version = self
                .volatile_input_version
                .checked_add(1)
                .expect("clock epoch exhausted");
            self.pending.extend(self.volatile.iter().copied());
            self.contract.invalidate(self.volatile.iter().copied());
            self.contract
                .facility_roots
                .extend(self.volatile.iter().copied());
        }
    }

    /// Makes every formula node pending, so that the next recalculation calculates every formula,
    /// as Excel's full calculation (`CalculateFull`, Ctrl+Alt+F9) does. Each node keeps its bound
    /// plan and its edges: nothing is compiled again and the graph is not rebuilt.
    pub fn mark_all_pending(&mut self) {
        self.contract.force_all = true;
        self.contract.invalidate(self.by_anchor.values().copied());
        // Built anew in node order, which builds the set in one pass (dc-dxm), rather than by
        // inserting every node.
        let mut nodes: Vec<NodeId> = self.pending.iter().copied().collect();
        nodes.extend(self.by_anchor.values().copied());
        nodes.sort_unstable();
        nodes.dedup();
        self.pending = nodes.into_iter().collect();
    }

    /// Replaces the dynamic prerequisites of a live node and files them in the graph.
    pub(crate) fn set_dynamic(&mut self, node: NodeId, dynamic: Vec<Prerequisite>) {
        if let Some(record) = self
            .nodes
            .get_mut(node.slot() as usize)
            .and_then(Option::as_mut)
            .filter(|record| record.id == node)
        {
            // Most evaluations read nothing beyond their placed references, so the edges are
            // unchanged and the index is left as it is (dc-10d9).
            if record.dynamic == dynamic {
                return;
            }
            let old = std::mem::replace(&mut record.dynamic, dynamic);
            self.index.remove(node, &old);
            self.index.insert(node, &record.dynamic);
        }
    }

    /// States what a live node reads, replacing what it read before, and files each prerequisite
    /// in the graph's reverse indexes. Its product caller is [`crate::begin_recalc`], which binds
    /// each unbound node's formula through CalcFml and states the cells, rectangles and whole
    /// rows or columns its references name; sheet spans have no product caller yet (dc-m3u's
    /// binding and calculation). Every sheet named must be in `inputs`, the snapshot at
    /// this state's revision.
    ///
    /// It is public because ceng-m1.1's acceptance cases state prerequisites with it by hand, the
    /// spans included. Stating prerequisites other than a bound node's placed references leaves
    /// the graph without edges its plan reads; ceng-m1.2's case does so on purpose to show that a
    /// read the graph did not order fails the recalculation
    /// ([`crate::RecalcFailure::UnscheduledRead`]) instead of reading an old value.
    pub fn set_prerequisites(
        &mut self,
        inputs: &ModelRoot,
        node: NodeId,
        prerequisites: Vec<Prerequisite>,
    ) -> Result<(), PrerequisiteError> {
        if inputs.revision() != self.revision {
            return Err(PrerequisiteError::StaleInputs {
                engine: self.revision,
                inputs: inputs.revision(),
            });
        }
        if self.node(node).is_none() {
            return Err(PrerequisiteError::UnknownNode(node));
        }
        if let Some(sheet) = prerequisites
            .iter()
            .flat_map(|prerequisite| prerequisite.sheets())
            .find(|sheet| inputs.sheet_by_id(*sheet).is_none())
        {
            return Err(PrerequisiteError::UnknownSheet(sheet));
        }
        if let Some(id) = prerequisites.iter().find_map(|p| match p {
            Prerequisite::Name(id) if inputs.definitions().name_table().get(*id).is_none() => {
                Some(*id)
            }
            _ => None,
        }) {
            return Err(PrerequisiteError::UnknownName(id));
        }
        let record = self.nodes[node.slot() as usize]
            .as_mut()
            .filter(|record| record.id == node)
            .ok_or(PrerequisiteError::UnknownNode(node))?;
        if let Some(old) = record.prerequisites.take() {
            self.index.remove(node, &old);
        }
        self.index.insert(node, &prerequisites);
        record.prerequisites = Some(prerequisites);
        Ok(())
    }

    /// A non-cell graph participant from the canonical name table. Its body is evaluated
    /// in each use's context, so a name has no shared cell value or synthetic anchor.
    pub fn name_node<'a>(
        &self,
        inputs: &'a ModelRoot,
        id: NameId,
    ) -> Option<&'a calc_doc_model::NameNode> {
        (inputs.revision() == self.revision)
            .then(|| inputs.definitions().name_table().get(id))
            .flatten()
    }

    /// Formula users of this name, including aliases bound in their plans.
    pub fn name_dependents(&self, id: NameId) -> Vec<NodeId> {
        let mut out = BTreeSet::new();
        self.index.name_dependents(id, &mut out);
        out.into_iter().collect()
    }

    /// Names bound from this name's body in live caller contexts.
    pub fn name_prerequisites(&self, id: NameId) -> Vec<NameId> {
        self.index.name_children(id)
    }

    /// The anchors of the nodes inside `area`.
    pub(crate) fn anchors_in(&self, area: GridArea) -> impl Iterator<Item = CellKey> + '_ {
        self.by_anchor
            .range(area.first()..=area.last())
            .map(|(at, _)| *at)
            .filter(move |at| area.contains(*at))
    }

    /// The cells whose formulas read the cell `at` through a value edge (a placed reference or one
    /// their last evaluation computed), in cell order, each once: the anchors of the nodes the
    /// reverse index finds for it (dc-gf0, for a skin's dependency trace). A node not bound yet
    /// has no edges and is not among them.
    pub fn dependents_at(&self, inputs: &ModelRoot, at: CellKey) -> Vec<CellKey> {
        let mut nodes = BTreeSet::new();
        self.index.dependents_of_cell(inputs, at, &mut nodes);
        let mut cells: Vec<CellKey> = nodes
            .into_iter()
            .filter_map(|node| self.node(node).map(FormulaNode::anchor))
            .collect();
        cells.sort();
        cells.dedup();
        cells
    }

    /// The nodes with a value edge covering `at`, without those in `leaving`, in ID order.
    pub(crate) fn dependents_of_cell(
        &self,
        inputs: &ModelRoot,
        at: CellKey,
        leaving: &BTreeSet<NodeId>,
    ) -> Vec<NodeId> {
        let mut out = BTreeSet::new();
        self.index.dependents_of_cell(inputs, at, &mut out);
        out.difference(leaving).copied().collect()
    }

    /// The nodes with a value edge covering one of `emptied`, the cells of `area` an edit empties,
    /// without those in `leaving`, in ID order. A node that reads only other cells of `area` is
    /// not among them.
    pub(crate) fn dependents_of_emptied(
        &self,
        inputs: &ModelRoot,
        area: GridArea,
        emptied: &[CellKey],
        leaving: &BTreeSet<NodeId>,
    ) -> Vec<NodeId> {
        let mut out = BTreeSet::new();
        self.index
            .dependents_of_cells_in_area(inputs, area, emptied, &mut out);
        out.difference(leaving).copied().collect()
    }

    pub(crate) fn set_revision(&mut self, revision: AuthoredRevision) {
        self.revision = revision;
    }

    /// Removes a live node: its edges, its plan, its anchor entry and its record, and releases its
    /// ID. Its published result stays until the next publication removes it.
    pub(crate) fn remove_node(&mut self, node: NodeId) {
        if let Some(record) = self
            .nodes
            .get_mut(node.slot() as usize)
            .and_then(|slot| slot.take_if(|record| record.id == node))
        {
            if let Some(prerequisites) = &record.prerequisites {
                self.index.remove(node, prerequisites);
            }
            self.index.remove(node, &record.dynamic);
            self.index.set_name_bindings(node, Vec::new());
            self.bindings.remove(&node);
            self.cached_arrays.remove(&node);
            self.contract.retire(node);
            self.volatile.remove(&node);
            self.entered.remove(&node);
            self.subtotal_cells.remove(&record.anchor);
            self.pending.remove(&node);
            self.range_redemand.remove(&node);
            self.retired.insert(record.anchor);
            self.anchor_epoch = self
                .anchor_epoch
                .checked_add(1)
                .expect("anchor index version exhausted");
            self.results.retire(node, record.anchor);
            self.by_anchor.remove(&record.anchor);
            self.anchor_index.remove(record.anchor);
            let released = self.node_ids().release(node);
            debug_assert_eq!(
                released,
                Ok(()),
                "a live node's ID is live in its allocator"
            );
        }
    }

    /// Drops a live node's prerequisites and plan, leaving it unbound and pending.
    pub(crate) fn unbind_node(&mut self, node: NodeId) {
        self.drop_binding(node, true);
    }

    pub(crate) fn rebind_names(&mut self, node: NodeId) {
        self.drop_binding(node, false);
    }

    fn drop_binding(&mut self, node: NodeId, replace_formula: bool) {
        if let Some(record) = self
            .nodes
            .get_mut(node.slot() as usize)
            .and_then(Option::as_mut)
            .filter(|record| record.id == node)
        {
            if let Some(prerequisites) = record.prerequisites.take() {
                self.index.remove(node, &prerequisites);
            }
            let dynamic = std::mem::take(&mut record.dynamic);
            self.index.remove(node, &dynamic);
            self.bindings.remove(&node);
            self.volatile.remove(&node);
            self.subtotal_cells.remove(&record.anchor);
            self.pending.insert(node);
            if replace_formula {
                self.contract.replace(node);
            }
            self.index.set_name_bindings(node, Vec::new());
        }
    }

    /// Applies a structural edit's moves (L3, dc-m3u): each node whose anchor is `None` is
    /// removed with its result; every other node keeps its ID, moves to its new anchor with its
    /// published result, and is left unbound and pending, so the next recalculation binds its
    /// rewritten formula and calculates it. Published spills, templates and the record of
    /// removed results go: every formula is calculated again. Returns the removed nodes.
    pub(crate) fn apply_moves(&mut self, moves: Vec<(NodeId, Option<CellKey>)>) -> Vec<NodeId> {
        self.contract.coordinates = self.contract.advance();
        let mut removed = Vec::new();
        let mut anchors = BTreeMap::new();
        for (node, anchor) in moves {
            if self.node(node).is_none() {
                continue;
            }
            match anchor {
                None => {
                    self.remove_node(node);
                    removed.push(node);
                }
                Some(at) => {
                    // The node keeps its own result, which moves with it.
                    self.unbind_node(node);
                    anchors.insert(node, at);
                }
            }
        }
        self.by_anchor.clear();
        for (node, at) in anchors {
            if let Some(record) = self
                .nodes
                .get_mut(node.slot() as usize)
                .and_then(Option::as_mut)
                .filter(|record| record.id == node)
            {
                record.anchor = at;
            }
            self.by_anchor.insert(at, node);
        }
        self.anchor_index = AnchorIndex::build(&self.by_anchor);
        self.anchor_epoch = self
            .anchor_epoch
            .checked_add(1)
            .expect("anchor index version exhausted");
        // The results removed nodes left go with them.
        self.results.clear_orphans();
        self.retired.clear();
        self.spills = SpillIndex::default();
        self.cached_arrays.clear();
        self.blocked_spills.clear();
        self.spill_epoch = self
            .spill_epoch
            .checked_add(1)
            .expect("occupancy version exhausted");
        self.templates.clear();
        self.subtotal_cells.clear();
        removed
    }

    /// Records a node issued by this state's allocator at `anchor`, unbound. The caller makes it
    /// pending.
    pub(crate) fn add_node(&mut self, id: NodeId, anchor: CellKey) {
        self.anchor_epoch = self
            .anchor_epoch
            .checked_add(1)
            .expect("anchor index version exhausted");
        self.record_node(id, anchor);
        self.anchor_index.insert(anchor, id);
    }

    /// [`EngineState::add_node`] but for the anchor index, which [`EngineState::new`] builds once
    /// all its nodes are recorded.
    fn record_node(&mut self, id: NodeId, anchor: CellKey) {
        let slot = id.slot() as usize;
        if self.nodes.len() <= slot {
            self.nodes.resize_with(slot + 1, || None);
        }
        self.nodes[slot] = Some(FormulaNode {
            id,
            anchor,
            prerequisites: None,
            dynamic: Vec::new(),
        });
        self.by_anchor.insert(anchor, id);
        self.contract.replace(id);
    }
}

/// Issues a node ID. [`IdAllocator::issue`] refuses only when every one of its `u32::MAX` slots is
/// live or retired: about four billion formulas held at once, or every slot issued about four
/// billion times. Like a collection's capacity overflow, that panics instead of returning a
/// refusal.
pub(crate) fn issue_node_id(ids: &mut IdAllocator<NodeId>) -> NodeId {
    ids.issue().expect("every node ID slot is live or retired")
}
