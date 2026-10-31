# DNA Calc recalculation contract: envelope and first end-to-end slice

**Specification date:** 31 October 2026  
**Status:** Proposed normative contract, with a deterministic `SliceV1` reference profile and explicit Excel measurement gates.  
**Basis:** The supplied `7-recalc-contract-prompt.md`, especially Appendix A §§1.3, 1.9 and 4, Appendix B §§D–F, and Appendix C's first-slice proposal. [P]

## 0. Decisions that make the contract implementable

Implement a serial engine with one owner of model commits, calculation admission and result installation. Keep its public envelope as records passed between existing callers. Do not introduce a general transaction framework, a new authored node kind, stable row/column identities, or a runtime-specific calculation API.

Six clarifications are necessary before coding:

1. **Separate admission from completion.** `AdmissionReceipt` precedes the values caused by a command. `CompletionReceipt` follows its required result installations. The phrase “the command receipt precedes its values” in the synthesis must mean the former.
2. **A calculation command has a fixed admitted input frontier.** An edit after admission creates a successor command and a new pass; it does not silently alter the old command's promise. Reusing running work is permitted across that boundary. In this specification, that is the precise meaning of merging an edit into running calculation.
3. **A semantic evaluation is different from a physical attempt.** Include an evaluation ordinal in invocation identity. Yield and physical retry preserve it; an explicitly authorized new semantic evaluation changes it.
4. **Certificates distinguish dependencies from observations.** A VBA cached read is checked against its authorized read view, not the latest cache, and installs no reverse dependency edge. An ordinary formula read remains a dependency even when a narrow command permits a stale boundary cache.
5. **Freshness is a qualified claim for a command.** A completed C1-only calculation can correctly return 4 while B1 is dirty and cached at 2. No result or receipt exposes an unqualified workbook-wide `settled` flag.
6. **Effect execution and result publication have separate outcomes.** An invocation can perform an irreversible effect and subsequently have its result discarded. Cancelling or superseding a command cannot undo the effect.

The eight demonstrations below have exact outcomes under `SliceV1`. The profile's cached-read frontier, scope normalization outside Manual mode, and several Excel ordering details remain measurement gates, listed in §13. Implementing the profile is not evidence that those Excel measurements have been completed.

### Evidence and terminology

`MUST`, `MUST NOT` and `MAY` state proposed DNA Calc requirements. A **documented Excel fact** is cited to a primary source. A **pack observation** is attributed to [P], rather than represented as a measurement performed for this answer. Neither the repositories nor a running Excel instance were inspected here. The tests in this document are specifications, not reported test runs.

“Cache” means committed calculated results, including results intentionally retained while dirty. “Candidate” means a private, uncommitted result. “Current” always names the command/frontier or current live requirement against which it is assessed.

## 1. The envelope

The following is Rust-flavoured pseudocode. Existing value, address, patch and callable types are reused. `Arc`, vectors and bitsets describe ownership and logical contents; they do not prescribe allocation per scalar read. Constructors that allocate identities, authorize read views or certify results are private to their owners.

### 1.1 Identities, commands and frontiers

```rust
// Transport/edit identity and accepted calculation identity are distinct.
struct CommandId { client_session: SessionId, sequence: u64 }
struct CalcCommandId { domain_incarnation: DomainId, sequence: u64 }
struct CalcPassId { command: CalcCommandId, ordinal: u32 }

struct WorkbookInstanceId { document: DocumentId, incarnation: u64 }
// SheetInstanceId likewise includes a lifetime/generation, not a reusable slot.

enum EvaluationLocus {
    Cell { sheet: SheetInstanceId, address: CellAddress },
    Name { name: NameId, caller: QualifiedCellAddress }, // reserved in slice
}

struct EvaluationTarget {
    workbook: WorkbookInstanceId,
    origin_revision: Revision,
    formula_generation: FormulaGeneration,
    locus: EvaluationLocus,
}

struct InvocationId {
    pass: CalcPassId,                 // includes CalcCommandId
    workbook: WorkbookInstanceId,     // prevents cross-workbook locus collisions
    locus: EvaluationLocus,
    evaluation: u64,                  // semantic evaluation ordinal at this locus
    call_site: CallSiteId,            // includes a reserved formula-root site
    occurrence: u64,                  // repeated execution of this site
}

struct AdmissionFrontier {
    model: Revision,
    external_through: DomainEventSeq, // fixed RTD/link/fact input cut
}

enum ResultDestination {
    Live,
    PinnedSave { save: CommandId },   // authority rooted in this save's pinned model
}

struct CalcRequest {
    id: CalcCommandId,
    source: CommandId,
    predecessor: Option<CalcCommandId>,
    cause: CalcCause,                 // explicit command, automatic edit, successor...
    mode: CalcMode,                   // snapshot of application-domain policy
    scope: CalcScope,                 // resolved workbook/sheet/AreaSet, not viewport
    evaluation: EvalSelection,        // DirtyAndVolatile | ForceAll
    tables: TablePolicy,
    admitted: AdmissionFrontier,
    inputs: InputViewId,
    profile: CompatibilityProfileId,
    destination: ResultDestination,
}

struct ObservationFrontier {
    model: Revision,
    inputs: InputViewId,
    results: ReadViewId,              // immutable committed result root
    order: ObservationOrder,          // semantic fence/cut, not UI delivery order
}

struct ReadView {
    frontier: ObservationFrontier,
    model_pin: ModelPin,
    results_pin: ResultViewPin,
}
```

**Identity invariants.** IDs are unique within their stated lifetime, never recycled while a retained result, journal record or provider token can refer to them, and never allowed to wrap silently. Reopening a document creates a new workbook incarnation. Moving, deleting, replacing or restoring a formula through undo creates a new target generation in the slice. An edit to a precedent advances invalidation versions without necessarily replacing the dependent's formula generation.

`origin_revision` explains where the original address came from. It is **not** a requirement that every later valid result have the current global revision number. Target continuity and certificates, rather than blanket revision equality, permit independent pure work to survive edits.

The evaluation ordinal changes for every new semantic evaluation of a locus, including a second evaluation in the same pass. Occurrence distinguishes repeated visits to a call site in arrays, recursion and LAMBDA execution. A continuation, thread, work slice, retry counter, template ID or block offset is not an invocation ID.

An admission frontier closes unrelated external inputs. It does **not** reject a later return that fulfils an async invocation already admitted by that command. Those returns are obligations of the command; an unrelated later RTD message is a new input event.

### 1.2 Reads and certificates

```rust
enum ReadPolicy {
    DependencyWithinCommand,
    CachedMacroObservation,
    RegisteredWorksheetCallback { registration: CallableIdentity },
    DiagnosticSnapshot,
}

enum ReadPurpose { Dependency, Observation, Diagnostic }
struct ReadRequest {
    purpose: ReadPurpose,
    policy: ReadPolicy,
    view: ReadView,                   // carries ObservationFrontier
}

enum DependencySource {
    AuthoredInput,
    Calculated { proof: ResultProofRef },
    BoundaryCache { view: ReadViewId, state: CachedState },
}

struct ReadSpan {
    subject: ReadSubject,             // cell/range/name/reference/fact facet
    facets: ObservableFacets,
    value_versions: VersionSpan,
    required: InvalidationVersionSpan,
    satisfied: InvalidationVersionSpan,
    source: DependencySource,
}

struct CachedObservation {
    view: ReadViewId,
    observed: ReadSpan,
    // Immutable observation evidence. Never a reverse dirty edge.
}

enum AbsenceBasis {
    Proven { occupancy: Version, producer_query: Version },
    Cached { view: ReadViewId, pending_outside_scope: ProducerSummary },
}
struct AbsenceProof { region: AreaSet, basis: AbsenceBasis }

struct ReadCertificate {
    dependencies: Vec<ReadSpan>,
    observations: Vec<CachedObservation>,
    absences: Vec<AbsenceProof>,
    binding_proofs: Vec<BindingProof>, // includes misses enabling a fallback
    facts: Vec<FactRead>,             // version + declared invalidation policy
    semantic_environment: SemanticEnvVersion,
    callable_versions: Vec<CallableVersion>,
}

enum ReadOutcome<T> {
    Value(T),
    Wait(WaitReason),                 // no worksheet result yet
    Refused(ReadRefusal),             // e.g. unavailable cache or capability
}
```

A missing formula cache is `MissingCache`, not an authored blank and not zero. `SliceV1` refuses a read requiring such a cache until an explicit permitted calculation supplies it. This is a typed engine limitation; it is not synthesized as an Excel worksheet error. The seven-cell fixture supplies all required boundary caches.

The recorder batches range reads into spans or versioned chunks. No SUM-sized allocation of one certificate entry per element is required. Certificate payloads may be interned behind a small handle; the logical fields above remain available to validation and diagnostics.

`ReadPurpose` is not a permission. The invocation frame authorizes the policy; a caller cannot label an arbitrary read “diagnostic” to avoid dependency tracking. Invalid purpose/policy pairs are refused. A registered callback's capabilities determine whether an uncalculated read is refused, deferred or permitted; it cannot silently obtain VBA's cached-read privileges.

### 1.3 Results, observable changes and publications

```rust
enum ComputationOrigin {
    Evaluated { root: InvocationId },
    ImportedCache { file: FileIdentity }, // unverified, no live command identity
}

struct FreshnessClaim {
    command: CalcCommandId,
    pass: CalcPassId,
    frontier: AdmissionFrontier,
    qualified_by: ReadQualifications, // boundary caches and cached observations
}

struct ResultCertification {
    fresh_for: FreshnessClaim,
    satisfies: InvalidationVersion,
    reads: Arc<ReadCertificate>,
}

struct ResultStamp {
    target: EvaluationTarget,
    computed_by: ComputationOrigin,
    certification: Option<ResultCertification>,
    completion: TargetCompletion,    // THIS target, not the whole command
}

struct ObservableChange {
    subject: ChangeSubject,
    facets: ObservableFacets,
    before: FacetVersions,
    after: FacetVersions,
    reason: ChangeReason,
}

struct PublicationUnit {
    id: PublicationUnitId,
    command: CalcCommandId,
    destination: ResultDestination,
    model: Revision,
    base: ReadViewId,
    next: ReadViewId,
    sequence: PublicationSeq,
    results: Vec<StampedResultPatch>,
    topology: TopologyDelta,          // dependencies + spill ownership/removals
    changes: Vec<ObservableChange>,
}

struct AdmissionReceipt {
    command: CalcCommandId,
    destination: ResultDestination,
    scope: CalcScope,
    admitted: AdmissionFrontier,
    predecessor: Option<CalcCommandId>,
}

struct CompletionReceipt {
    command: CalcCommandId,
    destination: ResultDestination,
    scope: CalcScope,
    admitted: AdmissionFrontier,
    outcome: CompletionOutcome,
    coverage: ScopeCoverage,         // calculated/reused/errors/refused/unfulfilled
    qualifications: ReadQualifications,
    results: ReadViewId,
    after_publication: PublicationSeq,
}
```

