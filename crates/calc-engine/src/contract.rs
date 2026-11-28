//! SliceV1 identities and certificates used by the serial engine. A certificate describes the
//! reads of one actual evaluation. File caches never receive a producing invocation or proof.

use calc_doc_model::{AuthoredRevision, ModelRoot};
use calc_fml::{FactRead, ReadRequest};
use calc_value::{CellKey, GridArea, NodeId, SheetId};

/// Client-issued identity of an authored or calculation request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CommandId {
    /// Session that issued the command.
    pub client_session: u64,
    /// Sequence within the issuing session.
    pub sequence: u64,
}
/// Result store eligible to receive a calculation's publication.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResultDestination {
    /// Publish to the workbook's live results.
    #[default]
    Live,
    /// Publish into the result view reserved for a save.
    PinnedSave {
        /// Client request that owns the pinned save.
        save: CommandId,
    },
}

/// Domain-issued identity of an admitted calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalcCommandId {
    /// Incarnation of the calculation domain.
    pub domain_incarnation: u64,
    /// Sequence within the domain incarnation.
    pub sequence: u64,
}
/// Reason a calculation pass was started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PassReason {
    /// First pass of a newly admitted command.
    Initial,
    /// Recalculate dependents of a delivered asynchronous value.
    AsyncDependents,
    /// Repair a dependency discovered during evaluation.
    DependencyRepair,
    /// Recalculate after a spill footprint changes.
    SpillShape,
    /// Iteration pass identified by its sweep number.
    IterationSweep(u32),
    /// Data-table pass identified by its scenario number.
    Scenario(u32),
}
/// Identity of one pass within an admitted calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalcPassId {
    /// Calculation that owns this pass.
    pub command: CalcCommandId,
    /// Pass number within the calculation.
    pub ordinal: u32,
    /// Reason this pass was started.
    pub reason: PassReason,
}
/// Identity of one open workbook instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkbookInstanceId {
    /// Document identity assigned by the host.
    pub document: u64,
    /// Open-instance generation of the document.
    pub incarnation: u64,
}
/// Identity of one formula or definition invocation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InvocationId<L = CellKey> {
    /// Pass in which the invocation ran.
    pub pass: CalcPassId,
    /// Workbook instance containing the evaluated formula or definition.
    pub workbook: WorkbookInstanceId,
    /// Location or definition evaluated by the invocation.
    pub locus: L,
    /// Identity of the root evaluation within the pass.
    pub evaluation: u64,
    /// Call site in the compiled formula or definition.
    pub call_site: u32,
    /// Occurrence path distinguishing repeated calls at a site.
    pub occurrence: Vec<u32>,
    /// Reason for semantic reevaluation.
    pub reason: ReevaluationReason,
}
/// An explicit definition evaluation has its own locus. Its caller places relative
/// references; it is not a formula cell or a shared value attached to that name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NameEvaluationLocus {
    /// Resolved definition being evaluated.
    pub definition: calc_fml::reference::BoundName,
    /// Cell used to place the definition's relative references.
    pub caller: CellKey,
}
/// Invocation identity for an explicitly evaluated name definition.
pub type NameInvocationId = InvocationId<NameEvaluationLocus>;
/// Workbook and formula generation to which a result belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EvaluationTarget {
    /// Workbook instance owning the target.
    pub workbook: WorkbookInstanceId,
    /// Authored revision from which this evaluation originated.
    pub origin_revision: AuthoredRevision,
    /// Generation of the target formula.
    pub formula_generation: u64,
    /// Dependency graph node for the formula.
    pub node: NodeId,
    /// Cell holding the formula.
    pub locus: CellKey,
}
/// Authored model and external input cutoff fixed at command admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdmissionFrontier {
    /// Authored model revision admitted to the command.
    pub model: AuthoredRevision,
    /// Highest external input sequence admitted to the command.
    pub external_through: u64,
}
/// Identity of a captured result read view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReadViewId(pub u64);
/// Versions visible to an evaluation when it made its observations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObservationFrontier {
    /// Authored model revision the evaluation observed.
    pub model: AuthoredRevision,
    /// Captured result view the evaluation could read.
    pub results: ReadViewId,
    /// External input version the evaluation observed.
    pub inputs: u64,
}

