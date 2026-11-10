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
  `fullCalcOnLoad` (`calc-doc/src/save/author.rs:903-916`). This decides what a preserved stale cache means (8).

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
| Range reads are recorded as spans or versioned chunks; no per-element certificate entry | `EvalLocal.footprint: Footprint`, one entry per distinct area read in first-read order (`recalc.rs`, dc-hsa5.19) |
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

Iterative owner steps (dc-hsa5.11, 2026-11-03): the existing iteration session now
retains its final stopping decision across a yield at the last cell of a sweep.
Previously a finite budget could publish after one sweep while an unlimited
budget performed all required sweeps. Staged cyclic values stay private until
the session finishes; that publication boundary is a DNA policy pending P14,
not an observation of Excel's intermediate caches. Current stopping compares
the largest evaluated scalar change strictly below MaxChange, including
downstream readers, and limits ordinary iteration sweeps by MaxIterations.
Existing spill/dependency repair waves keep
their separate bound and reopen the stopping decision; their evaluations are
not evidence of Excel's iteration count. The completed eng9 delivery under
`calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103` retains 692 pure
trials over 86 conditions, at least three each; workbook trials share processes
within the recorded groups. The host replays 76 non-range inputs at unlimited
and one-evaluation budgets: 507 command/value observations, 1014 comparisons,
all agreeing. Rebuild's threshold equality is strict; downstream amplifiers can
keep the scalar sweep running. A dynamic read joining an existing cycle uses the
same Tarjan analysis and cyclic read seeds as static ordering. Text/error outputs
do not drive another scalar sweep; a numeric value after such a seed compares
with zero. This reproduces the recorded B6 outputs without measuring individual
SCC execution counts. All ten cyclic Range conditions retain their seeds in
Excel. DNA retains typed dc-hsa5.11 refusals: a scope containing only known
cyclic producers refuses at admission; pure readers can be admitted and then
refused by their executed value reads, as described below. The 80 distinct
command/value observations remain nonpasses (160 slots at two budgets), and
retained caches are never counted as calculated. Mixed/dynamic range variants,
other threshold commands, observer counts, mid-cycle caches, mixed topology and
cross-workbook behavior remain open. The recorded
dc_86f and dc_10y_iteration first-calculation values cover order, held seeds and
the recorded limits; their reopen observations describe later calculations.
Cross-workbook cyclic requests retain their named refusal before admission.

Manual mixed scalar Range (dc-hsa5.11.2, 2026-11-04): twelve eng10 conditions
retain cyclic targets and their readers while independent D1=2+2 calculates 4,
for zero/asymmetric seeds and limits 1, 2 and 5. The exact input/result packet is
under P14/eng10-20261104. DNA admits this partial command with StillRequired,
emits TargetsRefused naming dc-hsa5.11, and completes Incomplete. Cyclic targets
keep their old results and unmet requirements; those retained values never
count as calculated. The eng17 extension below replaces conservative reader
closure with executed-path refusal. Independent results name this command's
producing invocation. Value reads cannot consume an unresolved selected cyclic
target. Cancellation and supersession retain the discovered refusal counts,
and a narrow completion preserves known cycle demand
outside its scope. Automatic mixed requests, callbacks, tables, multi-cell
arrays/spills and newly discovered cyclic Range dependencies remain refused.
These final-value observations do not settle SCC execution counts or mid-cycle
caches. The original pure cyclic Range conditions remain nonpasses.
A typed refusal during an admitted mixed command fails it before replacing a
warm result: a missing outside cache can stop a reader before its own cycle is
discovered. A later authorized workbook calculation retains that seed. This is a
DNA integrity control, without a new Excel observation.

Nonnumeric stopping (dc-hsa5.11.1, 2026-11-04): the eng10 fresh-process
delivery distinguishes logical outputs from numeric stopping samples. A NOT
self-reference beside a numeric 0.125 increment and its downstream reader does
not keep a sweep running at MaxChange just below or equal to 1. An all-error
self-reference at MaxChange zero stops with the first command's recorded #N/A,
including MaxIterations 2 and 5; a numeric peer continues to drive the sweep.
The engine now distinguishes no numeric sample from a numeric zero change.
The latter retains the strict threshold rule. Fifteen unchanged authored inputs
give 180 HostSession final-value comparisons at two budgets. These observations
do not settle general SCC execution counts or intermediate cache visibility;
iterative results retain their existing uncertified publication boundary.

Eng14 pure observations and eng17 implementation (2026-11-05): the unchanged
P14/eng14-20261104 import verifies 3,450 delivered files and 150 successful fresh
owned-process trials over 15 conditions, ten each. Manual, iteration on, MTC off
and events off were checked with exact seeds/formulas before each pair of
commands. The 300 commands give 580 cell observations. Seven optional-argument
failures, 143 deferred first attempts and ten shape-form preparation refusals
remain recorded; successful v3 alone supplies the shape condition.

For the measured pure scalar Manual Range profile, only known SCC members are
initially refused. A reader that actually requests a refused value leaves the
schedule without a staged result or a change to its historical value, identity,
dynamic edges or unmet requirement. It is not declared cyclic merely for reading
one. Its waiters resume and meet the same value-read refusal. TargetsRefused may
arrive in several batches, with each target named once; required coverage stays
constant as discovered readers move from scheduled to refused. Sparse/dense
scratch, cancellation before/after discovery, warm identities and topology are
tested through HostSession and the worker.