`TargetCompletion` distinguishes a value, a worksheet error and an engine refusal. A worksheet error can be a fully calculated result. An engine refusal cannot count as successful coverage. `CompletionOutcome` is `Complete`, `Incomplete(reason)`, `Cancelled`, `Superseded { by }`, `Failed(reason)` or `Shutdown`.

The immutable stamp describes what was established. Current live status is assessed against current target and invalidation requirements; do not store a permanent `fresh: bool`. Revalidation for a successor attaches a new certification while retaining the original `computed_by`. Importing an XLSX cache cannot fabricate a producing command or a certificate.

`certification` is present only for a validated calculated value/worksheet error; it is absent for imported unverified caches and engine refusals. Partial read logs for a refused evaluation remain diagnostics, not successful certificates. `QualifiedCellAddress` includes workbook incarnation, sheet instance and address. Workbook identity participates in invocation equality, hashing and draw keys.

Destination is immutable command authority. Live requirements are checked against the live model/result root; pinned-save requirements are checked against that save's retained roots. Private save installations do not enter the live subscriber stream or clear live requirements. Publication sequence/barriers are interpreted within their destination stream.

`ObservableFacets` covers at least value/type/error, formula text, formula kind/dialect, shape, reference identity, occupancy, binding, relevant row visibility, numeric semantics, host facts and calculation status. Pure numeric equality alone is insufficient. A status-only publication is required when a result becomes qualified for a command without changing its numeric value.

### 1.4 Authored transactions and external completions

```rust
struct PreparedTransaction {
    command: CommandId,
    base: Revision,
    preconditions: EditPreconditions,
    forward: ModelPatch,
    inverse_authored: ModelPatch,
    coordinates: AddressTransform,
    bindings: BindingDelta,
}

struct AsyncToken {
    invocation: InvocationId,
    target: EvaluationTarget,
    provider: ProviderInstanceId,
    provider_generation: ProviderGeneration,
    request: ProviderRequestId,
}

enum ExternalOperation {
    AsyncReturn { token: AsyncToken, result: ProviderResult },
    RtdUpdate { subscription: SubscriptionId, sequence: u64, value: Value },
    LinkRefresh { source: ExternalSourceId, revision: SourceRevision },
}
```

The engine stores the authoritative pending record; it does not trust identity fields copied from a callback payload. The adapter resolves a provider handle to that record. Async operation identity and permission to publish are separate checks.

### 1.5 Creation, consumption and lifetime

| Type | Creator → immediate consumers | Required lifetime; never confuse with |
|---|---|---|
| `CommandId` | UI/macro/host command source → host deduplication and journal | Retained with outcome through retry/recovery horizon; not a calculation pass |
| `CalcCommandId`, `CalcRequest` | Calculation-domain owner → engine, host waiters, trace | Admission through terminal receipt, then as provenance; not an edit revision |
| `ResultDestination` | Host at command admission → validator, receipts and serializer | Immutable through command/result lifetime; a private saved revision is not the current live model |
| `CalcPassId` | Engine semantic driver → evaluator, draw ledger, provider records | Through pass work and retained provenance; not an executor slice |
| `InvocationId` | Evaluator through engine-owned allocator → callable runtime, effects, random, async | Activation plus effect/return replay horizon; not a cell or function identity |
| `EvaluationTarget` | Engine from a pinned model → evaluator and result/async validator | Until all results/returns referring to it retire; not a current address lookup |
| `AdmissionFrontier` | Domain owner at admission → engine, receipt, save | Immutable for command lifetime and provenance; not global quiescence |
| `ObservationFrontier`, `ReadView` | Engine/view owner → readers, invocation frames, save | Pinned through activation/save/diagnostic use; not latest published UI tiles |
| `ReadRequest`, `ReadOutcome` | Authorized caller/reader → value reader/evaluator | One read or suspended read; waiting/refusal is not a worksheet value |
| `ReadCertificate` | Reader recorder plus binder/fact services → commit/reuse validator | Candidate, committed result and retained dependent proofs; not just a list of cell edges |
| `ResultStamp`, `FreshnessClaim` | Engine validator → cache, host, diagnostics, save | Result/provenance lifetime; local completion is not command completion |
| `ObservableChange` | Authored/result/fact commit owner → invalidation and projections | Until consumers process it; equal value is not absence of change |
| `PreparedTransaction` | Model command preparer → owner commit path and undo journal | One successful commit or rejection; inverse restores authored intent only |
| `PublicationUnit` | Engine committer → result store, transport, semantic projection | Until installed/acknowledged or replaced by a coherent reset; not a tile fragment |
| `AdmissionReceipt` | Host before execution → requester and ordered stream | Request/retry horizon; acknowledgement is not calculated coverage |
| `CompletionReceipt` | Owner after terminal obligations → waiters, UI, save | Immutable terminal record; not a subscriber acknowledgement |
| `AsyncToken` | Engine/provider adapter at launch → pending registry and return gate | Until return/revocation plus duplicate-detection horizon; not provider identity alone |

Pins are reference-counted immutable roots, not copies of a workbook. Compact proof records may outlive heavy snapshots. Retain a full view only while an invocation, save or diagnostic can still read it. IDs and effect tombstones are not reclaimed in a way that permits an old token to authenticate a new operation.

## 2. Admission, scope and semantic passes

### 2.1 Command normalization

The host resolves a command into `CalcRequest` before admission. The engine receives an authorized scope and evaluation policy; it does not infer demand from the viewport.

| Request in the slice | Authorized work | Boundary rule |
|---|---|---|
| Automatic edit | Dirty/volatile work required by the edit under the domain's mode; retain unfinished authorized work from a superseded calculation | Current authored inputs; calculated dependencies within scope |
| `CalcNow` | Dirty and volatile work in the requested calculation-domain scope | No waiting for future RTD/link events |
| Manual `CalcRange` | Force formula evaluations in the supplied `AreaSet`, resolving dependencies within that set | Formula precedents outside it use the pinned boundary cache |
| Manual `CalcSheet` | Dirty/volatile work on the selected sheet | Cross-sheet formula precedents may be cached and must be qualified |
| `Full` | Force all formulas in the resolved domain | Reuse of already-clean results cannot evade the new force demand |
| `RebuildFull` | Rebuild binding/dependency state, then force the resolved domain | Same read/effect/publication rules |

Only the Manual `CalcRange`, dirty calculation and explicit forced pure scopes are needed for the eight demonstrations. General iteration and data-table evaluation return bounded, named unsupported outcomes until their algorithms exist. They do not trigger an invisible whole-workbook fallback.

The admitted mode/scope does not change when interest or application settings later change. In `SliceV1`, a mode change affects future admission; stopping already-admitted work is a separate cancellation/supersession decision.

Microsoft documents that `Range.Calculate` resolves dependencies inside the range, and its recalculation account distinguishes Manual range calculation from Automatic-mode behaviour. Therefore the slice's Manual rule must not be generalized to every Excel mode without a probe. [S1, S2]

At admission:

1. At external command ingress, check the source command's deduplication record. Same ID and identical original payload returns the existing admission/outcome; same ID with a different payload is a protocol conflict.
2. Pin the model, external input view, calculation mode, semantic environment, callable table and compatibility profile.
3. Resolve the requested scope and its required formula instances. Explicit ranges retain their exact areas; no bounding-rectangle widening.
4. Register dirty/volatile demand and pending spill producers before any qualifying blank reads.
5. Enqueue `AdmissionReceipt`, then make work runnable. A command causing no work may subsequently complete without any value publication.

Ingress deduplication is not rerun against an internally generated successor as if it were another client submission. The owner allocates that successor a fresh `CalcCommandId` under the accepted source and recorded predecessor/cause. Retransmitting the original request returns its existing outcome/successor link; it creates no additional successor. Compare original client payloads, not normalized requests whose resolved revision may differ.

Structural scope resolution is revision-qualified. If a later edit moves an explicit range, the successor uses the recorded address transform and declared range-target policy; deletion can produce a typed target-gone outcome. The slice may reject unsupported structural rebinding. It must never interpret the original coordinates against a different model without a decision.

### 2.2 Command state machine

| State/event | Required transition and action |
|---|---|
| Submitted, validation fails | `Rejected`; no calculation admission or effects |
| Submitted, validation succeeds | `Admitted → Active`; emit admission before execution |
| Active, budget exhausted | `Yielded`; retain all resumable state and identities |
| Yielded, worker task resumes | `Active`; no new pass or invocation merely because time elapsed |
| Active, only dependencies/provider results remain | `Waiting(reason)`; no busy retry loop |
| Waiting, required input/return becomes available | `Active`; resume or open a specified semantic pass |
| All required work terminal and installed | `Complete` if no refusals/unfulfilled targets; otherwise `Incomplete` |
| Live authored edit supersedes a live command | Revoke current-publication authority; `Superseded { by: K1 }`; admit successor at its own frontier; a pinned-save command is not retargeted |
| User cancel | Stop new effect admission and publication; request cooperative stop; terminal `Cancelled` after owner has captured disposition |
| Shutdown | Stop admission, revoke live result authority, record pending effects, enter resource draining; terminal `Shutdown` |
| Unhandled engine failure | `Failed`; do not convert it into a worksheet error |

Resource draining can outlive a command's terminal receipt. The receipt describes calculation authority and coverage, not a false guarantee that a foreign call has stopped. In a serial non-preemptible call, the owner cannot process a queued cancellation until control returns; trace the request time separately from its effective processing time.

Terminal receipts are immutable and emitted once. Transport retransmission returns the same receipt. A command with `Superseded` or `Cancelled` outcome is not subsequently upgraded to `Complete` because an old worker returned.

### 2.3 Pass state machine

A pass has one fixed input/model view and an explicit reason: `Initial`, `AsyncDependents`, `DependencyRepair`, `SpillShapeCheck`, or another measured semantic rule. A successor after an edit starts its own `Initial` pass at a new frontier. The pass ID includes the new command ID, so volatile evaluation identities cannot accidentally survive the edit.