/// Workbook region selected for calculation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CalcScope {
    /// All sheets in the workbook.
    Workbook,
    /// A single sheet.
    Sheet(SheetId),
    /// The supplied sheet-qualified grid areas.
    Range(Vec<GridArea>),
}
/// The measured Manual Range.Calculate profile keeps its targets required for the next
/// command. Satisfied is the SliceV1 policy for other admissions, not broad Auto-mode parity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RangePostState {
    /// Satisfy the targets' requirements upon publication.
    Satisfied,
    /// Keep the targets required for the next command.
    StillRequired,
}
/// Policy for selecting formulas within a calculation scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvalSelection {
    /// Select required and volatile formulas.
    DirtyAndVolatile,
    /// Select all formulas in the scope.
    ForceAll,
    /// One assigned formula, evaluated on the owner before a later calculation.
    /// Only a single-cell range is admitted; Manual entry leaves it required.
    FormulaEntry,
}
impl EvalSelection {
    /// Whether selection forces evaluation of each formula in its admitted scope.
    pub fn forces_all(self) -> bool {
        matches!(self, Self::ForceAll | Self::FormulaEntry)
    }
}
/// Reason for starting a fresh semantic evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReevaluationReason {
    /// A dependency discovered during evaluation changed the result's prerequisites.
    DependencyRepair,
    /// A callable binding changed after a reload.
    CallableReload,
    /// An XLL returned from an uncalculated read.
    XllUncalcedUnwind,
    /// Arguments observed provisionally became final.
    ProvisionalArguments,
    /// A later command required a new evaluation.
    NewCommandDemand,
    /// A formula was evaluated during assignment.
    FormulaEntry,
    /// An authored input or external input changed.
    AuthoredOrInputChange,
    /// A spill footprint change initiated another pass.
    SpillShapePass,
    /// An iterative cycle advanced to a new sweep.
    IterationSweep,
    /// A data-table calculation entered a scenario.
    TableScenario,
}

/// An opaque adapter handle resolves to this exact owner-issued token. Addresses alone
/// cannot authorize a return, including after Undo restores identical formula text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AsyncToken {
    /// Destination whose pending activation can receive the completion.
    pub destination: ResultDestination,
    /// Owner-issued token identity.
    pub id: u64,
    /// Invocation that issued the request.
    pub invocation: InvocationId,
    /// Runtime activation identity.
    pub runtime: u64,
    /// Formula generation and workbook instance eligible for completion.
    pub target: EvaluationTarget,
    /// Callable registration that issued the request.
    pub provider: calc_fml::CallableHandle,
    /// Provider request identity.
    pub request: u64,
    /// Target requirement version at suspension.
    pub required: u64,
}
/// Reason an asynchronous completion could not be admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncRejection {
    /// Token identity has no matching registration.
    UnknownToken,
    /// The supplied token differs from the registered identity.
    IdentityMismatch,
    /// The owner revoked the token.
    Revoked,
    /// The formula target or generation changed.
    TargetChanged,
    /// The issuing provider changed.
    ProviderChanged,
    /// The target requirements advanced after suspension.
    RequirementsChanged,
    /// The original arguments no longer apply.
    ArgumentsChanged,
    /// The suspended continuation cannot receive the completion.
    ContinuationUnavailable,
    /// The token already received a different value.
    ConflictingDuplicate,
    /// The returned value failed admission checks.
    InvalidValue,
}
/// Outcome of delivering an asynchronous completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncDisposition {
    /// The completion was admitted.
    Accepted,
    /// An identical completion was already received.
    Duplicate,
    /// Admission failed for the recorded reason.
    Rejected(AsyncRejection),
}

/// A cached region read recorded by an invocation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CachedObservation {
    /// Whether the read included an unversioned external cache.
    pub external_cache: bool,
    /// Result view pinned for the read.
    pub view: crate::CachedReadLease,
    /// Evaluator request that produced the observation.
    pub request: calc_fml::ReadRequest,
    /// Region covered by the read.
    pub region: GridArea,
    /// Boundary-cache lineage when available.
    pub lineage: Option<BoundaryReadSpan>,
    /// Whether the cached value was observed provisionally.
    pub provisional: bool,
}
/// Record of an executed user-callable invocation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CallInvocation<L = CellKey> {
    /// Semantic identity of this call occurrence.
    pub identity: InvocationId<L>,
    /// Runtime activation identity.
    pub runtime: u64,
    /// Runtime identity of the parent call, when nested.
    pub parent: Option<u64>,
    /// Callable registration actually invoked.
    pub callable: calc_fml::CallableHandle,
    /// Whether the call completed.
    pub completed: bool,
    /// Whether the call runs in a synchronous interval without preemption.
    pub non_preemptible: bool,
}
/// Cached result view associated with a root invocation.
#[derive(Clone, Debug)]
pub struct InvocationReadView<L = CellKey> {
    /// Root invocation that owns this read view.
    pub root: InvocationId<L>,
    /// Pinned results available to the invocation.
    pub view: crate::CachedReadLease,
}

