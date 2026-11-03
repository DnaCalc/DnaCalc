# DNA Calc recalculation contract, phases 1-2

Date 2026-10-31. The contract the engine lane implements for K1/K2 and the phase-2 slice (SYNTHESIS.md 1.3, 4).
Base: `pro/7-recalc-contract-answer.md` (**A7**), reconciled with SYNTHESIS.md (**S**), `pro/4-calc-semantics-answer.md`
(**A4**) and the code at DnaCalc-review **fd2336d** (paths below are under `crates/`). Probe ids **Pnn** refer to
`EXCEL_CALC_PROBES.md`. Evidence labels as A4: D documented, O observed on this machine, I design inference, U unmeasured.
The profile implemented here is **SliceV1**: a deterministic DNA reference policy. Where it fixes something Excel has
not been measured on, it says so; implementing SliceV1 is not evidence of Excel behaviour.

## 0. Decisions

1. **Two receipts.** `AdmissionReceipt(K)` precedes every value K causes; `CompletionReceipt(K)` follows every
   installation K requires. S's "the command receipt precedes its values" means the admission receipt.
2. **Fixed admitted frontier.** A calculation command pins model revision and external-input cut at admission. An edit
   creates a successor command (new pass, predecessor link); validated pure work carries over. Today a commit discards
   the whole recalculation (`calc-host/src/command.rs:436-438`, `calc-engine/src/recalc.rs:634-642`).
3. **Semantic evaluation vs physical attempt.** `InvocationId` carries an evaluation ordinal. Yield and physical retry
   keep it; a new semantic evaluation needs a typed `ReevaluationReason` (2.3), including the documented XLL
   `xlretUncalced` unwind that A7 omits (A4 2.10).
4. **Dependencies vs observations.** A certificate holds dependency reads (validated, reverse edges installed) apart from
   cached observations (validated against their pinned view, no reverse edge, no demand).
5. **Qualified freshness only.** `FreshFor(K, T)` (3.3); no result, receipt or projection carries an unqualified
   workbook-wide "settled". Status-only publications exist.
6. **Effects are at-most-once per InvocationId.** Execution state and result disposition are separate; cancel and
   supersede never undo or repeat an effect.
7. **VBA cached reads read a pinned root** fixed at invocation admission; SliceV1 uses serial observation fences.
8. **Save pins one model revision and one result view**; the cache policy is explicit. P06 selects
   `PreservePermittedCaches` for ordinary Manual Save(false). Save/SaveAs pins include admitted BeforeSave edits (8).
9. **Strict blank reads need an absence proof**; a spill footprint installs as one unit.
10. **No value-based early cutoff in SliceV1**; all admitted dirty or forced work is discharged.

## 1. Evidence corrections found while reconciling

- **The cached-VBA-read observation is narrower than S 1.3 and A7 4.2 state.** `VBA_UDFS.md:91-93` rests on
  `calc-vba/tests/fixtures/excel-recalculation/probe.bas:37-40,147-157`: `HiddenProbe()` reads **constant** Z1. Measured
  (O, 16.0.20527): changing Z1 does not re-invoke the UDF (no hidden dependency edge), and a forced `Range.Calculate`
  reads the new constant. Whether a VBA read of a **dirty formula** returns its old cache is documented as permitted
  (D, macro-sheet-equivalent access to uncalculated cells). **P01 now measured (O, 20527, safe mode):**
  ten fresh workbooks per condition returned the dirty formula's old value 2 when calculating only the observer,
  with no producer invocation. Demo 5's narrow cached read is observed. Reversed `C1,B1` produced an Empty
  observation followed by 11 on a second invocation; this is not evidence that the two invocations share a pin.
  Self-read gave 0 at entry then Empty on forced calls; two reads around WorksheetFunction.Sum stayed at 2.
  See `calc-engine/tests/fixtures/excel-calc-probes/P01/README.md` for exact scope and retained evidence.
- **Range.Calculate appears to leave its target dirty.** In the same run (Manual mode, `probe.bas:70`) B1 =
  `PlainProbe(1,A1)` went 4 -> 5 under `B1.Calculate` (a clean target is forced, O) and 5 -> 6 under the following
  `Worksheet.Calculate`, while the equally clean B3 stayed 4; B12 repeated the pattern (`hidden-force` then
  `toggle-back`). Inference (I): a range calculation does not clear the target's requirement. A7 demo 6 assumes it
  does. **P02 now observed (20527, safe mode):** in ten isolated Manual range trials counter 32 ran
  once under the range command and again under F9, while clean K1 did not run. Choose `StillRequired`
  for this profile. The full matrix's ForceFullCalculation carryover is excluded from this inference.
- **Iteration order is partly measured**, unlike A4 2.8's caution: cell order within a sheet, sheets by name ignoring
  case, Gauss-Seidel visibility (+200 per 100 iterations for A1=B1+1, B1=A1+1), seeding from held values
  (`calc-engine/src/recalc.rs:36-67`, fixtures `dc_86f`, `dc_10y_iteration`, `dc_y2o`; 16.0.20430/20527). P14 keeps
  only the unmeasured parts.
- **Excel shows a file's `<v>` on open without recalculating** (variant `wrong-cache`, `calc-doc/tests/fixtures/d1/
  excel-open.json`, 16.0.20430), calculates an ordinary formula without `<v>`, and an array without `<v>` only under
  `fullCalcOnLoad` (`calc-doc/src/save/author.rs:712-723`). This decides what a preserved stale cache means (8).

## 2. Envelope types and invariants

```rust
struct CommandId { client_session: SessionId, sequence: u64 }          // edit/transport identity, deduplicated
struct CalcCommandId { domain_incarnation: DomainId, sequence: u64 }   // accepted calculation
struct CalcPassId { command: CalcCommandId, ordinal: u32, reason: PassReason }
enum PassReason { Initial, AsyncDependents, DependencyRepair, SpillShape, IterationSweep(u32), Scenario(u32) }
struct WorkbookInstanceId { document: DocumentId, incarnation: u64 }   // reopen = new incarnation
enum EvaluationLocus { Cell { sheet: SheetInstanceId, at: CellAddress }, Name { name: NameId, caller: QualifiedCell } }
struct EvaluationTarget { workbook, origin_revision: Revision, formula_generation: FormulaGeneration, locus }
struct InvocationId { pass: CalcPassId, workbook, locus, evaluation: u64, call_site: CallSiteId,
                      occurrence: OccurrencePath }      // A4: path (LAMBDA/MAP element/recursion), not one u64
struct AdmissionFrontier { model: Revision, external_through: DomainEventSeq }
enum ResultDestination { Live, PinnedSave { save: CommandId } }
struct CalcRequest { id, source: CommandId, predecessor: Option<CalcCommandId>, cause, mode, scope: CalcScope,
                     evaluation: EvalSelection /* DirtyAndVolatile | ForceAll */, tables: TablePolicy,
                     admitted: AdmissionFrontier, inputs: InputViewId, profile: CompatibilityProfileId, destination }
struct ObservationFrontier { model: Revision, inputs: InputViewId, results: ReadViewId, order: ObservationOrder }
struct ReadView { frontier: ObservationFrontier, model_pin: ModelPin, results_pin: ResultViewPin }
enum ReadPolicy { DependencyWithinCommand, CachedMacroObservation,
                  RegisteredWorksheetCallback { registration: CallableIdentity }, DiagnosticSnapshot }