```rust
enum PassState {
    Ready,
    Running,
    Yielded(Checkpoint),
    Waiting(WaitSet),
    Complete,
    Superseded,
    Cancelled,
    Shutdown,
    Failed(EngineFailure),
}
```

`Running ↔ Yielded` preserves the pass. `Running ↔ Waiting` preserves existing activations. Completing a pass does not imply that the command has no further passes. New pass creation records its reason and affected targets; it does not reevaluate the entire workbook by default.

For the async fixture, the initial pass evaluates other ready work and waits for its admitted async obligations. A valid return resumes the original formula continuation using its original invocation IDs. After that formula is installed, an `AsyncDependents` pass evaluates the affected dependents. A later optimization may batch multiple returns; it must retain this distinction between continuation and new dependent evaluations. Microsoft describes saved formula state and subsequent dependent passes for asynchronous UDFs. [S2]

Precisely, the serial fixture marks not-yet-started dependents of pending async results `DeferredTo(AsyncDependents)` in the command work ledger. The initial pass closes once its own formulas/async continuations are terminal; deferred targets remain **unfulfilled command work**, not completed targets. The next pass then owns those targets. With multiple initial async calls, the slice waits for all of that pass's admitted obligations before opening the dependent pass. A dependent activation that already started an effect must instead resume its existing continuation in its owning pass. No pass waits for a target that it has delegated to a pass which cannot yet start.

Additional passes are semantic events, not a repair mechanism for arbitrary failed commits. A pure candidate invalidated by changed admitted inputs is re-evaluated under the new authorized evaluation. A temporary physical retry against the same view and semantic evaluation reuses its draw ledger.

### 2.4 Yield, supersede, cancel and shutdown are different operations

**Yield** changes execution ownership temporarily. It must preserve program counter, locals, array/scan cursor, evaluation stack, pending callbacks, read recorder and nondeterministic inputs. It does not revoke result authority or repeat function entry.

**Supersede** retires a calculation promise because a named successor now owns live demand. It can salvage eligible pure candidates after validation. It does not rewind effects or mutate the retired command's frontier.

**Cancel** ends the user's requested work. Already installed values remain as qualified historical caches; unfinished targets remain pending/dirty with cancellation status. There is no automatic successor solely because the user cancelled.

**Shutdown** prevents further admission and releases/drains resources under provider policy. A trusted native call without a stop mechanism cannot be declared stopped by setting a flag. Crash recovery marks a started effect with unknown outcome `Indeterminate`; it never automatically replays it.

## 3. Reads, freshness and commit validation

### 3.1 Versions express both value and outstanding work

For each relevant cell/facet maintain a value version and calculation requirement:

```rust
struct RequirementState {
    value_version: ValueVersion,
    required: InvalidationVersion,
    satisfied: InvalidationVersion,
}
// Locally clean iff satisfied == required.
// Locally clean does not remove scoped/cached-read qualifications.
```

Every new semantic invalidation advances `required`, even if the cell was already dirty. A result can satisfy only the version it validated. A publication for requirement N must not clear N+1.

The old cached value version may remain unchanged while `required` advances. This is why checking only numeric equality or a value version is unsound. Updating a source from pending to calculated can also change a dependency's validity/status facet even when its number is unchanged.

### 3.2 Four read contracts

| Read | Value selected | Validation and invalidation behaviour |
|---|---|---|
| In-scope formula dependency | Current authored input, or result certified for the required command/pass work | Validate value, requirement, satisfaction and relevant facets; install reverse dependency |
| Out-of-scope formula dependency | Boundary cache pinned when the pass begins | Validate the selected historical entry and carry its qualification; retain a reverse dependency for later source changes |
| Cached VBA observation | Cache/model view authorized at the invocation's observation fence | Validate against that immutable view; record evidence; no reverse dependency, no demand calculation |
| Diagnostic snapshot | Requested pinned snapshot | No calculation demand, effects or dirty edges |

Constants are read from the pinned authored model even when outside a formula calculation range. An out-of-scope **formula** is different: its cache is not silently recalculated. Reference resolution, returned reference identity and value dereferencing each record the facets actually consulted.

`RegisteredWorksheetCallback` is a registration-checked route. In the slice, a normal worksheet callback asking for an uncalculated cell receives a typed unavailable/defer response before unsupported work begins; VBA's object-model `Value2` read uses `CachedMacroObservation`. Do not coerce either into the other. Microsoft explicitly distinguishes ordinary worksheet functions from VBA/macro-equivalent functions with access to uncalculated cells. [S3]

Boundary cache and VBA qualifications propagate through consuming formula results. Use shared proof references rather than flattening a growing transitive list into every result. A consumer of a scoped result cannot upgrade it to a stronger freshness claim by performing more arithmetic.

### 3.3 Definition of “fresh for command K”

For target T, `FreshFor(K, T)` means all of the following hold:

1. T is covered by K's resolved scope and still has the authorized target identity for the certification.
2. K's evaluation demand for T has been satisfied: an explicit forced evaluation occurred, or dirty work was evaluated/reused as allowed. A reusable cache cannot silently satisfy a newly requested forced evaluation.
3. Every dependency/read obeyed K's selected read policy and view; the certificate validates under that policy.
4. All relevant in-scope occupancy questions are resolved; no strict blank proof relies on an unresolved admitted producer.
5. The result and required topology/status changes were installed atomically.
6. The claim records K, the pass, the input frontier and all cached/observation qualifications.

This definition permits **qualified command freshness**. It does not assert that every reachable formula was recalculated, that an internally observed cache was new, or that later events have not made the live projection dirty.

UI/API wording should distinguish, for example:

- `Calculated for K7; cached input B1 was dirty`.
- `Calculated for K8; VBA observed B1 in view V12`.
- `Current for K9 at revision R4; other command work remains`.
- `Cached result; waiting for spill E1`.

The internal status can be compact enums and IDs. The explanation is available on demand; it need not fill every grid cell.

### 3.4 Validation and installation

The single owner executes validation plus installation without an intervening authored/input commit:

```rust
fn try_install(candidate: Candidate, destination: ResultDestination) -> InstallOutcome {
    check_command_and_target_authority(&candidate, destination)?;
    check_semantic_environment_and_callable_versions(&candidate)?;
    validate_dependency_reads_against_required_view(&candidate.reads)?;
    validate_cached_reads_against_their_authorized_pins(&candidate.reads)?;
    validate_binding_facts_and_absence_queries(&candidate.reads)?;
    resolve_or_reject_spill_reservation(&candidate)?;
    check_requirement_version_still_matches(&candidate)?;
    install_value_status_topology_and_invalidations_atomically(candidate)
}
```

Within a unit, dependent candidates may reference an included producer proof; otherwise their producer must already be installed in the result store. A subscriber need not receive an off-screen B1 tile before C1, but the authoritative result store must already contain B1's certified result or install it in the same dependency-closed unit.

Failed validation does not publish a partly installed result. A pure attempt can be discarded or re-evaluated under the applicable command/pass. An effectful attempt cannot be restarted by this failure path.

For a previously published result, a change service must also trigger invalidation. Merely recording fact versions at evaluation time is insufficient. Each fact declares `OnChange`, `OnNextAuthorizedCalculation`, or `ObservationOnly` policy, and commit uses that policy. Local repaint alone creates no calculation demand. Precision-as-displayed and other numeric semantic changes are not local repaint.

`SliceV1` has no value-based early cutoff: all admitted dirty/forced work must be discharged even when a predecessor's number stays equal. Later facet-aware cutoff is permitted only in proven effect-free cones, while preserving force obligations and all observed facets. Reusing E1's already-computed valid candidate after an unrelated A1 edit is not permission to skip a downstream invocation because two numbers compare equal.

### 3.5 Early publication and receipts

For each ordered subscriber stream:

```text
AdmissionReceipt(K)
    precedes each PublicationUnit caused by K;
all required installed units / status transitions
    precede successful CompletionReceipt(K).
```

The completion receipt contains a publication barrier. A mirror exposes the receipt as applied only after installing that barrier or a replacement snapshot with equivalent coverage. The engine does not wait for every slow subscriber before declaring its own command terminal.

C1 can publish once B1 is certified and C1's unit validates, while unrelated E1 work remains. Its result says `FreshFor(K, C1)`; the command remains active. A status change from old `4` to newly calculated `4` still matters and must not be suppressed by value-only coalescing.

A multi-cell spill is one publication unit, including removals. Transport fragments are assembled before any renderer, hit-test or accessibility projection sees the new unit. The first slice can send one complete message per unit; it does not need the future tile protocol to prove atomicity.

## 4. Cached VBA reads constrain scheduling

### 4.1 The selected first-slice policy

`SliceV1` uses **serial observation fences**. The reference order is fixed from the command's dependency-compatible ordering before interest changes. Pure ready work may be reprioritized inside a fence segment. An unrestricted callback is a fence: pure-result installation from a later segment cannot cross it, and an earlier segment's required installation cannot be delayed past it merely because another cell is visible.

For an otherwise unspecified ordering tie, the slice uses saved reference-chain rank and then workbook/sheet/address order. This is a deterministic DNA reference policy, not an assertion about Excel's chain. Dependencies, pending spill barriers and effect ordering override interest priority.

At callback admission, pin the entire authorized immutable model/result root. The callback uses that view across a supported suspension, including cells not read until after resumption. Do not snapshot only its argument references or already-discovered reads: its readable set is unknown in advance.

This requires a retained root, not a speculative multi-version execution engine. The slice does **not** run later pure segments ahead and expose them selectively. A future logical observation-view implementation may allow such computation, but it must preserve the same visible frontier.

For the scheduling demonstrations, use the pure scope `{B1,C1,E1,G10}`. D1 is still in the workbook and retains its cache; it is not invoked by those commands. The separate callback demonstration proves the fence. This prevents a purported pure scheduling test from accidentally changing the semantics of `PeekB1()`.

### 4.2 Exact `PeekB1` behaviour in the fixture

After the baseline calculation, Manual mode and `A1 := 10`:

- B1's authored formula is unchanged, its cache is 2, and it is dirty.
- Calculating D1 alone invokes `PeekB1` with a cached read view containing B1 = 2; D1 becomes 2.
- That read neither evaluates B1 nor installs a hidden D1→B1 dependency.
- Calculating B1 explicitly produces 11. That cache change alone does not dirty D1 because of its previous hidden read.
- A subsequent explicit D1 calculation observes 11.