The original untaken IF(TRUE,7,A1) reader now evaluates 7; a dynamic value reader
retains its seed while independent D1 evaluates 4. ROWS(A1#) can use the known
selected scalar's actual admission-time cache image to evaluate 1. That shape
read retains the pinned view, dirty/imported SourcePart ancestry and external
flag. Value reads, including SUM/INDEX and a value read after ROWS, remain refused
through IFERROR. A missing or array image grants no scalar shape. Manual
StillRequired and cache-qualified projections remain stale; display equality
is not a fresh pass. Pure hidden-self readers retain their seeds, so a later
Rebuild still starts from the original values. A closed chain of actual value
waits can now establish a newly discovered scalar cycle in the same first-wave
Manual profile: its producers retain their seeds, executed readers are excluded,
and independent D1 evaluates Excel's recorded 4. Only the proven producers join
the cyclic set. The shared exclusion releases waiters without changing prior
identities or dynamic topology. Sparse/dense scratch and cancellation before
discovery, after producer discovery and after reader exclusion are checked.
This path excludes callback effects, pending placements, external/legacy cache
reads, unsupported bindings, multi-cell outputs and later repair waves. It grants
no new shape pin for a dynamically discovered cycle. Broader dynamic profiles
keep their typed whole-command refusal; ordinary worksheet errors remain values.

Eng17 logical stopping follow-up (engine21): eight conditions, ten fresh owned
Excel processes each, distinguish zero from a numeric coercion of prior logical
values. Original inputs, stored logical seeds, formulas, request/result/receipt
links and source hashes were verified. Manual, iteration enabled, MTC off and
events off were recorded on Excel 16.0.20527 with VS15ExcelAdaptor connected.
The positive/negative eighth controls and the two thresholds establish zero as
the prior logical baseline for a numeric stopping sample in this scalar profile.
Logical outputs still supply no numeric stopping sample. No output value is
coerced by this change.

HostSession replays all eight new and all nine original eng14 stopping inputs at
unlimited and one-evaluation budgets: 68 typed command-result comparisons pass.
The new controls reproduced six mismatches before the change; the two older
logical-TRUE conditions are now included in the ordinary passing test. Every
compared command actually calculates its target; no imported or held seed is a
match. Final values do not establish internal sweep counts or mid-cycle reads.
Iteration retains its existing uncertified invocation boundary. Broader array,
topology, MTC and history profiles remain open under dc-hsa5.14. The late eng15
observer delivery is described below.

Eng14's separate unchanged authored-entry nonfinite capture has 21 immediate
bit matches through the existing compare route. Its source is the workbook plus
oracle.entries.json composite key, not the physical XLSX digest. Six saved/reopen
NaN-to-#NUM! differences remain capture failures; the host tests retain that
distinction and recalculate the original formulas after reopen. The capture
reused an Excel process with an add-in connected; it is not a fresh-process or
MTC-off claim, and the current comparison has no strict-target certification.

Eng15 asymmetric observer observations (imported 2026-11-05): the unchanged
P14/eng15-20261104 inputs, source, cumulative traces and trial1 result objects
retain two conditions with ten fresh owned-process trials each. All 425 delivery
hashes and 20 request/input/result/receipt/source chains were checked. Manual,
iteration on, MTC off, events off and zero startup entries/counters were verified.
Each of the 40 A1:D1 Range commands enters the selected observer once. Outside
A/B/C snapshots retain 2/5/7, while D displays its authored and seeded zero.
The explicit observer's formal Double arguments log +0/+0; their pre-coercion
types remain unknown. The hidden observer's two Value2 reads log Empty/Empty.
Thus held numeric seeds cannot supply these observed arguments or hidden reads.
The arithmetic producers have no counters, so their attempted work and SCC
execution counts remain unmeasured. Range returns DBNull/null; it is not a
Boolean success assertion. Callback-bearing cyclic Range remains a typed
implementation gap, with no held-cache pass. Full/Rebuild and general mid-cycle
visibility remain pending. Two failed overflow-display preparations elsewhere
in the delivery remain recorded; neither is a successful Range trial.

Eng13 instrumented Range observations (2026-11-04): the delivered packet at
`~/codex-runs/excel-results-eng13` contains 80 valid trials, ten for each of eight
conditions, each in a fresh owned process, with two explicit Range commands.
The 1,348 delivery hashes, pinned sources, input bytes/formulas, settings, zero
startup entries, cumulative traces and typed snapshots were checked. Three failed
preparations remain recorded; successful v4 alone supplies the observed inputs.
These are ordinary automation trials with VS15ExcelAdaptor connected, Manual,
iteration enabled, MTC off and events off, Excel 16.0.20527. Range returns
System.DBNull/null; that is not a Boolean success assertion.

The SCC-count and convergent-only conditions log no producer entries under either
A1:B1 or A1:D1. Explicit and hidden pair observers log no entries under A1:B1;
under A1:D1, D1 enters once per command in all ten trials of each condition,
40 observer entries in total. The explicit observer logs Double +0/+0 arguments; its formal parameters are
Double, so these logs do not reveal the pre-coercion argument type. Both hidden
Value2 reads log Empty/Empty, while external A1:C1 snapshots retain numeric-zero
seeds. The typed distinction is observed; its cause is not identified. Thus a selected observer can run with no observed producer entries or changed
producer snapshots in these instrumented Range trials. The pair conditions use
unprobed arithmetic producers, so attempted or discarded internal producer work
is not measured. This is
not evidence of fresh producer calculation or an observed mid-cycle sweep.
The observer-reader refusal in the current partial Range route remains an
implementation gap (dc-hsa5.11): qualifying the selected cyclic read boundary and its effects
must be implemented before those observer commands can claim parity. Retained
producer caches remain nonpasses. Full/Rebuild scheduling, uninstrumented SCC
execution counts, general mid-cycle visibility, MTC-on and other histories remain
open; the pending eng12 observer questions are not answered by this delivery.

Eng19 implementation from eng16 (2026-11-05): the unchanged P14/eng16-20261105
packet contains sixteen conditions, ten fresh processes each, with 2,634 delivery
hashes checked. All trials agree on their two command snapshots and complete
typed callback traces. HostSession replays the original formulas and seed caches
with callbacks implementing the recorded bodies, at unlimited, one and three
evaluation budgets: 384 final-value comparisons plus exact callback entry order
and typed argument/read traces. Of those value checks, 246 are calculations and
138 check retained displays (102 selected refused targets and 36 outside scope).
Exit payloads, callback counters and Caller addresses are retained in the original
events but are not independently reproduced by these registered callback bodies.
This isolates the engine schedule; it does not claim that the original file-VBA
setup project has run. No held cyclic target counts as a calculated match.

Full and Rebuild use the same recorded scalar sweep profile. The converging
instrumented pair enters four sweeps on the first command and one on the second.
An independent diverging peer keeps every observed cycle member running for all
100 sweeps in both commands. Cached callback reads see the current scalar sweep,
including the executing member's seed/current value. A direct reference callback
outside the cycle first enters with Range objects whose Value2 is Empty, then
enters after each of five producer sweeps. The preliminary entry publishes no
value and retains its completed invocation record before a distinct
ProvisionalArguments evaluation. Physical yields retain that entry's continuation.

Manual Range retains the cyclic producers and their unmet requirements. A scope
containing only those producers now completes Incomplete with TargetsRefused.
A selected cached observer can run once and sees Empty for those selected cyclic
producers; it neither demands them nor creates value dependencies through its
observations. Ordinary formula reads still refuse those values. Static scalar
callback admission does not extend the closed-wait discovery profile to callbacks.
Arrays, spills, tables, automatic cyclic Range and cross-workbook cycles retain
their existing refusals. The broader iteration publication boundary remains
uncertified; these trials do not establish MTC-on, arbitrary histories or array
iteration semantics. Earlier text describing these scalar observer/count cases as
open records the prior implementation state.

The same delivery answers the ten Legacy denominator/blank-origin questions.
In the recorded J2 controls, a multi-cell second argument to MROUND/QUOTIENT
gives #VALUE!; unary plus gives
0 and 2 in the recorded controls. Legacy paired IFS/SWITCH preserve blank-reference
origin, computed Empty materializes as zero, and empty text remains empty text.
Replay through the existing compare route calculates all 50 cells and matches
40: six roots and 34 observers. Four roots and six dependent observers differ.
Legacy second-argument multi-cell references are incorrectly intersected instead
of producing #VALUE!, while Legacy computed Empty in paired IFS/SWITCH results
is still rendered as empty text instead of zero. These are separate measured
implementation gaps, not new gains. The ten ISFORMULA controls return #NAME?
on both sides; they do not establish Boolean formula identity.

Nested regression predictors (engine17, dc-b1l): the unchanged R25 Legacy
captures select full `CHOOSE({1,2},range1,range2)` predictor arrays in the
second argument of LINEST and LOGEST, including through INDEX. The existing
lazy-call instruction now carries the enclosing argument admission separately
from its selector's admission, so materializing branch references keeps both
predictor columns. All 494 formerly refused cells now have actual evaluated
results without imported or boundary caches: 288 match Excel and 206 remain
numeric mismatches (LINEST 105, LOGEST 101). Those failures remain counted and
belong to dc-j9k.14; this establishes neither the numeric algorithms nor wider
unmeasured Legacy array contexts. No source workbook or captured answer changes.

Bare cell callees (engine17, dc-b1l): the existing original VBA-name observations
give #REF! for A1(2), XFD1(2) and the persisted $A$1(2), while module-qualified
A1/XFD1/R1C1 calls reach their VBA procedures. Reference binding now records the
bare callee token and uses the existing reference invocation route, including
the ordinary argument lowering. Builtins such as LOG10 keep precedence; the
same sites support notation and structural transforms. Original XLSM/XLSB files
and their saved/reopened projects exercise actual calculation. The corpus has
no registered live observation for these cells, so this creates no claimed
corpus pass. Self/cyclic/errored or callable-valued callees and argument effects
remain eng17 questions; the graph's conservative reference prerequisites are
unchanged.

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
Implementation update (2026-11-09): a pure wait now resumes at the instruction that read the cell, with the
operands it found and nothing before it run again; restarting the formula is the fallback. Excel restarts the formula
and re-enters earlier UDFs (measured); see EVALUATOR_SUSPENSION.md. Draws: a wait in place keeps the stream, so every
draw happens once; the fallback restart starts the same keyed stream again and draws the same values
(`calc-engine/src/random.rs` header). Replay safety per instruction is in EVALUATOR_SUSPENSION.md section 6.

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

Rounding past DBL_MAX (dc-j9k.19, 2026-11-04): ROUND, ROUNDDOWN and TRUNC explicitly
qualify their measured exponent-field-2047 encodings in CalcValue. A copied value retains
that qualification; an ordinary NaN, even with identical bits, never receives it.
Publication validates every displayed spill or fixed-array member before installing the image.
N, unary minus and single-number extrema preserve the bits. ABS removes the
sign while retaining the qualification. Arithmetic returns #NUM!, and type/error
predicates see a number. Eng15's unchanged readings establish separated overflow
encodings in both signs and mixed finite/overflow extrema. Comparison scales the
exponent-carry representation and the finite operand together by one half, then
uses the existing number comparison. Near ties in the top binade still refuse;
these observations do not establish their rounding rule. Extrema retain the
selected value's qualification and use the same comparison for an overflow pair.
Generic array sorting, grouping and criteria comparisons still refuse nonfinite
inputs; they cannot infer the qualification from a bare f64. Nested SORT/INDEX
controls cover both ordinary NaN signs so a finite selection cannot hide one.
Concatenation and TEXT with General or @ reuse the existing General renderer;
TEXT with the recorded numeric code 0 remains #VALUE!. The 94 immediate captured
readings now match, with no typed refusal; the 30 capture save/reopen differences
remain separate failures. No custom-format or unmeasured near-tie parity is claimed.
General rendering covers the four recorded .Text readings; custom cell formats remain explicitly
unmodelled. Eng15 retains unchanged Value2 readings at widths 8.43, 20 and 40;
this does not establish rendering parity. Save writes #NUM! caches,
Manual reopen retains their file-cache provenance, and full recalculation restores the encodings.
The saved-cache reader continues to require finite numeric XML because these caches have error
type. New oracle capture and additional formatting/order measurements remain open.

Capture phase evidence (dc-hsa5.15, engine20): failure-bearing verdict report version 6 retains
every original failure, cell and message with its available immediate, saved and
reopened typed observations. Classification describes value transitions rather
than their cause. The four engine captures contain 63 recorded losses: 36 raw
nonfinite numbers saved and reopened as #NUM!, 24 finite changes during both
save and reopen, two only on reopen and one during save. The 27 finite changes
were captured with iteration enabled; that setting alone does not prove why
they changed. Counts and original captures remain unchanged. Missing, duplicate
or unchanged phase evidence cannot remove a recorded failure or grant calculation
proof. No new Excel execution is claimed. Questions for explicit save settings and
phase snapshots remain under `~/codex-runs/excel-questions-eng20/`.
Reports without recorded failures keep their byte-identical version 5 encoding.

## 5. The VBA read view

Linked workbook inputs (dc-ylr, 2026-11-03): the portable host calculation domain
orders acyclic source workbooks before their consumers and runs each through the
ordinary engine in its own context. Engine-created immutable snapshots contain
authored inputs and validated calculated results, retaining their producing
calculations and source pins. Imported, dirty, refused and qualified formula
values remain unavailable; a consumer may then use its labelled external-link
cache, which remains a nonpass. Spill children and shape reads retain that label.
Commands pin the linked-input version; replacing a live source invalidates its
consumers, while an isolated save retains its original cut. The native comparison
loader resolves local link targets and siblings and records source hashes. It
never retrieves remote URLs. Domain mutation is unavailable during admitted work;
cross-workbook cycles return a named dc-ylr refusal. This is acyclic snapshot
coordination, with no cross-workbook iterative-session claim. The existing oracle
deliberately omits its source workbook, so its 73 external-cache cells remain
nonpasses when comparing the repository's files.

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
  (`calc-doc/src/save/author.rs:903-916`). Excel recalculates those on open (1). Honest, but it makes Excel call a UDF
  on open that Excel itself might not have called.
- `PreservePermittedCaches` is selected by P06 (O, 20527 safe mode) for ordinary Manual Save(false): old
  caches of the same formula instance survive with ca=1, calcCompleted=0 and calcOnSave=0. Do not infer freshness
  from absent flags: disabled-sheet cases can keep stale caches without ca or calcCompleted. Flags follow the
  measured file profile; they do not create a producing command or certificate.
- `CalculateBeforeSave` is a `CalculationDomain` setting (xlsb conversion at `calc-doc/src/xlsb/workbook.rs:658-660`,
  and save writes `calcOnSave` from `calculate_before_save`, `calc-doc/src/save/parts.rs:490-499`, supplied by
  `calc-host/src/save.rs:435`). True: admit a `PinnedSave` calculation of the workbook's dirty/volatile demand,
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
1010/1200/1010 observations once per observer with finite and unlimited budgets. The H1 counterpart
now uses an explicitly granted identity callable, retaining one entry across physical continuations.
The native XLL projection accepts explicit registration grants separately from thread safety;
ordinary `$` registrations receive no early-execution permission. Grants reject volatile, async,
cached-read and command registrations and are republished through callable generations. This
establishes the portable scheduling path and native adapter code; new Windows DLL execution is
still unrun. Public host/worker Rebuild clears derived plans, bindings and static/dynamic edges,
then forces the workbook; the domain exposes the corresponding source-ordered request. Authored
revisions, history, identities and held spill seeds survive. Active live work makes Rebuild unavailable.
Broader chain-history and multithreaded order claims remain outside
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
The domain now owns ordinary HostSessions. Its Save(true) applies the saved session's handler edits
before admission, calculates dirty/volatile work in every domain workbook (including unrelated peers),
then prepares the saved workbook's owned image. A disabled active sheet suppresses this admission;
other disabled sheets narrow the saved workbook's scope. Peers currently use their workbook scope;
per-peer disabled-sheet policies are not projected. Unchanged Save(false) preserves connected input
cuts. A changed source cut cannot retarget an existing parked invocation to discharge a save.

Ordinary saves now retain a live calculation across finite tasks and provider waits. They can wait
for a compatible existing command without entering its prefix again. An incompatible narrow scope
or retained range obligation refuses; callers must finish that command before a new save. Final
images require matching completion/results and no remaining required or stale target in the save
scope. Cancellation, changed model/inputs/providers or incomplete coverage write no image. An image
already prepared retains its contents through later edits. This is the ordinary live strategy;
isolated calculated saves keep their separate destination and original input cut.

Worker Save and SaveWithContext return SavePending when calculation is needed, schedule no task
while waiting for a provider, and emit one SaveReady notification. PollSave returns the owned bytes
through the existing command journal. The controller retains the download callback by workbook,
token and generation and drops it on replacement. Manual Save(false) stays noncalculating. These
async save controls establish DNA behavior, not new Excel Save(true) measurements. P07's UI
Save(false) cancellation profile remains open. The event hookup below covers explicit
source projects; broader P22 parity remains open.

Workbook event hookup (dc-hsa5.12, 2026-11-03): an explicitly loaded source
project supplies its standard/document module kinds and worksheet identities.
The compiler/runtime select private handlers and subscribe the host's canonical
document sources. Standard-module UDFs and handlers retain the same project
state. Handler selection stays in the compiler/runtime. OneCalc retains its engine-free route.

File projects (dc-snz.15): the worker now reads the opened package's VBA source,
matches document modules to persisted workbook/worksheet code names, and installs
the same owner before the first calculation. Its explicit Preserve policy compiles
and executes no project. Failed source, signature, document identity or class/designer
admission returns a typed refusal before replacement. Standard-module UDFs,
including Option Private Module and recorded Private Function worksheet calls,
share one state with the attached document events. The pinned embedding API binds
public handles, so file admission compiles the original project first, then adds
internal forwarding entries in the private procedures' own modules. The worksheet
registry publishes the original names. The original declarations retain their VBA
visibility, and saved source never contains the generated entries. Optional defaults
and ByRef copies use the ordinary adapter; Long, Double, Boolean, String and Variant
are admitted. A private signature outside those scalar carriers registers a typed
refusal for that worksheet entry. Unused private array, ParamArray and object
helpers therefore allow the project's supported entries to run. A refused call
executes no procedure prefix, and IFERROR cannot turn it into a calculated value.
Explicit source-module loading keeps its existing public-only worksheet inventory.
The XML/binary forced-UDF distinction remains open.

File-project event values cannot yet retain their read provenance after entering VBA
module state. A Target.Value2 read therefore refuses file caches, stale values and
calculated values with held, imported, external or other cache qualifications
(dc-snz.15). The event refusal remains visible even when VBA catches the error.
Literal values and ordinary certified calculations remain readable. The explicit-source
event API retains its existing observation policy; full event-state provenance remains open.

Events-off file admission (dc-snz.15, 2026-11-04) distinguishes ordinary classes by
the compound project's `PROJECT Class=` declaration, checked against its directory
and document identities. It compiles every original module through the existing
dynamic VBA session and leaves document event routes unsubscribed. Re-enabling
host events refuses delivery until the project is prepared again with events on;
that admission still checks unsupported automatic handlers. Finite member
declarations permit complete source compilation without supplying implementations.
Reached unsupported members remain typed refusals through VBA error handlers and
worksheet IFERROR. Dynamic JIT native WithEvents bindings are declined before
initialization; default admission selects the interpreter, and an explicit JIT
request refuses. Ordinary class state and Range arguments use the existing shared
session and reference projection. The original P04 SizeUdf executes its authored
unarmed return of 2 without changing saved source. P01 temporary-array UBound and
P10 Input$ remain compiler gaps; general object writes and native subscriptions
remain unimplemented. These controls are not new Excel captures or event parity.

Caller addresses (dc-snz.15) now use the existing reference qualifier printer and
the command's pinned workbook/sheet identities. Named and omitted Address arguments
share one projection route; absolute A1 addresses can include the workbook and sheet.
Namespace reads record OnChange facts, without reading formula values or the
per-calculation file-facts snapshot. Clean F9 leaves a nonvolatile address observer
uninvoked; sheet rename, Undo and Save As update its qualifier while retaining VBA
state. The original P04 PlainProbe now logs its caller and returns its actual
argument, with source unchanged on save/reopen. Missing names, external-workbook
and multi-area qualifiers, relative coordinates, R1C1 and RelativeTo remain typed
refusals through VBA handlers and IFERROR. The namespace-edit control is a fresh
synthetic package retaining its exact VBA binary, not a changed Excel observation.

Original-source diagnostics (engine17, dc-snz.15): the 250 P01 preparations stop
in the pinned CalcVBA binder because UBound/LBound require an array l-value;
`UBound(Split(...))` supplies a temporary expression. A separate direct
`Split(...)(1)` control compiles but raises ordinary VBA error 424 in both VM
and JIT. The 108 P10 preparations stop because the expression functions Input
and Input$ are unresolved; the Input statement has a separate symbol route.
No helper is removed, temporary introduced, or input state invented to admit
these original projects. The pin remains 7eb4fdd9.

Original-source admission follow-up (engine22, dc-snz.15): CalcVBA
`a4bf979239e1cb67326d13b760c04ecd13bbf55f` compiles temporary-array
UBound/LBound and Input/Input$. The host also declares ExecuteExcel4Macro
in its existing finite member catalogue; reaching that unsupported member
still refuses through VBA error handlers and worksheet IFERROR. Complete
P01 and P10 projects now compile without removing helpers or changing their
source. Admission and save/reopen preserve every original module.

The same-input comparison removes all 358 original project-compilation
failures: 250 P01 and 108 P10 controllers. P01's 530 formula positions now
calculate, but live matches remain 0/530 because no matching live capture is
registered. Supplemental saved references give 80 matches, 160 mismatches
and 290 missing/excluded references. The mismatches calculate #VALUE!;
the representative fresh PeekB1 call directly raises ordinary VBA error 91
because RunCase has not assigned observedSheet. No setup is invented.
P10's folder includes 204 owner XLSX files besides the controllers. All 528
formula positions remain typed refusals for missing provider functions;
the add-in/session setup has not run. Its live matches remain 0/528.
Input/Input$ compilation does not establish file-I/O execution or original
RunCase replay. The 54 existing oracle books retain byte-identical reports
and evaluations at 38,822/40,141 matches, 1,319 inclusive failures and 77
capture failures, with no strict certification.

The 350 P02 saved-reference mismatches are a different source-history case.
Their original Note calls UBound on an unallocated trace array, which RunCase
initializes only in the capture session. Calling the complete fresh project with
caller metadata now directly records ordinary VBA error 9. The application
route calculates #VALUE!; IFERROR can catch this ordinary error. Saving and
reopening preserves every original module. The separate initialized logger
control succeeds, but no initialization is added to the opened project. These
350 saved mismatches remain failures; this is runtime evidence without a new
Excel observation.

Native17 delivery audit (engine21): 18,165 delivery hashes and all 600 workbook
summary rows were checked. The retained Windows executable at source 652fcb31
uses the recorded P07 release controller as an explicit new runtime condition.
Its 279 formerly waiting workbook runs complete, and 558 former file-cache
positions now return calculated values. Supplemental saved-reference matches
are 440,642/441,470 (99.8124%); 828 nonmatches comprise 440 saved mismatches and
388 missing references. Of those 388, 378 positions calculated but have excluded
dirty/volatile or unattested saved references, and ten retain typed EVALUATE
refusals. The delivery gains 400 saved comparisons, loses none and changes no
previously calculated value. No new live Excel observation was acquired: live
pass remains 0/441,470, with 441,460 unmeasured and ten refused. The 440 historical
Peek mismatches remain failures. These are audited delivered Windows results,
not a new Linux native-provider execution or a gain attributed to this code change.

Eng17/eng18 delivery audit (engine21): all 5,497 hashes and 160 successful
request/result/receipt chains were checked, including distinct fresh process
identities and preserved automation security. These cover 80 logical-seed,
50 formula-entry and 30 event trials. The event successes observe Manual nested
Calculate, document names across disabled-event renames, and a cached read
retained in module state. Twenty unhandled-fault eng17 attempts, ten eng18 nested
fault attempts and one earlier preparation failure remain failed or blocked.
The dialogs supply no successful later-handler or post-dismissal observation.
No whole original-project parity or event-state cache provenance is inferred.

Workbook_Open runs once after file-project installation and before automatic
calculation; replacement does not replay it. Open, Change, Calculate and BeforeSave
are the admitted event surface. The compiler's resolved document symbols and current
event descriptors reject other reserved event-prefixed procedures, including active
conditional declarations; inactive declarations do not create handlers. Event faults
remain recorded once through the host/worker route. This establishes the DNA lifecycle
and shared state, without new Excel Open/P22 ordering evidence. The bounded synchronous event command surface is described below; broader
macro object-model operations remain open.

The corpus runner uses this owner only under a capture's recorded enabled macro
policy and recorded event switch. Disabled captures preserve source without compiling
it. Missing or interactive macro policy on a VBA-bearing package prevents a certified
comparison. Linked source workbooks follow the same application policy. An admitted
event failure prevents a calculated pass in that comparison.

DNA-only comparison inventories explicitly select opened-with-macros-disabled and
events off (dc-ztvv, 2026-11-04); they make no assertion about a previous Excel session.
Their optional attested Excel-saved oracle is reported separately from live captures
and still requires actual DNA calculation proof. File caches and refused VBA calls
never become matches. See the bulk-port plan for provenance and exclusions.

Prefix publication (dc-cynd, 2026-11-04): a completed spill can be installed before a
later blank reader's certificate is validated against the admitted requirement cut.
The pending-producer query now recognizes an installed certificate for that exact
required version, as dependency-span validation already does. It does not discharge
a refused, imported or older producer. The P05 disabled-macro file regression fails
before the fix and covers short/growing/shrinking footprints at unlimited and
one-evaluation budgets; the disabled UDF remains a counted refusal. This changes
certificate validation, without a new claim about Excel callback or SCC counts.

Target-generation storage (dc-hsa5.16, engine21): formula target versions now use
the same generation-checked bounded slot pages as admitted requirement cuts.
Lookup and retirement still compare the complete NodeId; a stale handle cannot
read or remove its replacement. Replacement and rebuild retain the same monotonic
target counter. Pinned saves share immutable pages, with writes copying at most
four bounded pages. Small or sparse books pay page overhead; no workbook-sized
array is allocated for a high sparse slot. This changes metadata storage, without
changing evaluation order, result provenance or certificate/async target checks.

Comparison proof (dc-d7r1, 2026-11-04): the existing verdict recognizes the exact
primitive scalar published from a one-cell evaluated array, as `array_outcome`
already does. The retained fn2 WRAPCOLS controls show the array in the version-2
evaluation record and its scalar in application projection. The typed comparison
against Excel is unchanged. Larger arrays, changed values/types, blank conversion,
nested arrays, references and callables gain no proof from this rule.

Selected blank origins (dc-ejvk.4, 2026-11-04): the retained Dynamic fn2 q212/q213
cases distinguish a computed scalar Empty in paired IFS/SWITCH result slots from
a direct blank reference. Eager admission now converts that computed Empty to
zero only on the existing whole scalar-selection route, retaining their origin
before reading references; the direct-reference ISBLANK controls remain true.
The same HostSession regression retains IF/IFNA and array observer controls.
Legacy forms, SWITCH defaults and broader composed origins remain unmeasured.

The UI ingress delivers Change after an authored cell-write commit, before
automatic calculation. This remains a DNA policy pending UI recordings. P22
(20527, ordinary COM) records automatic Calculate before Change, with new formula
values visible at Change; Manual Change sees held values. Automation ingress
uses the same transaction and evaluator, completes synchronous automatic work
before Change, and retains its ingress in receipts and replay records. A waiting
provider before Automation Change has a named dc-hsa5.12 event failure; that
placement is not inferred from synchronous observations. Same-value writes and clears retain explicit targets; formats and
calculated values do not become Change targets. It gathers calculated sheets
across prefix publications and delivers Calculate only at terminal command
publication. These distinguish the documented [Change](https://learn.microsoft.com/en-us/office/vba/api/excel.worksheet.change)
and [Calculate](https://learn.microsoft.com/en-us/office/vba/api/excel.worksheet.calculate(even))
events. Waiting for the whole command is a conservative DNA boundary; per-sheet
timing, multi-sheet order, no-op commands, external updates and reentrant edits
still need P22 recordings. The event switch currently belongs to this workbook
session, rather than an application-wide Excel EnableEvents property.

A persistent BeforeSave owner enters the existing pre-pin authored-batch route;
an explicit context owner overrides it, SaveCopyAs skips it, and disabled events
suppress both. An explicit host owner's authored batch survives cancellation.
Automatic VBA BeforeSave now uses OxVba7eb4fdd9's Boolean and BooleanByRef
carriers for the real
[documented signature](https://learn.microsoft.com/en-us/office/vba/api/excel.workbook.beforesave).
The runtime selects the private handler, shares its state with UDFs, and writes
Cancel back before the host can pin a save image. Programmatic worker saves pass
SaveAsUI=false. Handler errors fail the save; each invocation drains its native
event fault before a later top-level event can enter. A handler may commit the
single-cell Value2 writes described below before cancellation; these edits stand.
The existing P22 family retains 70 fresh-required COM trials and 54 P06 replay
trials; its metadata omissions remain recorded. Host tests replay write-event
order and entry values from all 30 basic Manual/Automatic/events-disabled COM
traces. Nested COM event edits are observed in Excel; the bounded DNA owner below
now supports synchronous nesting. The complete original P22 event source still
requires unsupported workbook/application events; UI-dialog behavior remains unmeasured.

The portable document projection resolves Me.Name from its attached document and
supports explicit Target.Value2, Row, Column,
Count and Address under one synchronous event view. Count returns Long and raises
overflow for more than 2,147,483,647 cells, as the [Range.Count contract](https://learn.microsoft.com/en-us/office/vba/api/excel.range.count)
requires. It uses DNA nominal
interface identities, not Excel COM IIDs. Reads use the host's existing projected
caches and add no graph demand. Target handles expire with that view; catching
their refusal in VBA cannot turn it into a calculated result. General object
UDF signatures, stored event Targets, additional Me members and WorksheetFunction
callbacks remain unavailable in this profile. Single-cell event writes are described below. Document-project UDF entries use
the runtime's owning value calls so an older Variant entry cannot swallow these
capability refusals. Long, Double, Boolean, String and Variant signatures are
admitted explicitly within the runtime's four-argument and owning-slot limits;
other signatures or unsupported owning bodies return a named refusal during preparation.
The existing standard-module loader retains its wider adapter.

Reset reattaches the same sources; replacement prepares and attaches all new
sources before publishing registrations. Callback tables release their borrow
before entering user code and unsubscribe on owner drop. Event delivery is
synchronous and budgeted. A runtime busy/error/budget refusal is recorded once,
without retrying effects or undoing the authored edit. The bounded failure queue
counts omitted older failures, and the worker reports them through the ordinary
wire route. P22's Excel traces and the broader macro object model remain open.


Synchronous mutable event owner (dc-hsa5.12, engine18): each registered callback
receives an exclusive loan of the existing HostSession. The VBA adapter borrows it
only for a read or an existing Automation intent. A nested write commits, calculates
in Automatic mode and delivers its nested events before its setter returns; no
post-handler edit queue or second evaluator is introduced. The tested form uses
`Dim app As Excel.Application: Set app = Application`. Its typed Application slots
support one-address local Range lookup, Calculate, CalculateFull, CalculateFullRebuild
and the session's EnableEvents switch. Typed Target and Range support scalar
single-cell Value2 puts, clears and Range.Calculate. Multi-cell/multi-area writes,
array coercions, late-bound object members and provider waits remain typed gaps.
Manual Range keeps the existing scope and StillRequired policy, verified by actual
UDF entry counts before and after F9, rather than by display equality alone.

Nested handlers and same-project UDFs borrow the exact active TypedContext and its
remaining budget. Macro-owned UDF calls run synchronously through the existing
owning-value entry; they cannot retain a new suspended activation. A UDF's read view
cannot use or mutate a surrounding event Target, including through VBA On Error or
worksheet IFERROR. File-project cached/stale-read qualifications remain refused.
Nested unhandled event faults remain on the shared context so the triggering VBA
call aborts even under On Error Resume Next; only the outer boundary drains them.
Committed prefix writes remain. Later top-level events receive a clean scope.

The event switch or retiring owner stops the remaining sheets in a delivery batch.
Waiting, failed or cancelled automatic work does not admit Automation Change.
Parked provider work is unfinished even when the worker schedules no further task.
Intent records retain synchronous parent command identities. Replay regenerates
children from handlers and compares their normalized command/parent positions,
ingress, intents and acceptance, detecting missing and unexpected children. This
checks a command transcript, not arbitrary VBA state or Excel event parity.

Rectangular event writes (dc-hsa5.12, engine19): the same Automation intent now
accepts scalar broadcasts, Empty clears and exact-shape two-dimensional arrays
over one local rectangle, bounded to 1,048,576 cells. Every element is converted
before the transaction commits. The existing area merger forms one rectangular
Change target; one Undo restores the entire assignment. Automatic calculation
finishes before nested Change observes the new cells. Mixed number/text/logical/
error arrays preserve their cell types, and a bad final element leaves all prior
target values untouched. Multi-area, oversized and differing array/target shapes
remain typed refusals. Calculation failure after a committed write retains the
existing committed-prefix and event refusal behavior.

This follows the documented [Range.Value array assignment](https://learn.microsoft.com/en-us/office/vba/api/excel.range.value)
and [Value2 type distinction](https://learn.microsoft.com/en-us/office/vba/api/excel.range.value2).
The owner controls establish DNA transaction/event behavior; they are not new
Excel observations. Fresh scalar/array/shape-mismatch event questions remain
unanswered under `~/codex-runs/excel-questions-eng19/rectangular-writes.questions.json`.

Native interpreter/JIT controls exercise nested Change and Calculate return order,
reentrant UDFs, Full/Rebuild entry counts, Range/F9 counts, shared-budget exhaustion,
BeforeSave write-before-cancel, event-switch failure state and refusal propagation.
Engine19 pins the owned CalcVBA 37651ac8 implementation. Bare global Application
receivers, literal-to-Variant member arguments and CVar String/Variant linking
now use their admitted owning carriers. Each ByVal Variant argument is boxed
before later arguments can mutate its source; ByRef aliases retain their identity.
Native library returns convert to the destination's physical carrier, including
injected object temporaries. The earlier 7eb4fdd frontend gaps are reproduced by
the retained before logs. The owner test exercises the original bare receiver
syntax, numeric/String literals, CVar and calculation through both runtimes.
No original workbook source was rewritten to admit it. The eng16 profile above
separately establishes measured scalar SCC counts, current-sweep reads and cached
Range observers with Full/Rebuild schedules. Broader topology, history and
publication contexts remain open under dc-hsa5.14.


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
unrelated formulas. Definition pages and explicit, versioned diagnostic name evaluations
use the existing host/worker route. P12 cross-scope Excel measurement and shared value
memoization remain open; the later retention update describes plan eviction.

A definition node has no synthetic cell or shared cached value. Its existing evaluator block
returns a reference or value per caller; unused names do not evaluate, and repeated volatile uses
remain separate. Local definitions select their sheet for unqualified references and nested names;
workbook aliases inherit the current use context. Static and INDIRECT routes test the same policy;
new Excel parity for nested cross-scope aliases awaits P12. Calculation-pass identity
for detached roots and transitive eligibility for any shared memo remain open.
Published and explicit diagnostic body evidence is described below.

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

Name retention and memo preconditions (dc-hsa5.6, 2026-11-03): the existing plan interner now
holds weak entries and checks semantic equality after hashing. Insertions visit four metadata
buckets; explicit collection removes all expired entries. Live instances, continuations and
LAMBDA values own their plans. Result storage groups retain weak allocation leases so an old
address cannot identify a different plan. Name operations, including history restoration and edits
to unused names, discard invalid name templates before the next bind. Unchanged plans and their
result certificates retain their identities. Cache statistics count entries and live owners;
they make no byte or resident-memory claim.

Passive Host/engine inspection derives memo candidates only from existing valid bindings. Each
record retains its owning plan and names one body block and NameId; one NameId can have different
bodies in different definition contexts. Analysis follows nested aliases with recursion checks.
Literals, operators and static references can require a read certificate; volatile calls, user
functions (including early-execution grants), external inputs, callable values, local environments
and catalogue calls without a complete facility classification are ineligible. Unclassified lazy
calls are conservatively ineligible even when a branch is not selected. A candidate still requires
caller/definition-sheet/workbook/scenario and numeric/semantic context, valid binding and absence
proofs and an executed read certificate. No shared name-value cache is introduced. Unused names
remain passive; P12 Excel measurement and shared memo execution remain open.

Name-body invocation evidence (dc-hsa5.6, engine19): the existing evaluator's retained
child frames now carry the selected NameId/version, original caller, definition worksheet,
body index and a formula-local ordinal with its active definition parent. Completion
records the actual scalar kind, worksheet error, reference summary, array shape, callable
or refusal. A returned closure can execute after its defining body ends, so the parent
denotes active execution rather than lexical ownership or complete LAMBDA ancestry.
Body indices are local to the producing formula; they are not independent reuse keys.

The ordinary result certificate retains these records under its producing root invocation.
`HostState::name_invocation_view` returns that root, model/result frontier and the ordinary
cell projection. Current refusals and stale calculations retain their existing qualifications;
literals, cleared cells and imported caches do not acquire a formula's orphaned evidence.
The query neither binds nor evaluates names. Repeated body evaluations remain separate,
and no shared name-value cache is introduced. Passive records do not make a pure dynamic
Range calculation effectful. Physical yields retain the same name/callback frames;
cancellation installs no unfinished name records.

Evidence is bounded to 1,024 completed bodies per formula invocation and sixteen reference
areas per retained body, with explicit omitted counts. Calculation continues over every
body and the full reference. Host controls cover nested scoped aliases, untaken names,
closures, definition edits/staleness, typed refusals, cancellation and both limits.
This is execution evidence for stamped formula results, not a new P12 Excel measurement.
Legacy iteration/table stamps and shared memo reuse remain outside this invocation view.

Explicit diagnostic evidence (dc-hsa5.6, engine19): the existing detached evaluator can
retain the same bounded name-body records. Host `evaluate_name_with_evidence` returns the
authored revision, result view, workbook incarnation and actual numeric mode with its
outcome. The existing worker `EvaluateName` command projects those records into its reply;
the command journal retains the complete reply for retries, including retries after edits.
The selected NameId/version and caller anchor remain the root identity, including a
workbook definition shadowed on the caller sheet. The wire numeric label is bounded to
Fast/Exact and is informational; full numeric-target identity remains in the Host record.
Pre-execution version/compilation failures carry no body evidence. Runtime refusals
retain their completed body records without acquiring a value or cell calculation stamp.

Explicit name diagnostics reuse the engine's existing dirty closure to refuse consumed
formula values that are stale or unpublished. Their local read state admits no producer
work; a diagnostic missing input is terminal rather than a resumable calculation wait.
Reference results can remain references without consuming their cells. This fixes a
Manual-mode diagnostic that previously used a precedent's old calculation under a newer
authored revision. Blank reads use the existing spill-absence corridor check for scheduled
dynamic producers and pending formulas whose binding is unavailable; an unresolved possible
expansion refuses. Authored occupants can still prove a corridor blocked. No result, graph
edge or history entry is installed. Consumed unresolved external-link caches, including
transitive cached inputs, refuse at the diagnostic boundary; reference-only results retain
their reference identity. A completed body's numeric kind does not override that refusal.
The dirty closure
is computed per explicit name diagnostic; no additional index or shared name-value cache
is added. Rendering/validation/filter consumers keep their existing published-value policy;
a shared freshness context for those loops remains open to avoid a whole-workbook scan
for every displayed cell.
Host and worker controls cover selected bindings, snapshot refusals, worksheet errors,
serialization and command retries. P12 Excel parity and non-cell calculation-pass
envelopes remain open.

Explicit name passes (dc-hsa5.6, engine21): selected diagnostics now reserve an owner
command sequence from the same allocator as cell and save passes. The typed name locus
contains the actual BoundName and caller; no formula cell is manufactured. Validation
precedes allocation. The pass retains its input version, supplied owner clock and full
owner numeric mode even before any cell publication. The existing random stream supplies
actual draws, with the first 1024 raw bit patterns retained in ordinal order and an explicit
omitted count. Evaluation continues after that limit. Repeated aliases draw separately;
LET reuses its evaluated local. The worker journal replays the entire original reply.
Passive name views reserve no sequence, and diagnostics publish no result or history entry.

The existing compiler receives callable registration metadata, without executable owners.
A reached known UDF refuses under DiagnosticSnapshot before user code enters; IFERROR
cannot turn that refusal into a value. Untaken branches stay lazy. Name Manager authoring
uses the existing Excel file-form converter, and its display uses the shared formatter;
modern function prefixes survive save/reopen. The engine22 work below adds recorded
aliases and explicit effect-capable name calculation. Shared value reuse and the
unmeasured broader contexts remain open.

P12 recorded alias contexts (dc-hsa5.6, engine22): the eng19 delivery contains ten
fresh-process repetitions of each of three name-scope conditions. The retained
original result, request, receipt and saved workbook for each condition are under
`excel-calc-probes/P12/eng19-20261106`; provenance lists every audited repetition.
Re-entering the authored formula syntax and evaluating the recorded selected names
gives 40 matching comparisons across one-step and unlimited execution budgets.
Known local-sheet function calls now use the existing scoped-name and Invoke route,
with arguments evaluated in the caller context. Name watches cover definition edits,
rename, Undo/Redo and scope moves. A missing sheet-qualified name or an ambiguous
sheet/workbook qualifier refuses before entering argument UDFs; explicit bracketed
workbook calls retain their existing route. This is the measured bounded profile.
Excel rewrote the original Names.Add definition `=A1` as `=Sheet1!A1`, still relative,
so constant-name discriminators remain requested under `~/codex-runs/excel-questions-eng22/`.
Explicit selected-name calculation now has a retained non-cell activation through
HostSession and the worker command journal. It owns the existing CalcFml continuation,
actual caller and name invocation identity, full numeric mode, draw stream and current
read frontier. Nested blocks, arrays and cooperative UDF suspension use that continuation;
resuming does not replay an entered effect. A completed worker reply keeps its original
revision and projected value when later commands change the workbook.

Edits, cancellation, reload, changed owner facts or calculation mode, changed input or
result frontiers, and Rebuild expire pending work before more user code. Retirement drops
the activation before releasing its owner lane; completed scalar evidence releases unused
workbook pins. Cached observations retain their qualifications and remain distinct from
current dependency reads. Missing producers, asynchronous waits and semantic re-entry
return a typed dc-hsa5.6 refusal, including under IFERROR. This route publishes no cell,
creates no synthetic formula target and supplies no cell freshness certificate. Passive
diagnostics retain their existing refusal before UDF entry. Shared name-value reuse,
general non-cell async returns and a Name Manager calculation control remain open.

Explicit Rebuild storage (dc-hsa5.17, engine22): discard the existing derived reverse
indexes once, then clear per-anchor binding state through live anchor identities. This
avoids removing each reader separately from a shared sorted vector, while preserving
formula IDs, target generations, results and held spill storage. Binding reset still
invalidates pending name work. The existing benchmark now measures a warm HostSession
Rebuild after its cold calculation, full calculation and edits, including dispatch and
binding work. Its older rebuild_s field measures the cold first calculation. The warm
route projects the whole inventory and participates in the existing thread comparison.
Measured speed and memory changes belong to their recorded binary pair and Linux runs;
they do not establish Excel scheduling or Windows performance.

Persistence controls (dc-hsa5.14, dc-hsa5.15, engine22): the eng19 and eng20 imports
audit 4,420 delivery hashes and 180 successful fresh-process trials. The 21 failed
attempts and 40 unrun original Peek setup trials remain recorded outside the repository.
The eng20 P06 supplement retains original results and immediate saved packages for
four workbooks with CalculateBeforeSave false and true, ten trials per condition.
The numeric workbooks actually reopen in Automatic mode; events and MTC are off.

Of the original 36 nonfinite persistence changes, 32 have the same live raw number
before and after either save setting, serialize as #NUM!, reopen as #NUM! and recover
the raw encoding on explicit Rebuild. The other four ABS observers are already #NUM!
in the new Rebuild control, unlike their original entry-time observation. Their original
loss cannot yet be assigned solely to serialization. For all 27 original finite phase
failures (24 save-and-reopen, two reopen-only and one save-only), Save(true) reproduces
the original saved value and its later explicit Rebuild reproduces the original reopened
value. The original worker did not record CalculateBeforeSave or reopen calculation,
so that causal explanation remains conditional. New questions ask for those missing
controls without rewriting the original captures.

The existing host Save route matches 324 selected finite phase comparisons across both
save settings and execution budgets. Retained post-Save values are counted as comparisons,
not additional executions. Imported reopen caches remain FileCached and unscored until
the separate Rebuild. The original 63 capture failures, corpus denominators and live
scores remain unchanged. These controls do not measure SCC entry counts, MTC-on behavior,
linked iterative workbooks or the broader array/topology profiles.

Owner contexts for current decisions (dc-hsa5.6, engine21): Host validation and
formula-based filter criteria supply the owner's full numeric mode, including
before any cell publication. One context retains it for the batch. If published
formula values belong to another mode, current reads refuse them; literal inputs
and unused branches remain available. The existing stale/retired-anchor closure
is retained. Historical conditional painting keeps its published-value policy.

Context compilation uses the existing metadata-only callable compiler. A reached
known UDF refuses before user code enters, including inside IFERROR; an untaken
branch stays lazy. Replacing a callable registration expires a current detached
plan until it is compiled again. No executable owner, producer work, result or
history entry is granted. Colour-filter authoring passes the same context through
conditional-rule preparation, formula evaluation and subject reads. A refusal
precedes any visibility, style or history commit. Ordinary painting retains its
published-value policy, and population/threshold colour rules keep their existing
typed refusals. The five route controls cover Fast/Exact arithmetic, an Exact
literal, reached and lazy known UDFs, zero entries and atomic refusal.

Detached current-value contexts (dc-hsa5.6, engine20): validation audits and filter
decisions can borrow one `DetachedEvaluationContext` for their whole batch. Its
explicit policy selects historical published values or current values. The current
policy computes the existing dirty closure once, refuses consumed stale, held,
external-cache and imported/boundary-qualified values, and uses the ordinary reader
for both expressions and returned references. A reference identity alone consumes
no values. The borrow admits no producer work, effects, graph edits or publication.
As with other engine entry points, the caller supplies the matching engine/model pair.

Detached plans retain their compilation model pin. Current evaluation requires the
same model lineage, revision, definitions allocation and caller worksheet; changing
file identity can replace definitions without advancing the authored revision.
Names also retain their binding proofs. Expired plans refuse before their bodies
run; consumers compile again for the requested context. The pin shares authored
pages and definitions rather than copying cells. Historical painting retains its
published-value policy and makes no current-input claim.

Validation scans apply that policy to the entry and its criteria, including custom
formulas, numeric bounds and list references. Candidate validation uses the same
reader after its disposable ordinary calculation. Color filters still lack per-rule
read certificates, so their conditional-format decisions conservatively require
every formula value in the borrowed workbook view to be current and unqualified.
An unrelated unavailable formula can therefore refuse color filtering. This is a
DNA admission policy; no new Excel observation or shared name-value memo is claimed.

The native desktop engine runner accepts `--early-execution XPure` as an explicit grant, resolved
to active registration IDs and checked by the native adapter. It records those IDs and accepts
`"calculation":"full"` or `"rebuild"` in its existing cases; dirty calculation remains the default.
This supplies the native P08 route. Its Windows execution remains unmeasured in this Linux run.

Formula source regrouping (dc-hsa5.10, 2026-11-03): `regroup_formula_sources` and
`HostSession::compact_formula_sources` compare exact rendered spelling through the existing
shared-reference mover, within explicit occupied-cell and UTF-16 work budgets. They merge whole
retained sources and independently authored/restored members only under one legacy cell template;
arrays, dynamic forms and tables stay separate. The model verifies placement, text, form and
complete old/new membership before a storage replacement at the same revision and lineage.
Results, history, pending provider activations and existing pins retain their identities.
Equality, hashing, name-source analysis and save checks use temporary renderings; requesting editor
text remains the operation that retains an expanded member cache.

Ordinary Host/worker saves may use a bounded regrouped serialization root while leaving live
storage unchanged. The writer selects a surviving member as master, recomputes its tight bounds,
and places `si` only on actual members. It retains per-cell formula/cell attributes and caches
through the existing writer. New shared rectangles avoid retained shared, array and table
rectangles; after one million overlap comparisons, additional candidates stay individual.
Untouched file groups retain their original representation. This follows the shared-formula
membership and master-reference rules in the [CellFormula schema documentation](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.cellformula?view=openxml-3.0.1).

Measured controls reduce 128 authored source records (1,044 UTF-16 units) to one (7 units), with
zero retained member expansions. An Undo-restored member rejoins its 127 retained neighbors.
A full-sheet extent holds two members/two membership runs and writes two occupied rows in a
330-byte worksheet. These are authored source statistics, not process-memory savings: old pins
and engine plans may retain prior allocations. Host/worker save/reopen controls also cover removed
masters, holes, raw attributes, exact cosmetics, dialects, arrays, tables, intersecting rectangles
and a parked provider. New live Excel-open validation remains for the Windows lane.

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