struct ReadRequest { purpose: ReadPurpose, policy: ReadPolicy, view: ReadView }
enum ReadOutcome<T> { Value(T), Wait(WaitReason), Refused(ReadRefusal) }
struct ReadSpan { subject, facets: ObservableFacets, value_versions, required, satisfied, source: DependencySource }
enum DependencySource { AuthoredInput, Calculated { proof }, BoundaryCache { view, state } }
struct AbsenceProof { region: AreaSet, basis: Proven { occupancy, producer_query } | Cached { view, pending_outside } }
struct ReadCertificate { dependencies: Vec<ReadSpan>, observations: Vec<CachedObservation>, absences, binding_proofs,
                         facts /* version + OnChange|OnNextAuthorizedCalculation|ObservationOnly */,
                         semantic_environment, callable_versions }
enum ComputationOrigin { Evaluated { root: InvocationId }, ImportedCache { file: FileIdentity } }
struct FreshnessClaim { command, pass, frontier: AdmissionFrontier, qualified_by: ReadQualifications }
struct ResultStamp { target, computed_by: ComputationOrigin, certification: Option<ResultCertification>,
                     completion: TargetCompletion /* Value | WorksheetError | EngineRefusal */ }
struct PublicationUnit { id, command, destination, model: Revision, base: ReadViewId, next: ReadViewId,
                         sequence, results, topology: TopologyDelta, changes: Vec<ObservableChange> }
struct AdmissionReceipt { command, destination, scope, admitted, predecessor }
struct CompletionReceipt { command, destination, scope, admitted, outcome: CompletionOutcome,
                           coverage: ScopeCoverage, qualifications, results: ReadViewId, after_publication }
struct PreparedTransaction { command: CommandId, base: Revision, preconditions, forward, inverse_authored,
                             coordinates: AddressTransform, bindings: BindingDelta }
struct AsyncToken { invocation, target, provider, provider_generation, request }
enum ExternalOperation { AsyncReturn { token, result }, RtdUpdate { subscription, sequence, value },
                         LinkRefresh { source, revision } }
```

| Invariant | Today in code (fd2336d) |
|---|---|
| IDs unique within their lifetime, never recycled while a result, journal record or token can name them, never wrap | `NodeId` is generational (slot + generation, `BySlot` `calc-engine/src/recalc.rs:231-273`); `CalcTick` counts up per recalculation (`calc-engine/src/result.rs:10-25`) |
| One `CalcCommandId` per accepted calculation; a pass id includes it, so draws cannot survive an edit | `CalcTick` is one per recalculation; no pass id; spill re-waves reuse the tick (`recalc.rs:18-23`) |
| `origin_revision` explains an address; it is not a validity test | `Calculation { revision, tick, mode, .. }` per result (`result.rs:27-44`); validity is "not in the stale set" (`calc-host/src/save.rs:64-75`) |
| A stamp records what was established; live status is computed against requirements, never a stored `fresh: bool` | `FormulaResult::{Calculated, FileCached, NotModeled}` (`result.rs:157-171`) maps to `TargetCompletion` + `ComputationOrigin::ImportedCache` |
| An imported cache never gets a producing command or certificate | the reader never hands out a file cache or refusal as a value (`calc-engine/src/read.rs:4-9`, `recalc.rs:619-621`) = A7's `MissingCache` refusal |
| `qualified_by` carries boundary caches and cached observations; arithmetic on a qualified input cannot upgrade it | precursors: `Calculation::reads_external_links` and `held_by_circular_rule` (`result.rs:36-81`) |
| `ReadPurpose` is not a permission: the invocation frame authorizes the policy; invalid pairs are refused | none (one reader for all reads) |
| Range reads are recorded as spans or versioned chunks; no per-element certificate entry | `EvalLocal.footprint: Vec<GridArea>` (`recalc.rs:323-332`) |
| `ObservableFacets` covers value/type/error, formula text, kind/dialect, shape, reference identity, occupancy, binding, row visibility, numeric semantics, host facts, calculation status | dialect already keys plans (`calc-engine/src/state.rs:401`, `calc-fml/src/eval/mod.rs:107-113`) |
| `ResultDestination` is immutable; `PinnedSave` installations never reach live subscribers | none |
| `PreparedTransaction` is prepared against one base; a stale base is rejected, never patched | `HostCommand { base, edits }`, `CommandError::StaleBase` (`calc-host/src/command.rs:103-109,164-171,238-239`) |

Pins are reference-counted immutable roots (`ModelRoot` snapshots already are `Arc`s, `command.rs:464-480`), held
only while something can read them; effect tombstones are never reclaimed so that an old token authenticates anew.

## 3. State machines

### 3.1 Command (calc-host normalizes, engine executes)

Normalization (host, before admission): `Automatic edit` -> dirty and volatile under the domain mode, retaining
unfinished authorized work of a superseded command; `CalcNow` (F9) -> dirty and volatile in the domain; `CalcSheet` ->
dirty and volatile on the sheet, cross-sheet formula precedents may be qualified boundary caches; Manual `CalcRange` ->
`ForceAll` on the exact `AreaSet` (no bounding-rectangle widening), formula precedents outside it are boundary caches;
`Full` -> `ForceAll` over the domain (clean results cannot satisfy it); `RebuildFull` -> rebuild bindings, then `Full`.
Iteration and data tables return named bounded unsupported outcomes in the slice, never a whole-workbook fallback.
`RangePostState::{Satisfied, StillRequired}` is a profile field; SliceV1 uses `StillRequired` for Manual
`CalcRange`, following P02's isolated run (O, 20527 safe mode). Automatic mode admits edit-triggered work
before a later explicit range command; it must not inherit the Manual normalization blindly.
`CalculateRowMajorOrder` is separate: P02's reversed chain returned 3 versus dependency-aware range's 11.

| State, event | Transition |
|---|---|
| Submitted, ingress dedup hit (same `CommandId`, same payload) | return existing admission/outcome; different payload = protocol conflict |
| Submitted, validation fails | `Rejected`; no admission, no effects |
| Submitted, valid | pin model, inputs, mode, environment, callable table, profile; resolve scope; register dirty, volatile and pending spill producers; emit `AdmissionReceipt`; `Active` |
| Active, budget spent | `Yielded`; all resumable state and identities kept |
| Yielded, worker task resumes | `Active`; no new pass or invocation because time passed |
| Active, only dependencies or provider returns remain | `Waiting(reason)`; no busy retry |
| Waiting, input or return arrives | `Active`; resume, or open a pass with a recorded reason |
| All required work terminal and installed | `Complete` if no refusal or unfulfilled target, else `Incomplete(reason)` |
| Live authored edit commits | revoke live publication authority; `Superseded { by: K1 }`; admit K1 at its own frontier; a `PinnedSave` command is not retargeted |
| User cancel | stop effect admission and publication; cooperative stop; `Cancelled`; no successor |
| Shutdown | stop admission, revoke live authority, record pending effects, drain; `Shutdown` |
| Engine fault | `Failed`; never turned into a worksheet error |

Terminal receipts are immutable and emitted once; a superseded or cancelled command is never upgraded to `Complete`.
Resource draining may outlive the receipt. Today: `HostProgress` (`calc-host/src/recalc.rs:39-55`) has no admission,
waiting or superseded states; `calculate_full` cancels running work (`recalc.rs:127-136`), which K1 replaces with
supersession.

### 3.2 Pass

`Ready -> Running <-> Yielded(Checkpoint) | Waiting(WaitSet) -> Complete | Superseded | Cancelled | Shutdown |
Failed`. One fixed model/input view per pass and a recorded reason. With async calls, the initial pass completes its
own formulas and continuations; dependents not yet started are `DeferredTo(AsyncDependents)` (unfulfilled command work,
not completed targets); the next pass owns them. A pass never waits on a target it delegated to a pass that cannot
start. Today's `RecalcWork` (`calc-engine/src/recalc.rs:280-321`) is the resumable state of one pass plus spill
re-waves (`rewave`, `waves`, cap `MAX_WAVES = 32` at `recalc.rs:107-109`, not an Excel measurement).
P04 (O, 20527 safe mode) found eight armed SizeUdf evaluations before #SPILL! for a seed-1 volatile
SEQUENCE alternating 2/3, under range/Full/Rebuild, ten trials each. A 2,3,3 script stopped after three
calls; 2,2,3 after two. The held footprint matters: seed 2 could settle after the first returned 2.
Use the eight-evaluation bound for this measured sizing path; do not replace every graph-repair wave
limit with eight or infer that every SizeUdf call equals a whole-workbook pass.
The 70-workbook nonvolatile changed-seed control made zero calls under clean F9 and one under range/Full/Rebuild.
The forced commands installed two rows; clean F9 kept one. Later scripted sizes and the third-call error were not
requested. Volatility/reevaluation
policy is therefore part of the measured sizing behavior; the bound is not an instruction to call every size
expression eight times.

### 3.3 Invocation

```rust
enum EffectExecution { Allocated, Admitted, Running, Suspended(Activation), WaitingExternal(AsyncToken),
                       Completed(InvocationOutcome), Stopped { effects_so_far }, Indeterminate }