The pack reports the dirty non-argument cache observation. Microsoft independently documents that internally used cells omitted from a UDF's explicit dependencies can leave its result unchanged. Neither establishes a universal command-entry snapshot or every ordering detail. [P, S3, S4]

### 4.3 Invocation-aware callbacks

```rust
struct InvocationFrame {
    id: InvocationId,
    parent: Option<InvocationId>,
    target: EvaluationTarget,
    view: ReadView,
    lease: CallableLease,             // code AND project-state generation
    mode: InvocationMode,
    reentry: ReentryPolicy,
}
```

A typed `Range.Value2` callback executes through this frame. It cannot enqueue behind the engine activation waiting for it. `WorksheetFunction` evaluates its supplied arguments with its authorized context; it does not implicitly issue a worksheet calculation command. Worksheet-context writes and unauthorized nested commands are refused before effects begin.

The code and state lease survive suspension together. Reload does not pair an old instruction pointer with newly reset globals. In the slice, reload revokes affected result authority, retains old leases only while the old activation drains, and binds new invocations to the new generation. Broader reload compatibility remains a probe.

The project-session effect lane is shared across live and pinned-save commands. A started observation-capable or irreversible activation retains it through suspension/async wait until completion or definitive stop. No independent activation accessing that same mutable state may enter meanwhile. Entry follows recorded semantic order, with command admission order resolving cross-command ties. Permitted nested callbacks remain in the invocation lineage and follow its reentry policy; an impermissible reentry is refused instead of waiting behind itself. Live edits can still commit, and eligible pure work follows the existing fences. Stateless async providers do not acquire a VBA project-state lane merely by being asynchronous.

Microsoft documents `Value2` as a value getter/setter and distinguishes its data conversions from `Value`; its documentation provides no calculate-and-drain guarantee. The getter in this contract is therefore a read, while calculation is a separate command. [S5]

## 5. Effectful invocations, nondeterminism and async returns

### 5.1 An invocation has an execution state and a result disposition

```rust
enum EffectExecution {
    Allocated,
    Admitted,
    Running,
    Suspended(Activation),
    WaitingExternal(AsyncToken),
    Completed(InvocationOutcome),
    Stopped { effects_so_far: EffectSummary },
    Indeterminate,
}
enum ResultDisposition {
    Pending,
    Installed(PublicationUnitId),
    Discarded(DiscardReason),
}
```

The legal execution path is allocation → admission → running → completed, with running/suspended and running/waiting-external transitions preserving identity and activation. No transition goes back from running or suspended to “call the entry point again”.

Before admission, the engine must establish target authority, argument readiness, allowed observation fence, callable capabilities and the required code/state leases. It records admission/started state before entering irreversible code. Only the owner can consume the one-shot admission permit.

An engine restart/retry cannot consume it twice. A second delivery while running returns the existing state. A retry after completion uses the recorded outcome only if that is valid for the original invocation; it does not execute again. A return from an already-started invocation can be discarded without erasing its effect record.

For durable recovery, the started record must reach the recoverable journal before external entry. A crash between that record and the completed record leaves an uncertain outcome. The contract promises **at-most-once engine entry for that invocation identity**, not exactly-once external effects or guaranteed effect completion. Provider idempotency would be an additional protocol.

If a late dependency appears after an effect, suspend and resume the existing activation if supported. If the runtime cannot preserve it, return a typed incomplete outcome such as `ContinuationUnavailable`; do not restart the function to reconstruct its stack. A non-suspendable native function executes as its declared non-preemptible interval, with a reason and duration counter. A cancellation request during that interval does not authorize another entry or undo its earlier effects.

A legitimate new semantic evaluation may call the function again. Its reason must be a new requested calculation, a relevant authored/input change, or a measured semantic pass rule. A validation failure, viewport change, budget exhaustion or a newly minted ID is not such a reason by itself. Microsoft documents repeated formula evaluation during recalculation; the exact effect counts for particular cases remain empirical. [S1]

### 5.2 Recorded nondeterministic inputs

Keep the selected random generator. Specify its use through a draw ledger:

```rust
DrawKey = (InvocationId, draw_ordinal);
DrawRecord = (DrawKey, generator_version, recorded_bits_or_replay_material);
```

Physical retries of the same semantic evaluation reuse its keys and recorded values. Separate executed RAND sites have separate invocation identities; LET binds one RAND result and reuses the value. Repeated visits to one site get distinct occurrences. A genuinely new evaluation or pass gets new keys.

Do not test that two random values must be numerically different; they can coincide. Test distinct keys/draw events. Record clock/external observations under their own facilities rather than pretending all nondeterminism is random.

The first slice does not implement a guessed Excel volatile-spill retry count. It supports new pass identities and a scripted draw fixture; a declared bounded fallback/refusal applies until the measured policy is selected. Freezing a draw across all semantic shape passes is forbidden: Microsoft documents indeterminate-size spills when volatile shapes fail to stabilize across passes. [S6]

### 5.3 Async acceptance state machine

An async operation has states `Pending`, `Consumed`, and `Revoked`. Keep the terminal tombstone needed to recognize duplicates. On receipt, the owner makes one atomic decision:

1. Resolve the adapter's opaque handle to the authoritative pending token; reject unknown handles.
2. Check exact invocation and provider request identity, provider instance/generation, workbook incarnation, target generation/locus and active result authority.
3. Check the formula activation's required invalidation version and its captured argument/dependency certificate. Input edits can invalidate an invocation without replacing its formula.
4. If valid, change `Pending → Consumed`, record the result once, and resume the existing continuation. Any further read, computation and publication still undergo ordinary validation.
5. Otherwise record `Rejected(reason)` and leave the authored model, result cache and activation unchanged. Revocation/cleanup metadata may advance; no new formula evaluation is launched by the rejection.

The first accepted return consumes the token even if later continuation validation fails. It never becomes pending again. Identical duplicate returns are ignored with a recorded duplicate disposition; conflicting duplicates are protocol violations. Neither resumes twice.

For live-destination commands, the slice revokes outstanding return authority on overwrite, deletion, movement, undo restoration, workbook close/reopen, provider reload, command cancellation or supersession. It does not transfer async operations through structural edits. A pinned-save invocation validates against its saved target/requirement roots: an unrelated later live edit does not rewrite those roots or revoke that private authority. Save cancellation, provider retirement, actual workbook-session closure and shutdown still revoke it. Origin-revision equality alone is not the general validation rule: pure candidates can be revalidated across revisions, while the slice deliberately uses a conservative live-command lifetime rule for outstanding async work.

An accepted async return completes the stored call. It does not restart the formula from its first operator, nor repeat an effect before the async call. RTD updates and link refreshes update input snapshots and authorize subsequent calculation according to mode; they are not returns for pending call handles. This matches the distinction made by Microsoft's handle-based async interface. [S7]

## 6. Authored edits, supersession, reuse and undo

### 6.1 Atomic authored commit

`PreparedTransaction` is prepared against one base revision. In the first slice a stale base is rejected for re-preparation; there is no silent last-writer-wins patch application.

The owner performs the following as one visibility transition:

1. Check command deduplication, base revision, target/protection preconditions and prepared patch consistency.
2. Apply authored membership, inputs, formulas/dialects, references, names, styles, merges, rules and geometry required by that operation.
3. Allocate fresh target generations for replaced/moved/deleted/restored formula instances; update bindings and address transforms.
4. Advance direct and transitive calculation requirements, fact/binding facets, and pending spill reach/absence-query versions.
5. Publish the new authored revision and its invalidation state together; retain the authored inverse for undo.

No reader may see F1 = 10 paired with the old conclusion that no pending producer can occupy E10. The invalidation/spill metadata transition belongs to the edit, before the next calculation slice.

### 6.2 What an intervening edit does to calculation

For an active live calculation K0, the slice handles an authored edit as follows:

1. Commit the edit atomically.
2. Revoke K0's live publication authority and issue `Superseded { by: K1 }` when a successor is authorized.
3. Admit K1 with a new model/input frontier, predecessor K0, and recorded cause. Its first pass has a new identity.
4. Revalidate K0's completed pure candidates against K1's target identities, required facets, input view, callable/environment versions and occupancy. Carry only those passing every check.
5. Discard conflicting candidates and evaluate the outstanding work. Preserve `computed_by` for carried results; record the new certification.

Automatic mode supplies the successor demand. For an interrupted explicit Manual calculation, a successor is a continuation of that **same authorized scope**, with its transformed target if supported. It cannot widen C1-only calculation into a workbook calculation. The original waiter receives the superseded receipt and successor link; a higher-level “wait for this request” helper may follow the link explicitly. Cancellation creates no successor.

An unrelated edit need not change a candidate's formula generation or reads. Conversely, an unchanged formula generation does not prove its inputs are valid. Both sides are exercised by demonstration 4.

Reuse is for pure, deterministic work under matching semantic inputs. Do not carry volatile draws or completed irreversible executions into a new semantic evaluation merely because the scalar result happens to match. New independent `ForceAll` requests still require new semantic evaluations. The edit-successor demo uses dirty calculation, avoiding ambiguity about whether an inherited candidate satisfies a newly requested force.

The conservative forced-successor rule is explicit: `SliceV1` salvages predecessor candidates for dirty successors only. An inherited `ForceAll` successor freshly evaluates its pure targets at the new frontier. An effect obligation that had not started may be admitted once by that successor; one already started in its predecessor is not automatically reissued to discharge the same original force request. If its usable continuation/result cannot be retained under the slice's authority rules, report `Incomplete(ExplicitReevaluationRequired)` for that obligation. A later independently requested calculation or a genuinely new relevant automatic demand may authorize a new semantic invocation; the inherited force request alone does not.

For the slice, started effectful/async activations are not transferred into the successor's result authority. They are stopped cooperatively or drained, with effects retained and late results rejected. A successor may execute an effect again only where its **new calculation demand actually requires a new semantic evaluation**. If the only reason would be recovery of discarded state, report incomplete work instead. Reusing admitted effects across unrelated edits is an optimization outside this slice.

### 6.3 Undo

Undo prepares an authored inverse against the current revision and commits a **new** revision. Restored formula instances receive fresh generations. Reappearance of identical text at an identical address does not resurrect an old async target or invocation.

