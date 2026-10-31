# Question for GPT-6 Pro: Excel's calculation semantics, and the experiments that pin them down

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet that must reproduce
Excel's observable behaviour exactly, while recalculating in a different order (visible cells
first, incremental publication, cancellable work, later parallel). That is only safe where Excel's
ordering is unobservable. A red-team review of our architecture (Appendix B excerpt) concluded that
the riskiest unknowns are Excel calculation semantics that leak ordering or freshness: cached
reads inside VBA UDFs, scoped calculation in Manual mode, volatile and spill passes, iteration,
data tables, save as a completion barrier. We have Excel 16 (Microsoft 365, build 20527, 64-bit,
Windows, AMD) on a machine where agents may drive it through COM and VBA; we cannot inspect its
binaries (clean room).

Please produce:

1. **A semantics compendium** of Excel's recalculation model as far as public documentation and
   well-established public knowledge go, separating documented fact, widely reported behaviour,
   and inference. Cover at least: dirty marking and the calculation chain; Automatic, Automatic
   except tables, Manual; Calculate / CalculateFull / CalculateFullRebuild / Range.Calculate /
   Sheet.Calculate / F9 variants and what each guarantees about freshness; volatile functions and
   volatile-by-construction (INDIRECT, OFFSET, CELL, INFO, NOW, RAND, RANDARRAY, TODAY, and
   Application.Volatile UDFs) and how volatility propagates; dynamic arrays: spill sizing, #SPILL!,
   implicit intersection and @, Formula vs Formula2; iteration (circular references, Iteration,
   MaxIterations, MaxChange: which cells iterate in what order, what one iteration means);
   data tables (TABLE(), how scenarios are evaluated and restored); multi-threaded calculation and
   what forces single-threading; UDFs (VBA, XLL including thread-safe and asynchronous UDFs, RTD):
   when they run, how often, what they may read, what a read of a dirty cell returns; LAMBDA and
   LET evaluation (repeated evaluation, recursion limits); external links; calculation on open and
   CalculateBeforeSave; what the saved cached values mean; Precision as displayed; 1904 dates.
2. **An experiment catalogue**: for every point where documentation is silent or ambiguous and
   our design depends on it, a concrete black-box experiment we can automate through COM/VBA:
   workbook setup, operations, what to record (values, invocation counters, order logs, saved
   cache contents), and how each possible outcome would constrain our design. Prioritise the
   measurements listed in Appendix B and anything you think they miss. Mark the ten most
   design-critical.
3. **Invariants we can rely on**: a short list of statements about Excel's observable behaviour
   that are safe to build a reordering, incremental engine on, each with its evidence level.

Be precise and cite sources. Where unsure, say so and give the experiment.

---

# Appendix A: our recalculation contract as currently synthesised (excerpt)

### 1.3 Recalculation contract

```rust
pub struct RecalcRequest { mode: CalcMode, command: Auto|CalcNow|CalcSheet|CalcRange(AreaSet)|Full|RebuildFull,
                           tables: Include|Defer, demand: DemandSet, interest: InterestSet /* versioned */ }
fn update_interest(&mut self, interest: &InterestSet);   // reorders; discards nothing
fn merge_edit(&mut self, change: &PreparedEngineChange); // new tick for volatiles; validated reuse
fn step(&mut self, budget: WorkBudget, cancel: &CancelToken) -> Progress;
fn drain(&mut self) -> Publication;                      // { revision, tick, seq, units }
pub enum CellState { Settled { computed_at, valid_for }, Pending { last }, Refused(Owner), FileCached }
```
- **Demand** comes from mode, command, target and forced-vs-dirty evaluation; Automatic Except Tables and
  range/sheet calculation leave work pending by design (`calc-host/src/recalc.rs:74-82`; GPT(r2)). Interest only
  orders ready work inside that demand. Freshness outside the command's scope is reported, not forced.