/// A value version is the producing pass. Every semantic reevaluation starts a new pass;
/// physical dependency retries keep the pass and the root invocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RequirementState {
    /// Producing pass of the held calculation value, if available.
    pub value_version: Option<CalcPassId>,
    /// Latest version that requires evaluation of this target.
    pub required: u64,
    /// Latest requirement version satisfied by the target.
    pub satisfied: u64,
}

/// One stamp per span, including formula requirements. These are maxima of globally increasing
/// versions, so changing any member advances the span; no per-cell certificate is allocated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpanVersions {
    /// Latest authored change affecting the span.
    pub authored: u64,
    /// Latest producing pass observed in the span.
    pub value: Option<CalcPassId>,
    /// Latest formula requirement in the span.
    pub required: u64,
    /// Latest satisfied formula requirement in the span.
    pub satisfied: u64,
    /// Latest formula generation change affecting the span.
    pub formulas: u64,
    /// Version of pending production affecting the span.
    pub pending: u64,
    /// Existing iteration/table session values have actual calculation ticks, but no SliceV1
    /// invocation proof. Keep that boundary explicit through ordinary dependents.
    pub uncertified: u64,
    /// Latest calculation tick for legacy session results.
    pub legacy_tick: u64,
    /// Latest version observed through a pinned result cache.
    pub boundary_cache: u64,
    /// Latest version observed from a value loaded with the workbook.
    pub imported_cache: u64,
}
/// Dependency read over a sheet-qualified region.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReadSpan {
    /// Region observed by the evaluation.
    pub region: GridArea,
    /// Kind of read made by the evaluator.
    pub request: ReadRequest,
    /// Versions observed across the region.
    pub versions: SpanVersions,
    /// Boundary-cache lineage associated with this dependency.
    pub boundary_caches: Vec<BoundaryReadSpan>,
}
/// Version of a workbook fact observed during evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FactStamp {
    /// Fact query issued by the evaluator.
    pub read: FactRead,
    /// Version observed for the queried fact.
    pub version: u64,
}
/// Purpose of an observation that establishes absence over a region.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AbsenceKind {
    /// A read observed blank cells.
    ReadBlank,
    /// A spill checked for blockers in its output range.
    SpillReservation,
}
/// Evidence that a read or spill reservation observed an unoccupied region.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AbsenceProof {
    /// Region covered by the absence observation.
    pub region: GridArea,
    /// Purpose for which the absence was established.
    pub kind: AbsenceKind,
    /// Authored change version observed for the region.
    pub authored: u64,
    /// Occupancy version observed for the region.
    pub occupancy: u64,
    /// Version of spill corridors affecting the region.
    pub corridor: u64,
    /// Version of formula producers affecting the region.
    pub producers: u64,
    /// Number of producers still pending when the region was observed.
    pub pending_producers: usize,
    /// An exact scoped command may use the pinned absence of excluded producers.
    /// This never establishes workbook-wide absence or freshness.
    pub cached: Option<ReadViewId>,
    /// Contiguous blank runs recorded by the observation.
    pub blanks: Vec<BlankRun>,
}
/// A batched read through the command's pinned boundary cache. Imported values have
/// no producing invocation. Dirty caches remain qualified even if their numeric value matches.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BoundaryReadSpan {
    /// Region covered by this cached read.
    pub region: GridArea,
    /// Pinned result view used for the read.
    pub view: ReadViewId,
    /// Version of calculated values in the cached region.
    pub values: u64,
    /// Version of dirty cached values in the region.
    pub dirty: u64,
    /// Version of imported cached values in the region.
    pub imported: u64,
    /// Source parts that supplied the imported values.
    pub imported_sources: Vec<std::sync::Arc<calc_doc_model::SourcePart>>,
}
/// Common dependency/freshness evidence stays inline. Other evidence is allocated only
/// when used; field reads still expose the complete certificate through Deref.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ReadCertificate {
    /// The calculation consumed a saved external-link cache without a versioned live input.
    pub external_cache: bool,
    /// An existing session boundary has calculation provenance but no invocation certificate.
    pub legacy_boundary: bool,
    /// Whether a read depended on the command's pinned boundary cache.
    pub uses_boundary_cache: bool,
    /// Whether a read depended on a value imported from the workbook.
    pub uses_imported_cache: bool,
    /// Versioned cell and region dependencies read by the evaluation.
    pub dependencies: Vec<ReadSpan>,
    /// Version of the calculation's semantic environment.
    pub semantic_environment: u64,
    /// Version of the formula's reference bindings.
    pub bindings: u64,
    /// Version of volatile inputs observed by the evaluation, if any.
    pub volatile_inputs: Option<u64>,
    pub(crate) details: OptionalReadDetails,
}