Undo restores authored intent, not clock samples, random ledgers, RTD delivery history, module counters, provider requests or irreversible effects. Derived values may be reused only after target-appropriate validation; otherwise retain a qualified stale cache or recalculate according to mode. A conflicting inverse is rejected/reprepared, not blindly applied to an unrelated base.

## 7. Spills, blank reads and occupancy

### 7.1 A strict blank proof

A read is `ProvenAbsent` only if, at its authorized frontier:

1. No authored occupant exists.
2. No installed spill owns the coordinate.
3. No unresolved admitted producer may occupy it.

The proof records both occupancy version and the version of the potential-producer query, including negative results. Register absence watchers so that creating, dirtying, moving or removing a relevant producer invalidates previously published blank readers. Merely discovering a new edge after the spill has installed is too late.

For a dirty dynamic-array anchor, its previous footprint is not a bound on its future footprint. In the first slice a conservative query may scan the sheet's pending anchors. Unknown shapes use a sound southeast reach from the anchor to the grid edge; an independently validated shape/width bound may narrow it. `SEQUENCE(F1)` has a one-column footprint in the fixture, but its prior height of one cannot exclude E10 after F1 changes.

The scan is explicit, bounded by admitted anchors and counted (`anchors_considered`, `readers_blocked`, `reason`). It must not be described as selective or constant-time before measurement. No bitmap spanning an entire sheet is required.

### 7.2 Strict reads versus deliberately cached blanks

If both E1 and G10 are in scope after `F1 := 10`, G10 must wait for E1's occupancy determination. Its old cache of 1 may remain visible as pending. It cannot publish 1 as current using a strict absence proof.

If a Manual command calculates **only G10**, E1 is outside scope. G10 may read the pinned cached blank and return 1 with `CachedAbsent` plus the out-of-scope pending-producer qualification. It must neither widen scope by calculating E1 nor wait forever for unauthorized work. That weaker claim is not a proof that E10 is currently empty.

Cached VBA and diagnostic reads use their own pinned observation rules; they can also observe a cached blank without asserting current occupancy absence.

### 7.3 Atomic footprint replacement

One spill unit installs the anchor outcome, shape, complete member values, ownership, removals, obstruction status, relevant dependency changes, and resulting reader invalidations. On growth from one to ten rows, E10 becomes 10 before G10's strict read can succeed. On shrink back to one, E2:E10 disappear atomically and G10 must be invalidated before it can become 1 under a new absence proof.

Authored content, merges, tables and grid boundaries are checked against the pinned model and again at commit. In the slice, an occupied destination causes the supported `#SPILL!` obstruction result; an unsupported obstruction category yields a named refusal, not a partial array. Old spill children owned by the same anchor are not treated as foreign authored obstructions.

For competing anchors, preserve the pack's first-established policy provisionally, but define “established” by **semantic reservation order**, not physical completion time or interest priority. Existing valid ownership precedes new competing reservations; equal-stage new reservations use fixed reference order. Priorities cannot choose the winner. Multi-anchor Excel tie behaviour is still a probe; the main fixture has one anchor. [P]

## 8. Save is a pinned operation

```rust
enum CachePolicy {
    PreservePermittedCaches,
    RequireCalculatedScope(CalcScope),
}

struct SaveRequest {
    id: CommandId,
    workbook: WorkbookInstanceId,
    model: ModelPin,
    base_results: ReadViewId,
    inputs: InputViewId,
    cache_policy: CachePolicy,
    format: SaveFormat,
}

struct SaveImage {
    model: ModelPin,
    results: ResultViewPin,           // final immutable view used by serializer
    cache_manifest: CacheManifest,    // internal; not a new required XLSX part
}
```

`SaveImage` is the pair the codec consumes. The serializer never walks a mutable “latest results” map. After pinning, later edits/publications cannot change either selected image.

### 8.1 CalculateBeforeSave = false

In Manual mode, resolve to `PreservePermittedCaches`. Atomically select the authored model and compatible committed result root. Saving adds **no calculation demand**, no callback invocation and no drain requirement.

Permitted caches belong to the same formula instance/semantics in the pinned authored model. Their dependencies may be stale, which the cache manifest and recalculation metadata must state. A result for an overwritten formula cannot be attached solely because the address matches. Do not serialize uncommitted candidates.

Spill caches are preserved as a complete coherent old footprint only when structurally compatible with the pinned model. If an edit creates an obstruction in an old footprint, omit the incompatible derived footprint/cache as a unit and mark it unverified/dirty; never overwrite authored content to preserve a spill cache.

### 8.2 CalculateBeforeSave = true

In Manual mode, obtain a calculation satisfying the resolved save scope for the **pinned authored model** and fixed inputs. For the one-workbook slice the save scope is that workbook's normal dirty/volatile calculation demand. Exact cross-workbook and data-table scope is a compatibility measurement, not a reason to guess a broader rule.

The initial result view is only the calculation's base. After the required command completes successfully, pin its **final** result view and serialize that view with the original model pin. Later live edits must not redirect the save calculation or replace the saved revision.

If the live model advances, a calculation admitted with `PinnedSave` destination continues against that saved revision. Its historical results do not clear current live requirements. Completed matching results and pure candidates may be reused if their scope, model, inputs and demand validate.

An in-flight live irreversible invocation is not silently copied into, or jointly owned by, a save. If the save requires that same already-running semantic work, the slice waits for the live command and then pins its matching completed view. If that command is superseded/cancelled or fails to cover the save requirements, the waiting save returns `Incomplete`; it does not repeat the effect. When no such live obligation exists, the host admits a dedicated pinned-save calculation. This is a concrete ownership choice; more elaborate shared activation ownership is deferred.

Pending async work keeps a required save calculation waiting. Cancellation, refusal or failure produces an explicit save outcome; it does not silently downgrade to `PreservePermittedCaches`. A current-looking cache based on scoped or VBA observations retains its qualification even after the save calculation completes.

Microsoft documents CalculateBeforeSave specifically as controlling calculation before save in Manual mode. The concurrent pinning rules here are DNA Calc's proposed operation contract. [S8]

### 8.3 File encoding and reopen

XLSX persists formula text and last-calculated caches separately. It does not transport this engine's live invocation IDs, read certificates, effect ledger or resumable activations. [S9]

The slice's codec must:

- Preserve formula text/dialect, authored values, compatible complete spill caches and declared calculation settings.
- Encode Manual mode and the chosen `calcOnSave` setting; mark incomplete calculation honestly, using `calcCompleted=false` and applicable per-formula `ca` flags for dirty/unverified formulas.
- Avoid carrying a stale calculation chain; rebuild or omit it under the codec's existing package policy.
- Keep the exact `fullCalcOnLoad`/`forceFullCalc` compatibility encoding behind the measured file policy. Do not claim a particular Excel reopen result solely from flag names.
- Import serialized formula caches as `UnverifiedFromFile` unless a separately defined trusted native snapshot contains valid certificates. Mark them for conservative later calculation; do not run UDFs merely to display imported caches in the controlled Manual reopen test.

The listed calculation and formula attributes exist in SpreadsheetML, but the target Excel build's interaction among them must be tested. [S10, S11] The first save oracle checks bytes **before reopening**, then performs a controlled DNA Calc Manual reopen. A separate Excel interoperability probe observes any recalculation on open. Workbook mode can interact with application mode, so control the Excel process and open order.

For the acceptance fixture, `fullCalcOnLoad` and `forceFullCalc` start absent and remain absent in Save(false); the writer does not introduce them to force an open-time calculation. The byte oracle explicitly checks Manual mode, `calcOnSave=false`, `calcCompleted=false`, and dirty formula `ca` flags. The broader import/export policy for pre-existing flags is part of the Excel save probe.

The save test uses XLSX. The pack identifies current XLSB saving as conversion to XLSX; this contract does not invent a same-format XLSB writer. [P]

## 9. Scheduling and the finite-work promise

The serial stepping entry point accepts a logical work budget and returns `Progress`, `Yield(checkpoint)`, `Waiting(reason)` or `Terminal`. Budget checks must exist inside supported long kernels and VM/JIT execution, not only between cells.

For eligible pure work within one fence segment, `SliceV1` uses interest priority with an age escape: after seven interest-preferred resumable quanta, service the oldest ready off-interest continuation on the eighth. Count readiness age in scheduler service decisions, not wall-clock time; break ties by the fixed reference order. Promoting a visible target promotes its authorized prerequisite cone, not additional scope.

With finite admitted work, finite supported quanta and eventual provider completion, this guarantees eventual service. It makes no false termination guarantee for an infinite formula or a non-cooperative foreign call. Report both logical budget use and longest non-yielding interval; declared non-preemptible calls have separate counters.

Exact kernels yield only at their existing safe store boundaries or with the complete required floating-point state preserved. A yield must not add a binary64 rounding point or change operation order. That requirement follows the supplied numeric contract; the first arithmetic/SEQUENCE fixture does not by itself establish it for every x87 kernel. [P]

The browser driver must return control to an actual worker task before scheduling the next slice. An unbounded chain of resolved promises is insufficient because microtasks are drained separately from ordinary task selection. The test must observe another worker message/task being handled between slices. [S12]

## 10. The eight acceptance demonstrations

### 10.0 Common fixture and trace vocabulary

Each demonstration starts from an independent clone. Setup is in Manual mode. Register `PeekB1` as nonvolatile, stateful/observation-capable, with a real typed cached callback. Enter E1 with dynamic-array semantics.

| Cell | Authored content | Verified baseline cache |
|---|---|---:|
| A1 | `1` | 1 |
| B1 | `=A1+1` | 2 |
| C1 | `=B1*2` | 4 |
| D1 | `=PeekB1()` | 2 |
| F1 | `1` | 1 |
| E1 | `=SEQUENCE(F1)` | 1, footprint E1 only |
| G10 | `=E10+1` | 1 |

E2:E10 have neither authored occupants nor spill ownership. Establish baseline by explicitly calculating B1 before C1/D1 and E1 before G10, or by loading a trusted harness snapshot with those certificates. Ordinary unverified file caches are not sufficient to assert a verified baseline. Clear fixture logs after setup.

The pure scope `S = {B1,C1,E1,G10}` is a precise `AreaSet`, not a bounding range. In Manual tests, `CalcRange(S)` forces these four formula targets. D1 is outside scope and remains cached at 2.

Use structured trace events with command/pass/invocation, target generation, model/input/view IDs, versions and reason fields as appropriate:

```text
Admit, PassStart, Read, Candidate, Validate, Reject, Reuse,
Yield, WorkerTask, Resume, CommitEdit, Invalidate,
EffectStart, EffectComplete, AwaitAsync, ReturnAccepted, ReturnRejected,
Install/Publish, Complete, SavePin, SaveImage, SaveWritten.
```

`X < Y` below means a required happens-before edge. Independent events may interleave. Compare partial orders and forbidden events, not one incidental scheduler log. Where a fence or reservation policy fixes order, assert that order explicitly.

### Demonstration 1 — A finite slice returns to the real worker loop

**Input/operations.** From baseline, atomically set A1 = 2 and F1 = 10. Admit `CalcRange(S)` through the existing UI/worker route. Use a deterministic small budget and a supported SEQUENCE checkpoint after producing three of its ten private elements. Arrange for a Ping message to be queued before the next resume task, using the real driver/task path.

**Required trace.**

```text
Admit(K) < PassStart(P)
Yield(E_activation, cursor=3) < WorkerTask(Ping) < Resume(E_activation, cursor=3)
Install(E1:E10) < Complete(K)
```

The invocation, pass, continuation, locals and read view are unchanged on resume. The Ping handler must execute in the worker, not merely receive a message on the page while the worker remains busy. A trace-only Yield or a resolved promise is a failure. Do not add a test-only unwind path absent from the production driver.

**Final values.** B1 = 3, C1 = 6, E1:E10 = 1…10, G10 = 11, D1 = 2. Before the spill unit installs, no subscriber sees its first three private elements as committed cells.

**Invariant.** Budget suspension is resumable computation plus an actual task handoff, without partial publication or new semantic evaluation.

### Demonstration 2 — Interest changes order, not demand

**Input/operations.** From baseline, set A1 = 2 and F1 = 2. Admit the same S. Initially prioritize E1; suspend E after one private element. Handle an interest update making C1 visible, then continue. Compare with a run retaining E1 priority.

**Required trace.** In the changed-interest run:

```text
Yield(E, cursor=1) < WorkerTask(Interest(C1))
Interest(C1) < Install(B1=3) < Publish(C1=6) < Resume(E)
```

At this point the age escape must not yet be due; reset/record the service counter in fixture setup and provide enough budget for B/C to finish in fewer than seven service decisions. Assert that count. The reference run may resume E before B/C. Both runs use the same admitted scope and semantic input frontier. No invocation of D1 is permitted. E keeps its continuation and eventually finishes.

**Final values in both runs.** B1 = 3, C1 = 6, E1 = 1, E2 = 2, E3:E10 absent, G10 = 1, D1 = 2. Effect trace is empty after setup. Value/shape/provenance semantics agree; pure publication order may differ.

**Invariant.** Interest reprioritizes eligible pure work and its authorized prerequisites. It cannot widen/drop scope, cross observation fences, choose a spill winner or starve retained work.

### Demonstration 3 — C1 publishes while unrelated work remains

**Input/operations.** From baseline, set A1 = 2 and F1 = 2. Admit S and prioritize C1. Leave E1 pending until B/C have installed.

**Required trace.**

```text
AdmissionReceipt(K) < Install(B1=3) < Publish(C1=6)
Publish(C1=6) < Install(E1:E2) < CompletionReceipt(K, Complete)
```

At C1's publication, E1 is demonstrably unfinished and K is still active. The C1 stamp names K/P and validates B1's installed proof. The final receipt covers all S and carries its publication barrier. The subscriber applies that barrier before exposing completion.

**Final values.** The same as demonstration 2. D1 remains 2. A separate replay of `CalcRange(C1)` with already-equal numeric output must still emit the command-specific status/certification change.

**Invariant.** Local certified publication is independent of command completion. Admission precedes caused values; completion does not precede its required values.

### Demonstration 4 — An edit rejects conflicting work and reuses independent work

**Input/operations.** Start from baseline, switch to Automatic with no pending work, then commit A1 = 10 and F1 = 2. Its automatic command K0 has dirty work B1/C1/E1/G10; D1 is a clean nonvolatile hidden observer. At normal pre-commit test seams, retain a B1 candidate of 11 and an E1 candidate `[1,2]`, without installing either. Process an authored edit A1 = 20 between slices. The successor is K1.

**Required trace.**

```text
Candidate(K0,B1=11) and Candidate(K0,E1=[1,2])
    < CommitEdit(A1=20,R1->R2)
    < Superseded(K0,by=K1) < Admit(K1) < PassStart(K1/P0)
Reject(old_B1, changed_read_or_requirement)
Reuse(old_E1, certified_for=K1)
Install(B1=21) < Publish(C1=42) < Complete(K1)
```

E1's kernel evaluation count across K0 and K1 is **one**, and its producing identity stays K0's. Revalidation must also check its footprint/occupancy, not merely F1 = 2. B1's formula generation may be unchanged; its read and required version are not.

Before K1 installs B1, its published value version still denotes baseline 2. Direct validation of the old C1 = 4 result's B1 dependency proof must fail because B1's requirement/satisfaction no longer matches, even though B1's value version has not changed. Full promotion of old C1 also fails. Checking that dependency sub-proof separately makes this an explicit invalidation-version negative check rather than relying only on C1's own dirty flag.

**Final values.** A1 = 20, B1 = 21, C1 = 42, F1 = 2, E1:E2 = `[1,2]`, G10 = 1, D1 = 2. No old B1 = 11 or C1 = 4 candidate may be certified current for R2. K0 receives a superseded outcome, never success for R2.

**Invariant.** Changed revision is neither blanket rejection nor blanket reuse. Target, reads, invalidation and occupancy decide. The test counts actual reused computation rather than accepting numerically equal recomputation.

### Demonstration 5 — Typed cached reads and suspension preserve effects

**Part A: the read.** From baseline in Manual, set A1 = 10 and calculate D1 alone.

```text
Admit(KD1) < EffectStart(I1)
Read(I1,B1,CachedMacroObservation,V,2,dirty=true)
    < EffectComplete(I1) < Publish(D1=2) < Complete(KD1)
```

There is no B1 evaluation and no D1→B1 invalidation edge. B1 remains dirty at 2. Calculate B1 alone (11), then D1 alone again (11). The second D1 call has a new semantic identity. Merely installing B1 = 11 did not invoke or dirty D1 through the previous cached observation.

**Part B: real suspension.** Use a fresh fixture variant whose D1 callable has this conceptual typed implementation; setup does not execute the counted invocation:

```vb
Function CountAndPeekB1() As Double
    Calls = Calls + 1
    ' Supported VM/JIT safepoint here: suspend the existing activation.
    CountAndPeekB1 = Sheet1.Range("B1").Value2
End Function
```

Set `Calls = 0`, keep A1 = 10 with B1's dirty cache at 2, and calculate D1. Suspend immediately after the increment, service a worker/control task, then resume.

**Required trace and values.** One `EffectStart(I)`, one increment, `Yield(I,pc_after_increment) < WorkerTask < Resume(I,same_pc)`, one typed B1 callback through I's frame, one completion. D1 = 2 and Calls = 1. Same code/state lease and observation view survive. This must pass in VM3 and the promised JIT path; a JIT cell-boundary yield does not pass an inside-invocation suspension gate.

**Part C: native declaration.** A native variant without a safepoint records one non-preemptible interval and one entry. A cancel request queued during it becomes effective when control returns; effects already performed remain. It is never reported as cooperatively suspended, and never reinvoked to service cancellation.

**Part D: the fence must actually constrain eligible work.** Use the counted fixture with A1 = 10, cached B1 = 2 and Calls = 0. Force `{D1,B1,C1}` with fixed reference rank D1 before B1 before C1, and prioritize C1. Suspend D1 after its increment, then handle an interest change again prioritizing B1/C1. Require:

```text
EffectStart(D1,I) < increment(Calls=1) < Yield(I)
    < WorkerTask(Interest(C1)) < Resume(I)
    < Read(I,B1,CachedMacroObservation,2) < EffectComplete(I)
    < Evaluate/Install(B1=11) < Publish(C1=22)
```

The later B1/C1 segment may neither execute nor install while D1 is suspended. Final D1 = 2 and Calls = 1. Reversing interest leaves that observation/effect trace unchanged. A separate fixed reference-order control B1-before-D1 observes 11 in D1. These are exact `SliceV1` fence tests; they do not claim Excel uses either chosen reference order.

**Invariant.** Cached reads preserve their allowed observation, typed reentry cannot deadlock behind its own activation, and continuation resume does not repeat an irreversible prefix.

### Demonstration 6 — Manual C1-only completion is qualified

**Input/operations.** Baseline → Manual → A1 = 10 → calculate only C1 → calculate only B1 → calculate only C1 again.

**Required first command.**

```text
Admit(KC1,scope={C1})
Read(C1,B1,BoundaryCache,V,2,required>b1_satisfied)
Publish(C1=4,qualified_by=dirty_boundary_B1)
Complete(KC1,scope={C1},outcome=Complete)
```

B1 stays dirty at 2. There are no evaluations of B1, D1, E1 or G10 in that command. C1's local demand is satisfied, but its stamp and receipt disclose the dirty cached boundary; they make no transitive/global freshness claim.

**Next values.** The B1-only command gives 11 and invalidates the appropriate C1 dependency/status. The final C1-only command gives 22. A diagnostic read between operations causes no additional calculation.

**Invariant.** A completed narrow command does not imply that its precedents or workbook are current. Intentional cached dependencies are represented rather than silently forced or rejected.

### Demonstration 7 — A pending spill prevents a false blank proof

**Input/operations.** Baseline → F1 = 10 in Manual → calculate `{E1,G10}` with G10 highest interest, before E1's new shape is resolved.

**Required trace.**

```text
CommitEdit(F1=10) includes pending_producer(E1) and invalidation(G10)
BlockedOnOccupancy(G10,E1)
InstallAtomic(E1:E10=[1..10],ownership_and_topology)
Read(G10,E10,Calculated,10) < Publish(G10=11) < Complete(K)
```

Old G10 = 1 can remain displayed only as cached/pending. There is **no** current-for-K publication of G10 = 1 based on `ProvenAbsent(E10)` while E1 can still occupy it. All spill elements and ownership appear together.

The occupancy block may be discovered before dispatch or by `ReadAttempt(G10,E10,strict) -> Wait(PotentialSpill(E1))`; either trace is valid. The oracle checks the barrier and forbidden publication, not which implementation layer discovers it.