- **Publication unit** (R1/R2): value, observed edges, shape and spill reservation commit together, validated
  against read/binding/topology/effect versions. `Settled` = no dirty node and no unresolved may-spill or
  dynamic-reference root in the backward cone, within the demand scope. Clean prerequisites keep their own
  tick (`calc-engine/src/read.rs:435,455`).
- **Effectful units** are admitted and committed, never validated-and-retried: a unit calling a stateful UDF is
  serialised against conflicting edits and reloads, and a suspended activation is preserved, not replayed
  (GPT(r2)). Today a read of an uncalculated cell discards the evaluation and re-runs it later
  (`calc-engine/src/recalc.rs:607-610`), which replays VBA side effects (Gemini(r2)).
- **VBA reads**: argument references follow measured rules; internal reads of cells outside the arguments are
  UDF callback observations, not dependency edges (Excel kept the old value until explicit calculation,
  `docs/plan/VBA_UDFS.md:91-93`, `probe.bas`, `observations.json`; GPT(r2), Gemini(r2)). An uncalculated
  prerequisite is evaluated synchronously on demand with cycle checks, without recording an edge.
- **Volatiles under edits**: `merge_edit` re-dirties the volatile set under a new tick, since RAND keys on (seed,
  tick, cell) (`random.rs:4-8`; Opus(r2)). **Spill shadow**: per-sheet may-spill index, conservative reach.
  **Unsplittable units**: SCCs, data-table runs, ordered effectful groups; atomic is not uninterrupted (GPT(r2)).
- **Completion points**: save, `Application.Calculate` and a macro's `Range.Value` demand the relevant scope and
  drain at one tick (Opus(r2)); save writes pending cells under an explicit file-cache policy (GPT(r2)).
- **Open**: volatiles recalculate at open, everything under `fullCalcOnLoad`; file values stay `FileCached`
  until settled (Fable(r2)). **Cancel means yield**, never inside a live x87 sequence; ready counts and a height
  heap; early cutoff only in cones without effectful callables (UDFs are effectful by default,
  `VBA_UDFS.md:78-80`); `SetInterest` journalled for replay.
- **Presentation-only invalidation**: resize, zoom and font changes re-render tiles (General digits, `####`)
  without recalculating; today cells render at standard width (`apps/dna-calc/src/cells.rs:53`; Opus(r2)).


# Appendix B: red-team findings on calculation semantics and its measurement list (excerpts)

### 1. Cached VBA observations make otherwise pure scheduling observable

**Target: A §§1.3–1.4 and S4–S5.**

The synthesis correctly separates VBA's internal cell reads from dependency edges, but then proposes synchronously calculating an uncalculated cell before returning its value. These are independent choices. Microsoft permits VBA UDFs to observe uncalculated cells; an observation read is not automatically a demand for a fresh value. [S1]

Use this workbook:

~~~text
A1 = 1
B1 = A1+1
C1 = PeekB1()
~~~

~~~vb
Function PeekB1()
    PeekB1 = Sheet1.Range("B1").Value2
End Function
~~~

After calculating, switch to Manual, change A1 to 10, and calculate C1 alone. Compare its observation before and after explicitly calculating B1. Record the initially cached value and the full call trace. Also test a VBA UDF observing its own caller.

The architecture problem extends beyond this probe. Suppose an unrestricted UDF will observe B1's cache. Computing B1 early because it is visible can change the answer the UDF observes, even if all UDF calls retain their original order. Serializing effectful units only against other effectful units is insufficient.

**Required distinction:**

~~~rust
struct ReadRequest {
    purpose: ReadPurpose,
    policy: ReadPolicy,
    observation: ObservationFrontier,
}
enum ReadPolicy {
    DependencyWithinCommand,
    CachedMacroObservation,
    RegisteredWorksheetCallback,
    DiagnosticSnapshot,
}
~~~