/// Less frequent read evidence. Absence, observation and callable proofs are retained
/// in full when present; an unallocated instance has exactly the default contents.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ReadDetails {
    /// Version and lineage of linked workbook inputs, if read.
    pub linked_inputs: Option<crate::LinkedInputStamp>,
    /// Cached observations collected during invocation.
    pub observations: Vec<CachedObservation>,
    /// RTD topic reads and sequences observed by the evaluation.
    pub external_inputs: Vec<calc_fml::RtdRead>,
    /// Callable registrations and versions observed by the evaluation.
    pub callables: Vec<calc_fml::CallableBindingProof>,
    /// User callable invocations executed by the evaluation.
    pub invocations: Vec<CallInvocation>,
    /// Actual executed definition bodies, scoped by this result's producing invocation.
    pub name_invocations: Vec<calc_fml::NameInvocation>,
    /// Number of executed name invocations omitted from the retained evidence.
    pub omitted_name_invocations: u64,
    /// What the user functions this evaluation ran asked of the formula's volatility, when its
    /// plan can run one: no request, the last request executed `false`, or `true`. `None` for a
    /// plan with no user call.
    pub udf_volatility: Option<calc_fml::UdfVolatility>,
    /// Pinned boundary-cache reads with their source lineage.
    pub boundary_caches: Vec<BoundaryReadSpan>,
    /// Facts and versions observed by the evaluation.
    pub facts: Vec<FactStamp>,
    /// Blank and spill-reservation observations.
    pub absences: Vec<AbsenceProof>,
    /// Resolved identities/versions and scoped namespace probes, including misses.
    pub names: Vec<calc_fml::reference::NameBindingProof>,
}
static EMPTY_READ_DETAILS: ReadDetails = ReadDetails {
    linked_inputs: None,
    observations: Vec::new(),
    external_inputs: Vec::new(),
    callables: Vec::new(),
    invocations: Vec::new(),
    name_invocations: Vec::new(),
    omitted_name_invocations: 0,
    udf_volatility: None,
    boundary_caches: Vec::new(),
    facts: Vec::new(),
    absences: Vec::new(),
    names: Vec::new(),
};