**Final values.** E1:E10 = 1…10 and G10 = 11. B1/C1/D1 are unchanged from baseline.

**Required companion cases.** Shrink F1 to 1: remove E2:E10 atomically, invalidate G10, then publish G10 = 1 with a new valid absence proof. In a separate clone, calculate G10 alone while E1 is dirty/outside scope: complete with 1 and `CachedAbsent` qualification, without running E1. An authored E10 obstruction must produce the declared spill outcome and must never be overwritten.

**Invariant.** Occupancy is a dependency even when the previous grid cell was blank; scope controls whether unresolved occupancy is awaited or explicitly observed as a stale cache.

### Demonstration 8 — Save(false) preserves a coherent chosen cache view

**Input/operations.** Baseline → Manual → A1 = 10. Set CalculateBeforeSave = false. Begin a save using `PreservePermittedCaches`; pin R1 and the committed cache/validity view Vdirty. Suspend serialization after pinning. Change live A1 to 100 at R2, then finish the save.

**Required trace.**

```text
CommitEdit(A1=10,R1) < SavePin(R1,Vdirty)
SavePin(R1,Vdirty) < CommitEdit(A1=100,R2) < SaveWritten(R1,Vdirty)
```

There are zero formula or UDF invocations caused by save. The file contains A1 = 10, the unchanged formulas, B1 cache 2, C1 cache 4, D1 cache 2, F1 = 1, E1 cache/footprint `[1]`, G10 cache 1, and honest incomplete/recalculation metadata. It contains neither live A1 = 100 nor any private candidate. Live A1 remains 100.

Inspect the package bytes before reopening. Open it in a new DNA Calc instance in controlled Manual mode with the fixture provider registered. The selected caches are displayed as unverified/dirty where appropriate, with a new workbook incarnation and no resumed commands/effects. Explicitly calculating B1:C1 then produces 11 and 22.

**Required policy companion.** From another baseline clone with A1 = 10, Save(true) waits for the defined dirty/volatile save calculation, pins its final result view, and serializes B1 = 11, C1 = 22. D1 can remain its permitted nonvolatile cached observation of 2; saving is not an implicit Full command. Cancel/refuse the required save calculation and verify that the operation does not silently write a success under the false policy.

**Invariant.** Save selects one authored revision and one immutable cache image. Intentionally stale caches can be coherent; they are not serialized or reopened as fabricated fresh calculations.

## 11. Async identity sentinel after the eight demonstrations

Use a deterministic provider and a fixture variant `D1 = CountThenAsync(A1)`, with Manual mode explicitly set. The counter increments before the async suspension; the provider releases explicitly chosen payloads. New provider invocations occur only at the stated explicit calculation steps, subject to the forced-successor rule. For the positive dependent-pass control, temporarily use `C1 = D1+1` and calculate `{D1,C1}`; C1 waits in scope and does not need an existing boundary cache.

Each row starts from an independent fixture unless the row explicitly continues the previous sequence. Log target generation, provider generation, request token, invocation, counter, return disposition and publication. Native XLL lifecycle integration follows this deterministic provider test; it is not a prerequisite for defining identity.

| Case | Operations | Required oracle |
|---|---|---|
| Valid return | Start I0; return payload 41 for its pending token | Accept once; resume existing continuation; D1 = 41, dependent C1 = 42 in the specified dependent pass; counter increments once |
| Overwrite | Start I0 at target g1; overwrite D1 with 99 (g2); return 41 | Reject revoked/changed target; D1 remains 99; no continuation entry or new increment |
| Undo cannot revive | Start I0; overwrite; undo restoring identical formula text at g3; deliver I0's old return | Reject; g3 is not g1. Undo itself does not replay I0. A new explicit invocation can subsequently succeed |
| Duplicate | For a valid I1 deliver 42, then 42, then 43 with the same token | First return accepted; later identical return classified duplicate, conflicting return protocol violation; no second resume/publication/increment |
| Provider reload | Start under provider p1; retire/reload to p2; return p1 payload | Reject retired provider/authority; old leases may drain but cannot authenticate current publication; new p2 invocation can succeed |
| Argument edit | Start for A1 = 1; edit A1 = 2 without changing D1 formula generation; return old payload | Reject revoked command/argument certificate/requirement; formula generation alone is insufficient |
| Move and undo | Start at D1; move it or structurally shift it; optionally undo; return old token | Reject under the slice's no-transfer policy, even if text/address later recur |
| Cancel/supersede | Start and wait; cancel or supersede; return | Reject result authority; retain the fact that the prefix effect occurred |
| Close/reopen | Start; close document; reopen same file; return old token | Reject workbook incarnation, irrespective of coincidentally equal cell IDs |
| External cutoff control | Admit I0, then receive later RTD/link inputs beyond K's cut; return I0 | The async fulfilment is not rejected merely for arriving after the cut; unrelated stream inputs belong to later demand. If the later event independently revoked K, that revocation is the reason |

For rejected returns, the current authored model, cache and activation are unchanged apart from status/diagnostic/cleanup bookkeeping. Rejection never directly launches a replacement evaluation. Because the slice supersedes live commands on authored edits, even an unrelated authored edit revokes an outstanding async token; finer survival across unrelated edits is deliberately deferred.

## 12. Regression properties and scheduled exploration

### 12.1 Cheap checks on every commit

Use a small independent executable reference model for integer-valued acyclic formulas, scoped boundary caches, one spill and scripted effect/async events. It should calculate from its own simple semantics, rather than call the production certificate validator to decide expected outcomes. Compare values, types/shapes, qualifiers, effect entries and permitted event order.

A practical initial budget is a fixed small seed set over up to eight formula targets and twenty operations per trace, plus the named regression fixtures. These are deterministic workload limits, not a promised wall-clock duration. Measure the gate and keep large exploration outside it. The real-worker smoke proves event-loop handoff; the VM/JIT sentinel runs on the supported native target, with a browser typed-frame counterpart where applicable.

| Property | Generated variation and independent assertion |
|---|---|
| Command deduplication | Deliver identical IDs/payloads repeatedly and mismatched payloads once. One authored commit/admission; mismatch is rejected |
| Yield equivalence | Interrupt at every supported checkpoint. Compare semantic values/effects with uninterrupted execution; continuation identity and state are retained |
| Pure priority equivalence | Permute interest inside eligible fence segments. Same final values/shapes/qualifications and empty or unchanged effect trace; same scope |
| Invalidation sensitivity | Change value, required version, satisfaction, occupancy query, namespace miss, fact or callable version independently. Relevant proof fails; unrelated E candidate remains reusable |
| Repeated dirtiness | Dirty a cell twice without changing its cache. Requirements advance twice; a candidate for the first change cannot clear the second |
| Historical observation integrity | Change the live B1 cache after D1's authorized observation. The observation remains evidence for its original view; no hidden reverse edge/retry appears |
| Invocation uniqueness/cardinality | Repeat one site, recurse, suspend and re-evaluate within a pass. Distinct semantic evaluations have distinct IDs; each admitted invocation enters once |
| Draw identity | Two RAND calls produce two keys, LET one; retries reuse keys; new semantic evaluations/passes do not. Compare ledger events, not coincidental numeric inequality |
| Atomic authored visibility | Observe during a prepared structural edit/undo. See pre-state or post-state including invalidation/bindings; undo advances revisions/generations and preserves effect history |
| Spill growth/shrink | Vary heights 1…10, obstructions and interest. No partial footprint; strict absence excludes pending producers; disappearing cells invalidate readers |
| Facet-aware change | Equal numbers with changed formula text/kind/shape/reference/visibility still notify their relevant observers; equal scalar values cannot hide a required status publication |
| Publication/receipt order | Admission before caused values; success after required installs; no cancelled/superseded publication certified current; mirror observes completion only after its barrier |
| Async rejection immutability | Vary one identity/generation/requirement at a time and deliver duplicates. Rejection changes no current value or effect count; acceptance consumes once |
| Scope honesty | Calculate a subset with dirty outside precedents or potential spills. Complete with qualifications or declared refusal; no unauthorized outside evaluation |
| Save pinning | Interleave later model/cache changes after save pin. Parse saved formulas/values/shape/settings; they match the pinned image, and Save(false) has no invocation events |
| Finite-work fairness/lifetime | Repeated interest changes with finite ready work meet the declared service rule; terminal commands release private candidates/views while provider leases obey explicit draining |

A small persistent-recovery fixture should also simulate crashes before admission, after the durable started record, and after effect completion but before outcome recording. It must never replay a started/indeterminate irreversible invocation. A file cache reopen is not an effect-journal recovery test.

When property comparison ignores publication order, it still checks required causal edges and qualified results. A test may not hide an effect-order difference by sorting the trace.

### 12.2 Exploration on a schedule

Run these periodically or when the relevant implementation changes; do not make every large corpus a per-commit prerequisite:

1. **Interleaving exploration.** Enumerate bounded schedules around read, safepoint, candidate validation, authored commit, observation admission, spill reservation, publication, save pin, cancel, return and reload. Include accepted-return/overwrite races and equal-value invalidation.
2. **Long generated histories.** Alternate edit/undo, range/full calculation, spill growth/shrink, save/reopen and command duplication. Retain and deterministically shrink failures to a workbook plus operation trace.
3. **Excel differential probes.** Run §13 on pinned Excel builds/profiles, preserving raw files and traces. A compatibility rule changes only with a recorded observation and updated fixture expectations.
4. **Spill/absence scale.** Increase pending unknown-shape anchors geometrically. Count anchors examined, readers blocked, invalidations, retained proof bytes and time to the first qualified current result. First paint of an old cache is not that metric.
5. **Continuous external input.** Deliver RTD/link bursts while finite commands and saves execute. Their admitted frontiers do not expand indefinitely; later events create later demand. Cancel/supersede outcomes are reported honestly under sustained edits.
6. **Slow subscribers.** Fragment spill units, delay delivery, coalesce/reset mirrors and rotate future dictionaries. Observe atomic units, bounded retained memory and correct completion barriers.
7. **Provider/runtime failures.** Late returns after close/reload, non-cooperative calls, crashes and partial drain. Check lease reclamation and absence of effect replay. Test the real XLL lifecycle after the deterministic fixture works.
8. **Numeric continuation checks.** On supported Exact kernels and CPUs, compare interrupted and uninterrupted witness results, operation/store sequence and floating-point environment. Extend only as kernels acquire safepoints.