The purpose answers whether the read contributes an invalidation dependency, a replay observation or a diagnostic. The policy answers which values it may return and whether calculation is permitted. The observation frontier answers which completed calculations are visible to this invocation.

Initially, put conservative fences around unrestricted callbacks and the pure-result transitions they may observe. Later, a logical read view can permit physical computation to run ahead without exposing those values to earlier invocations. Do not promise that a fence covering only the cell's known argument references is sufficient: the callback's actual read set may be discovered inside user code.

This establishes a real tradeoff. If arbitrary VBA can inspect any cached cell, fully unrestricted visible-first publication and exact observations cannot both be assumed. Measure the compatible calculation order and choose the permitted visibility frontier explicitly. A serial reference implementation chosen by DNA Calc is not, by itself, evidence of Excel's order.

### 2. "Settled" conflates a completed command with workbook freshness

**Target: A §1.3, including completion points, effect admission and volatile ticks.**

A result may correctly implement a narrow calculation command while using an out-of-scope cached precedent. Scope is therefore part of command completion; it cannot erase the age or dirtiness of inputs. Range calculation resolves dependencies within its selected range. [S2]

~~~text
A1 = 1
B1 = A1*2
C1 = B1+1
~~~

Calculate all; set Manual; change A1 to 5; calculate only C1. Then calculate B1, then C1 again. The implementation must reproduce each operation's permitted read behaviour without advertising the first C1 result as current with every workbook input.

Use two dimensions:

~~~rust
struct ResultStamp {
    computed_by: CalcCommandId,
    reads: ReadCertificate,
    completion: CommandCompletion,
    freshness: Freshness,
}
~~~

A read certificate needs value versions and invalidation versions. A dirty formula can still have the same published value version it had before the edit; validating only that value version misses the fact that the value is awaiting recomputation. For an intentionally cached read, preserve that condition in provenance instead of rejecting it or silently forcing calculation.

Correct the completion-point sentence now:

- Reading Range.Value/Value2 is a read with defined semantics, not an unconditional calculate-and-drain operation.
- Saving may preserve dirty caches. In Manual mode, CalculateBeforeSave explicitly controls pre-save calculation. [S3]
- An explicit calculation command has a scope and a completion receipt.
- A snapshot export can request a stronger freshness policy, but that must be a separate declared operation.

#### Invocation identity is also missing

"Never retry effectful units" needs to mean never accidentally repeat the **same semantic invocation**. It must not mean one UDF call per cell per tick. Excel's calculation process can evaluate formulas more than once while adjusting its chain. [S2]

~~~rust
struct InvocationId {
    command: CalcCommandId,
    pass: CalcPassId,
    locus: EvaluationLocus,
    call_site: CallSiteId,
    occurrence: u32,
}
~~~

A suspended effectful invocation resumes its existing activation; it is never re-executed because of a scheduler retry. A pure attempt may be recomputed with the same recorded nondeterministic inputs. A new semantic evaluation receives a new invocation identity, including within one pass. Occurrence must distinguish repeated calls inside a LAMBDA, array traversal or recursion.

The random key quoted in A §1.3, (seed, tick, cell), is consequently an incomplete contract even if random.rs internally has more machinery. Contrast RAND()-RAND() with LET(r,RAND(),r-r). Also test a named RAND used twice and RAND inside a repeated LAMBDA.

More decisively, Microsoft documents indeterminate-size #SPILL! for SEQUENCE(RANDBETWEEN(1,1000)) when shape does not stabilize across calculation passes. Freezing the same random draw across every pass could manufacture stability. [S4] Retain the selected generator; specify draw identity and replay semantics separately.

#### External completions are different kinds of events

~~~rust
enum ExternalOperation {
    AsyncReturn { invocation: InvocationId, result: Value },
    RtdUpdate { subscription: SubscriptionId, sequence: u64, value: Value },
    LinkRefresh { source: ExternalSourceId, revision: SourceRevision },
}
~~~