#[derive(Clone, Debug, Default)]
pub(crate) struct OptionalReadDetails(Option<Box<ReadDetails>>);
impl std::ops::Deref for OptionalReadDetails {
    type Target = ReadDetails;
    fn deref(&self) -> &ReadDetails {
        self.0.as_deref().unwrap_or(&EMPTY_READ_DETAILS)
    }
}
impl std::ops::DerefMut for OptionalReadDetails {
    fn deref_mut(&mut self) -> &mut ReadDetails {
        self.0.get_or_insert_with(Box::default)
    }
}
// Allocation history never changes certificate equality or a hash-table key.
impl PartialEq for OptionalReadDetails {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}
impl Eq for OptionalReadDetails {}
impl std::hash::Hash for OptionalReadDetails {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&**self, state);
    }
}
impl std::ops::Deref for ReadCertificate {
    type Target = ReadDetails;
    fn deref(&self) -> &ReadDetails {
        &self.details
    }
}
impl std::ops::DerefMut for ReadCertificate {
    fn deref_mut(&mut self) -> &mut ReadDetails {
        &mut self.details
    }
}
/// Outcome of calculating an individual formula target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetCompletion {
    /// Calculation produced a worksheet value.
    Value,
    /// Calculation produced a worksheet error value.
    WorksheetError,
    /// Engine refused the target.
    EngineRefusal,
}
/// How the engine obtained the recorded outcome.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ComputationOrigin {
    /// A formula invocation produced the result.
    Evaluated {
        /// Root formula invocation that produced the result.
        root: InvocationId,
    },
    /// The engine recorded a typed refusal.
    EngineRefusal,
    /// The engine applied its circular-reference rule.
    CircularRule,
}
/// Command and input frontier for which a result is certified.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FreshnessClaim {
    /// Calculation whose requirement the certification covers.
    pub command: CalcCommandId,
    /// Pass that certified the result.
    pub pass: CalcPassId,
    /// Authored and input cut against which freshness was established.
    pub frontier: AdmissionFrontier,
    /// Requirement version covered by the certification.
    pub required: u64,
}
/// Provenance and freshness evidence for one formula result.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResultStamp {
    /// Result store that owns the stamp.
    pub destination: ResultDestination,
    /// Workbook and formula target of the result.
    pub target: EvaluationTarget,
    /// Input versions visible to the producing evaluation.
    pub observed: ObservationFrontier,
    /// Origin of the computed value or refusal.
    pub computed_by: ComputationOrigin,
    /// Dependencies and input observations of the producing evaluation.
    pub reads: ReadCertificate,
    /// Freshness claim established for the current requirement.
    pub certification: FreshnessClaim,
    /// Result classification for this target.
    pub completion: TargetCompletion,
}
/// Receipt describing the scope and inputs of an admitted calculation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmissionReceipt {
    /// Result store eligible to receive publication.
    pub destination: ResultDestination,
    /// Identity assigned to the admitted calculation.
    pub command: CalcCommandId,
    /// Client command that requested the calculation.
    pub source: CommandId,
    /// Initial pass identity.
    pub pass: CalcPassId,
    /// Scope admitted for calculation.
    pub scope: CalcScope,
    /// Requirement policy applied after a scoped range calculation.
    pub range_post_state: RangePostState,
    /// Formula selection policy applied within the scope.
    pub selection: EvalSelection,
    /// Authored model and external inputs fixed at admission.
    pub admitted: AdmissionFrontier,
    /// Earlier calculation identified as this admission's predecessor, if any.
    pub predecessor: Option<CalcCommandId>,
}
/// Terminal disposition of an admitted calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionOutcome {
    /// The command reached completion.
    Complete,
    /// The command ended with unmet work.
    Incomplete,
    /// A later calculation superseded the command.
    Superseded {
        /// Calculation that superseded this command.
        by: CalcCommandId,
    },
    /// The command was cancelled.
    Cancelled,
    /// The command ended with an execution failure.
    Failed,
}
/// Counts describing the outcomes inside the requested scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScopeCoverage {
    /// Number of required targets in the scope.
    pub required: usize,
    /// Number of targets covered by calculated results.
    pub calculated: usize,
    /// Number of targets with a typed refusal.
    pub refused: usize,
    /// Number of targets still held without satisfying their requirement.
    pub held: usize,
    /// Number of targets with calculation provenance but no invocation certificate.
    pub uncertified: usize,
    /// Number of targets qualified by a boundary-cache read.
    pub boundary_cached: usize,
    /// Number of targets qualified by an imported-cache read.
    pub imported_cached: usize,
}
/// Identity of one result publication within a calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationUnitId {
    /// Calculation that owns the publication.
    pub command: CalcCommandId,
    /// Publication sequence within the calculation.
    pub sequence: u64,
}
/// Terminal receipt for an admitted calculation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionReceipt {
    /// Destination associated with the calculation.
    pub destination: ResultDestination,
    /// Calculation covered by the receipt.
    pub command: CalcCommandId,
    /// Requested calculation scope.
    pub scope: CalcScope,
    /// Authored and input frontier fixed at admission.
    pub admitted: AdmissionFrontier,
    /// Terminal disposition of the calculation.
    pub outcome: CompletionOutcome,
    /// Coverage counts for the requested targets.
    pub coverage: ScopeCoverage,
    /// Result view observed at completion.
    pub results: ReadViewId,
    /// Latest publication acknowledged by the receipt, when present.
    pub after_publication: Option<PublicationUnitId>,
}

/// The stop condition met by an actual sequence of iteration passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationStop {
    /// The configured pass limit was reached.
    MaximumIterations,
    /// The largest observed change is below MaxChange.
    Converged,
    /// No numeric stopping sample was available.
    NoNumericSample,
}

/// The value image used for a read of an iterative producer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IterationReadKind {
    /// The producer had yet to run on this pass and supplied its seed or prior pass value.
    PriorPass,
    /// The producer had staged its current pass value.
    CurrentPass,
}

/// A witnessed read within an iterative sweep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct IterationRead {
    /// Formula that read the value.
    pub reader: CellKey,
    /// Cell whose value the reader consumed.
    pub source: CellKey,
    /// Which pass supplied the value.
    pub kind: IterationReadKind,
}

/// What the iteration scheduler measured at its stopping boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationObservation {
    /// Cells admitted into the circular group, in scheduler order.
    pub members: Vec<CellKey>,
    /// Additional members beyond the retained list.
    pub omitted_members: usize,
    /// Completed passes.
    pub passes: u32,
    /// Largest stopping sample, as binary64 bits.
    pub residual_bits: Option<u64>,
    /// Configured MaxChange, as binary64 bits.
    pub max_change_bits: u64,
    /// Observed reason the scheduler stopped repeating the pass.
    pub stop: IterationStop,
    /// Retained distinct iterative reads from the final pass.
    pub reads: Vec<IterationRead>,
    /// Whether the final pass exceeded the read sample limit.
    pub reads_limited: bool,
}
/// Actual evaluator entries in one dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionBand {
    /// Pass ordinal for these entries.
    pub pass: u32,
    /// Cells entered by the dispatch. Worker bands have no ordering within the band.
    pub cells: Vec<CellKey>,
    /// Whether a pool of native workers handled this dispatch.
    pub workers: bool,
}