enum ResultDisposition { Pending, Installed(PublicationUnitId), Discarded(DiscardReason) }
enum ReevaluationReason {            // the only routes to a NEW evaluation ordinal at a locus
    NewCommandDemand, AuthoredOrInputChange, XllUncalcedUnwind /* D: MS18/MS19 */,
    ProvisionalArguments /* P03: provisional Range values/coerced arguments */, SpillShapePass, IterationSweep, TableScenario }
```

Allocation -> admission -> running -> completed; running/suspended and running/waiting-external keep identity and
activation; nothing returns to "call the entry point again". Before admission the owner checks target authority,
argument readiness, observation fence, capabilities and code+state leases, and records `Admitted` (durably, when the
journal exists) before entering irreversible code. A second delivery while running returns the existing state; a
crash between started and completed leaves `Indeterminate`, never replayed. A late dependency after an effect:
suspend and resume if supported, else `Incomplete(ContinuationUnavailable)`; a non-suspendable native call is a
declared non-preemptible interval with a counter. **Pure physical retry** (today's discard-and-wait on a not-yet-
calculated read, `recalc.rs:9-17,611-614`, `read.rs:84-90`) keeps the evaluation ordinal and the draws, which is
today's random behaviour (`calc-engine/src/random.rs:4-8`); it is allowed only for cones without effectful callables.
An effectful cone takes one of the reasons above or waits.

P03 (O, 20527 safe mode) observed native status 64 followed by a second entry under ordinary and `$`
registrations in every reversed-area trial, with the counter increment before each coercion. `#` succeeded
without that unwind in the same tested scope; `xlfVolatile(false)` suppressed its clean F9 calls.
VBA Range objects can expose Empty before a second entry with 11, while Double arguments can receive 0
then 11. An expression argument can wait instead. This supports typed semantic reevaluation; it does not
prove that every VBA callback receives an Empty Variant or that every arrangement repeats.

Draw ledger: `DrawKey = (InvocationId, draw_ordinal)`; physical retries reuse recorded draws; a new pass or semantic
evaluation draws anew. Today's stream key `(seed, tick, cell)` (`random.rs:19-28`) is reused across spill re-waves,
which can manufacture a stable `SEQUENCE(RANDBETWEEN(1,1000))` that Excel reports as `#SPILL!` (D, MS24). Tests check
distinct keys, never numeric inequality.

### 3.4 Async completion

Token states `Pending -> Consumed | Revoked`, with a tombstone for duplicates. On a return the owner: (1) resolves the
adapter's opaque handle to the stored token (unknown = reject); (2) checks invocation, request, provider instance and
generation, workbook incarnation, target generation and locus, live authority; (3) checks the activation's
requirement version and argument certificate; (4) if valid, `Consumed`, records the result once, resumes the existing
continuation; (5) else `Rejected(reason)` with model, cache and activation unchanged and no replacement evaluation.
The first accepted return consumes the token even if later validation fails; identical duplicates are recorded,
conflicting ones are protocol violations. SliceV1 revokes live tokens on any live authored edit, overwrite, move,
undo, close/reopen, provider reload, cancel or supersession (conservative; P07 may relax it). A `PinnedSave` token
survives unrelated live edits. RTD updates and link refreshes are input events for later demand, never returns.

**P07 measured (O, 20527 safe mode).** Normal COM/UI completion has one prefix entry and a suffix after the
native return; this supports the observable no-prefix-repeat requirement, not a claim about Excel's internal
frame representation. Native ended/canceled events occur on the UI route, not the tested COM route. Ten UI Esc
and ten overwrite+Undo trials canceled before the provider returned: no suffix or value installation followed.
Undo restored identical formula text but editing also canceled calculation, so generation and cancellation are
not isolated. A zero native return status supplied no Boolean acknowledgement (output type zero); installation
must be measured separately. COM mutation entry time alone does not prove an edit happened before completion.
The registration-replacement attempt stalled; retain conservative identity checks for unresolved conditions.
See `calc-engine/tests/fixtures/excel-calc-probes/P07/README.md`.

Implementation update (dc-hsa5.4, 2026-11-01): the existing CalcFml continuation parks a
`UserFunctionStep::Awaiting { request }` without polling the provider. `HostState::pending_async`
returns the owner-issued token; an adapter retains it behind its opaque native handle and posts
`complete_async` to the owner. Delivery validates the whole token, live command, workbook, target,
callable generation, requirement and argument reads, then supplies the value without entering user
code. A later budgeted task resumes the same callable/frame and formula suffix. Dependents receive
an `AsyncDependents` pass. Tombstones distinguish identical and conflicting duplicate returns.
Every live edit conservatively revokes authority, including moves, overwrite/Undo and reload;
reopen has a fresh host namespace. Native and browser fixtures exercise the shared host route.
The worker projects waiting/return events and schedules no task while parked. Native XLL handle
integration and async pinned-save calculations remain open; the synchronous
entry returns a named refusal when it has no owner able to retain a wait.

## 4. Reads and freshness

`RequirementState { value_version, required, satisfied }`: locally clean iff `satisfied == required`; every
invalidation advances `required` even if already dirty; a result satisfies only the version it validated. Today a node
is pending or not (`calc-engine/src/state.rs:656-680`), so a second invalidation during a run cannot be told apart.

| Read | Value selected | Validation, invalidation |
|---|---|---|
| In-scope formula dependency | authored input or result certified for the required work | value, requirement, satisfaction, facets; reverse edge |
| Out-of-scope formula dependency | boundary cache pinned at pass start | historical entry validated, qualification carried; reverse edge kept |
| Cached VBA observation | cache/model at the invocation's pinned view | against that view; evidence only; no edge, no demand |
| Diagnostic snapshot | requested pin | no demand, effects or edges |

Constants are read from the pinned model even out of scope. `RegisteredWorksheetCallback` answers an uncalculated read
by the registration's capability (refuse, defer, or the XLL unwind of 3.3), never with VBA's privilege. A4's
`ExplicitFreshSnapshot` is not a read policy here: it is the save/export policy `RequireCalculatedScope` (8).

**FreshFor(K, T)** holds iff: T is in K's scope with its certified target identity; K's demand on T is met (a reusable
cache cannot meet a new force); every read obeyed K's policy and validates; every strict occupancy question in scope
is resolved; result and topology were installed atomically; the claim names K, pass, frontier and qualifications. It
does not claim precedents or the workbook current. Projections say e.g. "calculated for K7; cached input B1 dirty".