An async return completes a particular outstanding call. An RTD message updates an ongoing subscription. A link refresh changes an external input snapshot. Provider generation alone cannot distinguish two calls to the same provider from the same cell with different arguments.

Microsoft's XLL async interface is explicitly call-handle based and imposes argument-lifetime and callback restrictions. [S5] An adapter must preserve these obligations. Do not turn every return into an ordinary edit that restarts the formula and repeats effects preceding the asynchronous call.

Separate yielding, superseding pure work, user cancellation, and shutdown. "Cancel means yield" is not enough to describe outstanding external operations or a cancelled calculation command.

### 6. Spill absence, iterative calculation and table scenarios exceed scalar publication units

**Target: A §1.3 and 3b-2.**

#### Blank reads need proof that no pending producer can occupy them

~~~text
Z1 = 1
A1 = SEQUENCE(Z1)
C10 = A10+1
~~~

After calculation, change Z1 to 10. A10 is still blank in the published grid until the new spill is installed. A1 need not be a producer in C10's old backward cone. Publishing C10 as settled at 1 is therefore unsound.

The read certificate must include occupancy/absence and unresolved-producer barriers, not just existing edges. The proposed may-spill index can implement this, but "conservative reach" needs an operational definition.

Unknown shape can force a very broad barrier: many dirty anchors above a viewport may potentially reach its blank cells. Measure thousands of such anchors, most ultimately returning scalars. Count how many block a visible reader and measure time to its first **settled**, not merely provisional, value.

Spill reservation also needs a deterministic conflict policy, dependency invalidation when an old footprint disappears, and treatment of obstructions, merged cells, tables and grid edges. Treating reservation, edges and values atomically is necessary; it does not select the policy.

#### An SCC is a graph-analysis unit, not a complete iteration algorithm

~~~text
A1 = (B1+1)/2
B1 = A1/2
C1 = LogPair(A1,B1)
~~~

Starting from cached zeros, one A-first sweep gives (0.5,0.25); one B-first sweep gives (0.5,0). These are mathematical examples, not asserted Excel outputs.

An IterationSession needs initial caches, prepasses, participant order, stopping rules and the effects of every semantic sweep. Microsoft's account includes circular dependents and volatile work in iteration, and describes sheet ordering. [S8] Running each SCC privately to convergence and its downstream DAG once can change both values and UDF traces.

Probe one/two iterations, nonconvergence, errors, sheet order and a downstream counting UDF. Dynamic edges may merge or split SCCs during the process. Execution metadata should permit that without treating every discovered cycle as an ordinary permanent cycle error.

#### A data-table run is a scenario transaction

The table must isolate substituted inputs, reference resolution, name memoization, cached callback reads, volatile invocation identity and temporary outputs. Publishing a scenario value into the base workbook's cache would contaminate unrelated evaluations. The base workbook must remain unchanged whenever other work can observe it. Retain the scenario overlay and execution state across yield; release or restore temporary state on completion, cancellation or error.

A shared FunctionContext decorator is not enough: specify an InputView plus scenario-specific result/read view and publication destination. Probe duplicate scenario inputs with RAND and a counter UDF, a reference-valued name, callback reads, and Automatic Except Tables. Preserve the relevant Excel treatment of table-induced cycles; Microsoft describes a special case for these. [S9]

## 3. Measurements that settle the unproven claims

These are targeted experiments, not a new mandatory per-commit process.