/// Bounded execution evidence from a calculation explicitly asked to retain it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionObservation {
    /// Evaluator entries including those beyond the retained list.
    pub total_entries: u64,
    /// Recorded evaluator entries, grouped at their dispatch boundary.
    pub bands: Vec<ExecutionBand>,
}

impl ExecutionObservation {
    pub(crate) fn observe(
        &mut self,
        pass: u32,
        workers: bool,
        cells: impl IntoIterator<Item = CellKey>,
    ) {
        const LIMIT: u64 = 200;
        let mut retained = Vec::new();
        for cell in cells {
            if self.total_entries < LIMIT {
                retained.push(cell);
            }
            self.total_entries += 1;
        }
        if !retained.is_empty() {
            self.bands.push(ExecutionBand { pass, cells: retained, workers });
        }
    }
}

/// Authored, execution and publication observations reported by the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalcEvent {
    /// Evaluator entries retained from an observed calculation.
    ExecutionObserved(ExecutionObservation),
    /// An authored revision was committed.
    Committed {
        /// Revision made current by the commit.
        revision: AuthoredRevision,
    },
    /// A calculation command was admitted.
    Admitted(AdmissionReceipt),
    /// These targets remain unmet and retain their old result provenance.
    TargetsRefused {
        /// Calculation responsible for the refusals.
        command: CalcCommandId,
        /// Targets whose requirements remain unmet.
        targets: Vec<CellKey>,
        /// Unsupported construct reported for the targets.
        construct: &'static str,
        /// Work package responsible for the construct.
        owner: &'static str,
    },
    /// The calculation yielded with work remaining.
    Yielded {
        /// Pass that yielded.
        pass: CalcPassId,
        /// Remaining work reported by the scheduler.
        remaining: usize,
    },
    /// An admitted command was superseded.
    Superseded(CompletionReceipt),
    /// A repeating calculation reached its stopping boundary.
    IterationObserved(IterationObservation),
    /// A held result was certified for the current command.
    Reused {
        /// Cell whose result was reused.
        at: CellKey,
        /// Command that originally computed the result.
        computed_by: CalcCommandId,
        /// Command for which the result was certified.
        certified_for: CalcCommandId,
    },
    /// A held result could not be certified.
    Rejected {
        /// Cell whose held result was rejected.
        at: CellKey,
    },
    /// A formula began a new semantic invocation.
    Reevaluated {
        /// Identity of the prior invocation.
        previous: InvocationId,
        /// Identity of the replacement invocation.
        next: InvocationId,
        /// Callable invocation records carried with the reevaluation event.
        invocations: Vec<CallInvocation>,
    },
    /// An activation suspended awaiting asynchronous completion.
    AwaitingAsync(AsyncToken),
    /// The owner attempted to admit an asynchronous return.
    AsyncReturned {
        /// Identity of the delivered token.
        token: u64,
        /// Admission outcome for the return.
        disposition: AsyncDisposition,
    },
    /// A result publication became visible to the command's destination.
    Published(PublicationUnitId),
    /// A calculation reached a terminal outcome.
    Completed(CompletionReceipt),
}

use crate::read::Progress;
use crate::state::{ArrayRole, Binding};
use crate::{EngineState, FormulaResult};
use std::collections::{BTreeMap, BTreeSet};