**Install** (single owner, no authored commit in between): check command and target authority; environment and
callable versions; dependency reads against the required view; cached reads against their pins; bindings, facts,
absences; spill reservation; requirement version still matching; then install value, status, topology and
invalidations atomically. A dependent may cite a producer proof in the same unit, else the producer must already be
installed. Failed validation installs nothing; an effectful attempt is never restarted by this path. Facts declare
`OnChange | OnNextAuthorizedCalculation | ObservationOnly`; today `set_workbook_facts` dirties nothing and relies on
CELL/INFO volatility (`state.rs:776-780`), while `set_environment` dirties every formula (`state.rs:782-788`, the 1a fix).

**Publication order** per subscriber stream: `AdmissionReceipt(K) <` each unit K causes; all required units `<`
successful `CompletionReceipt(K)`, which carries a barrier the mirror applies before exposing completion. A spill is
one unit including removals. The original path publishes at `finish`; the dc-hsa5.4 update below adds
nonterminal certified scalar units through that same host/worker projection route.

## 5. The VBA read view

- **SliceV1: serial observation fences.** Reference order is the command's dependency-compatible order fixed before
  interest changes; ties by saved chain rank, then workbook/sheet/address (a DNA policy, not Excel's chain). Pure ready
  work reorders by interest only inside a fence segment. An unrestricted callback is a fence: no installation from a
  later segment crosses it, and no earlier required installation is delayed past it for visibility.
- **What is pinned.** At callback admission, the whole immutable model and result root of the `CalculationDomain`
  (A4 3.3: every workbook the object model can reach), held across suspension. Not the argument references or the
  reads found so far: the read set is discovered inside user code.
- **No run-ahead in the slice.** A logical observation view that lets pure work run ahead and expose results
  selectively comes later (S decision 13) and must keep the same visible frontier.
- **Frame.** `InvocationFrame { id, parent, target, view: ReadView, lease: CallableLease /* code + project state */,
  mode, reentry }`. `Range.Value2` executes through the frame and cannot queue behind its own waiting activation;
  `WorksheetFunction` evaluates with the frame's context and issues no calculation command; worksheet-context writes and
  unauthorized nested commands are refused before effects.
- **Project-state lane.** A started observation-capable or effectful activation holds its project's state lane through
  suspension and async waits; other activations on that state wait; permitted nested callbacks follow the reentry
  policy; impermissible reentry is refused, never self-queued. Reload revokes result authority, keeps old leases only
  to drain, binds new invocations to the new generation.
- **Implemented 2026-11-01**: DNA VBA Range reads use an invocation-pinned cached model/result view;
  they neither demand producers nor add hidden dependency edges. Scalar argument readiness remains an ordinary
  dependency. The root evaluator retains supported typed VM/JIT activations across worker slices. A dependency or
  spill-placement wait discovered after an effect is a typed refusal until that continuation exists. Stored Ranges
  resolve workbook-owned identities through the admitted reference view, including edits, deletion and close.
  Explicit P03 reevaluation and synchronous nested callback entry are now supported as below.
  Async owner completion is described in 3.4.

P10 (O, 20527 safe mode) refines retained-reference authority: a Range tracks insert/delete-above/cut and union
areas, but deleting its cell or closing the owner makes its properties fail with error424 even though TypeName
still says Range and the variable is not Nothing. Reopen with the same name does not revive it; a new Hold call
must supply a new argument. Resolve tracked references through the invocation's qualified view/structural history,
and check workbook incarnation/continued membership rather than trusting the wrapper's type or old coordinates.
Module replacement resets globals and Static state and new calls use the changed signature. F9 can invoke a
zero-argument reader before Hold repopulates state, leaving a cached #REF! until a distinct forced evaluation.
That trace does not create a hidden dependency. The old suspended-frame lease policy and true native DLL
replacement remain DNA choices/unmeasured conditions, not conclusions from replacing an inactive VBA module.

## 6. Transactions, supersession, undo

P09 (O, 20527 safe mode) requires every ingress to retain formula form independently of the Formula getter's
text: the same SQRT range text gives scalar 2 through Formula and a dynamic four-cell result through Formula2.
Fixed CSE extent is separate from a dynamic spill; Copy Destination and FillDown retain their source form.
SavedAsArray is tri-state (the mixed modern-copy range returned Null before Full). All 200 Manual/Auto reopens
preserved the measured forms/flags/values. A saved modern spill error can use #VALUE! plus cm/vm rich-error
metadata while Value2 returns Error2045; file load must retain that metadata, not infer the class from one token.

- **Commit** (owner, one visibility transition): dedup and base check; apply membership, inputs, formulas and dialect,
  references, names, styles, merges, rules, geometry; fresh target generations for replaced, moved, deleted or restored
  formulas; advance direct and transitive requirements, fact and binding facets, pending spill reach and absence-query
  versions; publish revision and invalidation state together; keep the authored inverse. Today `dispatch` already
  prepares document and engine shares against one base and commits both (`command.rs:211-230,436-462`).
- **Edit during a live command K0**: commit; supersede K0; admit K1 (new frontier, predecessor, cause); revalidate K0's
  pure candidates against K1 (target, facets, inputs, callable and environment versions, occupancy); carry those that
  pass (keeping `computed_by`), discard the rest. SliceV1 salvages for dirty successors only; an inherited `ForceAll`
  re-evaluates pure targets; a started effect is never re-issued to discharge the same force
  (`Incomplete(ExplicitReevaluationRequired)`). A Manual `CalcRange` successor keeps the same scope, transformed.
- **Undo** prepares an inverse against the current revision and commits a new revision (today: `history.rs:1-12` for
  batch inverses, `command.rs:464-499` for structural restore, which rebuilds the engine with every formula pending).
  Restored formulas get fresh generations; undo restores authored intent, never draws, clock samples, RTD history,
  module state or effects, and never revives an async target.

## 7. Spills and occupancy

`ProvenAbsent` at a frontier needs: no authored occupant, no installed spill owner, no unresolved admitted producer
that may reach it (occupancy and producer-query versions recorded, absence watchers registered). A dirty anchor's
reach is southeast to the grid edge unless a validated bound narrows it; the scan is bounded by admitted anchors and
counted (`anchors_considered`, `readers_blocked`). In scope, a reader waits; a Manual command that excludes the anchor
reads `CachedAbsent` with the pending-producer qualification. A footprint change installs anchor outcome, shape,
values, ownership, removals, obstruction and reader invalidations as one unit. Conflicts: first established by
semantic reservation order, never completion time or interest (today first-established, `calc-engine/src/spill.rs:7-10`;
Excel's winner is P05).

P05 (O, 20527 safe mode) confirms reader-only cached absence during growth, fresh values when the
anchor/readers are included, and cleared children on shrink in the measured range. A blank future
child-only target did not expand the anchor; existing-child scope is not established by that result.
Conflict winners varied with entry history/MTC and sometimes between nominally identical MTC-on
trials. First-by-semantic-reservation stays a DNA policy, not a measured universal Excel winner.

The P05 child-scope supplement establishes that an existing child-only calculation also kept its
old ten-cell footprint during pending growth/shrink; F9 then updated it. Reader-first C10,E10,A1
scope finished at 11, but the E10 UDF was invoked with 1 then 11 in every trial. These are separate
semantic evaluations, not an invisible physical retry. The in-scope wait-before-reader policy
above is therefore stricter than this observed Excel trace; P05 does not certify its callback
ordering. A matching profile needs a typed provisional evaluation rather than merging those calls.

## 8. Save

`SaveRequest { id, workbook, model: ModelPin, base_results, inputs, cache_policy, format }` ->
`SaveImage { model, results: ResultViewPin, cache_manifest }`; the serializer never walks a mutable result map.
`CachePolicy::{PreservePermittedCaches, RequireCalculatedScope(scope), DropStaleCaches}`.

- `DropStaleCaches` remains the explicit current-only policy: a formula not in the stale set is written with its calculated value,
  every other formula without `<v>` (`calc-host/src/save.rs:46-103`), plus `fullCalcOnLoad` when an array lacks one
  (`calc-doc/src/save/author.rs:712-723`). Excel recalculates those on open (1). Honest, but it makes Excel call a UDF
  on open that Excel itself might not have called.
- `PreservePermittedCaches` is selected by P06 (O, 20527 safe mode) for ordinary Manual Save(false): old
  caches of the same formula instance survive with ca=1, calcCompleted=0 and calcOnSave=0. Do not infer freshness
  from absent flags: disabled-sheet cases can keep stale caches without ca or calcCompleted. Flags follow the
  measured file profile; they do not create a producing command or certificate.
- `CalculateBeforeSave` becomes a `CalculationDomain` setting (absent today; only the xlsb converter reads `calcOnSave`,
  `calc-doc/src/xlsb/workbook.rs:562`). True: admit a `PinnedSave` calculation of the workbook's dirty/volatile demand,
  pin its final view, serialize with the original model pin; waiting on pending async; cancel or failure gives an
  explicit save outcome, never a silent downgrade. A live in-flight effectful invocation the save needs is waited for,
  not shared; if it is superseded the save is `Incomplete`.
- P06 requires these normalization qualifications: Save/SaveAs admits BeforeSave edits before the serialization
  model pin; SaveCopyAs skipped those handlers. Ordinary Manual true updated live caches in the saved workbook
  and a dirty peer, so Excel's observed calculation scope exceeds one workbook. With the active sheet disabled,
  true left both sheets stale; activating an enabled sheet let it calculate while the disabled one stayed stale.
  In Automatic Except Tables, Save recalculated a stale TABLE even with CalculateBeforeSave=false. The existing
  PinnedSave-only isolation/destination rule above remains a DNA strategy; it does not by itself describe Excel's
  observed live cache updates. A public Excel command profile must account for those updates without duplicating
  effectful evaluations. Pending-async waiting and concurrent-edit pinning remain separate measurements.
- P07 adds a measured UI Manual Save(false) case: Ctrl+S canceled the pending async calculation in ten trials.
  Live C1/D1 became Error2051, but both the immediate saved file and a later SaveCopyAs kept the last successful
  numeric caches 41/42 with the edited input 201..210, ca=1 and calcCompleted=0. A permitted persistent cache
  can therefore differ from the live error display; retain its historical origin and unmet requirement. This
  does not establish non-canceling save of a pending continuation, Save(true) waiting, or concurrent-edit pinning.
- Reopen imports `<v>` as `UnverifiedFromFile` with a new incarnation; no command, certificate or effect resumes.

## 9. Scheduling and finite work

Serial stepping executor: `advance(budget) -> Progress | Yield(checkpoint) | Waiting(reason) | Terminal`; budget checks
inside long kernels and VM/JIT code, not only between cells (today the budget counts evaluations,
`calc-engine/src/recalc.rs:214-229`). Within a fence segment: interest priority with an age escape (after seven
interest-preferred quanta the oldest off-interest continuation runs; ties by reference order); promoting a target
promotes its prerequisite cone, never extra scope. Exact kernels yield only at binary64 store points or with full x87
state. The browser driver returns to a real worker task between slices (a resolved-promise chain does not).

Implementation update (dc-hsa5.4, 2026-11-01): root instructions and SEQUENCE kernels retain
continuations across worker tasks. First-wave interest promotes ready prerequisite cones inside fixed
callback fence segments, with an eighth-quantum age escape. Pure volatile scalars
retain their pass and draws when reordered. Volatile arrays, iteration/table sessions and later repair
waves retain reference order. Competing spills retain semantic reservation order.
The host requests installation handoffs before a later fence can enter: completed prefix values,
spill ownership and removals publish together. An unprovable prefix stops before the callback.
Headless callers enabling those handoffs must drain `publish_ready` between `step` calls.
The callable runtime now supplies checked slot/generation bindings, invocation identities and pinned
cached/reference views. Root typed Long/Double and DNA numeric Range calls retain owning VM/JIT activations;
counted native tests demonstrate suspension and reload without repeating the prefix. Variant adapters
retain an explicit synchronous interval. Lazy argument blocks, defined names and direct LAMBDA
bodies now retain their cursors, lexical frames and async activations across slices.
Reload revokes command publication authority and rebinds compiled consumers whose callable watches
changed. Unresolved compilation refusals are retried conservatively when registrations change.
P03 now records registration-authorized XLL unwind and provisional scalar-argument evaluations
with distinct ordinals/reasons and prior call records. A provisional body retains its activation
across physical safepoints and finishes before the next semantic evaluation is admitted. The
producer and surviving fences install before that next cached view. Nested synchronous callbacks
inherit the invocation frame/pin and file positive/negative callable watches; an occupied state
lane requires explicit reentry permission. An actual Value2 read of an explicit Range argument
can now identify an admitted unfinished producer while preserving the pinned cached read. It
requests a fresh semantic evaluation after that producer; merely passing or retaining an unused
Range does not. The synthetic Host/worker controls use a reference returned by INDIRECT so the
producer is discovered late: the VBA Range remains an object, observes Empty then 11, and enters
twice; the unused Range and scalar-expression controls enter once. A retained spill-child control
observes 2 then 21 after its producer's input changes; the query includes published and complete
imported array footprints before the producer stages again. This establishes those routes,
not parity for every area ordering. Hidden stored-Range reads keep P01's observer-only policy:
they add no persistent value edge and admit no new producer. A successful read can request a
fresh entry around an already admitted independent producer; the existing graph closure excludes
the caller and its known consumers, preventing artificial cycles. The cached view explicitly
carries its executing caller, whose self observation is Empty even with array metadata or a prior
published value. Seven shared controls cover these rules through VM/JIT and both budgets, with
both plain Range and explicit Excel.Range types. The Excel-compatible projection has a distinct
DNA nominal interface identity before the numeric-only DnaCalcObjects.Range reference, so stored
Value2 retains Empty. Original Empty counters pass VM/JIT and finite/unlimited budgets.
Nested P07 waits now retain lazy arguments, defined-name and direct LAMBDA frames; sequential
sites keep distinct tokens and one root activation. Lazy selection uses the existing catalogue
kernels over retained arguments, without replaying argument effects or random draws. The helper
kernels MAP, REDUCE, SCAN, BYROW, BYCOL and MAKEARRAY retain their item cursor, accumulator,
completed values and current LAMBDA child. Nested helpers can wait on successive async requests
without replaying an earlier body. Synchronous user callbacks still refuse waits whose caller
cannot retain them.
Post-effect pure read instructions retain their operands until an admitted prerequisite finishes;
placement retains the completed array. An independent pure prerequisite cone can enter the
waiting segment while the callback's original cached view stays pinned. An effectful prerequisite
across a later observation fence remains an explicit refusal. Physical continuation never creates
a new semantic ordinal. Failed reads do not become consumed-value certificates.

P01/P03 range profiles (dc-hsa5.4, 2026-11-02): requested area order breaks ties among independent
targets. Direct reference arguments of callbacks which explicitly permit provisional entries keep
their invalidation edges, while their producers may follow them in the requested range order.
Expression arguments retain dependency order. In the measured reversed scalar case an admitted
unfinished independent producer exposes Empty, even with a warm held cache; its later installation
precedes a new semantic entry observing 11. Observer-only scope retains the old dirty cache 2.
Complete array images retain their P05 child values. Warm, contiguous and reversed controls run
through HostSession.
P08's seeded Full profile orders independent pure work before cached-read callbacks, while keeping
the callbacks' reference order and retaining other callback barriers. Iteration and TABLE sessions
keep their existing order. Two 1000-cell chains, seeded-cold and warm, produce the recorded
1010/1200/1010 observations once per observer with finite and unlimited budgets. H1 is a pure
formula mirror in this control; it establishes no native XPure capability. Rebuild has no separate
public normalization yet. Broader chain-history and multithreaded order claims remain outside
this profile; callbacks are never declared interchangeable.
P07's owner boundary is in 3.4.
RTD events use a separate provider/subscription sequence and immutable input snapshot. They mark
provider consumers required for later demand, never supply a formula result or resume an async
activation. Manual mode waits for a calculation command. Duplicates/stale/conflicting sequences,
retired subscriptions and old provider generations cannot change the current snapshot. A new
input event conservatively supersedes live work at the next owner step; input reads carry
subscription versions and commands retain their admitted cut. Native XLL/RTD adapters still need
to connect to these owner APIs; this is not evidence of a native RTD server running.
Live range scopes follow cut/paste and cell shifts through the product command and Undo/Redo paths,
as well as axis/sheet edits. Overwritten cut destinations and deleted locations leave the command;
Undo never revives them. Disjoint surviving pieces stay disjoint, including cross-sheet moves.
P05 boundary reads pin a calculated spill or a complete imported array image, including child values
and `#` shape-only reads. Incomplete imported arrays are refused. Imported values retain SourcePart
lineage, never a producing invocation. Blank reads with unresolved producers only outside the
command retain cached-absence qualifications; strict in-scope producers still require resolution.
Scoped completion does not establish workbook freshness. The application's excluded D1 callback
can therefore still keep E/G conservatively stale for the workbook.
P04 volatile dynamic resizing compares the held or preceding valid footprint and starts a new sizing
pass when it changes, with fresh draws and at most eight sizing evaluations per anchor. A repeated
size settles earlier; an unstable size publishes #SPILL! without a footprint. Error elements do not
invalidate an array's shape. Nonvolatile resizing runs once; the separate graph-repair bound stays 32.
Deterministic tests exercise finite and unlimited budgets. The VBA witness alternates sizes under
Application.Volatile and demonstrates eight entries versus one for the nonvolatile control, in VM and JIT.
Owned save plans pin installed prefix values while another cell remains suspended. Ordinary Manual
XML Save(false) preserves permitted scalar caches with ca=1 and calcCompleted=0/calcOnSave=0.
Re-entry never inherits an orphaned cache; dirty array topology remains excluded from this save policy.
CurrentCalculatedOnly stays an explicit option.

Calculated-save implementation (dc-hsa5.4, 2026-11-01): `HostSession::prepare_calculated_save`
pins one workbook model and runs the ordinary engine against a separate `PinnedSave` result
destination. The snapshot contains no node ID allocator. Live and save calculations share unique
tick/result-view sequences; admission, completion, async tokens and result stamps name their
destination. Only the save writer reads that private result view. An existing live invocation is
waited for rather than copied or entered again; replacement, incomplete coverage or cancellation
refuses the image. A private save keeps its original model through live edits, holds the callable
state lane while unfinished, and resumes an accepted async return only on a later budgeted step.
Its completed return tombstones pass back to the workbook owner. Reload/close/provider replacement
revokes unfinished work. The worker exposes `SaveCalculated`, `PollSave` and `CancelSave` through
the existing command journal; successful bytes name the saved model revision, so an older image
does not clear later live edits. Ordinary application Save remains Save(false).

Tests cover nested async save waits, existing-live waits, duplicate delivery and command replay,
cancel/reload/close, pinned bytes and Manual reopen as unverified file caches. A later RTD input
does not enter an already admitted save: the retained async suffix reads the save's original
input cut, while the live workbook's next command reads the newer input. Provider identity
matches the admitted callable generation, including after replacement. This implements the
DNA isolated-workbook save strategy. P07 does not yet measure concurrent-save pinning or
non-canceling Save(true) waits in Excel. Existing stored Range handles resolve from
their pinned view, but creating a new handle after a live structural edit can refuse with
`ExpiredView`; that save fails explicitly rather than inventing a calculated cache.

P06 command profiles (dc-hsa5.4, 2026-11-03): `HostSession::prepare_save_with_context` accepts
the operation, CalculateBeforeSave, active sheet, disabled sheets and an optional registered
BeforeSave owner. Save/SaveAs commit its returned authored batch before the serialization pin;
SaveCopyAs skips that owner. Cancel retains any committed handler edit and produces no image.
With a disabled active sheet calculation is suppressed; with an enabled active sheet only the
enabled sheets calculate. Disabled scalar caches retain their historical origins and conservative
dirty flags. This does not reproduce the absent ca/calcCompleted flags in every Excel disabled
file. Loading a VBA project does not register an event owner automatically.

Ordinary application Save(false) now follows the semiautomatic TABLE exception: it admits the
TABLE outputs and their existing dependent cone through the live calculation route before pinning.
Unrelated volatile callbacks stay outside that command. The same writer updates the TABLE master and child caches,
retaining the formula/attributes and adding a cache cell the source omitted. Save/reopen controls
retain 20/30 as file caches, never as calculated provenance. TABLE sessions still lack SliceV1
certificates: their attached receipt remains Incomplete with its uncertified count, and the file
does not claim calcCompleted. An admitted refusal, held circular value, missing result, provider wait or
failed calculation prevents this command from producing an image. The existing isolated
calculated-save operation continues to require its stronger completion and async protocol.
Cross-workbook Save(true) coordination, automatic VBA event registration and cooperative ordinary
save calculations remain open. When new save calculation is required, the ordinary synchronous
save API refuses an existing active calculation rather than copying its effects.


Defined-name implementation update (dc-hsa5.5, 2026-11-01): the canonical `NameTable` stores
non-cell definition participants by stable `NameId`, scoped folded key and binding version,
with shared record and lookup pages. Formula plans bind those identifiers and keep the namespace
probes that selected them, including misses before workbook fallback. Definition edits rebind
only those users and their downstream cells; unrelated plans, results and requirements survive.
Runtime INDIRECT lookups add executed name watches and contextual alias edges to the same graph.
Deletion and restoration never revive a removed name identity or an earlier binding version.
The existing ordered `names()` view remains available for file writers and materializes on request;
ordinary edits, binding and product authoring use the indexed table and a sparse pending-name preview.

Name history follow-up (dc-hsa5.6, 2026-11-01): Undo retains complete scoped record groups,
including imported duplicate fallbacks and their order, through the existing authored batch route.
Recreated rows receive fresh live identifiers and retain qualified source-record origins. This lets
an earlier edit Undo target the recreated record and lets the existing XML writer retain raw attributes
without confusing equal duplicate formulas. Ordinary update inverses use indexed lookup; deletion
order capture and restoration scan the name order. Scoped consumers rebind without rebuilding
unrelated formulas. NameView commands, P12 cross-scope Excel measurement, memo eligibility and
plan eviction remain open.

A definition node has no synthetic cell or shared cached value. Its existing evaluator block
returns a reference or value per caller; unused names do not evaluate, and repeated volatile uses
remain separate. Local definitions select their sheet for unqualified references and nested names;
workbook aliases inherit the current use context. Static and INDIRECT routes test the same policy;
new Excel parity for nested cross-scope aliases awaits P12. Per-name invocation identities,
`NameView` evaluation commands and transitive eligibility for any shared memo remain open.

Name operations (dc-hsa5.6 / dc-971p.34, 2026-11-03): the canonical compiler now carries
`NameEvaluationContext` with the caller, current definition identity and definition worksheet.
Optional `NameAnalysis` records source occurrences outside interned semantic plans; copied plans
still share while their source evidence retains each caller. This makes the existing nested-scope
policy explicit; it supplies no new P12 Excel measurement.
`EngineState::prepare_name_change` and the existing host/worker `NameEdit` command rename and move
ordinary names in one authored/history batch. They rewrite compiler-bound cell, name and
conditional-rule occurrences, retain live IDs and source origins across Undo/Redo, preserve array
extents, and preview the complete changed name table during formula-file conversion. Local
variables, other scoped identities and string literals keep their text. A candidate must preserve
name bindings and placed references in every checked caller context before anything commits.
Analysis stops at 100,000 source/context pairs. Ambiguous nested aliases, changed reference
contexts, macro names and table metadata return explicit refusals; host validation/chart sources
that cannot be rewritten also refuse. A rename/move and a definition edit are separate commands.
These controls cover the public worker route and saved/reopened names, duplicate fallbacks and
raw imported attributes. Shared name-value memoization and plan eviction remain open.

## 10. First slice: the eight demonstrations and their oracles

Fixture, Manual mode, independent clone per demo: A1=1, B1=`=A1+1` (2), C1=`=B1*2` (4), D1=`=PeekB1()` (2; typed,
nonvolatile, observation-capable), F1=1, E1=`=SEQUENCE(F1)` (dynamic; [1]), G10=`=E10+1` (1). Baseline certified by
explicit calculation or a trusted harness snapshot, not by file caches. Pure scope S = {B1,C1,E1,G10} as an `AreaSet`.
Oracles are partial orders over trace events (`Admit, PassStart, Read, Candidate, Validate, Reject, Reuse, Yield,
WorkerTask, Resume, CommitEdit, Invalidate, EffectStart, EffectComplete, AwaitAsync, ReturnAccepted, ReturnRejected,
Install/Publish, Complete, SavePin, SaveImage, SaveWritten`) plus forbidden events, never one scheduler log.

| # | Operations | Oracle (`<` = happens-before) and final state |
|---|---|---|
| 1 Real yield | A1:=2, F1:=10; `CalcRange(S)` via UI/worker; small budget; SEQUENCE checkpoint after 3 of 10; Ping queued | `Yield(E,cursor=3) < WorkerTask(Ping) < Resume(E,cursor=3)`; same invocation, pass, locals, view; `Install(E1:E10) < Complete(K)`; no subscriber sees partial E. B1=3, C1=6, E1:E10=1..10, G10=11, D1=2 |
| 2 Interest reorders | A1:=2, F1:=2; S; suspend E after 1 element; interest -> C1 | `Interest(C1) < Install(B1=3) < Publish(C1=6) < Resume(E)` within fewer than seven service decisions (asserted); same scope; no D1 invocation; final B1=3, C1=6, E1:E2=[1,2], G10=1 in both runs |
| 3 Early publish | as 2, C1 prioritized | `AdmissionReceipt < Install(B1) < Publish(C1) < Install(E1:E2) < CompletionReceipt(Complete)`; C1 stamp names K/P; replaying `CalcRange(C1)` with equal output still emits a status change |
| 4 Edit vs work | Automatic; A1:=10, F1:=2 -> K0; hold candidates B1=11, E1=[1,2]; commit A1:=20 between slices | `CommitEdit < Superseded(K0,by=K1) < Admit(K1)`; `Reject(old B1)`; `Reuse(old E1, certified_for=K1)` with E1 kernel count 1 across K0+K1 (footprint revalidated); old C1=4 proof fails on B1's requirement although B1's value version is unchanged. Final A1=20, B1=21, C1=42, E1:E2=[1,2], G10=1, D1=2; K0 never `Complete` |
| 5 Cached read, suspension | A: A1:=10, calc D1 alone; then B1, then D1. B: `CountAndPeekB1`, suspend after increment. C: native variant, cancel queued. D: force {D1,B1,C1}, rank D1<B1<C1, interest C1 | A: `Read(I1,B1,CachedMacroObservation,2,dirty)`, D1=2, no B1 evaluation, no D1->B1 edge; B1=11; D1=11 with a new identity. B: one EffectStart, Calls=1, `Yield(I,pc) < WorkerTask < Resume(I,pc)`, D1=2, in VM3 and JIT (a cell-boundary yield fails). C: one entry, one non-preemptible interval, never re-entered. D: `Resume(I) < Read(B1=2) < EffectComplete < Install(B1=11) < Publish(C1=22)`; reversing interest changes nothing |
| 6 Narrow Manual | A1:=10; calc C1; calc B1; calc C1 | `Read(C1,B1,BoundaryCache,2)`, `Publish(C1=4, qualified_by=dirty B1)`, `Complete(scope={C1})`; no B1/D1/E1/G10 evaluation; then B1=11, C1=22; diagnostics cause no calculation. Under `RangePostState::StillRequired` (P02) C1 also stays required |
| 7 Pending spill | F1:=10; calc {E1,G10}, G10 first | `CommitEdit(F1)` includes pending producer E1 and G10 invalidation; `BlockedOnOccupancy(G10,E1)` or `Wait(PotentialSpill)`; `InstallAtomic(E1:E10) < Read(G10,E10,10) < Publish(G10=11)`; never G10=1 on `ProvenAbsent`. Companions: shrink to 1 removes E2:E10 atomically then G10=1 on a new proof; G10 alone -> 1 with `CachedAbsent`; authored E10 -> `#SPILL!`, E10 untouched |
| 8 Save(false) pin | A1:=10; save with the chosen policy; pin R1; A1:=100 at R2; finish | `SavePin(R1) < CommitEdit(R2) < SaveWritten(R1)`; zero invocations from save; file has A1=10, no candidate, no A1=100; B1/C1 per policy (preserve: 2/4 with `ca`, `calcCompleted=false`; drop: no `<v>`); D1=2 and G10=1 kept. Reopen in Manual: new incarnation, no resumed effects; preserve keeps B1:C1 at 2,4 until an explicit calculation produces 11,22. The drop policy must measure its separate open behavior. Save(true) companion: B1=11, C1=22, D1 cached 2; cancelled save calculation writes no success |

**Async sentinel** (after the eight; deterministic provider, `D1 = CountThenAsync(A1)`, counter before suspension):
valid return accepted once (D1=41, dependent C1=D1+1=42 in the `AsyncDependents` pass, counter 1); rejected with
model, cache, activation and counter unchanged for: overwrite (g1 -> g2), undo restoring identical text (g3 != g1),
provider reload (p1 -> p2), argument edit with unchanged formula generation, move (and undo), cancel/supersede, and
close/reopen (incarnation); duplicate identical return recorded, conflicting one a protocol violation; an RTD/link
event after K's cut does not reject an admitted return. No rejection launches a new evaluation.

**Done when** all eight pass through the shared headless engine and the real UI/worker route where named, the VM3/JIT
continuation sentinel passes on its native target, stale async returns are rejected, and save byte and reopen checks
pass. A platform without the promised safepoint reports that item incomplete. Lanes (A7 14): A engine/read/publication
(demos 2-4, 6-7) alone certifies results; B host/transactions/worker/save (1, 3, 8) alone advances the model and
admission sequence; C callable/effects/async (5, sentinel) never publishes or infers freshness.

## 11. Tests

**Per commit** (inside `cargo xtask check`, deterministic, small): an independent executable reference model for
integer acyclic formulas with scoped boundary caches, one spill and scripted effect/async events, which computes
expected outcomes from its own semantics (never the production validator), a fixed seed set of at most 8 formula
targets and 20 operations per trace, plus the named fixtures. Properties: command dedup; yield equivalence at every
checkpoint; pure priority equivalence inside fence segments; invalidation sensitivity (value, requirement,
satisfaction, occupancy query, namespace miss, fact, callable version each alone); repeated dirtiness (two
invalidations, two requirement steps); historical observation integrity; invocation uniqueness and enter-once; draw
keys (two RAND keys, LET one, retries reuse, new passes do not); atomic authored visibility incl. undo; spill
growth/shrink 1..10 with obstructions; facet-aware change (equal numbers, changed text/kind/shape); publication and
receipt order; async rejection immutability; scope honesty; save pinning (Save(false) has no invocation events);
finite-work fairness. A small crash fixture (before admission, after started record, after effect before outcome)
never replays a started effect. A trace comparison may ignore publication order but never sorts away effect order.

**Scheduled** (on a schedule or when the area changes; never something a commit waits on): bounded interleaving
exploration around read, safepoint, validation, commit, observation admission, spill reservation, publication, save
pin, cancel, return, reload; long generated edit/undo/calc/save/reopen histories with deterministic shrinking to a
workbook plus operation trace; the Excel probes (EXCEL_CALC_PROBES.md); spill-absence scale (geometric pending
anchors: anchors examined, readers blocked, time to first qualified current result); RTD/link bursts against finite
frontiers; slow subscribers and fragmented units; provider failures and lease reclamation; Exact-kernel continuation
witnesses on supported CPUs. Failure records hold workbook, operations, admitted inputs, profile, decisions,
generations, views, draw material, effect outcomes and platform provenance; a seed alone is not a replay.

## 12. Disagreements and proposed resolutions

| Topic | Positions | Resolution |
|---|---|---|
| Receipt timing | S: one receipt precedes values; A7: admission vs completion; A4: completion with `remaining` | A7's two receipts; A4's `remaining` is A7's `coverage` |
| Native unwind/reinvoke | A7: nothing re-enters an entry point; A4: XLL `xlretUncalced` unwinds and is called again (D) | allowed only as `ReevaluationReason::XllUncalcedUnwind` with a new ordinal and recorded prefix effects |
| Pure retry after a late read | code: discard and re-run, same draws (`recalc.rs:9-17`, `random.rs:4-8`); A7: retry keeps identity | keep as physical retry for effect-free cones only; effectful cones suspend, wait or take a typed reason |
| Draws across spill passes | code: same stream every wave, cap 32 unmeasured; A4/A7: new pass, new draws | `SpillShape` passes get new keys; cap and stabilization rule from P04; refusal counted meanwhile |
| Occurrence identity | A7 `u64`; A4 `OccurrencePath` | path (MAP/LAMBDA elements, recursion) |
| Pass vs phase | A7 pass with reason; A4 `CalcPhaseId` kind | pass carries `PassReason` incl. sizing, sweep, scenario |
| Read policies | S/A7 four; A4 adds XLL worksheet/macro-equivalent and `ExplicitFreshSnapshot` | XLL variants are `RegisteredWorksheetCallback { registration }` capabilities (`#` gets macro-equivalent reads); fresh snapshot is a save/export policy |
| Fence extent | A7: pinned root; A4: everything the callback may observe, other workbooks included | pin the `CalculationDomain` root |
| Cached-read evidence | S/A7 cite a measured stale read | P01 (O, 20527 safe mode) confirms old dirty-formula cache for observer-only calculation; reversed-area re-entry sees Empty then the new value |
| Range.Calculate end state | A7: target satisfied; fixture hints it stays dirty | P02 isolated Manual trials: `StillRequired`; clean unrelated counter did not run |
| Edit during calculation | code: cancel all; A7: successor plus revalidated reuse | A7 (demo 4) |
| Save(false) caches | code: drop stale; A7: preserve with flags | P06 selects preserve for ordinary Manual false; flags are not freshness proofs, and semiautomatic TABLE save can calculate despite false |
| Iteration order | A4: do not promote sheet-name order; code: measured on 20430/20527 | keep measured rules (O) for the measured shapes; P14 for the rest |
| Atomic SCC/table publication | S/A7 imply session units; A4: a strategy, observable boundary unmeasured | sessions may yield internally; external atomicity from P08/P14/P15 |

## 13. Open pending Excel measurements

SliceV1 fixes each provisionally; an Excel-parity claim waits for the probe and a versioned profile change.

| Open question | SliceV1 choice | Probe |
|---|---|---|
| What a VBA read of a dirty formula returns, per command kind; self/caller reads | P01: observer-only reads old 2; reversed areas can return Empty then re-enter with 11; self 0/Empty. Retain cached observation, distinguish new evaluation; other contexts remain open | P01 (O, 20527 safe mode) |
| Out-of-range precedents in Range/Sheet calculation; post-command dirtiness; mode dependence | P02 confirms boundary caches and Manual `StillRequired`; row-major separate; automatic edits calculate before explicit commands. UI sheet grouping remains open | P02 (O, 20527 safe mode) |
| Repeated semantic evaluation: VBA provisional calls, XLL unwind, registration privileges | P03 confirms status-64 unwind/re-entry for ordinary/$, successful # coercion, and provisional VBA values; preserve typed reasons and registration capabilities. Null not observed | P03 (O, 20527 safe mode) |
| Draw scope (RAND pair, LET, name, LAMBDA, MAP); spill-size pass count and stabilization | P04 confirms reuse/occurrence boundaries and eight calls before #SPILL! for the volatile sizing path; nonvolatile forced controls use one call and clean F9 none. Held footprint/repeated size determine earlier completion. Global graph-wave count remains open | P04 (O, 20527 safe mode) |
| Spill conflict winner, stale-footprint clearing, child-only calculation | P05 confirms cached absence, shrink clearing and no scope expansion from an existing child; reader-first callbacks see 1 then 11. Conflict winners vary; strict wait/semantic reservation remain DNA policies | P05 (O, 20527 safe mode) |
| Save scope, stale `<v>` with Save(false), open-time recalculation | P06 selects Manual preserve, domain-wide live updates for ordinary true, handler-before-pin order, disabled/TABLE exceptions; first-source Manual retains 2/3 and synthetic Auto refreshes 10/11 in ten fresh processes each. P07 UI false cancels and saves held caches despite a live error; true with async and concurrent pinning remain open | P06/P07 (O, 20527 safe mode) |
| Stale async returns Excel accepts; overlap of calls from one cell | P07 confirms no repeated prefix and no late installation after UI cancel/overwrite+Undo; Undo is confounded by cancellation. COM overlap and real provider replacement remain open; keep conservative revocation | P07 (O, 20527 safe mode) |
| Whether Excel's observable intermediate caches depend on chain history or position | P08 Full/Rebuild reads were new in all 200 cases, but observer order varied in multiworker trials; off/one-worker order was stable. Keep explicit observation order and serial fences; seeded history does not establish general commutation | P08 (O, 20527 safe mode) |
| Formula vs Formula2 through every ingress | P09 confirms form in semantic identity across ten setter/copy/fill/CSE paths and 200 reopens; preserve fixed extent, @, dynamic/rich metadata and tri-state SavedAsArray. Other clipboard/import transports remain open | P09 (O, 20527 safe mode) |
| Module/provider reload and stored Range across edits | P10 confirms tracked structural references, invalidity after deletion/close without same-name revival, and fresh module state/signature after replacement. Keep incarnation/code/state qualification; native pending replacement and DNA suspended-frame behavior remain open | P10 (O, 20527 safe mode) |
| Iteration remainder, data tables, entry-time evaluation, names, events | named unsupported or explicit setup | P12-P15, P22 |