| Question or claim | Decisive experiment | Record |
|---|---|---|
| VBA read "calculates first" | Peek a dirty non-argument cell in Manual/range/full calculation | Old/new observations, nested calls, dirty state |
| Effect order survives interest changes | Same workbook, alternate viewport priority around Peek and a pure producer | Invocation order and values seen |
| Range.Value and save are completion barriers | Read dirty values; save with CalculateBeforeSave false/true | Cache XML/binary data before reopen |
| One effect invocation per unit | Counter before a late dynamic dependency; cold/warm/rebuild chain | Semantic passes versus scheduler retries |
| Tick/cell random key is sufficient | RAND pairs, LET, named RAND, repeated LAMBDA, documented volatile spill | Draw count, shape passes, final errors |
| SCC convergence is the iteration contract | One/two iterations across sheets with logging dependents | Prepasses, sweep order, seed caches, termination |
| Table context forwarding is sufficient | Scenario overrides with names, RAND, UDF cache reads and cancellation | Base/scenario values and restoration |
| Spill shadow is selective | Many unknown-shape anchors potentially covering visible blanks | Blocked readers, first settled latency |
| Equal template text is safe to coalesce | Formula versus Formula2 SQRT example through all entry/save routes | IR, @ behaviour, shape, SavedAsArray |
| Formula transforms generalize from row insert | Copy, cut/move, partial insert/delete, sheet copy/move, undo | Rewritten formulas and affected bindings |
| NameId binding invalidates correctly | Add/delete shadowing local name after successful global lookup | Existing/new formula bindings |
| Names scale after per-table Arcs | Edit one of 100k–1m names with snapshots/closures retained | Copied bytes, retained generations, rebind count |
| Callback ABI survives reload | Suspend F(Double); load F(String); resume and invoke retained handle | Code/state generation, failures, reclamation |
| Stored Range survives safely | Insert/delete/cut, switch workbook, close/reopen, multi-area Range | Parent, Address, Areas, Value2, errors |
| Typed path has the intended performance | Empty/scalar/string/array procedures and one Range callback | Allocations, conversion bytes, latency |
| Finite budget means responsiveness | One very expensive formula plus queued edit/interest | Longest non-yielding span, p95/p99 latency |
| Explicit FEC overhead is acceptable | Arithmetic, large range, name-heavy and callback workloads | Dispatches, allocations, bytes/read, throughput |
| Block storage survives editing | Dense fill, sparse membership, scattered edits, insert/undo cycles | Peak memory, block fragmentation, save size |
| Tiles remain coherent under pressure | Multi-tile spill, dictionary rotation, slow subscriber, reset | Atomic mirror state and bounded retained bytes |
| Native runtime improves the product | Same provider churn/CPU workload on adapter alternatives | Lifecycle failures, responsiveness, overhead, binary/build cost |

For Excel observations, retain input files, operations, saved caches, build/platform/CPU provenance and trace outputs. Documentation identifies the questions; where order or edge-case behaviour is unspecified, the target Excel trace decides.

For scalability, use operation counters over a geometric ladder of workbook sizes. Large corpus and schedule-exploration runs remain explicit or scheduled, in line with owner 4.6. Keep the fast loop focused on concrete regressions and contract invariants.


# Appendix C: what we have measured about VBA UDF recalculation (docs/plan/VBA_UDFS.md)

# VBA functions in DNA OneCalc

`dc-snz.3` value calls are closed; `dc-snz.5` remains in progress (2026-10-31).
Only `calc-vba` depends on the OxVba embedding API, pinned to
`c7ce134918f21038ec1f5f586ebfcac2d1851588`. Native builds prefer JIT;
browser builds omit JIT and use the interpreter. A native admission decline can select
the interpreter before any code runs. Runtime errors and exhausted budgets never retry.
The OxVba/CalcVba rename is deferred; OneCalc stays free of calc-engine/calc-doc/calc-host.

Load a standard module in **VBA functions**, enter 40 in A1 and `=Add2(A1,2)` in the
formula field to obtain 42. Load again to replace registrations and start fresh module
state; a compile failure leaves the prior module active. Unload removes registrations.
Nothing is persisted to a workbook. Public Functions register bare and qualified names
with ASCII case matching; Private functions and Subs stay private, and Main never runs.