/// Generation-checked persistent node metadata. Requirement pins and formula target
/// generations share the existing bounded slot pages; a reused slot is a different key.
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeCut<T>(crate::slots::SlotCut<(NodeId, T)>);
pub(crate) type RequirementCut = NodeCut<RequirementState>;
impl<T: Clone> NodeCut<T> {
    pub fn get(&self, node: &NodeId) -> Option<&T> {
        self.0
            .get(node.slot())
            .filter(|(held, _)| held == node)
            .map(|(_, value)| value)
    }
    fn insert(&mut self, node: NodeId, value: T) {
        self.0.insert(node.slot(), (node, value));
    }
    fn remove(&mut self, node: NodeId) {
        if self.get(&node).is_some() {
            self.0.remove(node.slot());
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ContractState {
    pub workbook: WorkbookInstanceId,
    epoch: u64,
    generation: u64,
    pub environment: u64,
    pub bindings: u64,
    pub coordinates: u64,
    pub facts: u64,
    pub external_inputs: u64,
    pub external_versions: BTreeMap<calc_fml::TopicKey, (calc_fml::RtdLease, u64)>,
    generations: NodeCut<u64>,
    requirements: RequirementCut,
    pub demanded: BTreeSet<NodeId>,
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
            external_inputs: 0,
            external_versions: BTreeMap::new(),
            generations: NodeCut::default(),
            requirements: RequirementCut::default(),
            demanded: BTreeSet::new(),
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
        self.external_inputs = previous.external_inputs;
        self.external_versions = previous.external_versions.clone();
        self.generation = previous.generation;
        self.generations = NodeCut::default();
        self.requirements = RequirementCut::default();
        self.demanded.clear();
        for node in nodes {
            self.replace(node);
        }
    }
    pub fn snapshot(&self) -> RequirementCut {
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
        self.generations.remove(node);
        self.requirements.remove(node);
        self.demanded.remove(&node);
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
            if current.required != current.satisfied {
                self.demanded.insert(node);
            } else {
                self.demanded.remove(&node);
            }
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
        if current.required != current.satisfied {
            self.demanded.insert(node);
        } else {
            self.demanded.remove(&node);
        }
    }
}

pub(crate) fn meets(a: GridArea, b: GridArea) -> bool {
    a.sheet() == b.sheet()
        && a.first().row() <= b.last().row()
        && b.first().row() <= a.last().row()
        && a.first().column() <= b.last().column()
        && b.first().column() <= a.last().column()
}

/// The versions of what produces `region`'s values. `reader`, the formula that read the region,
/// is left out: its own spill over cells it read is no producer of what it read (Excel P05,
/// 16.0.20527 through Touchstone, 2026-11-16, cell-context family spill-placement own-*: a
/// formula reads the cells its own spill will occupy as blank, and its spill does not feed back).
pub(crate) fn span_versions_at(
    state: &EngineState,
    progress: &Progress,
    region: GridArea,
    cut: Option<&RequirementCut>,
    reader: Option<NodeId>,
) -> SpanVersions {
    let mut versions = SpanVersions {
        authored: state.contract.authored(region),
        ..SpanVersions::default()
    };
    for node in span_nodes_at(state, progress, region).filter(|node| Some(*node) != reader) {
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

fn span_nodes_at(
    state: &EngineState,
    progress: &Progress,
    region: GridArea,
) -> impl Iterator<Item = NodeId> {
    // A scalar span with no published, imported or staged array footprint has
    // exactly its authored anchor as a possible producer. Keep the ordinary
    // validator, without two range bounds or a temporary ordered allocation.
    if region.first() == region.last()
        && state.spills.is_empty()
        && state.cached_arrays.is_empty()
        && progress.footprints.is_empty()
    {
        return state
            .node_at(region.first())
            .into_iter()
            .chain(BTreeSet::new());
    }
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
    None.into_iter().chain(nodes)
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

pub(crate) fn observation_ancestry_at(
    state: &EngineState,
    progress: &Progress,
    region: GridArea,
) -> Vec<CachedObservation> {
    let mut observations = Vec::new();
    for node in span_nodes_at(state, progress, region) {
        let result = if progress.is_evaluated(node) {
            progress.staged(node)
        } else if progress.is_scheduled(node) {
            None
        } else {
            state.results.of(node)
        };
        if let Some(stamp) = result.and_then(FormulaResult::stamp) {
            for read in &stamp.reads.observations {
                if !observations.contains(read) {
                    observations.push(read.clone());
                }
            }
        }
    }
    observations
}

pub(crate) fn producer_version_at(
    state: &EngineState,
    region: GridArea,
    except: NodeId,
    cut: Option<&RequirementCut>,
) -> u64 {
    state
        .bindings
        .spill_iter()
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
    cut: Option<&RequirementCut>,
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
    cut: Option<&RequirementCut>,
    includes: &dyn Fn(CellKey) -> bool,
) -> usize {
    state
        .bindings
        .spill_iter()
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
            // Prefix publication can install a producer before this validator
            // is called with an empty Progress and the original requirement cut.
            // As in span_versions_at, only a stamp for that required version
            // discharges it; an imported or older result still remains pending.
            if result
                .and_then(FormulaResult::stamp)
                .is_some_and(|stamp| stamp.certification.required == current.required)
            {
                current.satisfied = current.required;
            }
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
        calc_fml::FactSubject::Area(area) => state.contract.authored(area),
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
        &|_| false,
    )
}

/// Validate only names/probes this plan actually bound; unrelated name edits preserve it.
pub(crate) fn name_bindings_valid(
    inputs: &ModelRoot,
    proofs: &[calc_fml::reference::NameBindingProof],
) -> bool {
    let table = inputs.definitions().name_table();
    proofs.iter().all(|proof| {
        proof
            .probes
            .iter()
            .all(|probe| table.lookup_key(&probe.key).map(|n| n.id()) == probe.found)
            && proof.resolved.is_none_or(|bound| {
                table.get(bound.id).is_some_and(|n| {
                    n.binding_version() == bound.version && n.definition().scope == bound.scope
                })
            })
    })
}

pub(crate) fn certificate_valid_at(
    state: &EngineState,
    inputs: &ModelRoot,
    progress: &Progress,
    stamp: &ResultStamp,
    facts: u64,
    cut: Option<&RequirementCut>,
    volatile_inputs: u64,
    // A read of a connection's provisional value the owner admits at the end of this step.
    provisional: &dyn Fn(&calc_fml::RtdRead) -> bool,
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
        || stamp
            .reads
            .linked_inputs
            .as_ref()
            .is_some_and(|pin| pin.version != state.linked_input_version)
        || stamp.reads.bindings != state.contract.bindings
        || stamp.reads.external_inputs.iter().any(|read| {
            // The engine's sequence for the lease the read asked for: the latest admitted one
            // when that is this lease, else nothing (0). A read of another incarnation that saw
            // a value therefore goes stale when a newer incarnation is admitted.
            state
                .contract
                .external_versions
                .get(&read.topic)
                .filter(|(lease, _)| *lease == read.lease)
                .map_or(0, |(_, sequence)| *sequence)
                != read.sequence
                && !provisional(read)
        })
        || !name_bindings_valid(inputs, &stamp.reads.names)
        || stamp
            .reads
            .callables
            .iter()
            .any(|proof| !state.callables.validates(proof))
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
        .all(|read| {
            span_versions_at(state, progress, read.region, cut, Some(target.node)) == read.versions
        })
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

    /// Whether the cell is covered by this set.
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
        _ => edit.tracked_areas(area),
    }
}

/// One actual blank run within a rectangular read, using the reader's row-major offsets.
/// Keeping runs avoids inventing absence for populated cells and avoids per-cell certificates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlankRun {
    /// Offset of the first blank in the flattened region.
    pub offset: u64,
    /// Number of consecutive blanks in the run.
    pub len: u64,
}
impl BlankRun {
    /// Find the first cell in the blank run that intersects the requested read area.
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
            && !inputs
                .definitions()
                .pivots()
                .tables()
                .any(|table| table.area.intersection(area).is_some())
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

#[cfg(test)]
mod read_details_tests {
    use super::ReadCertificate;
    use std::hash::{Hash, Hasher};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn empty_read_details_keep_content_equality_and_clone_isolation() {
        let empty = ReadCertificate::default();
        assert!(empty.details.0.is_none());
        let mut used = empty.clone();
        used.udf_volatility = Some(calc_fml::UdfVolatility::Set(false));
        let retained = used.clone();
        used.udf_volatility = None;
        assert!(used.details.0.is_some());
        assert_eq!(empty, used);
        assert_eq!(
            retained.udf_volatility,
            Some(calc_fml::UdfVolatility::Set(false))
        );
        let hash = |value: &ReadCertificate| {
            let mut state = std::collections::hash_map::DefaultHasher::new();
            value.hash(&mut state);
            state.finish()
        };
        assert_eq!(hash(&empty), hash(&used));
        assert_ne!(empty, retained);
    }
}

#[cfg(test)]
mod generation_cut_tests {
    use super::{ContractState, NodeId};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn formula_generations_keep_full_handles_and_pinned_versions_on_slot_reuse() {
        for slot in [0, 256, 65_536, u32::MAX] {
            let old = NodeId::from_parts(slot, 1);
            let new = NodeId::from_parts(slot, 2);
            let mut state = ContractState::new();
            state.replace(old);
            let generation = state.generation(old);
            let pinned = state.clone();
            state.replace(new);
            let replacement = state.generation(new);
            assert!(replacement > generation);
            assert_eq!(
                state.generation(old),
                1,
                "a stale handle cannot read its replacement"
            );
            assert_eq!(pinned.generation(old), generation);
            assert_eq!(pinned.generation(new), 1);
            state.retire(old);
            assert_eq!(
                state.generation(new),
                replacement,
                "stale retirement cannot erase a live target"
            );
            state.retire(new);
            assert_eq!(state.generation(new), 1);
            assert_eq!(
                pinned.generation(old),
                generation,
                "retirement leaves the pinned save intact"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rebuilding_identity_renews_targets_without_rewriting_an_admitted_requirement_cut() {
        let node = NodeId::from_parts(u32::MAX, 7);
        let mut old = ContractState::new();
        old.replace(node);
        old.require_at([node], 17);
        let pinned = old.snapshot();
        let mut rebuilt = ContractState::new();
        rebuilt.inherit_identity(&old, vec![node]);
        assert_eq!(rebuilt.workbook, old.workbook);
        assert!(rebuilt.generation(node) > old.generation(node));
        assert_eq!(rebuilt.requirement(node).required, 1);
        rebuilt.require_at([node], 23);
        old.retire(node);
        assert_eq!(pinned.get(&node).unwrap().required, 17);
        assert_eq!(rebuilt.requirement(node).required, 23);
    }
}