After parallel execution exists, explore its small synchronization kernels with a controlled scheduler/model checker. Do not require a parallel runtime or formal-methods framework to complete this serial contract slice.

Failure records contain the initial workbook, authored operations, accepted input values/sequences, profile, command/pass decisions, target/callable/provider generations, observation views, draw material, effect outcomes, relevant interest/publication choices and platform provenance. A runtime seed alone does not reconstruct those inputs.

**Semantic replay** reproduces permitted results and effect observations for the recorded semantic history. **Publication replay** additionally reproduces intermediate delivery/installation choices. The latter must record those choices or use a declared deterministic commit policy; it cannot be inferred from a random seed.

## 13. Excel measurements that remain open

These are compatibility choices, not gaps an implementer may fill ad hoc. Ship/run `SliceV1` fixtures with the provisional choices above; tag the profile as such. Before claiming the corresponding Excel behaviour, run the decisive probe and version the selected policy. Do not make documented permissions sound like measured ordering guarantees.

| Question and options the envelope preserves | Decisive experiment and recorded evidence | Current slice choice |
|---|---|---|
| **Cached VBA frontier:** command-entry cache versus latest permitted semantic cache at invocation; possible per-read advancement within an invocation | Seed B=2; Manual A=10; D-only, B-then-D, B:D, Full, RebuildFull, cold/warm chains. Log entry/caller and each in-memory observation; flush logs after calculation. Add self-read and two reads with any legally permitted reentry. Record values and order, not worksheet logging side effects | Cache at fixed invocation fence, pinned across suspension. If Excel exposes no comparable suspension, label that part a DNA responsiveness rule |
| **Range scope and dirty transitions:** Manual boundary caching versus particular edge-case refresh; mode-dependent normalization | C-only, then B-only, then C-only; B:C together; noncontiguous/reversed areas; cross-sheet precedents; Automatic/Except Tables variants. Infer dirtiness through subsequent operations rather than inventing an internal dirty API | Manual scope remains exact; cached boundaries disclosed. Other mode mappings are not frozen |
| **Registered callback privileges:** refuse/defer on dirty read versus macro-equivalent cache access | Same dirty-B probe through VBA, ordinary XLL registration, macro-equivalent registration and eligible thread-safe registration. Record callback status, entry count and return value | Registration-checked policy, no implicit VBA privilege |
| **Repeated semantic evaluation:** one invocation versus several under chain repair/dynamic dependencies | Counter before a late dependency; run cold, warm, Full and RebuildFull. Log each entry separately from a suspension/resume | Resume never re-enters. Additional semantic evaluations require a recorded policy/reason |
| **Random/name/LAMBDA scope and volatile shape passes:** per-use versus reusable draws; stabilization threshold and pass triggers | RAND pair, LET-bound RAND, RAND-valued name twice, repeated LAMBDA and volatile SEQUENCE. Record accessible draw/call events, shapes, pass evidence and final error | Distinct semantic identities supported; retry draws fixed; no guessed volatile-spill retry count |
| **Spill contention/release:** establishment/chain order versus another measured winner rule; stale-footprint timing | Two overlapping anchors in both insertion orders; warm/full/rebuild; F:1→10→1; authored/merged/table/grid-edge obstruction; save/reopen. Inspect values, ownership and removals | First-established in fixed semantic reservation order; one-anchor main fixture |
| **Save scope and caches:** dirty calculation versus broader work; preservation/omission of stale spill caches; open-time recalculation | Manual save false/true with dirty B/C and changing F; inspect package before reopen; repeat with pending async; fresh Excel process and controlled initial application mode. Log any invocation during save/open | Pinned dirty/volatile workbook save scope; explicit preserve policy; no certificate resurrection |
| **Module/provider reload:** retain a compatible old activation versus reject; when callers become dirty | Suspend F(Double); replace with F(String); test old handle, new call, code/state lifetime and module counters. For native providers, delay a handle return across unload/reload | Old result authority revoked; old code/state retained only for drain; new generation required |
| **Stored Range across edits:** track moved targets versus reject/rebind by operation | Acquire a multi-area Range; insert/delete/cut; switch/close/reopen workbook; inspect Parent, Address, Areas and Value2 | Revision-qualified handle; async targets not transferred. Full macro object-model tracking is deferred |
| **Formula entry and binding:** immediate Manual-mode formula evaluation; Formula/Formula2 distinction; preserved versus rebound shadowed names | Enter/edit formula in each surface/mode; compare Formula/Formula2 through save; add/remove local shadow after global lookup for old/new formulas | Fixture setup is explicit, not an assertion about Excel formula-entry timing; semantic dialect and binding probes retained |
| **Iteration:** initial caches, prepasses, sheet/participant order, stopping and effects | One/two sweeps across sheets, nonconvergence/errors, downstream counter, dynamic edge merge/split. Record every invocation/value | No general iteration implementation; named bounded unsupported outcome |
| **Data tables:** per-scenario observations/effects and special cycles | Duplicate inputs with RAND/counter/name reference; cached callback read; cancellation and Automatic Except Tables; inspect base cache during yield | No table implementation; `InputView` and destination separation prevent future base-cache contamination |

For every Excel probe retain input workbook, exact operations, Excel build/bitness/platform/CPU, mode/open order, registration details, raw in-memory trace and saved package. Where logging can perturb order, compare an uninstrumented control. Use module/provider logs and inspect them after calculation; avoid introducing worksheet writes from UDFs.

For async lifecycle experiments driven by VBA, log handles and entry/return events directly. Microsoft's XLL event documentation notes that certain calculation lifecycle events are not raised for programmatic recalculation; their absence is not proof that no calculation occurred. [S13]

## 14. Work split for three coding agents and the acceptance gate

Assign shared type definitions and event names once, before parallel implementation. Each type above has an immediate caller; do not build speculative registries for iteration, distribution or arbitrary transaction composition.

| Lane | Owns | Concrete boundary and completion evidence |
|---|---|---|
| **A — engine/read/publication** | Command/pass state, certificate recording/validation, requirement versions, pure ready work, one-anchor occupancy, result views/units | `admit`, `advance(budget)`, `read(request)`, `try_install`; headless demos 2–4, 6–7 and version/absence properties |
| **B — host/transactions/worker/save** | Deduplication, mode/scope normalization, authored atomic commit/undo, successor admission, worker task driver, receipts, save pins and XLSX mapping | `prepare/commit`, `apply_interest`, `cancel/shutdown`, `prepare_save/serialize`; real-worker demos 1 and 3, edit/save tests and mirror ordering |
| **C — callable/effects/async** | Invocation/frame IDs, typed callback reentry, code/state leases, one-shot admission ledger, VM/JIT safepoints, provider handle registry | `invoke/resume`, `admit_effect`, `deliver_external`; demo 5 and the entire async sentinel |

Shared ownership rules: lane A alone certifies results; lane B alone advances the authored model/domain admission sequence; lane C cannot publish values directly or infer freshness. Provider adapters report events to the owner. The executor may schedule a continuation but cannot assign a new semantic identity or authorize an effect.

The slice is complete when all eight demonstrations pass through the shared headless engine and relevant real UI/worker route, the typed VM/JIT continuation sentinel passes, stale async returns are rejected, and save byte/reopen checks pass. A platform incapable of a promised safepoint reports that gate as incomplete; a cell-boundary imitation is not a substitute.

Record unsupported-unit reasons and counters. Any conservative boundary must be explicit, bounded for supported work and visible in the trace. There must be no uncounted “recalculate the workbook” escape hatch.

This gate establishes a small functioning contract: scoped calculation, honest observation and freshness, revision-aware pure reuse, irreversible invocation identity, occupancy-safe publication and a coherent saved image. Excel profile claims remain limited to measurements actually recorded.

## Sources

**[P] Supplied architecture pack.** `7-recalc-contract-prompt.md`, including Appendix A's final synthesis, Appendix B's code-verification and probe tables, and Appendix C's red-team review. All claims about existing DNA Calc/OxVba code and prior project observations are attributed to this pack, not a fresh code audit.

- **[S1]** Microsoft, [Excel performance: Improving calculation performance](https://learn.microsoft.com/en-us/office/vba/excel/concepts/excel-performance/excel-improving-calculation-performance). Range dependency scope, repeated evaluations, calculation modes and hidden-name/volatile context.
- **[S2]** Microsoft, [Excel Recalculation](https://learn.microsoft.com/en-us/office/client-developer/excel/excel-recalculation). Mode-sensitive range calculation and asynchronous formula/dependent-pass description. Some examples discuss older Excel versions; exact target behaviour still requires probes.
- **[S3]** Microsoft, [Excel Commands, Functions, and States](https://learn.microsoft.com/en-us/office/client-developer/excel/excel-commands-functions-and-states). Registered worksheet versus macro-equivalent/VBA permissions for uncalculated-cell access.
- **[S4]** Microsoft, [Custom function may not calculate expected value](https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/custom-function-calculate-wrong-value). Internally referenced cells and missing explicit dependency information.
- **[S5]** Microsoft, [Range.Value2](https://learn.microsoft.com/en-us/office/vba/api/excel.range.value2). Getter/setter and type-conversion contract.
- **[S6]** Microsoft, [How to correct a #SPILL! error](https://support.microsoft.com/en-us/excel/how-to-correct-a-spill-error). Indeterminate volatile shape and other spill errors.
- **[S7]** Microsoft, [Asynchronous user-defined functions](https://learn.microsoft.com/en-us/office/client-developer/excel/asynchronous-user-defined-functions). Call handles, argument lifetime and callback restrictions.
- **[S8]** Microsoft, [Application.CalculateBeforeSave](https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculatebeforesave). Manual-mode pre-save calculation setting.
- **[S9]** Microsoft, [Working with formulas](https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/working-with-formulas). Formula text and last-calculated value storage in SpreadsheetML.
- **[S10]** Microsoft, [CalculationProperties](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.calculationproperties?view=openxml-3.0.1). Calculation settings/status attribute vocabulary.
- **[S11]** Microsoft, [CellFormula](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.cellformula?view=openxml-3.0.1). Formula attributes including calculation flags.
- **[S12]** WHATWG, [HTML event loops](https://html.spec.whatwg.org/multipage/webappapis.html#event-loops). Tasks and microtask checkpoints.
- **[S13]** Microsoft, [Handling Events](https://learn.microsoft.com/en-us/office/client-developer/excel/handling-events). XLL calculation lifecycle events and programmatic-recalculation limitation.