The caller-owned calc-func registry and calc-fml resolver still own function registration
and invocation. Built-ins, locals and known defined names keep precedence. Plans retain
names rather than engine objects; callers recompile when adding a previously unknown
name. The default UserFunction entry still eagerly reads references for existing adapters;
the reference-aware entry borrows the evaluator's caller, reader and CalcFunc services.

One loaded VBA module retains globals, Static and Err across calls. Separate modules,
reloads and controller sessions have independent state. Optional arguments use folded
defaults, typed zero values or Variant Missing; explicit omitted positions differ from
blank cells. ParamArray packs a fresh zero-based array, including empty bounds 0..-1.
ByRef inputs remain private copies. Shared immutable code never shares mutable VBA state.

A single rectangular reference now becomes a live read-only Range object. Supported:
Value2, Count, Row, Column, Address, Cells/Item, and Rows/Columns Count. Variant reference
arguments have TypeName Range; a numeric literal is Double. Application.Caller identifies
the formula cell. VarType reflects the live Range default value (Number5, Empty0, text8, Variant array8204),
while TypeName/IsObject retain the Range. Date/Currency format-dependent defaults and other
object defaults remain open. Saved Range references read current inputs through the next call's
reader, so reads are recorded and missing inputs remain explicit refusals. Returning a
Range materializes Value2. Multi-area ranges, writes and other object members remain open. The bounded host
deduplicates Range identities by coordinates; VBA object-identity parity remains unmeasured.

Application.WorksheetFunction calls SUM/MIN/MAX/AVERAGE (1..30 arguments) or MATCH
(2..3 arguments) through the ordinary CalcFunc evaluator and its existing reader and
numeric policy. A failed MATCH raises catchable VBA error 1004. Scalar parameter positions
accept scalars or one-cell references; arrays and larger ranges are refused until measured.
Callbacks do not inherit the outer formula's implicit intersection or array lifting.
Other methods and unsupported argument counts are explicit host refusals. VBA On Error
and formula IFERROR cannot hide a missing read or a recorded host capability refusal.

Numbers, booleans, blanks, valid UTF-16, known worksheet errors and rectangular arrays
cross the adapter. CVErr keeps its code. VBA runtime failures, wrong arity, conversion
failures and exhausted budgets map to #VALUE! with diagnostics; ordinary VBA failures
can be caught by formula IFERROR. The per-call budget counts basic blocks, including
initialization and nested VBA calls; On Error cannot catch exhaustion. Compilation,
conversion and an executing host call are outside that cooperative budget.

A bounded per-thread cache retains up to 16 verified immutable modules and 8 MiB of
key/image descriptions. Keys include exact program/signature/defaults, backend, host
policy, target/CPU/codegen settings and helper/carrier ABI. This limit does not measure
executable arena memory. Reload can reuse code while starting new state; clearing or
evicting cached code leaves live registrations valid. No disk or cross-thread cache claim.

Excel run-3 evidence is retained in OxVba CORE-8/udf-run3 and udf-run3-objects. On Excel
16.0 build 20527, nonvolatile state stayed unchanged on ordinary recalculation without
changed inputs, advanced on changed input/explicit Range.Calculate, and volatile state
advanced each time. Initial calls happened twice; no fixed initial count is assumed.
Application.Volatile requests now reach the formula caller as evaluation effects.
An ordinary UDF plan is nonvolatile; its executed requests determine the caller cell's
volatility, separately from built-in volatility. Precise engine scheduling is still
in progress under dc-snz.6. Do not infer recalculation parity from persistent-storage tests.

Fresh observations are retained in
`crates/calc-vba/tests/fixtures/excel-recalculation/` (17 stages, 23 formula cells).
The existing VBE supervisor compiled explicitly with command578 and watched owned
dialogs before runtime. Normal startup crashed in the installed TestCrashGuard add-in;
the successful observations use safe mode and retain zero owned-process residue.
No security, Trust Center or registry setting changed. Raw results, source and hashes
remain separate from the derived observations. The exploratory expected text was
OBSERVATION_ONLY, so the supervisor's overall passed flag is false despite clean
compile/run; these are development observations, not certification.

