//! SliceV1 identities and certificates used by the serial engine. A certificate describes the
//! reads of one actual evaluation. File caches never receive a producing invocation or proof.

use calc_doc_model::{AuthoredRevision, ModelRoot};
use calc_fml::{FactRead, ReadRequest};
use calc_value::{CellKey, GridArea, NodeId, SheetId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CommandId {
    pub client_session: u64,
    pub sequence: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalcCommandId {
    pub domain_incarnation: u64,
    pub sequence: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PassReason {
    Initial,
    DependencyRepair,
    SpillShape,
    IterationSweep(u32),
    Scenario(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalcPassId {
    pub command: CalcCommandId,
    pub ordinal: u32,
    pub reason: PassReason,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkbookInstanceId {
    pub document: u64,
    pub incarnation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InvocationId {
    pub pass: CalcPassId,
    pub workbook: WorkbookInstanceId,
    pub locus: CellKey,
    pub evaluation: u64,
    pub call_site: u32,
    pub occurrence: Vec<u32>,
    pub reason: ReevaluationReason,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EvaluationTarget {
    pub workbook: WorkbookInstanceId,
    pub origin_revision: AuthoredRevision,
    pub formula_generation: u64,
    pub node: NodeId,
    pub locus: CellKey,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdmissionFrontier {
    pub model: AuthoredRevision,
    pub external_through: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReadViewId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObservationFrontier {
    pub model: AuthoredRevision,
    pub results: ReadViewId,
    pub inputs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CalcScope {
    Workbook,
    Sheet(SheetId),
    Range(Vec<GridArea>),
}
/// The measured Manual Range.Calculate profile keeps its targets required for the next
/// command. Satisfied is the SliceV1 policy for other admissions, not broad Auto-mode parity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RangePostState {
    Satisfied,
    StillRequired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvalSelection {
    DirtyAndVolatile,
    ForceAll,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReevaluationReason {
    DependencyRepair,
    NewCommandDemand,
    AuthoredOrInputChange,
    SpillShapePass,
    IterationSweep,
    TableScenario,
}

/// A value version is the producing pass. Every semantic reevaluation starts a new pass;
/// physical dependency retries keep the pass and the root invocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RequirementState {
    pub value_version: Option<CalcPassId>,
    pub required: u64,
    pub satisfied: u64,
}

/// One stamp per span, including formula requirements. These are maxima of globally increasing
/// versions, so changing any member advances the span; no per-cell certificate is allocated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpanVersions {
    pub authored: u64,
    pub value: Option<CalcPassId>,
    pub required: u64,
    pub satisfied: u64,
    pub formulas: u64,
    pub pending: u64,
    /// Existing iteration/table session values have actual calculation ticks, but no SliceV1
    /// invocation proof. Keep that boundary explicit through ordinary dependents.
    pub uncertified: u64,
    pub legacy_tick: u64,
    pub boundary_cache: u64,
    pub imported_cache: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReadSpan {
    pub region: GridArea,
    pub request: ReadRequest,
    pub versions: SpanVersions,
    pub boundary_caches: Vec<BoundaryReadSpan>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FactStamp {
    pub read: FactRead,
    pub version: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AbsenceKind {
    ReadBlank,
    SpillReservation,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AbsenceProof {
    pub region: GridArea,
    pub kind: AbsenceKind,
    pub authored: u64,
    pub occupancy: u64,
    pub corridor: u64,
    pub producers: u64,
    pub pending_producers: usize,
    /// An exact scoped command may use the pinned absence of excluded producers.
    /// This never establishes workbook-wide absence or freshness.
    pub cached: Option<ReadViewId>,
    pub blanks: Vec<BlankRun>,
}
/// A batched read through the command's pinned boundary cache. Imported values have
/// no producing invocation. Dirty caches remain qualified even if their numeric value matches.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BoundaryReadSpan {
    pub region: GridArea,
    pub view: ReadViewId,
    pub values: u64,
    pub dirty: u64,
    pub imported: u64,
    pub imported_sources: Vec<std::sync::Arc<calc_doc_model::SourcePart>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ReadCertificate {
    /// The calculation consumed a saved external-link cache without a versioned live input
    /// view. Arithmetic preserves this qualification; it is not a FreshFor guarantee.
    pub external_cache: bool,
    /// Reads cross an existing session boundary without an invocation certificate. This
    /// records actual calculation provenance, but never provides a FreshFor guarantee.
    pub legacy_boundary: bool,
    pub uses_boundary_cache: bool,
    pub uses_imported_cache: bool,
    pub boundary_caches: Vec<BoundaryReadSpan>,
    pub dependencies: Vec<ReadSpan>,
    pub facts: Vec<FactStamp>,
    pub absences: Vec<AbsenceProof>,
    pub semantic_environment: u64,
    pub bindings: u64,
    pub volatile_inputs: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetCompletion {
    Value,
    WorksheetError,
    EngineRefusal,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ComputationOrigin {
    Evaluated { root: InvocationId },
    EngineRefusal,
    CircularRule,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FreshnessClaim {
    pub command: CalcCommandId,
    pub pass: CalcPassId,
    pub frontier: AdmissionFrontier,
    pub required: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResultStamp {
    pub target: EvaluationTarget,
    pub observed: ObservationFrontier,
    pub computed_by: ComputationOrigin,
    pub reads: ReadCertificate,
    pub certification: FreshnessClaim,
    pub completion: TargetCompletion,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmissionReceipt {
    pub command: CalcCommandId,
    pub source: CommandId,
    pub pass: CalcPassId,
    pub scope: CalcScope,
    pub range_post_state: RangePostState,
    pub selection: EvalSelection,
    pub admitted: AdmissionFrontier,
    pub predecessor: Option<CalcCommandId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionOutcome {
    Complete,
    Incomplete,
    Superseded { by: CalcCommandId },
    Cancelled,
    Failed,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScopeCoverage {
    pub required: usize,
    pub calculated: usize,
    pub refused: usize,
    pub held: usize,
    pub uncertified: usize,
    pub boundary_cached: usize,
    pub imported_cached: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationUnitId {
    pub command: CalcCommandId,
    pub sequence: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionReceipt {
    pub command: CalcCommandId,
    pub scope: CalcScope,
    pub admitted: AdmissionFrontier,
    pub outcome: CompletionOutcome,
    pub coverage: ScopeCoverage,
    pub results: ReadViewId,
    pub after_publication: Option<PublicationUnitId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalcEvent {
    Committed {
        revision: AuthoredRevision,
    },
    Admitted(AdmissionReceipt),
    Yielded {
        pass: CalcPassId,
        remaining: usize,
    },
    Superseded(CompletionReceipt),
    Reused {
        at: CellKey,
        computed_by: CalcCommandId,
        certified_for: CalcCommandId,
    },
    Rejected {
        at: CellKey,
    },
    Published(PublicationUnitId),
    Completed(CompletionReceipt),
}

use crate::read::Progress;
use crate::state::{ArrayRole, Binding};
use crate::{EngineState, FormulaResult};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub(crate) struct ContractState {
    pub workbook: WorkbookInstanceId,
    epoch: u64,
    generation: u64,
    pub environment: u64,
    pub bindings: u64,
    pub coordinates: u64,
    pub facts: u64,
    generations: BTreeMap<NodeId, u64>,
    requirements: BTreeMap<NodeId, RequirementState>,
    writes: Vec<(GridArea, u64)>,
    occupancy: Vec<(GridArea, u64)>,
    pub facility_roots: BTreeSet<NodeId>,
    pub force_all: bool,
}

impl ContractState {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let document = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("workbook identity exhausted");
        Self {
            workbook: WorkbookInstanceId {
                document,
                incarnation: 1,
            },
            epoch: 1,
            generation: 1,
            environment: 1,
            bindings: 1,
            coordinates: 1,
            facts: 1,
            generations: BTreeMap::new(),
            requirements: BTreeMap::new(),
            writes: Vec::new(),
            occupancy: Vec::new(),
            facility_roots: BTreeSet::new(),
            force_all: false,
        }
    }
    pub fn inherit_identity(&mut self, previous: &Self, nodes: Vec<NodeId>) {
        self.workbook = previous.workbook;
        self.epoch = previous
            .epoch
            .checked_add(1)
            .expect("read version exhausted");
        self.bindings = self.epoch;
        self.coordinates = self.epoch;
        self.environment = previous.environment;
        self.facts = previous.facts;
        self.generation = previous.generation;
        self.generations.clear();
        self.requirements.clear();
        for node in nodes {
            self.replace(node);
        }
    }
    pub fn snapshot(&self) -> BTreeMap<NodeId, RequirementState> {
        self.requirements.clone()
    }
    pub fn advance(&mut self) -> u64 {
        self.epoch = self.epoch.checked_add(1).expect("read version exhausted");
        self.epoch
    }
    pub fn generation(&self, node: NodeId) -> u64 {
        self.generations.get(&node).copied().unwrap_or(1)
    }
    pub fn replace(&mut self, node: NodeId) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("formula generation exhausted");
        self.generations.insert(node, self.generation);
    }
    pub fn retire(&mut self, node: NodeId) {
        self.generations.remove(&node);
        self.requirements.remove(&node);
        self.facility_roots.remove(&node);
    }
    pub fn requirement(&self, node: NodeId) -> RequirementState {
        self.requirements
            .get(&node)
            .copied()
            .unwrap_or(RequirementState {
                value_version: None,
                required: 1,
                satisfied: 0,
            })
    }
    pub fn require_at(&mut self, nodes: impl IntoIterator<Item = NodeId>, epoch: u64) {
        for node in nodes {
            let mut current = self.requirement(node);
            current.required = current.required.max(epoch);
            self.requirements.insert(node, current);
        }
    }
    pub fn invalidate(&mut self, nodes: impl IntoIterator<Item = NodeId>) {
        let epoch = self.advance();
        self.require_at(nodes, epoch);
    }
    pub fn write(&mut self, region: GridArea, epoch: u64) {
        if let Some((_, version)) = self.writes.iter_mut().find(|(held, _)| *held == region) {
            *version = epoch;
        } else {
            self.writes.push((region, epoch));
        }
    }
    pub fn record_occupancy(&mut self, region: GridArea, version: u64) {
        if let Some((_, held)) = self.occupancy.iter_mut().find(|(area, _)| *area == region) {
            *held = version;
        } else {
            self.occupancy.push((region, version));
        }
    }
    pub fn occupancy(&self, region: GridArea) -> u64 {
        self.occupancy
            .iter()
            .filter(|(area, _)| meets(*area, region))
            .map(|(_, version)| *version)
            .max()
            .unwrap_or(0)
    }
    pub fn authored(&self, region: GridArea) -> u64 {
        self.writes
            .iter()
            .filter(|(held, _)| meets(*held, region))
            .map(|(_, v)| *v)
            .max()
            .unwrap_or(0)
    }
    pub fn satisfy(&mut self, node: NodeId, stamp: Option<&ResultStamp>, fallback: CalcPassId) {
        let mut current = self.requirement(node);
        if let Some(stamp) = stamp {
            if stamp.certification.required != current.required {
                return;
            }
            current.satisfied = stamp.certification.required;
            current.value_version = Some(match &stamp.computed_by {
                ComputationOrigin::Evaluated { root } => root.pass,
                _ => stamp.certification.pass,
            });
        } else {
            // Existing circular, iteration and table sessions keep their Calculation provenance;
            // no invocation certificate is invented for those boundaries.
            current.satisfied = current.required;
            current.value_version = Some(fallback);
        }
        self.requirements.insert(node, current);
    }
}

pub(crate) fn meets(a: GridArea, b: GridArea) -> bool {
    a.sheet() == b.sheet()
        && a.first().row() <= b.last().row()
        && b.first().row() <= a.last().row()
        && a.first().column() <= b.last().column()
        && b.first().column() <= a.last().column()
}

pub(crate) fn span_versions_at(
    state: &EngineState,
    progress: &Progress,
    region: GridArea,
    cut: Option<&BTreeMap<NodeId, RequirementState>>,
) -> SpanVersions {
    let mut versions = SpanVersions {
        authored: state.contract.authored(region),
        ..SpanVersions::default()
    };
    for node in span_nodes_at(state, progress, region) {
        let mut current = cut
            .and_then(|cut| cut.get(&node))
            .copied()
            .unwrap_or_else(|| {
                if cut.is_some() {
                    RequirementState {
                        required: 1,
                        ..RequirementState::default()
                    }
                } else {
                    state.contract.requirement(node)
                }
            });
        let result = if progress.is_evaluated(node) {
            progress.staged(node)
        } else if progress.is_scheduled(node) {
            None
        } else {
            state.results.of(node)
        };
        if let Some(stamp) = result.and_then(FormulaResult::stamp) {
            if stamp.certification.required == current.required {
                current.satisfied = current.required;
                current.value_version = Some(match &stamp.computed_by {
                    ComputationOrigin::Evaluated { root } => root.pass,
                    _ => stamp.certification.pass,
                });
            }
            versions.boundary_cache += u64::from(stamp.reads.uses_boundary_cache);
            versions.imported_cache += u64::from(stamp.reads.uses_imported_cache);
            if stamp.reads.legacy_boundary {
                versions.uncertified += 1;
            }
        } else if let Some(FormulaResult::Calculated { calculation, .. }) = result {
            if progress.is_evaluated(node) {
                current.satisfied = current.required;
            }
            current.value_version = None;
            versions.uncertified += 1;
            versions.legacy_tick = versions.legacy_tick.max(calculation.tick().get());
        }
        if !progress.is_evaluated(node)
            && (current.required != current.satisfied || progress.is_scheduled(node))
        {
            versions.pending += 1;
        }
        versions.formulas += 1;
        versions.value = versions.value.max(current.value_version);
        versions.required = versions.required.max(current.required);
        versions.satisfied = versions.satisfied.max(current.satisfied);
    }
    versions
}

fn span_nodes_at(state: &EngineState, progress: &Progress, region: GridArea) -> BTreeSet<NodeId> {
    let mut nodes: BTreeSet<_> = state
        .by_anchor
        .range(region.first()..=region.last())
        .filter(|(at, _)| region.contains(**at))
        .map(|(_, node)| *node)
        .collect();
    for spill in state
        .spills
        .iter()
        .filter(|spill| meets(spill.area(), region))
    {
        if let Some(node) = state.node_at(spill.anchor()) {
            if !progress.is_evaluated(node) {
                nodes.insert(node);
            }
        }
    }
    nodes.extend(
        state
            .cached_arrays
            .iter()
            .filter(|(node, spill)| {
                !progress.is_evaluated(**node)
                    && meets(spill.area(), region)
                    && matches!(state.results.of(**node), Some(FormulaResult::FileCached(_)))
            })
            .map(|(node, _)| *node),
    );
    nodes.extend(
        progress
            .footprints
            .iter()
            .filter(|(_, area)| meets(**area, region))
            .map(|(node, _)| *node),
    );
    nodes
}

pub(crate) fn boundary_ancestry_at(
    state: &EngineState,
    progress: &Progress,
    region: GridArea,
) -> Vec<BoundaryReadSpan> {
    let mut spans = Vec::new();
    for node in span_nodes_at(state, progress, region) {
        let result = if progress.is_evaluated(node) {
            progress.staged(node)
        } else if progress.is_scheduled(node) {
            None
        } else {
            state.results.of(node)
        };
        if let Some(stamp) = result.and_then(FormulaResult::stamp) {
            for span in &stamp.reads.boundary_caches {
                if !spans.contains(span) {
                    spans.push(span.clone());
                }
            }
        }
    }
    spans
}

pub(crate) fn producer_version_at(
    state: &EngineState,
    region: GridArea,
    except: NodeId,
    cut: Option<&BTreeMap<NodeId, RequirementState>>,
) -> u64 {
    state
        .bindings
        .iter()
        .filter_map(|(&node, binding)| {
            if node == except || !matches!(binding, Binding::Placed(_, ArrayRole::Spill)) {
                return None;
            }
            let at = state.node(node)?.anchor();
            let required = cut.and_then(|cut| cut.get(&node)).map_or_else(
                || {
                    if cut.is_some() {
                        1
                    } else {
                        state.contract.requirement(node).required
                    }
                },
                |r| r.required,
            );
            (at.sheet() == region.sheet()
                && at.row() <= region.last().row()
                && at.column() <= region.last().column())
            .then_some(required)
        })
        .max()
        .unwrap_or(0)
}

pub(crate) fn producer_pending_at(
    state: &EngineState,
    inputs: &ModelRoot,
    progress: &Progress,
    region: GridArea,
    blanks: &[BlankRun],
    except: NodeId,
    cut: Option<&BTreeMap<NodeId, RequirementState>>,
) -> usize {
    producer_pending_in(
        state,
        inputs,
        progress,
        region,
        blanks,
        except,
        cut,
        &|_| true,
    )
}

pub(crate) fn producer_pending_in(
    state: &EngineState,
    inputs: &ModelRoot,
    progress: &Progress,
    region: GridArea,
    blanks: &[BlankRun],
    except: NodeId,
    cut: Option<&BTreeMap<NodeId, RequirementState>>,
    includes: &dyn Fn(CellKey) -> bool,
) -> usize {
    state
        .bindings
        .iter()
        .filter(|(node, binding)| {
            **node != except && matches!(binding, Binding::Placed(_, ArrayRole::Spill))
        })
        .filter(|(node, _)| {
            state.node(**node).is_some_and(|record| {
                includes(record.anchor())
                    && record.anchor().sheet() == region.sheet()
                    && record.anchor().row() <= region.last().row()
                    && record.anchor().column() <= region.last().column()
                    && blanks.iter().any(|blank| {
                        let at = record.anchor();
                        let reach = GridArea::new(at, region.last());
                        reach.is_ok_and(|reach| blank.first_intersection(region, reach).is_some())
                            && potential_blank(inputs, at, region, *blank).is_some()
                    })
            })
        })
        .filter(|(node, _)| {
            let result = if progress.is_evaluated(**node) {
                progress.staged(**node)
            } else if progress.is_scheduled(**node) {
                None
            } else {
                state.results.of(**node)
            };
            if matches!(result, Some(FormulaResult::NotModeled { .. })) {
                return true;
            }
            let mut current = cut
                .and_then(|cut| cut.get(node))
                .copied()
                .unwrap_or_else(|| {
                    if cut.is_some() {
                        RequirementState {
                            required: 1,
                            ..RequirementState::default()
                        }
                    } else {
                        state.contract.requirement(**node)
                    }
                });
            if progress.is_evaluated(**node)
                && matches!(
                    progress.staged(**node),
                    Some(FormulaResult::Calculated { .. })
                )
            {
                // A terminal scalar held by the existing circular rule has an explicit
                // no-spill topology, even though it makes no evaluation certificate claim.
                current.satisfied = current.required;
            }
            current.required != current.satisfied
                || progress.is_scheduled(**node) && !progress.is_evaluated(**node)
        })
        .count()
}

pub(crate) fn fact_version(state: &EngineState, read: FactRead, facts: u64) -> u64 {
    match read.subject {
        calc_fml::FactSubject::Cell(at) => state.contract.authored(GridArea::cell(at)),
        calc_fml::FactSubject::Sheet(_) => state.contract.bindings,
        calc_fml::FactSubject::Workbook => {
            if read.invalidation == calc_fml::FactInvalidation::OnNextAuthorizedCalculation {
                facts
            } else {
                state.contract.bindings
            }
        }
    }
}

pub(crate) fn certificate_valid(
    state: &EngineState,
    inputs: &ModelRoot,
    progress: &Progress,
    stamp: &ResultStamp,
    facts: u64,
) -> bool {
    certificate_valid_at(
        state,
        inputs,
        progress,
        stamp,
        facts,
        None,
        state.volatile_input_version,
    )
}

pub(crate) fn certificate_valid_at(
    state: &EngineState,
    inputs: &ModelRoot,
    progress: &Progress,
    stamp: &ResultStamp,
    facts: u64,
    cut: Option<&BTreeMap<NodeId, RequirementState>>,
    volatile_inputs: u64,
) -> bool {
    if inputs.revision() != state.revision() {
        return false;
    }
    let target = stamp.target;
    let required = cut.and_then(|cut| cut.get(&target.node)).map_or_else(
        || {
            if cut.is_some() {
                1
            } else {
                state.contract.requirement(target.node).required
            }
        },
        |r| r.required,
    );
    if target.workbook != state.contract.workbook
        || state
            .node(target.node)
            .is_none_or(|n| n.anchor() != target.locus)
        || target.formula_generation != state.contract.generation(target.node)
        || stamp.certification.required != required
        || stamp.reads.semantic_environment != state.contract.environment
        || stamp.reads.bindings != state.contract.bindings
        || stamp
            .reads
            .volatile_inputs
            .is_some_and(|version| version != volatile_inputs)
    {
        return false;
    }
    stamp
        .reads
        .dependencies
        .iter()
        .all(|read| span_versions_at(state, progress, read.region, cut) == read.versions)
        && stamp
            .reads
            .facts
            .iter()
            .all(|fact| fact_version(state, fact.read, facts) == fact.version)
        && stamp.reads.absences.iter().all(|proof| {
            proof.authored == state.contract.authored(proof.region)
                && proof.occupancy == state.contract.occupancy(proof.region)
                && proof.corridor
                    == if proof.kind == AbsenceKind::ReadBlank && proof.producers == 0 {
                        0
                    } else {
                        state
                            .contract
                            .authored(if proof.kind == AbsenceKind::ReadBlank {
                                absence_corridor(proof.region)
                            } else {
                                proof.region
                            })
                    }
                && proof.producers == producer_version_at(state, proof.region, target.node, cut)
                && (proof.kind == AbsenceKind::SpillReservation
                    || producer_pending_at(
                        state,
                        inputs,
                        progress,
                        proof.region,
                        &proof.blanks,
                        target.node,
                        cut,
                    ) == if proof.cached.is_some() {
                        proof.pending_producers
                    } else {
                        0
                    } && blanks_still_absent(state, progress, proof, target.node))
        })
}

fn blanks_still_absent(
    state: &EngineState,
    progress: &Progress,
    proof: &AbsenceProof,
    except: NodeId,
) -> bool {
    let intersects = |area| {
        proof
            .blanks
            .iter()
            .any(|blank| blank.first_intersection(proof.region, area).is_some())
    };
    !progress
        .footprints
        .iter()
        .any(|(&node, &area)| node != except && intersects(area))
        && !state.spills.iter().any(|spill| {
            state
                .node_at(spill.anchor())
                .is_some_and(|node| node != except && !progress.is_evaluated(node))
                && intersects(spill.area())
        })
}

impl CalcScope {
    /// Retarget a scope through an accepted structural edit, preserving disjoint pieces.
    /// A removed sheet or fully deleted range becomes an empty successor, never Workbook.
    pub fn transformed(&self, edit: &calc_fml::reference::StructuralEdit) -> Self {
        use calc_fml::reference::StructuralEdit as E;
        match self {
            Self::Workbook => Self::Workbook,
            Self::Sheet(sheet) => match edit {
                E::RemoveSheet { sheet: removed, .. } if removed == sheet => {
                    Self::Range(Vec::new())
                }
                _ => Self::Sheet(*sheet),
            },
            Self::Range(areas) => Self::Range(
                areas
                    .iter()
                    .flat_map(|area| scope_area(*area, edit))
                    .collect(),
            ),
        }
    }

    pub fn contains(&self, at: CellKey) -> bool {
        match self {
            Self::Workbook => true,
            Self::Sheet(sheet) => at.sheet() == *sheet,
            Self::Range(areas) => areas.iter().any(|area| area.contains(at)),
        }
    }
}

fn scope_area(area: GridArea, edit: &calc_fml::reference::StructuralEdit) -> Vec<GridArea> {
    use calc_fml::reference::StructuralEdit as E;
    match edit {
        E::RemoveSheet { sheet, .. } if *sheet == area.sheet() => Vec::new(),
        E::MoveCells { area: source, .. } => {
            let destination = edit
                .area(*source)
                .expect("a validated cut fits on the grid");
            // Destination locations replaced by a cut cease to belong to the old
            // command. Undo must not mistake them for moved source locations.
            let mut pieces: Vec<_> = area
                .subtract(*source)
                .into_iter()
                .flat_map(|piece| piece.subtract(destination))
                .collect();
            if let Some(common) = area.intersection(*source) {
                pieces.extend(edit.area(common));
            }
            pieces
        }
        E::ShiftCells {
            area: band, down, ..
        } if band.sheet() == area.sheet() => {
            let stripe = if *down {
                GridArea::new(
                    CellKey::new(band.sheet(), 1, band.first().column()).expect("valid stripe"),
                    CellKey::new(band.sheet(), calc_value::MAX_ROWS, band.last().column())
                        .expect("valid stripe"),
                )
            } else {
                GridArea::new(
                    CellKey::new(band.sheet(), band.first().row(), 1).expect("valid stripe"),
                    CellKey::new(band.sheet(), band.last().row(), calc_value::MAX_COLUMNS)
                        .expect("valid stripe"),
                )
            }
            .expect("one sheet");
            let Some(common) = area.intersection(stripe) else {
                return vec![area];
            };
            let mut pieces = area.subtract(common);
            pieces.extend(edit.area(common));
            pieces
        }
        _ => edit.area(area).into_iter().collect(),
    }
}

/// One actual blank run within a rectangular read, using the reader's row-major offsets.
/// Keeping runs avoids inventing absence for populated cells and avoids per-cell certificates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlankRun {
    pub offset: u64,
    pub len: u64,
}
impl BlankRun {
    pub fn first_intersection(self, read: GridArea, region: GridArea) -> Option<CellKey> {
        if self.len == 0 || read.sheet() != region.sheet() {
            return None;
        }
        let end = self.offset.checked_add(self.len)?.checked_sub(1)?;
        if end >= read.cell_count() {
            return None;
        }
        let left = read.first().column().max(region.first().column());
        let right = read.last().column().min(region.last().column());
        if left > right {
            return None;
        }
        let width = u64::from(read.column_count());
        let start_row = read.first().row() + (self.offset / width) as u32;
        let end_row = read.first().row() + (end / width) as u32;
        let mut row = start_row.max(region.first().row());
        let last_row = end_row.min(region.last().row());
        if row > last_row {
            return None;
        }
        let mut column = if row == start_row {
            left.max(read.first().column() + (self.offset % width) as u32)
        } else {
            left
        };
        let row_right = if row == end_row {
            right.min(read.first().column() + (end % width) as u32)
        } else {
            right
        };
        if column > row_right {
            row = row.checked_add(1)?;
            column = left;
            if row > last_row {
                return None;
            }
            if row == end_row && column > read.first().column() + (end % width) as u32 {
                return None;
            }
        }
        CellKey::new(read.sheet(), row, column).ok()
    }
}

/// Any rectangular spill reaching a blank must cross this authored rectangle. A known
/// occupant proves that candidate impossible, without predicting a function's output shape.
/// A wrapping blank run has at most two minimal rectangles; all later cells include one.
pub(crate) fn potential_blank(
    inputs: &ModelRoot,
    anchor: CellKey,
    read: GridArea,
    blank: BlankRun,
) -> Option<CellKey> {
    if anchor.sheet() != read.sheet()
        || anchor.row() > read.last().row()
        || anchor.column() > read.last().column()
    {
        return None;
    }
    let reach = GridArea::new(anchor, read.last()).ok()?;
    let first = blank.first_intersection(read, reach)?;
    let free = |at| {
        let area = GridArea::new(anchor, at).expect("same sheet");
        inputs
            .cells(area)
            .is_some_and(|mut cells| !cells.any(|(at, _)| at != anchor))
            && inputs.definitions().unloaded_in(area).next().is_none()
    };
    if free(first) {
        return Some(first);
    }
    if first.row() < read.last().row() {
        let next = CellKey::new(anchor.sheet(), first.row() + 1, anchor.column()).ok()?;
        let area = GridArea::new(next, read.last()).ok()?;
        let second = blank.first_intersection(read, area)?;
        if free(second) {
            return Some(second);
        }
    }
    None
}

pub(crate) fn absence_corridor(region: GridArea) -> GridArea {
    GridArea::new(
        CellKey::new(region.sheet(), 1, 1).expect("the sheet origin"),
        region.last(),
    )
    .expect("one sheet")
}

#[cfg(test)]
mod scope_transform_tests {
    use super::*;
    use calc_fml::reference::StructuralEdit;
    use calc_value::IdAllocator;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_partial_cut_keeps_its_hole_and_only_adds_the_moved_piece() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let a = ids.issue().unwrap();
        let b = ids.issue().unwrap();
        let cell = |s, r, c| CellKey::new(s, r, c).unwrap();
        let scope = CalcScope::Range(vec![GridArea::new(cell(a, 1, 1), cell(a, 4, 4)).unwrap()]);
        let moved = scope.transformed(&StructuralEdit::MoveCells {
            area: GridArea::new(cell(a, 2, 2), cell(a, 3, 3)).unwrap(),
            to: cell(b, 6, 6),
            source_name: "A".into(),
            target_name: "B".into(),
        });
        let mut count = 0;
        for s in [a, b] {
            for row in 1..=10 {
                for column in 1..=10 {
                    let expected = (s == a
                        && row <= 4
                        && column <= 4
                        && !((2..=3).contains(&row) && (2..=3).contains(&column)))
                        || (s == b && (6..=7).contains(&row) && (6..=7).contains(&column));
                    assert_eq!(
                        moved.contains(cell(s, row, column)),
                        expected,
                        "{s:?}:{row},{column}"
                    );
                    count += usize::from(moved.contains(cell(s, row, column)));
                }
            }
        }
        assert_eq!(count, 16);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_partial_shift_does_not_widen_its_unaffected_columns() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let s = ids.issue().unwrap();
        let cell = |r, c| CellKey::new(s, r, c).unwrap();
        let scope = CalcScope::Range(vec![GridArea::new(cell(1, 1), cell(4, 4)).unwrap()]);
        let shifted = scope.transformed(&StructuralEdit::ShiftCells {
            area: GridArea::new(cell(2, 2), cell(2, 3)).unwrap(),
            down: true,
            insert: true,
        });
        for row in 1..=6 {
            for column in 1..=5 {
                let expected = (row <= 4 && (column == 1 || column == 4))
                    || (row <= 5 && (column == 2 || column == 3));
                assert_eq!(
                    shifted.contains(cell(row, column)),
                    expected,
                    "{row},{column}"
                );
            }
        }
        assert_eq!(
            CalcScope::Sheet(s).transformed(&StructuralEdit::RemoveSheet {
                sheet: s,
                order: Vec::new()
            }),
            CalcScope::Range(Vec::new())
        );
    }
}