Clean ordinary calculation and unrelated edits preserve a nonvolatile UDF's state.
Rewriting a referenced input with the same value still calls it again. A dirty upstream
formula also calls its UDF dependent even when the upstream result stays unchanged.
Explicit Range.Calculate and full/rebuild calculation call nonvolatile UDFs too.
Thus caching by argument values would be wrong. A zero-argument nonvolatile function
with Static state also remains unchanged during clean calculation.

Volatility is local to the formula cell, including calls of the same VBA procedure
with different flags. The last executed Application.Volatile request in the formula
wins; an ordinary call after a volatile call does not clear the request. An untaken
branch makes no request. Each reevaluation starts nonvolatile, so a conditional UDF
that stops requesting volatility becomes nonvolatile. A nested VBA helper's request
affects its caller; a request made outside a worksheet UDF has no scheduling effect.
Internal VBA reads of a cell absent from the formula's arguments did not create an
Excel dependency; its old value persisted until explicit calculation. Wider saved-Range
dependency behavior in the adapter remains an open parity question.

OneCalc deliberately calculates its one formula whenever its controller is asked.
That explicit call advances state even with unchanged inputs, like a forced range
calculation. It does not acquire a workbook graph or infer automatic dirty scheduling.
Workbook invalidation and ordinary/full recalculation belong to CalcEngine/CalcHost.
Microsoft's [Application.Volatile documentation](https://learn.microsoft.com/en-us/office/vba/api/excel.application.volatile)
and [recalculation documentation](https://learn.microsoft.com/en-us/office/client-developer/excel/excel-recalculation)
describe the broad rule; the retained observations specify the cases above.

Still open under dc-snz.5/OxVba bd-59co.2.10.6: exact volatility/effects and scheduling,
full Range/Caller/WorksheetFunction surfaces, locale-sensitive names, general object
reentry/disposal, End reset, and classes/UDTs in the embedding facade. Workbook/macros/
events and the desktop GUI shell remain in the broader dc-snz epic and dc-snz.1.

Validation: portable calc-vba tests explicitly compare interpreter/JIT calls on native;
apps/dna-onecalc/tests/vba.rs covers the controller route in native and browsers, including
optional/ParamArray/state, live objects/callbacks/refusals and cache reload ownership.
The existing dc-snz.3 acceptance and mounted UI cases still own load/reload/unload and
budget behavior. Final pinned revision and measured check results are in the run-3 report.

Native Git-pin verification: four adapter tests and seven controller tests pass;
the final repository check passes 1,307 tests, zero failed, three ignored.
Browser and structural verification passed on the previous pin (sweep with repository-wide acceptance skipped). The VarType correction now has final-pin native/browser checks; their results are recorded in the run-3 report.

Final pin native check: 1,314 passed, zero failed, three ignored. The controller
checks live default-type reflection and refusal propagation, with explicit native JIT
selection. Cells/Item currently admit positive indices; other offsets and full Range
identity/default-format behavior remain open. Final focused browser/acceptance and
corpus readings are retained in the run-3 report and codex-runs logs.

Final run-3 receipts: final-pin adapter4/controller7 pass in each Chrome/Firefox;
Windows and wasm dc-snz.3 acceptance pass, including the mounted load/reload/failed
load/budget/unload cases. Native CLI reports `5:Range:True` for Probe(A1).
The corpus remains 19,284/20,030 matched, 659 failed, with 73 external-link and14
circular-rule values separate. The repository-wide acceptance suite was skipped;
no whole-workset completion or Linux execution claim is made. Exact logs/report are
under C:/Work/DnaCalc/codex-runs (calcvba-report-3.md).
