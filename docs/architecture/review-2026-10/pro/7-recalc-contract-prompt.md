# Question for GPT-6 Pro: specify DNA Calc's recalculation contract for the first end-to-end slice

You red-teamed DNA Calc's target architecture earlier (your review is Appendix C). The synthesis
has been revised to accept all twelve findings (Appendix A: the final synthesis; Appendix B: its
appendix with detail and the verification of your findings against the code). Before the engine
lane writes code, we want the recalculation contract written down precisely enough that three
coding agents can implement it without inventing semantics.

Please produce a specification for the contract envelope and the first slice (your seven-cell
workbook and its eight demonstrations, Appendix A section 4, phase 2):
1. The types and their invariants, in Rust-flavoured pseudocode: CalcCommandId, CalcPassId,
   InvocationId, EvaluationTarget, ReadPolicy / read view, ObservationFrontier, ReadCertificate,
   ResultStamp, ObservableChange, PreparedTransaction, PublicationUnit, CompletionReceipt, and
   whatever else you find necessary; for each: who creates it, who consumes it, lifetime, what it
   must never be confused with.
2. The state machine of a command and of a calculation pass (including yield, supersede, cancel,
   shutdown), and of an invocation with irreversible effects (admitted once, never retried) and of
   an asynchronous completion (accepted or rejected by identity).
3. The rules for freshness: what "fresh for command X" means, how scoped calculation (Manual mode,
   Range.Calculate) reports it, how a visible cell may publish before unrelated work completes
   without claiming global freshness, and how blank-cell occupancy by pending spills is handled.
4. The read view a VBA UDF sees (cached vs calculated values) and how cached observations
   constrain scheduling.
5. Revision-aware transactions: authored edits arriving mid-calculation, invalidating conflicting
   pure work while reusing validated results; undo; save pinning one model revision and one result
   view under CalculateBeforeSave true/false.
6. The test oracle for each of the eight demonstrations: inputs, operation sequence, expected
   trace and values, and the invariant each checks; plus the property tests that should run on
   every commit (cheap) and the exploration tests that run on a schedule.
7. Where Excel's own behaviour is still unmeasured and the contract must keep both options open
   (list them with the experiment that decides).
Keep the envelope small: these are a handful of types used by immediate callers, not a framework.

---

# Appendix A: SYNTHESIS.md

# DNA Calc target architecture: synthesis (final)

Date 2026-10-31. Inputs: BRIEFING.md (owner rulings 4.7 and 4.8 override everything), four drafts, two review
rounds (`reviews/`, `reviews2/`), the GPT-6 Pro red-team, VBA-hosting and symbolic-search answers (`pro/`),
foundation-ideas.md and the risk files. Code: DnaCalc-review **5a9348b** (`git show`; HEAD dbcc791 adds
`docs/plan/SYMBOLIC_SEARCH.md`); OxVba **0ad2ad28** for CalcVBA state (calc-vba still pins c7ce1349).
Detail lives in `SYNTHESIS_APPENDIX.md`: full Excel fact-check (A), round-2 seams and positions (B, C), the
red-team findings checked against code (D), measurements (E), VBA hosting detail (F), research claim record (G).

Attribution: **Opus**, **Fable**, **GPT** (GPT-Astra 6), **Gemini** (Gemini 4 Argon), **RT** (GPT-6 Pro
red-team), **VBA-A** (GPT-6 Pro VBA answer), **SS-A** (GPT-6 Pro search answer). **[4.7]**/**[4.8]**: an owner
ruling decided against a position.

## 0. What Excel persists (summary; appendix A)

- **xlsx**: cell formulas as A1 text; `t` = normal | array | dataTable | shared. A shared group's master holds the
  text and a bounding `ref`; membership is per cell (`si`). Dynamic arrays are marked by `cm=` metadata. Prefixes
  (`_xlfn.`, `_xlpm.`, `_xleta.`, ANCHORARRAY, SINGLE) and dynamic-array storage follow measured per-function
  tables (`calc-doc/src/save/file_form.rs:1-74`). Defined names carry `localSheetId`; their relative references are
  stored from A1 and wrap at the grid edge (measured, `calc-fml/src/reference/mod.rs:236-250`).
- **xlsb**: parsed token streams; shared formulas as BrtShrFmla with PtgRefN offsets, members as PtgExp; names by
  index. The codec converts xlsb to SpreadsheetML text before loading and **saves as .xlsx**
  (`calc-doc/src/xlsb/mod.rs:1-14`); there is no same-format xlsb writer (decision 12).
- **A1 vs R1C1** differ observably in INDIRECT's `a1` flag, names that look like references, and off-grid
  relative references (names and INDIRECT-R1C1 wrap, measured; cell copy/fill routes unmeasured).
- **Consequence for 4.8**: Excel persists templates with relative offsets over areas; memory holds templates over
  blocks with plain coordinates, explicit membership and a semantic dialect, A1 at the edges, no row/column keys,
  optional indexes only where measurement shows a bad case.

## 1. Architecture

### 1.1 Layers and explicit contexts (all four; owner 4.1)

| Boundary | Context (every field required, no default methods) |
|---|---|
| func <- fml: `FunctionContext` | numeric mode; locale; date system; caller from the instance; span/range value reader; reference-text resolver; volatile draws by `InvocationId`; callable invoker; `NumberText`; `HostQuestions` covering all of today's `HostFacts`/`WorkbookFacts` (`calc-func/src/call.rs:113-216`) |
| fml <- engine / OneCalc / CF-DV / scenario: `FormulaEvaluationContext` | the above plus `ReferenceCatalog`, `CallableTable` snapshot, `EffectSink`, `EvalControl` (budget, cancel), and per read a `ReadRequest { purpose, policy, observation }` (1.3) |
| engine <- host: `CalcContext` | `SemanticEnv` apart from tick services; external operations (1.3); facts with versions **and** invalidation policy; typed CF/DV; `InputView` |
| host <- platform | clock, application facts, file identity, locale, font metrics, thread policy, runtime providers, security profile |
| model <- skins | commands, availability (advisory), projections, `ViewStateStore`, focus registry |

Absent services are typed `Absent(Owner)` refusals. A decorator overrides one service (today `WhatIf::evaluate`
drops host facts, random and functions, `calc-engine/src/data_table.rs:248-253`). The six forwarders and the
`call.rs:385-398` fallback go in one change, **together with read policy and invalidation facets** (RT: otherwise
the explicit context carries the old ambiguous semantics). The frame is built once per evaluation; hot scalar
fields stay direct, rare host services sit behind narrow interfaces, range readers record batch read stamps, so
SUM allocates no per-element certificate (RT 9; measured, appendix E).

### 1.2 Representations

**Formula blocks [4.8 overrides stable axis keys: Opus, GPT, Gemini, foundation R10].**
- Semantic identity: `SemanticTemplateKey { normalized_ir, placement, evaluation }` (RT 4). `evaluation` carries
  the dialect: `FormulaForm::{Legacy, Dynamic}` already exists (`calc-fml/src/eval/mod.rs:107-113`) and the
  engine already keys plans on it (`calc-engine/src/state.rs:401`); round 2's `Template` dropped it. Every ingress
  (UI, clipboard, `Range.Formula`/`Formula2`/R1C1 setters, files) sets it; `=SQRT(A1:A4)` via `Formula` and via
  `Formula2` never coalesce. `placement` covers cell `#REF!` vs name wrapping and template kind (Cell | Name | Rule).
  Authored spelling and per-member calc/file attributes (`ca`, `cm`, cached value) stay outside the interning key.
- `FormulaBlock { id, sheet, extent, membership: Dense | Runs | Bitmap, key }` over plain coordinates; a member is
  `(BlockId, offset)`, identity = address at a revision. Bound plans live in fml/engine caches. `BlockId` is not
  `NotebookBlockId`. Results and dirty sets are **proportional to members or populated chunks**, never to the
  bounding extent (a sheet has 17,179,869,184 coordinates); area cardinalities use checked wide arithmetic.
- **Edit algebra** (RT 4): copy (relative references move), move/cut (the formula keeps its precedents; references
  to moved precedents follow them), insert/delete of whole and partial rows/columns, sheet copy/move/delete (sheet
  order binds 3-D references). Each is a separate `AddressTransform` over reference occurrences; a block relocation
  is not one offset adjustment. Cost of an insert as in appendix B S2; optional indexes (reference occurrences,
  lazy row shifts) where counters show a scan. Bounded, optional compaction; fragmentation is benchmarked.
- **Save mode** per block: `SharedGroup` when eligible (one key, `si` on members only, master rewritten after
  shifts), else individual formulas; arrays, dynamic arrays and data tables keep their forms. Loads coalesce runs
  of equal keys from other writers and tolerate a dependent carrying text (appendix A).

**Defined names: first-class non-cell nodes, in large numbers [4.8].**
- `NameTable`: `NameId`, `scope`, folded key, name template, attributes, binding version, persisted notebook order;
  hashed lookup (today a linear scan, `calc-engine/src/read.rs:222-226`); tables in the same namespace; storage
  paged or structurally shared, since per-table `Arc`s still clone a million-entry map on one edit (RT 5).
- Formulas bind to `NameId` with a **`BindingProof { resolved, probes }`**: the namespace probes that led there,
  including misses that allowed a fallback (Sheet2!Foo absent, so workbook Foo), so adding or removing a shadowing
  name rebinds exactly the affected formulas (RT 5). Excel may keep or rewrite existing bindings; probe first.
- **Definition vs evaluation**: a name node is its definition and binding version. Results are reference-or-value
  per evaluation locus (Excel evaluates names per referring formula, not when unused). A shared memo requires
  **transitive** eligibility over names, callees, captures and facilities (`Outer = Inner+1`, `Inner = RAND()` is
  not eligible), re-derived when any dependency changes, plus read-certificate validity inside its scenario;
  scenario id is a lookup key, not a proof. LAMBDA closures own their plan and captured environment.
- **Name views** (RT 5, replaces round 2's "inspection adds no evaluations"):
  `NameView::{Definition, ExistingEvaluation { invocation }, Evaluate { anchor, command }}`. Passive inspection
  shows the definition and existing qualified results; `Evaluate` is an explicit command that may run RAND or a
  UDF, with visible anchor, scope and freshness.
- No other authored non-cell node kind **[4.8]**; activations, scenarios, certificates and iteration sessions are
  execution metadata (RT agrees).

**Values, styles, IDs.** Engine results columnar per block; authored pages unchanged until measured. `StyleRuns`;
`LoadedSheet.styles` and other `axis_edits` replays fold into the model. Excel grid limits **[4.8]**. Workbook
identity (`WorkbookInstanceId` with incarnation) is typed into references, callable frames, async targets, Range
handles and save requests; a `CalculationDomain` above workbooks owns application-wide calculation mode, iteration
and `Application.Calculate`, workbook settings (date system, precision) stay per workbook (RT 3). `PartitionId`
and `InputView` stay types only.

### 1.3 Recalculation contract (rewritten after the red-team)

Three things are separated (RT): **execution order** (what runs when), the **read view** an invocation may observe,
and **publication** to subscribers. Physical computation may run ahead; it is exposed to an invocation only through
its read view, and to subscribers only through publication units.

```rust
struct CalcCommandId; struct CalcPassId;                  // a command runs one or more semantic passes
struct InvocationId { command: CalcCommandId, pass: CalcPassId, locus: EvaluationLocus,
                      call_site: CallSiteId, occurrence: u32 }   // occurrence: LAMBDA, array, recursion
struct EvaluationTarget { workbook: WorkbookInstanceId, origin_revision: Revision,
                          formula_generation: FormulaGeneration, locus: EvaluationLocus }
struct ReadRequest { purpose: ReadPurpose, policy: ReadPolicy, observation: ObservationFrontier }
enum ReadPolicy { DependencyWithinCommand, CachedMacroObservation, RegisteredWorksheetCallback, DiagnosticSnapshot }
struct ReadCertificate { value_versions, invalidation_versions, absences, binding_proofs, facts }
struct ResultStamp { computed_by: CalcCommandId, reads: ReadCertificate,
                     completion: CommandCompletion, freshness: Freshness }
struct PreparedTransaction { base: Revision, command: CommandId, forward: ModelPatch,
                             inverse_authored: ModelPatch, coordinates: AddressTransform, bindings: BindingDelta }
enum ExternalOperation { AsyncReturn { invocation, result }, RtdUpdate { subscription, sequence, value },
                         LinkRefresh { source, revision } }
struct PublicationUnit; struct CompletionReceipt;          // receipt per command, with its admitted frontier
```

- **Commands and scope.** Demand comes from mode, command (`Auto | CalcNow | CalcSheet | CalcRange | Full |
  RebuildFull`), target, table policy and forced-vs-dirty evaluation (`calc-host/src/recalc.rs:74-82`; no range
  calculation exists yet). A command completes its scope; it may read out-of-scope cached precedents, and the
  result then says so: **freshness is scoped, there is no single "settled"** (RT 2). A cell shown as current names
  the command and frontier it is current for.
- **Certificates** carry value **and** invalidation versions (a dirty formula can keep its old value version),
  occupancy/absence (no pending producer may occupy a blank that was read), binding proofs and fact versions.
  Pure units are validated at commit and may be recomputed with the same recorded nondeterministic inputs.
- **Effectful invocations**: a scheduler retry never repeats a semantic invocation; a suspended activation
  resumes; a new semantic evaluation (Excel may evaluate a formula more than once per calculation) gets a new
  `InvocationId`. Effects are admitted, recorded as started/completed/indeterminate, never replayed after a crash.
- **Cached VBA observations** (RT 1; evidence decides): Excel returned the old value of a cell a UDF read outside
  its arguments until explicit calculation (`docs/plan/VBA_UDFS.md:91-93`); today the engine discards the
  evaluation and recalculates first (`calc-engine/src/recalc.rs:607-610`), and round 2's "evaluate synchronously on
  demand" was wrong. Internal reads use `CachedMacroObservation`; they are not dependency edges and do not force
  calculation. Because early visible-first computation could change what such a read sees, unrestricted callbacks
  get **conservative fences** first (the read set is discovered inside user code, so fencing argument references
  is not enough); a logical read view that lets computation run ahead comes later. Visible-first publication and
  exact observations cannot both be assumed unrestricted; the frontier is chosen after the probes (decision 13).
- **Random and volatile draws** are keyed by `InvocationId` (call site and occurrence), not (seed, tick, cell):
  `RAND()-RAND()` draws twice, `LET(r,RAND(),r-r)` once. Today a formula re-evaluated in the same recalculation
  redraws the same values (`calc-engine/src/random.rs:4-8`); that must be a deliberate per-pass rule, since freezing
  draws across passes can manufacture stability for `SEQUENCE(RANDBETWEEN(1,1000))`, which Excel reports as an
  indeterminate-size `#SPILL!`. An edit merged into running work opens a new pass for volatiles.
- **External operations** are distinct events: an async return completes one invocation (rejected by
  `EvaluationTarget` if the formula moved, was overwritten or undone); an RTD update advances a subscription; a
  link refresh changes an input snapshot. None restarts a formula or repeats effects before the async call.
- **Spills**: a blank read needs an absence certificate; unknown-shape anchors that may reach it are barriers. A
  deterministic conflict policy (today first-established wins, `calc-engine/src/spill.rs:3`), invalidation when an
  old footprint disappears, and obstructions, merges, tables and grid edges are specified. Measure how many anchors
  block a visible reader.
- **Iteration**: an SCC is a graph-analysis unit; an `IterationSession` owns initial caches, prepasses,
  participant order, stopping rules and every sweep's effects, including volatile work and sheet order; dynamic
  edges may merge or split SCCs mid-session. Excel's order is measured, not invented.
- **Data tables**: a scenario transaction with its own `InputView`, read view, name-memo scope, callback reads,
  invocation identities and output destination; scenario values never enter the base cache; state survives yield
  and is released on completion, cancellation or error; table-induced cycles follow Excel's special case.
- **Transactions**: structural edits commit membership, inputs, references, names, styles, merges, rules and
  geometry as one `PreparedTransaction`. Undo creates a new revision (already so: `calc-host/src/command.rs:443-447`)
  and restores authored intent, never effects; derived state is reused only where certificates still hold. A
  command id delivered twice applies once.
- **Completion points** (RT 2 corrects round 2): `Range.Value`/`Value2` is a read with defined semantics, not a
  calculate-and-drain; save follows `CalculateBeforeSave` and may keep dirty caches; an explicit calculation has a
  scope and a `CompletionReceipt`; a snapshot export may demand stronger freshness as its own operation. Completion
  is relative to a bounded admission frontier (an endless RTD stream never reaches global quiescence).
- **Yield, supersede, cancel, shutdown** are four different operations. Budgets bound work inside cells: long
  scans, sorts, LAMBDA recursion and VBA loops carry resumable state; Exact kernels yield only at existing binary64
  store points or save the full x87 register and control state; JIT loops get safepoints where resumability is
  promised; a non-cooperative native call is declared non-preemptible. The browser test proves a real task handoff.
- **Fairness**: aging or a finite-work guarantee for off-interest work within the permitted order.
- **Invalidation classes kept apart** (RT 9): authored semantic changes (including number formats under
  precision-as-displayed), observable host-fact changes, CF evaluation demand, and local rendering. Recording a fact
  version protects a commit; it does not dirty published readers, so each facility declares a policy. Early cutoff
  compares observable facets (formula text, kind, shape, reference identity, visibility), never only `f64`, and is
  off for cones with effectful callables.
- **Replay** has two goals: semantic replay (same authored state and admitted events give the same values and
  effect trace) and publication replay (recorded admission and publication decisions). A runtime seed provides
  neither alone.

### 1.4 Concurrency, scheduler and asupersync [4.7]

- `calc-sched` with our serial stepping executor (reference twin, browser driver). DNA Calc owns command demand and
  ordering, ready queues and interest priority, effect admission, checkpoints inside kernels, invocation identity
  and stale-return rejection, publication, freshness and replay (RT). Parallel levels accept a finite budget;
  native pool and block scheduling come after the contract works, without changing read, invocation or publication
  semantics.
- Owner-lane mailbox with **invocation-aware reentry**: engine waits on VBA, VBA calls the host, the host request
  queues behind the waiting activation: a deadlock with no lock held (RT 7). Cached reads, WorksheetFunction
  callbacks, COM STA reentry and macro operations go through the invocation's frame, not the general inbox.
- **asupersync [4.7]**: permitted, rider in notices, native-only adapter for task execution, channels, timers,
  cancellation delivery, resource draining and provider supervision; `Cx`, `Region` and `Outcome` never leak into
  calc-fml, calc-func or the scheduler interface. Never one region per recalculation that must close before the
  next edit; never cancel-and-respawn to reprioritise. Spike off the critical path on provider churn (delayed async
  returns, RTD bursts, reload), pinned version, features and toolchain (the red-team reports a nightly default
  feature set in 0.5.0 docs; to verify), compared with the serial driver plus a modest pool.

### 1.5 Commands, skins and wire (owner 4.2)

- Host commands in calc-host; the actor inbox separates `ActorMessage::{Command(CommandEnvelope), External,
  Interest, TransportAck}` (RT 11). `CommandEnvelope { id, base_revision, resolved_target, source, undo_group }`;
  availability is revalidated at execution. A UI adapter owns focus and browser-authorised effects; an old response
  never focuses a newly mounted skin or pastes into a moved target.
- UI model DOM-free (registry, neutral `Target` including `Name`, `EditSession`, `ViewStateStore`, focus registry,
  `view.reveal`); shell keymap as data; skin contract `mount`, `focus_target`, gestures, `view_state_schema`.
- **Notebook over cells or names [4.8]**: switching changes the projection only; names shown through `NameView`.
- **Wire**: control envelope apart from binary tiles; `TileFrame { base, next, revision, dictionary, transaction:
  PublicationUnitId, final_fragment }`, so a multi-tile spill applies atomically; dictionary definitions precede
  references; removals explicit; geometry and style versions related to value tiles; per-subscription continuity or
  explicit coverage; bounded queues with coalescing and atomic reset. A transport trait keeps the in-process server
  out of the page (`apps/dna-calc/src/port.rs:44-48`).
- **Accessibility** is a semantic projection from committed units (cell identity, coordinates and totals, headers,
  spans, selection, editability, pending/refused text), independent of painted tiles; real-browser focus, keyboard,
  IME and screen-reader checks stay (RT 11).

### 1.6 CalcVBA hosting and early binding (owner 4.5; VBA-A, RT 7)

- **State at OxVba 0ad2ad28**: `ComCallEarly` carries the compiler descriptor to `ComHal::invoke_early_bound` in VM3
  and JIT; native x64 dual calls use vtables; remaining cost is per-call Variant vectors and a `HashMap`
  (`oxvba-vm3/src/lib.rs:4264-4287`), libffi, a JIT helper and Variant-backed JIT frames (CORE-7).
- **Four tightenings (VBA-A)**: two call contracts (module procedures as typed callable entries with explicit state
  ownership; host objects as typed interfaces), sharing type descriptions but never making procedures COM objects;
  early binding is not low overhead; common semantics with explicit per-target ABIs (Windows COM, Linux native
  projection, wasm handles and generated adapters; `#[repr(C)]` fixes layout only); lifetime and observable state
  are part of the contract.
- **Ownership objects**: `ProjectImage`, `ProjectSession`, `BoundProcedure`, `Invocation` (appendix F). Entry
  profiles: typed native entry `(session, invocation, typed args, *result) -> CvStatus` reaching typed execution;
  bound typed-frame entry for the interpreter; a separate explicit Variant adapter. Genuine Variant signatures
  (`Range.Value2`, `WorksheetFunction.Sum`) stay Variant.
- **`CallableLease { project, generation, signature, code: CodeLease, state: ProjectStateLease }`** (RT 7): an old
  activation never runs against reset globals; replacement publishes at an execution boundary; leases may cover a
  batch. **`InvocationFrame { id, parent, view: ReadView, lease, mode, reentry: ReentryPolicy }`** carries
  continuations and permitted nesting.
- **Typed descriptors** cover ByRef aliasing and writeback, omitted/Empty/Null/Error, array bounds and element types,
  string and object ownership, error propagation (`Success`(may be `CVErr`), `UnhandledVbaError`,
  `ForeignCallFailure`, `Cancelled`, `BudgetExceeded`, `StaleBinding`, `HostExecutionAbort`), permitted callbacks
  and suspension. Performance promise: allocation-free steady-state **scalar** calls; objects, strings and arrays
  measured separately.
- **Range handles**: workbook instance and incarnation, sheet identity and generation, `AreaSet` (multi-area, not a
  bounding rectangle), structural-tracking policy; resolved against the invocation's read view; an index of live
  handles, not per-cell cost.
- **Object library** in `calc-object-model`, generated from one descriptor model into IDL/`.tlb` (MIDL or
  `CreateTypeLib2`), native vtables, portable adapters, interpreter adapters and JIT lowering, round-tripped through
  the importer. **GUIDs** (VBA-A, RT): DNA's own LIBID, CLSIDs, ProgIDs and IIDs with an explicit Excel
  source-compatibility mapping; an Excel IID is reused only by a facade implementing that interface's complete
  contract; type-library identity, activation identity and coexistence with Excel stay separate (decision 9).
- **Composition**: calc-* libraries depend only on `calcvba-host-api`; each app binary composes the runtime provider.
- **Security** (RT 7): trusted native vs sandboxed profiles with granted capabilities, memory/work limits and effect
  policy fixed at registration; a "pure" annotation proves nothing; a hard stop needs an isolation boundary.

### 1.7 Exact numerics and research (owner 4.4; SS-A)

- Excel follows the CPU it runs on: natively hardware x87 is the Exact path on any x86-64; vendor, model,
  program identity, numeric backend and floating-point environment are provenance; witnesses and published claims
  are per vendor (AMD measured). Soft path: arithmetic exact, transcendentals approximate today; under Exact,
  refusal is structured and follows the **executed** operation (an untaken LN branch refuses nothing), is distinct
  from a worksheet error, and counts as a fail. Switching Fast/Exact invalidates incompatible caches and memos.
- Research placement (SS-A 9.1): observation bank, numeric semantics, research tooling, product library, each
  owned; research in `tools/calc-func-lab` on calc-numeric and calc-verdict; `xtask` is the command surface via
  forwarding; the product never depends on search machinery. Identification is claimed as "unique survivor within
  grammar G, profile E, scope D, independently validated", not Excel's source.
- **Per-function claim record** replaces the bare status row (appendix G): claim strength (published fallback,
  conditional hypothesis, unique grammar survivor, independently validated, proved equivalent), hashes, bounds and
  coverage, versioned helper dependencies, survivors, first kills, exposure history, validation, product mapping.

### 1.8 Function metadata, defined once (owner question; RT on migration)

- Facts: `calc-func/src/catalogue.rs:1-11` is already a compile-time generated catalogue with stable `FunctionId`s
  (duplicate IDs or surfaces fail the build). Duplicates remain: `calc-func/src/file_prefix.rs`,
  `calc-doc/src/save/file_form_table.rs`, the nine-name `VOLATILE_FUNCTIONS` (`calc-fml/src/eval/mod.rs:190-201`),
  SUBTOTAL/AGGREGATE by spelling, `_xlfn` literals in about ten files, the 3-D, legacy-place and lifting tables
  (`eval/compile.rs:1438-1568`, `eval/legacy.rs`, `eval/run.rs:1123-1159`), the xlsb Ftab, `help.rs`, and the
  ExcelFunctionsHandbook repository.
- `calc-func-catalog` (data-only, on calc-value) owns every declaration per ID: surfaces and prefixes, Ftab index
  and arity, parameters and demand, 3-D admission, legacy places, lifting, volatility, effect class and facilities,
  subtotal role, file-form positions, help key. Where behaviour depends on arguments, registration or context the
  entry holds a **policy**, not a flag; thread safety, volatility and reorder permission are separate properties
  (RT 9). calc-func attaches kernels by ID; the build fails on an ID with neither kernel nor typed refusal. One
  prefix reader/printer in calc-syntax applies syntax roles (`_xlpm.`, `_xleta.`). Runtime callables stay in
  `CallableTable`.
- **Incremental migration** (RT): ownership is decided now; the schema and first consumers (volatility, file
  prefix, Ftab) land first; remaining consumers move in working slices; help and handbook export come last and
  never block the calculation work. Kept so by construction (only the catalogue constructs `FunctionId`) plus a
  periodic sweep check for stray name tables (owner 4.6).

### 1.9 Persistence and recovery (RT 8)

- `SaveRequest { model: Revision, results: ReadViewId, cache_policy: PreservePermittedCaches |
  RequireCalculatedScope(scope), format }`: save pins an authored revision and a result view; a mixed-revision
  cache is never written as fresh; stale caches carry the matching recalculation metadata.
- Formats: xlsx same-format; xlsb today converts to xlsx (decision 12). Opaque parts and relationships each get a
  preserve-when-safe, transform, convert or refuse policy; macro parts and signatures are never best-effort.
- Authored state, recoverable journal, calculation caches and external-effect records are kept distinct; recovery
  never reruns unknown completed effects.

### 1.10 Platform profiles

| | Windows | Linux | Browser |
|---|---|---|---|
| Recalc | serial + budgeted pool | same | serial worker with real event-loop yield; optional SAB pool |
| Exact | hardware x87, vendor provenance | same on x86-64; Soft elsewhere | Soft; executed transcendentals refuse under Exact |
| VBA | VM3 + JIT; COM projection | VM3 + JIT; native projection | interpreter, typed frames, generated host adapters |
| Extensions | XLL, COM, WIT (trusted or sandboxed) | WIT | WIT via jco |
| asupersync | optional adapter | optional adapter | never |

## 2. Seam pressures (summary; full text appendix B)

S1 function context vs presentation; S2 formula storage vs structural edits **[4.8]** (now with dialect, edit
algebra and save mode); S3 names at scale vs bind-by-spelling; S4-S5 visible-first recalc vs Excel semantics,
dynamic topology, effects and **cached observations**; S6 worker boundary and page bundle; S7 early-bound VBA vs
reload, reentry and state lifetime; S8 OneCalc's ceiling vs UDF registration; S9 skins vs one command model; S10
research vs product and numeric order; S11 environment vs services; S12 typed model vs lossless codec; S13 codec
save vs crate splits; S14 save under blocks. New in the final round: S15 **observation vs scheduling** (1.3); S16
**partial publication vs save and recovery** (1.9); S17 **identity across edits** for async returns and Range
handles (1.2, 1.3).

## 3. Target crate map

| Crate | Owns | Depends on |
|---|---|---|
| calc-value | values, ids, grid constants, workbook instance id | - |
| calc-func-catalog (new) | every function declaration (1.8); the only `FunctionId` constructor | value |
| calc-syntax (new) | lexer, parser, tree, semantic templates and dialect, A1/R1C1, prefix reader/printer, reference binding, address transforms | value, catalog |
| calc-numeric (new) | x87 substrate (hardware, soft), FP environment, `ExactDecimal`, digits | value |
| calc-func | kernels by ID, coercion, `FunctionContext`, `NumberText` trait | value, catalog, numeric |
| calc-fml | plans, FEC with read requests, evaluator with resumable kernels, `CallableTable`, LAMBDA, name evaluation, formatting module | func, catalog, syntax |
| calc-doc-model | pages, blocks, `NameTable`, `StyleRuns`, typed CF/DV, transactions and inverses | value, syntax |
| calc-doc | codecs; save modes; Rgce -> templates; package-part policies | doc-model, syntax, catalog |
| calc-sched (new); calc-sched-asupersync (optional, native) | executors, budgets, cancel kinds; adapter [4.7] | - ; sched + asupersync |
| calc-engine | contract types (1.3), certificates, scheduler, publication units, scenarios, iteration sessions, blocks | fml, doc-model, sched |
| calc-host | calculation domain, actor, command envelopes, journal, presentation, projections, save requests | engine, doc, sched |
| calc-object-model (new) | descriptor model, generated interfaces and adapters, Range handles | fml, value |
| calc-vba | registration, prepared adapters, leases, invocation frames | fml, object-model, calcvba-host-api |
| calc-host-vba (later) | macro-time backend, events | host, vba, object-model |
| dna-calc-model, -protocol, -worker, dna-calc; dna-formula-ui; dna-onecalc | model + transport trait; wire; translator + runtime composition; shell and skins; shared editor; OneCalc | dna-calc never sees host/engine |
| xtask (thin), calc-harness, calc-verdict, tools/calc-func-lab | scans; harness; verdicts; search | xtask: no product crates |

`calc-format` remains open (H); one `architecture.json`; kept as in round 2 (appendix C).

## 4. Implementation order (revised after the red-team)

Every step lands on master in working pieces and both apps keep working; no time estimates. The red-team's
diagnosis: 3b-1 as round 2 defined it would be built twice (cached reads, passes, absence certificates and effect
admission change what a certificate means), 1b would stall as one step, and C2/C3 would harden lossy storage.

**Phase 1: contract-shaping work, in parallel.**
- **P0 Excel probes that change API shape** (appendix E): cached UDF reads in Manual/range/full calculation;
  Manual range freshness; invocation counts and passes; RAND draw identity; module reload; Formula vs Formula2;
  CalculateBeforeSave. Run on this machine before the types below freeze.
- **0a/0b tools**: thin xtask and app smoke cases; deletions any time.
- **K1 contract envelope**: the small types of 1.3 (`CalcCommandId`, `CalcPassId`, `InvocationId`,
  `EvaluationTarget`, `ReadPolicy`, `ObservationFrontier`, `ReadCertificate`, `ResultStamp`, `PreparedTransaction`,
  `PublicationUnit`, `CompletionReceipt`), each with an immediate caller; not a framework.
- **K2 thin command envelope**: `ActorMessage` and `CommandEnvelope` in calc-host with the existing commands, before
  full U1.
- **E1** 1a clock fix (`calc-engine/src/state.rs:782-788`); 1c FEC in one change with read policy and invalidation
  facets; 1d `CallableTable` slots and binding proofs.
- **E1b catalogue**, incrementally: schema and ownership, volatility, file prefix and Ftab consumers first.
- **V1-V2 (VBA-A steps 1-2)**: neutral ABI with a non-spreadsheet host fixture (typed method, property, failing
  method, synchronous callback); then the complete typed paths, including one typed callback with reentry and
  reload exclusion through VM3 and JIT, measured against a plain native call. V0 stays: no new members in
  `oxvba-embed/src/spreadsheet.rs`.
- **C0 formula semantics decided before C2**: dialect at every ingress, the edit algebra and save modes, exercised on
  a sparse shared group, a Formula/Formula2 pair, absolute/relative/cross-sheet references, a relative name, a 3-D
  reference and CF/DV rules through copy, move, insert, delete, undo and save/reopen.

**Phase 2: the smallest end-to-end slice (RT).** One tiny workbook through the existing UI/worker route and a
headless harness; serial execution; no pool, no full skin extraction, no general iteration or macro object model:
```text
A1 = 1   B1 = A1+1 (off-screen)   C1 = B1*2 (visible)   D1 = PeekB1()   F1 = 1   E1 = SEQUENCE(F1)   G10 = E10+1
```
It must show: (1) a finite slice returns to the worker event loop; (2) interest changes reorder pure work without
changing calculation scope; (3) the command receipt precedes its values and C1 publishes before unrelated work;
(4) an edit between slices invalidates conflicting pure work and keeps validated results; (5) the typed callback
sees its permitted read view, and a counting UDF paused at a VM/JIT safepoint resumes without incrementing twice
(non-suspendable native calls follow the declared policy); (6) Manual calculation of C1 completes without claiming
global freshness; (7) F1 from 1 to 10 keeps G10 from settling on an unproven blank; (8) save with
CalculateBeforeSave off keeps the chosen cache policy and reopens coherently. Then a deterministic async provider
fixture returns after overwrite, undo and provider reload, and the stale results are rejected by identity.
Unsupported units take a bounded serial boundary with a reason and a counter, never an uncounted workbook fallback.

**Phase 3: widening on the proven contract.**
- **Engine**: spill absence at scale, `IterationSession`, data-table scenario transactions, names 2a (table, binding
  proofs, paged storage) and 2b (evaluation loci, transitive memo), then native pool and block scheduling (5b).
- **Codec**: C1 calc-syntax; C2 blocks with keys, membership and save modes; C3 block-aware save with pinned
  `SaveRequest`; C4 `StyleRuns` and axis replays; package-part policies.
- **UI**: U1 full command model, neutral `Target`, transport trait, UI/worker split; U2 `TileFrame` and accessibility
  projection; U3 renderer comparison (off the critical path).
- **CalcVBA**: V3 (VBA-A step 3) extract the spreadsheet model into calc-object-model and migrate registration
  transactionally; V4 type-library pipeline and Windows COM after the decision 9 observations.
- **Research**: R1 calc-numeric now; R2 `tools/calc-func-lab` with claim records; independent of the other lanes.
- **Off the critical path**: asupersync spike, renderer comparison, catalogue help/handbook export.

## 5. Open disagreements

Converged after round 2 (appendix C): block grain, publication staging, ordering, order policy wording, research
home, object-library crate, reload by generation. The red-team agrees with each, with the conditions folded into
1.2-1.6. Still open:
- **G renderer**: recycled DOM first (Fable, GPT) vs Canvas2D with an accessibility overlay (Gemini); RT: decide
  only after RenderPlan, tile mirror and a screen-reader check. Open until U3.
- **H formatting**: crate (Opus, page bundle) vs calc-fml module (others). Decide in U1 by measuring the bundle.
- **I layering check**: sweep plus a check when a manifest changes (Opus, Fable, GPT) vs every `xtask check`
  (Gemini). Recommend the former (owner 4.6).
- **Observation frontier** (new, RT 1): how far computation may run ahead of a cached VBA read. Settled by the P0
  probes, then decision 13.

## 6. Decisions for the owner

| # | Decision | Recommendation | Red-team position |
|---|---|---|---|
| 1 | One library dependency on `calcvba-host-api`; one runtime composition edge per app | Yes, after semantic signatures, code+state leases, reentry and per-target entry adapters are specified (V1) | Agree, with these ABI conditions |
| 2 | Retire plan 2.3's atomic generation | Yes; define scoped freshness, observation and effect semantics (K1) before any partial publication | Agree in principle; change sequencing (adopted) |
| 3 | Formula blocks now | Yes; C0 (dialect, edit algebra, save mode) before C2/C3 | Agree with prerequisites (adopted) |
| 4 | Names: definition vs evaluation; notebook over cells or names; CF/spills/tables not node kinds | Yes, with `NameView` (explicit evaluation), binding proofs and transitive memo eligibility | Mixed: rejected round 2's "inspection adds no evaluations" (adopted) |
| 5 | Exact: hardware x87 natively with vendor provenance; soft path refuses executed unmodelled transcendentals under Exact; Fast stays the browser default | Yes | Agree; refusal structured and per executed op (adopted) |
| 6 | Program search in `tools/calc-func-lab`, xtask forwarding | Yes | Agree |
| 7 | asupersync as optional native adapter, spiked off the critical path | Yes | Agree, narrower claim (adopted) |
| 8 | Excel probes before dependent steps | Yes, with the contract-changing probes first (P0) | Agree; promote them (adopted) |
| 9 | COM compatibility | Observe Excel's type libraries before V1/V3 freeze; DNA's own LIBID/CLSIDs/IIDs with a source-compatibility mapping; reuse an Excel IID only in a facade implementing its complete contract | Qualified agreement (adopted; changes round 2's "match IIDs") |
| 10 | Pause new UI feature slices and new `spreadsheet.rs` members | Yes, until K2 command ownership and the V2 typed callback proof land | Agree, with that exit (adopted) |
| 11 | Function facts once in `calc-func-catalog` | Yes, migrated incrementally | Agree, incremental (adopted) |
| 12 | XLSB: same-format writer or declared conversion to .xlsx (today) | Declare conversion in the capability surface now; same-format writing as separate work | Raised as a design gap |
| 13 | Observation frontier for cached VBA reads | Conservative fences now; choose the frontier after P0 | Raised (rank 1) |

**Missing decisions worth making now** (RT; recommend adopting all as stated):

| Area | Minimum decision |
|---|---|
| Authoritative state | Authored model, calculated cache, external inputs and per-skin view state have distinct owners and persistence |
| Transactions | Atomic authored edits, base-revision checks, command ids, inverse authored patches, validation before commit |
| Collaboration hooks | A server-sequenced operation log with revisions and target transforms; no CRDT, no stable axes |
| Command retry | A command id delivered twice applies once and launches no second effect |
| Irreversible effects | Started/completed/indeterminate records; no automatic replay after a crash |
| Application scope | Calculation policy and cross-workbook coordination owned by the `CalculationDomain` |
| Recovery | Journal/checkpoint schema, cache invalidation on restore, external-effect recovery policy |
| File preservation | Same-format vs conversion; ownership of unknown parts and signatures |
| Security | Trusted native and sandboxed profiles, capability enforcement, honest stop/timeout guarantees |
| Observability | Explain why dirty, why waiting, which read view, why refused, why a completion was rejected |
| Debugging | Observing a name or stepping a formula declares whether it creates an evaluation |
| Corpus testing | Keep failing workbooks and operation traces; shrink deterministically; compare state, shape, text, effects, persistence |
| Memory lifecycle | Bound snapshots, obsolete plans, closure environments, dictionaries, queues and object handles |
| Progress | Report a named command's admitted frontier and pending reasons, not one global "calculating" flag |

## 7. Changelog

- **Round 2** (Opus, Fable, GPT, Gemini reviews; owner notes): block membership, names as reference-or-value,
  OxVba 0ad2ad28 state, Exact per running CPU, function catalogue, split implementation order. Detail: appendix H.
- **Final round** (RT, VBA-A, SS-A; each red-team finding checked against code in appendix D):
  - 1.3 rewritten: execution order, read view and publication separated; command, pass and invocation identities;
    scoped freshness replaces "settled"; revision-aware transactions and certificates with invalidation versions and
    absences; external operations as three event kinds; spill absence, `IterationSession`, scenario transactions;
    cached VBA observations (round 2's synchronous evaluation withdrawn, evidence `VBA_UDFS.md:91-93`); draw identity
    by invocation (`random.rs:4-8` behaviour made a deliberate rule); completion points corrected; four cancel kinds;
    resumable kernels; fairness; invalidation classes; two replay goals (RT 1, 2, 3, 6, 9, 10, 12).
  - 1.2: semantic template key with dialect (`FormulaForm` already in code), edit algebra, save mode, sparse results;
    names with `NameView`, binding proofs, transitive memo eligibility, paged storage; workbook identity and
    `CalculationDomain` (RT 3, 4, 5).
  - 1.4-1.6: invocation-aware reentry, asupersync role narrowed; actor message kinds, `TileFrame` atomicity,
    accessibility projection; VBA four tightenings, ownership objects, entry profiles, code+state leases, invocation
    frames, Range identity, own GUIDs with compatibility mapping, security profiles (RT 7, 11; VBA-A).
  - 1.7-1.9: refusal per executed operation, research placement and claim record (SS-A); incremental catalogue
    migration and policies (RT 9); persistence and recovery with pinned save requests and XLSB conversion (RT 8).
  - 4: P0 probes, K1 contract envelope, K2 thin command envelope, C0 formula semantics, VBA-A steps 1-3, the
    red-team's smallest end-to-end slice, widening afterwards, pool and blocks after the contract (RT; VBA-A).
  - 6: red-team positions recorded; decisions 9 and 10 changed; 12 and 13 new; missing-decisions table added.
  - Appendix created for the round-2 detail and the new verification, probe, VBA and research material.

# Appendix B: SYNTHESIS_APPENDIX.md

# DNA Calc target architecture: appendix to SYNTHESIS.md

Date 2026-10-31. Detail moved out of `SYNTHESIS.md` to keep it within 500 lines. Sections A-C are the round-2
text kept for reference (section numbers inside them refer to the round-2 synthesis); D-G are new in the final
round (GPT-6 Pro red-team, VBA hosting and symbolic-search answers); H is the round-2 changelog.

## A. What Excel persists for formulas (full fact-check)


**xlsx** (ECMA-376 `f` element; MS-XLSX 2.2.3):
- Cell formula text is A1 without `=`; reference style is a separate setting (`calcPr/@refMode`). `t` = normal
  | array | dataTable | shared; `ref` on the master; `si`; `aca`, `ca`; data-table `dt2D/dtr/r1/r2/del1/del2`.
- **Shared**: master `t="shared" ref si` + text; dependents `t="shared" si`. `ref` bounds; **membership is per
  cell**; the loader checks only that members lie inside `ref` (`load/sheet.rs:680-682`) and expands each to
  its own A1 text (`:668-704`, `load/shift.rs:1-25`); a dependent carrying text fails as a second master
  (`:626-637`) where ECMA ignores the text (Fable(r2)); a touched group saves unshared (`save/sheet.rs:32-35`).
- **Arrays** `t="array"`, dynamic via `cm=` (`load/sheet.rs:558-610`); **data tables** `t="dataTable"`
  (`:521-548`). **Prefixes** (`_xlfn.`, `_xlfn._xlws.`, `_xlpm.`, `_xleta.`, ANCHORARRAY, SINGLE) and
  dynamic-array storage come from measured per-function tables (`save/file_form.rs:1-74`).
- **Defined names**: `<definedName name localSheetId hidden function vbProcedure xlm comment>`, 0-based
  `localSheetId` (`load/mod.rs:932-950, 1274-1285`). A name's relative references are stored **relative
  to A1** and wrap at the grid edge when placed (`RC[-1]` stored as `Nm!XFD1`): measured on Excel 16.0
  2026-10-23 and implemented (`calc-fml/src/reference/mod.rs:236-250`, `eval/compile.rs:1254-1260`).
  Under row/column edits a name moves only its absolute coordinates (`reference/transform.rs:25-30`).
  Today names are re-authored on save without the file-form pass (`save/author.rs:728-758`).

**xlsb** (MS-XLSB 2.2.2): formulas are parsed expressions (`Rgce`, reverse Polish). Shared: BrtShrFmla
(2.4.796) holds range and Rgce with **PtgRefN/PtgAreaN offsets**; a member's Rgce is a lone **PtgExp**
(2.5.98.40); offsets wrap (RgceLocRel 2.5.98.92); names by **index** (PtgName); PtgTbl + BrtTable for data
tables. The codec converts the package to SpreadsheetML first (`xlsb/mod.rs:1-14`): masters and ordinary
formulas are printed to A1 (`xlsb/formula.rs:1-10, 584-600`), members become empty `t="shared" si`
(`xlsb/sheet.rs:766-783, 829-884`), names print their spelling (`formula.rs:695-712`). The save path
splices the converted XML (`calc-doc/src/lib.rs:141-147`), so a token-to-template loader must keep the
converted parts or generate `<f>` at save (Gemini(r2)).

**A1 vs R1C1, observable**: (1) INDIRECT's `a1` flag (`calc-fml/src/eval/run.rs:1222-1240`; R1C1 offsets
wrap, measured, `tests/acceptance/dc_51ka.rs:253`); (2) names may not look like references in either style,
`R1C1` is a name token in A1 and `B3` in R1C1 (`reference/mod.rs:77-79`, `bind.rs:934-935`); (3) off-grid
relative references: names and INDIRECT-R1C1 wrap (measured), xlsb wraps (spec), the xlsx member mover
refuses, cell placement gives `#REF!` with copy/fill routes unmeasured (`reference/mod.rs:183-195`). GPT(r2)
and Gemini(r2) corrected round 1, which called the whole of (3) unmeasured.

**Consequence for 4.8**: Excel persists templates with relative offsets over areas, and calc-fml already has
`AxisRef::{Absolute, Relative}` (`reference/mod.rs:88-95`). In memory: templates over blocks with plain
coordinates and explicit membership, A1 at the edges, no row/column keys, placement rules per template kind.

Sources: [ECMA-376 f](https://webapp.docx4java.org/OnlineDemo/ecma376/SpreadsheetML/f.html), [MS-XLSX 2.2.3](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/5d1b6d44-6fc1-4ecd-8fef-0b27406cc2bf),
[MS-XLSB 2.2.2](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/220abf5e-f561-4333-9fe0-7ac590ed4ad5), [PtgExp](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/dc8d38db-f584-4ff6-b250-a854fe93b365),
[RgceLocRel](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/571409d8-2ed2-47cd-8792-be3c2c9556dc), [INDIRECT](https://support.microsoft.com/en-us/office/indirect-function-474b3a3a-8a26-4f44-b491-92b6306fa261), [Names in formulas](https://support.microsoft.com/es-es/Excel/names-in-formulas).


## B. Seam pressures (round 2 wording)


**S1. Function context vs presentation.** Format engine in calc-func and called by apps
(`calc-func/src/format/mod.rs:1052`, `apps/dna-calc/src/cells.rs:187`); grid width vs CELL("width")
(`calc-host/src/facts.rs:53-71`). Response: `NumberText`; one presentation resolver with base facts apart from
the CF overlay; width-dependent text owned by the resolver with platform metrics. No `Format` facet for CELL
(volatile, `calc-fml/src/eval/mod.rs:190-201`), but fact versions are recorded for validation.

**S2. Formula storage vs structural edits [4.8].** Shared groups expanded on load; a row insert rewrites every
formula (`calc-doc-model/src/structural.rs:1-31`, `calc-engine/src/edit.rs:479-609`); undo rebuilds the engine
(`calc-host/src/command.rs:447-459`). Response: blocks with membership; members individually dirty, ready and
cycle-checked; `Rect`/`Affine` edges, member bitsets, bounded chunks. **Cost, restated (GPT(r2))**: an insert at
row p on sheet S touches (a) blocks on S whose extent spans p (split there) or lies below p (shift); (b) every
reference occurrence, on any sheet, whose target on S is at or below p or whose range spans p, relative or
absolute (Excel moves both, `reference/transform.rs:6-11`); (c) for same-sheet relative references, the members
whose row and target straddle p (a run of at most |d| rows per reference), which split the block; (d) range
endpoints and grid-edge clipping. 3-D references and INDIRECT text are left alone (`transform.rs:19-23`); names
move absolutes only. Round 1's "at most 1 + 2r pieces" holds only for (c) and is withdrawn as a general bound.
Optional indexes, built where counters show a scan: a reference-occurrence index over all targets (absolute,
cross-sheet, untaken branches), and a lazy row-shift index. Undo: inverse model and engine patches.

**S3. Names at scale vs bind-by-spelling [4.8].** Linear lookup, whole-`Definitions` clone, all templates cleared,
name text inlined, LAMBDA closures tied to one plan. Response: 1.2.

**S4-S5. Visible-first recalc vs Excel semantics, dynamic topology and effects.** Off-screen spills into visible
blanks; observable UDF order and counts; iteration; tables; command scopes; new edges close cycles; retries replay
side effects. Response: 1.3; pure units validate, effectful units are admitted; ordered groups run in the serial
reference order until Excel's calc-chain order is measured.

**S6-S7. Worker boundary; early-bound VBA.** JSON per message (`wire.rs:830-838`), eight strings per cell, DOM
rebuilt per scroll, in-process server in the page: 1.6. VBA's remaining cost is per-call boxing, not lookup: 1.7.

**S8. OneCalc's ceiling vs UDF registration.** `Rc` registry in calc-func (`calc-func/src/udf.rs:5,53`); ROW()
refused in OneCalc (`catalogue/lookup.rs:1751`). Response: registry to calc-fml; calc-vba on calc-fml,
calc-object-model, `calcvba-host-api`. **S9. Skins**: Grid-owned shortcuts (`grid/mod.rs:695-794`), string menus,
per-skin requests; response 1.5, neutral `Target` including names.

**S10-S12.** No summary-tree SUM (`aggregate.rs:120,142`), no yield that inserts a binary64 store, vendor
provenance on Exact results. `set_clock` dirties every node (`calc-engine/src/state.rs:782-788`): tick services,
RTD as external updates. CF parsed from XML per query (`calc-host/src/conditional.rs:712`): typed CF/DV decoded
once, with the presentation resolver.

**S13. Codec save vs crate splits.** calc-doc needs the parser (`save/file_form.rs:40` imports `parse_formula`;
`edit.rs:160`); structural transforms call `bind_references` (`reference/transform.rs:32-38`); function tables
live in calc-fml and calc-doc (3-D admission `eval/compile.rs:1438-1568`, `eval/legacy.rs`, lifting
`eval/run.rs:1123-1159`, Ftab `xlsb/functions.rs`). Response: calc-syntax owns `syntax/` (lexer, parser, tree) and
`reference/` (notation, bind, structured, transform, `ReferenceCatalog`); every function table goes to
calc-func-catalog (1.10). Fable(r2), Gemini(r2).

**S14. Save under blocks.** The author path refuses a cached value in a cell without its own formula
(`save/author.rs:486-498`) and unshares moved groups. Response: save diffs blocks directly (C3).


## C. Round-2 settled outcomes and final positions


#### 5.1 Settled by evidence or ruling

| Dispute | Outcome | Why |
|---|---|---|
| Stable `AxisKey`s; non-grid participant keys; `Unbounded64`; shard executor | coordinates + optional indexes; names only; Excel limits | **[4.8]** |
| asupersync unusable (GPT(d)) | permitted, ring-fenced | **[4.7]**; GPT withdrew |
| One node per block; insert cost O(unique templates) (Gemini(d)); general 1 + 2r (round 1) | members dirtiable; cost list in S2 | false cycles; `transform.rs:6-11` (GPT(r2)) |
| Name memo when no relative references (round 1) | reference-or-value, guarded memo | all four (r2); `reference/mod.rs:139-146`, `run.rs:1000-1015` |
| Off-grid references "unmeasured" (round 1) | names, INDIRECT-R1C1 measured; cells open | `reference/mod.rs:236-250` (GPT(r2), Gemini(r2)) |
| ComCallEarly name-keyed (round 1) | descriptor-keyed at 0ad2ad28; boxing remains | `oxvba-vm3/src/lib.rs:4258-4300` |
| Exact hardware only on vendor match; soft transcendentals "refuse" (round 1) | hardware always natively, vendor as provenance; soft approximates today | owner 4.4; `excel_numeric/mod.rs:9-15` |
| "Only staged spills stop parallelism" (round 1) | also iteration, cycles, tables, observer, finite budget | `recalc.rs:644, 742-757` (Opus(r2), Gemini(r2)) |
| calc-syntax without parser/binder | owns both | `file_form.rs:40`, `transform.rs:32-38` |
| `PageSummary` SUM; philox; same-tick invariant; Excel build in target id; TLA+ | rejected as in round 1 | `aggregate.rs:120,142`; `random.rs`; `read.rs:435`; 4.4; OCT |
| VBA reload keeps values vs recalcs dependents | neither evidenced; measure | `observations.json` has no module-edit stage |

#### 5.2 Final positions on the remaining choices

| | Opus (r2) | Fable (r2) | GPT (r2) | Gemini (r2) | Synthesis |
|---|---|---|---|---|---|
| A block grain | chunked blocks with membership, bitset, affine | same | same; chunk size tunable, independent of storage | same, 64-row slices | converged |
| B partial publication | 3a then 3b over a `SchedUnit` | 3a then 3b on scalar nodes | same; semantics specified first | 3a then 3b immediately | converged: 3a, 3b-1, 3b-2 |
| C ordering | ready counts + heap in 3b | same (withdrew "measure first") | same; elaborate algorithm on counters | same; Pearce-Kelly deferred | converged |
| D order policy | no Excel mode drops off-interest work | same | interest reorders inside command demand | demand per mode; interest never drops | converged on GPT's wording |
| E research home | `tools/calc-func-lab` | same; "within xtask" = repository | same; xtask verbs forward | same | converged; owner confirms (6) |
| F object library | `calc-object-model` | same | same | same | converged |
| G renderer | measure; accessibility required | same | recycled DOM first, then compare | measure on scroll traces | open until U3 |
| H formatting | `calc-format` crate (page bundle) | calc-fml module | module unless a lower consumer exists | module | open; see below |
| I layering check | sweep + edge check on `Cargo.toml` change | same | sweep; on manifest or rules change | `Cargo.toml` DAG check in every `xtask check` (<10 ms) | 3-1: sweep + on manifest change |
| J reload | generation bump | same | same, plus effect ordering | same, no in-place swap | converged |

**H, evidence**: dna-formula-ui depends on calc-fml and calc-func today, and the page formats through
`calc_func` (`apps/dna-calc/src/cells.rs:173-192`). If tile text is formatted in the worker (1.6), a module
suffices and the page links calc-syntax + calc-func-catalog; if the page must format locally, a `calc-format`
crate keeps the evaluator out of the bundle. Recommend: decide in U1 by measuring the bundle; default module.


## D. Red-team findings checked against the code

The GPT-6 Pro red-team (`pro/3-architecture-redteam-review.md`) reviewed the pack only. Each finding was
checked here against DnaCalc-review 5a9348b (`git show`) where the code can answer it.

| # | Finding (red-team rank) | Code check | Disposition |
|---|---|---|---|
| 1 | Cached VBA observations make pure ordering observable | `docs/plan/VBA_UDFS.md:91-93`: Excel kept the old value of a cell read inside a UDF until explicit calculation. Today a read of an uncalculated cell discards the evaluation and re-runs it after the cell (`calc-engine/src/recalc.rs:607-610`), i.e. calculate-before-read. Round 2's "evaluate synchronously on demand" contradicted the observation. | Accepted; evidence decides against round 2. `ReadPolicy` and observation frontier added (1.3). |
| 2 | "Settled" conflates command completion and freshness; invocation identity missing | Calc modes exist (`calc-host/src/recalc.rs:74-82`); no range-calculation command exists in calc-host (no match). RAND streams are keyed (seed, tick, cell) and "a formula evaluated again in the same recalculation ... draws the same values" (`calc-engine/src/random.rs:4-8`), the exact property the red-team flags for indeterminate-size spills. | Accepted. `ResultStamp`, `Freshness`, `InvocationId`, draw identity per semantic pass. |
| 3 | Structural transactions and identities | Undo already restores as a new revision (`calc-host/src/command.rs:443-447`), but rebuilds the engine with everything pending. | Accepted; "undo is a new revision" is kept as an invariant, the rest added. |
| 4 | Formula blocks lack dialect | `FormulaForm::{Legacy, Dynamic}` exists (`calc-fml/src/eval/mod.rs:107-113`) and the engine's template key already includes it (`calc-engine/src/state.rs:401`); round 2's `Template` omitted it. | Accepted; dialect is part of the semantic key. Edit algebra and save mode added. |
| 5 | Name definition vs evaluation; transitive memo eligibility | Names inline into the caller's plan and run per use (`calc-fml/src/eval/run.rs:1000-1015`). | Accepted; `NameView`, `BindingProof`, transitive eligibility. Round 2's "inspection adds no evaluations" withdrawn. |
| 6 | Spill absence, iteration, data-table scenarios | Spill ownership is first-established-wins (`calc-engine/src/spill.rs:3`); the parallel route stops at waiting arrays (`recalc.rs:710-727`). | Accepted; absence certificates, `IterationSession`, scenario transactions. |
| 7 | Typed hosting needs continuations, code+state leases | OxVba 0ad2ad28 per-call boxing (`oxvba-vm3/src/lib.rs:4264-4287`). | Accepted, merged with the VBA answer (appendix F). |
| 8 | Persistence behind partial publication; XLSB writer unclear | The codec saves an .xlsb as .xlsx (`calc-doc/src/xlsb/mod.rs:11-14`); `calcOnSave` is read only by the xlsb converter (`xlsb/workbook.rs:562`). | Accepted; the "gap" is real: no same-format XLSB writer. Decision 12. |
| 9 | Fact validation is not invalidation; presentation-only too broad | `set_workbook_facts` dirties nothing because CELL/INFO are volatile (`calc-engine/src/state.rs:776-779`). | Accepted; four change classes kept apart (1.3). |
| 10 | Budgets vs Exact kernels and foreign calls | Budget counts evaluations (`recalc.rs:216-229`), not work inside a cell. | Accepted; resumable kernels, safe yield points. |
| 11 | Tiles need transaction closure; semantic projection | Today JSON per message, whole-window reset (`calc-host/src/session.rs:735-752`). | Accepted; `TileFrame`, bounded queues, accessibility projection. |
| 12 | A runtime seed is not a workbook replay contract | n/a (design). The red-team reports asupersync 0.5.0 docs list a nightly default feature set and a stable subset with defaults disabled, contrary to research-libraries.md; not checked here. | Accepted; semantic vs publication replay; pin version, features and toolchain for the spike. |

## E. Measurements that settle open claims

Targeted experiments, run when the dependent step starts (owner 4.6: not per-commit process). Excel cases
keep input files, operations, saved caches, build/platform/CPU provenance and traces.

| Question | Experiment (red-team; VBA answer) |
|---|---|
| Cached VBA reads | Peek a dirty non-argument cell in Manual, range and full calculation; a UDF observing its own caller |
| Effect order under interest changes | Same workbook, alternate viewport priority around Peek and a pure producer |
| Range.Value and save as barriers | Read dirty values; save with CalculateBeforeSave false/true; inspect caches before reopen |
| Effect invocations per unit | Counter UDF before a late dynamic dependency; cold, warm and rebuild chains |
| Random draw identity | `RAND()-RAND()`, `LET(r,RAND(),r-r)`, a named RAND twice, RAND in a repeated LAMBDA, `SEQUENCE(RANDBETWEEN(1,1000))` |
| Iteration | One/two iterations across sheets, nonconvergence, errors, a downstream counting UDF |
| Data-table scenarios | Duplicate scenario inputs with RAND and a counter UDF, a reference-valued name, callback reads, Automatic Except Tables, cancellation |
| Spill shadow selectivity | Thousands of unknown-shape anchors above a viewport; time to first settled value |
| Dialect | `Range.Formula` vs `Range.Formula2` with `=SQRT(A1:A4)` through UI, clipboard, setters and files |
| Edit algebra | Copy vs move of `B1=A1`, cut, partial insert/delete, sheet copy/move/delete, undo |
| Name binding | Add/delete a shadowing local name after a global lookup succeeded, for existing and new formulas |
| Names at scale | Edit one of 100k-1M names with snapshots and closures retained |
| Reload | Suspend F(Double), load F(String), resume and call a retained handle |
| Stored Range | Insert/delete/cut, switch workbook, close/reopen, multi-area Range |
| Typed path cost | Empty, scalar, string, array procedures and one Range callback, against a plain native call |
| Budget responsiveness | One very expensive formula with a queued edit; longest non-yielding span, p95/p99 |
| FEC overhead | Arithmetic, large range, name-heavy and callback workloads |
| Block storage under edits | Dense fill, sparse membership, scattered edits, insert/undo cycles |
| Tile coherence | Multi-tile spill, dictionary rotation, slow subscriber, reset |
| Native runtime choice | Provider churn with delayed async returns, RTD bursts, reload; serial driver plus modest pool vs adapter |
| VBA eligibility and context (VBA answer) | Public/Private, Option Private, module kinds; cell vs Run vs Macro dialog; idle vs in-macro calculation; argument kinds; ByRef aliasing; Err.Raise/CVErr; nested calls with conditional volatility |

## F. CalcVBA hosting detail (GPT-6 Pro VBA answer)

**Four tightenings** (`pro/2-vba-hosting-answer.md`, "Recommended direction"):
1. Two call contracts: a standard-module procedure is a typed callable entry with explicit runtime-state
   ownership; a host object exposes typed interfaces (COM on Windows, equivalent interfaces elsewhere). They share
   type descriptions and ownership conventions; module procedures are not made into COM objects.
2. Early binding is not low call overhead: a preserved IID/slot/signature is early-bound; Variant vectors,
   repeated interface acquisition, generic helpers and libffi can still make it expensive.
3. Common semantics, explicit target ABIs: Windows COM layout and conventions; a Linux native projection; wasm
   memory/handles plus typed imports or function-table adapters. `#[repr(C)]` fixes layout, not calling convention
   or ownership.
4. Execution lifetime and observable state are part of the contract: generations alone do not stop calls into
   reclaimed code; read-only Range access does not make VBA pure; cancellation does not undo mutated globals.

**Ownership objects**: `ProjectImage` (immutable verified code, references, descriptors, compiled artifacts),
`ProjectSession` (globals, Statics, class instances, init state, host bindings, affinity), `BoundProcedure`
(procedure, ABI identity, entry, ownership of image/session), `Invocation` (budget, cancellation, error state,
reentry, opaque host context). Control API: `prepare_project`, `create_session`, `enumerate_procedures`,
`bind_procedure`, `reset_session`, `close_session`; a failed replacement leaves the old project active.

**Descriptors**: a language signature (identity, kind, visibility, resolved types, ByVal/ByRef, Optional,
defaults, Missing, ParamArray, array rank/element, nominal class/interface/UDT) plus a lowered ABI signature
(target, calling convention, widths, hidden session/context/receiver/LCID/result parameters, ownership and
writeback, signature fingerprint, entry forms). Names serve diagnostics and rebinding only.

**Entry profiles**: a typed native entry (`CvStatus (CV_CALL*)(session, invocation, double a, double b, double*
result)`), reaching typed execution, not a Variant frame; a bound typed-frame entry for the interpreter (offsets
fixed at bind time); an explicit, separate Variant adapter for dynamic calls. Wasm host calls use build-time
generated adapters; an unsupported shape fails binding.

**Host-member plans** keep interface identity, member and accessor kind, logical slot, language and ABI
signatures, ownership/writeback and error plans; DISPID, accessor and slot are separate tokens (FUNCDESC). The JIT
emits a typed indirect vtable call through the receiver; pure dispinterfaces use a compiler-bound Invoke plan,
distinct from a fallback.

**Execution results**: `Success` (which may carry `CVErr`), `UnhandledVbaError`, `ForeignCallFailure`,
`Cancelled`, `BudgetExceeded`, `StaleBinding`, `HostExecutionAbort`. Capture `IErrorInfo` promptly. Backend
fallback is safe only before execution begins. Sessions are thread-affine and nonconcurrent by default; reentry
is a capability; synchronous ByRef events need synchronous callbacks with nested invocation state.

**Registration metadata in the glue**: types from the compiler; reference-versus-value demand from signature plus
measured rules; worksheet eligibility by host policy; affinity defaults to the VBA lane; purity conservatively
stateful; volatility from runtime requests; host writes refused in worksheet context; WorksheetFunction methods
bound to CalcFunc operations once; genuine Variant signatures (`Range.Value2`, `WorksheetFunction.Sum`) kept.

**GUID policy**: DNA's own LIBID, CLSIDs, ProgIDs and IIDs, with an explicit Excel source-compatibility mapping
(`Dim r As Excel.Range`); an external client that requires Excel's original interface needs a separately specified
facade implementing that interface's full contract, without registering under Excel's Application CLSID.

**First three steps**: (1) specify the neutral ABI and build a non-spreadsheet host fixture (counter/calculator
with a typed method, a property, a failing method, a synchronous callback); (2) prove and measure the complete
typed paths: formula -> `Add2(ByVal Double, ByVal Double)` -> typed JIT; VBA -> generated typed host method in
interpreter and JIT; `Range.Value2`; ByRef alias, failed HRESULT, synchronous callback, reload with an active
lease; browser typed-frame path; (3) extract the spreadsheet model into DNA Calc and migrate registration
transactionally, widening the object surface only after both runtimes and targets support the mechanism.

## G. Function research: placement and the per-function claim record (GPT-6 Pro search answer)

Four owned areas (search answer 9.1): observation bank (immutable Excel observations with profile, transport,
provenance, exposure history); numeric semantics (versioned Fpu contracts, backend adapters, conformance vectors);
research tooling (grammars, enumeration, exact preimages, solver queries, traces, first kills, probe selection);
product library (the selected executable graph, compact regression witnesses, published fallbacks with honest
status). Research depends on the cleared product interface; the product never depends on search machinery,
rankings, answer tables or learned policies. `xtask` is the command surface, reusing the loader and verdict
encodings; a narrow research interface to private Fpu operations, never a copied substrate.

The claim of an identification is "the unique surviving executable model within grammar G, under profile E and
scope D, passing the specified independent validation", not Excel's source program.

**Per-function claim record** (search answer 9.2): function, surface, aliases, selected elements, domain
predicate; Excel build/architecture/CPU and compatibility version; transport, coercion and publication contract;
claim strength (published fallback, conditional hypothesis, unique grammar survivor, independently validated model,
proved equivalence); program, grammar, numeric-model and evidence hashes; enumeration bounds and coverage; helper
dependencies and assumptions (versioned, so a changed helper makes dependent claims stale); constants and
provenance; surviving alternatives; first-kill records and family certificates; discovery/validation exposure;
frozen questions and predictions; validation results; product mapping and replay results; the next experiment
for each material uncertainty. "Retained after rollback" never means "identified".

## H. Round-2 changelog


- 0: per-cell group membership, dependent-with-text failure (Opus, Fable); names and INDIRECT-R1C1 wrapping
  measured, names move absolutes only (GPT, Gemini); xlsb member and save-splicing detail (Gemini).
- 1.1: full `HostQuestions`, WhatIf drops random and functions (Gemini); read purposes, RTD as external updates,
  fact versions (GPT). 1.2: block membership and save eligibility (all four); load coalescing, per-table `Arc`s
  (Fable); names as reference-or-value with guarded memo (all four); watches by `(scope, name)`, tables in the
  namespace, persisted name order (Opus); closures own their plan, name save via file form, axis replays (Gemini).
- 1.3: command-scoped demand, effectful units not retried (GPT); VBA internal reads not edges (GPT, Gemini);
  volatiles under `merge_edit`, completion points, presentation-only invalidation (Opus); open demand (Fable).
- 1.4: budgeted parallel levels (Gemini, Opus); event-loop yield (GPT); asupersync spike off the critical path (Opus).
  1.6: wire ordering fields (GPT); transport trait out of the page (Gemini).
- 1.7: OxVba 0ad2ad28 state, typed-slot entry, typed JIT entries (Opus, Fable, owner note); Range descriptors
  (Gemini, Opus); `.tlb` pipeline (GPT); runtime composition (Opus, GPT); spreadsheet.rs freeze (Fable).
- 1.8, decision 5: hardware x87 always natively, vendor as provenance (owner note, Opus, Fable); soft
  transcendentals approximate today, browser cost stated (Gemini, Opus).
- 1.10, decision 11: function metadata defined once in `calc-func-catalog` (owner question; Gemini's table list).
- S2: insertion cost restated, 1 + 2r withdrawn as general (GPT). S13-S14: parser and binder in calc-syntax
  (Fable, Gemini); save under blocks (Gemini). 3: parallelism claim corrected (Opus, Gemini); H reopened (Opus).
- 4: steps split into 0a/0b, 1a-1e, 2a/2b, 3a/3b-1/3b-2, C1-C4, U1-U3, V0-V4, R1-R2; codec half of blocks now;
  `SchedUnit` (Opus, Fable, GPT, Gemini); counter checks to sweep (Opus); measurements first (GPT).
- 5.2 final positions recorded; D in GPT's command-scoped wording. 6: 1 composition, 3 by lane, 4 reworded,
  5 restated, 8 widened (Fable, GPT), 9 GUID question and `FUNCDESC` (Opus, GPT), 10 scoped (GPT, Fable), 11 new.
- Rejected: Gemini(r2)'s per-commit `Cargo.toml` check (structural checks run periodically; the
  manifest-triggered check covers it); Gemini(r2)'s "drop names from decision 8" only in part (cell routes open).

# Appendix C: 3-architecture-redteam-review.md

# DNA Calc architecture red-team review

Review date: 31 October 2026  
Basis: the supplied architecture pack, especially Appendix A and the overriding owner rulings in Appendix B §§4.1–4.8.

## Verdict

Do not freeze the recalculation contract in A §1.3 or implement 3b-1 against its present meaning of "settled". The largest remaining risks concern observable execution, scoped freshness, and object identity across edits. They cross the evaluator, scheduler, VBA host, persistence layer and UI; adding them after scalar partial publication will change the contract already implemented.

Three changes should precede that work:

1. Separate execution order, the calculated values an invocation may observe, and publication to subscribers.
2. Give commands, semantic passes and individual invocations distinct identities; distinguish a scheduler retry from a new semantic evaluation.
3. Make authored transactions and result certificates revision-aware, including permitted stale reads, blank-cell occupancy, binding resolution and formula identity.

The formula-block work also needs an early correction: normalized text is not a complete semantic identity unless formula dialect and placement rules have already been normalized into it.

### Evidence boundary

This is a review of the supplied pack, not a fresh source-code audit. The pack identifies DnaCalc 5a9348b and OxVba 0ad2ad28; its later OxVba account supersedes the older audit in Appendix F. Neither repository nor a running Excel instance was available here. Code-location claims remain attributed to the supplied appendices. Microsoft and other primary documentation was checked for the external claims cited below. The workbook cases are proposed reproductions and measurements, not claimed executed tests.

All recommendations retain the owner's plain-coordinate model, cells and defined names as the semantic graph-node kinds, Excel's limits as the immediate scale target, early binding in both directions, and the permitted optional asupersync adapter. Activation records, scenarios, certificates and iteration sessions below are execution metadata, not additional semantic graph-node kinds.

## 1. Findings ranked by late redesign cost

The ranking reflects how much already-written code and persisted behaviour would have to change if the issue were discovered late.

| Rank | Finding | Main contracts affected | Decide before |
|---|---|---|---|
| 1 | Cached VBA observations make pure-work ordering observable | Evaluation, effects, scheduler, callbacks | 3b-1 and V1 |
| 2 | Command completion, freshness and invocation identity are conflated | Result state, cancellation, replay, external returns | 3b-1 |
| 3 | Structural transactions and workbook identity are incomplete | Model, undo, async, COM Range, commands | C2, U1, V1 |
| 4 | Formula blocks omit semantic dialect and general edit semantics | Templates, codec, binder, VBA setters | C2/C3 |
| 5 | Name definitions are not universal result slots | Notebook, memoization, bindings, closures | 2a/2b |
| 6 | Spill, iteration and data-table boundaries exceed scalar/SCC transactions | Scheduling, readiness, volatile passes | 3b contract |
| 7 | Typed hosting lacks a complete continuation and ABI contract | VM, JIT, COM, reload, OneCalc | V1/V3 |
| 8 | Partial publication is ahead of a coherent save contract | Caches, XLSB, opaque parts, recovery | C3 |
| 9 | Context validation is not an invalidation policy | Formatting, precision, metadata, cutoff | 1c/1e |
| 10 | Bounded work and Exact execution are not yet reconciled | Numeric kernels, worker responsiveness | 3a, R1 |
| 11 | Wire ordering is incomplete without transaction and semantic views | Tiles, accessibility, commands, replay | U1/U2 |
| 12 | Runtime and seed choices cannot supply workbook determinism | Adapters, effects, replay, fairness | calc-sched API |

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

### 3. Structural changes need atomic authored transactions and revision-qualified identities

**Target: A §1.2, S2, U1, C2/C3 and the saved-Range proposal.**

An address at a revision and (BlockId, offset) are storage locations. Neither alone authenticates a late result.

Construct this trace:

1. B10=SlowAsync(A10) begins at revision 20.
2. Insert row 5; the formula moves to B11.
3. Put an unrelated formula at B10.
4. Return the original asynchronous result.
5. Undo the insertion while another old return is pending.

The host needs to decide whether each original activation transfers through the edit or becomes invalid. Looking up its old address, reused slot or old block offset is unsafe.

~~~rust
struct EvaluationTarget {
    workbook: WorkbookInstanceId,
    origin_revision: Revision,
    formula_generation: FormulaGeneration,
    locus: EvaluationLocus,
}
struct PreparedTransaction {
    base: Revision,
    command: CommandId,
    forward: ModelPatch,
    inverse_authored: ModelPatch,
    coordinates: AddressTransform,
    bindings: BindingDelta,
}
~~~

Commit formula membership, inputs, references, names, styles, merges, rules and related geometry as one authored-state transition. Effects and calculation results have separate lifecycles.

**Undo must create a new revision.** It restores authored intent; it does not rewind time, RTD deliveries, arbitrary module state or an external UDF's effects. An inverse engine patch is valid only for derived state whose certificates still hold. Otherwise invalidate and recompute. Reusing old calculation caches is an optimization with a validity check.

#### Saved Range needs more than SheetId and a rectangle

A live Range object needs workbook incarnation, sheet identity/generation, AreaSet and a structural-tracking policy. Resolve its values against the invocation's appropriate read view, while preserving the workbook and location to which the object actually belongs.

For example, a saved Range acquired in workbook A must not silently refer to workbook B's coincidentally equal local SheetId. A multi-area Range cannot be reduced to its bounding rectangle. [S26] Structural edits may require moving or invalidating the retained descriptor; this needs an Excel probe.

Use an index of outstanding handles or retained address transforms. This adds cost to live objects, not to every cell, and does not introduce stable row/column keys.

#### Define the calculation domain above workbooks

The ownership of application-wide calculation mode, iteration settings and Application.Calculate needs a named home. Workbook-specific date systems and precision settings should not accidentally absorb those application policies. Microsoft distinguishes application-wide calculation settings from workbook settings. [S2], [S6]

A CalculationDomain can coordinate workbook actors and open/closed external links without implementing distribution. At minimum, type workbook identity into references, callable frames, asynchronous targets and save requests. Probe opening two workbooks with different saved settings in both orders.

### 4. Formula blocks need semantic identity and a general edit algebra

**Target: A §1.2, S2, C2/C3 and 5b.**

Two identical input strings need not describe the same calculation. Microsoft's Formula/Formula2 documentation distinguishes implicit-intersection evaluation from array evaluation. Formula entry can prevent spilling even where Formula2 entry of the same text can spill. [S7]

~~~vb
Range("B2").Formula = "=SQRT(A1:A4)"
Range("D2").Formula2 = "=SQRT(A1:A4)"
~~~

Before coalescing formulas, either retain FormulaDialect at ingress or normalize intersection/lifting into the authoritative IR. Cover UI, clipboard, Formula, Formula2, R1C1 setters and file input. TemplateKind::Cell alone does not encode this.

A possible contract is:

~~~rust
struct SemanticTemplateKey {
    normalized_ir: TemplateIrId,
    placement: PlacementSemantics,
    evaluation: EvaluationSemantics,
}
~~~

Authored spelling and per-member calculation/file attributes remain separate from interning. Sharing compiled code is not permission to discard semantic attributes.

Row insertion also proves too little. Consider B1=A1:

- Copy B1 to B2: a relative reference moves with the copy.
- Move B1 to B2: the formula still refers to its original precedent.

These are distinct documented operations. [S24]

The operation must distinguish moving a formula, copying it, moving its precedents, partial-cell insertion/deletion, and sheet copy/move/delete. A block relocation cannot implement all these operations with one offset adjustment. Sheet order is also a binding dependency of 3-D references; the statement that 3-D references are left alone during row insertion must not become a universal transform rule. [S25]

**Before C2 freezes:** one sparse shared group, a Formula/Formula2 pair, absolute/relative/cross-sheet references, a relative name, a 3-D reference and CF/DV rules. Exercise copy, cut/move, insert, delete, undo and same-format save/reopen. Compare formulas and shapes as well as values.

#### Sparse membership must imply sparse results

"Columnar per block" must mean storage proportional to allocated members or populated chunks, not the bounding rectangle. Excel's full sheet contains 17,179,869,184 coordinates. A sparse bounding extent must not allocate a result vector or bitmap across all of them; logical area cardinalities also need checked wide arithmetic before conversion to platform-sized allocation counts.

Benchmark fragmentation after scattered edits and repeated insert/undo, not only the initial million-row fill. Report block count, index bytes, result bytes, retained snapshots and compaction work. A bounded, optional compaction policy is needed to avoid turning edit history into permanent storage growth.

### 5. Name definition, evaluation and notebook observation still collapse together

**Target: A §§1.2, 1.5 and decisions 4(a)–4(b).**

The following promise is impossible as written: a notebook can show the evaluated result of any defined name, yet inspection adds no evaluations.

Define an unused name Unseen=RAND(), or an unused counter-UDF name. Excel does not calculate unused names merely because a full calculation occurs. [S2] Showing a new value for Unseen is an additional evaluation. Giving it an explicit anchor resolves ambiguity; it does not remove the evaluation.

Define the distinction in the UI and API:

~~~rust
enum NameView {
    Definition,
    ExistingEvaluation { invocation: InvocationId },
    Evaluate { anchor: CellAddress, command: CalcCommandId },
}
~~~

Passive inspection displays the definition and any existing qualified result. An explicit Evaluate operation may create a new activation, with visible anchor, scope and freshness. A name used at different caller locations can have several legitimate results.

#### Binding caches need resolution proofs

Negative watches must include misses that permitted a successful fallback. If Sheet2!B1 resolves Foo to workbook Foo because Sheet2!Foo was absent, storing only the global NameId omits a relevant namespace fact. Local/global scope and qualification are part of Excel's name resolution. [S27]

~~~rust
struct BindingProof {
    resolved: ResolvedSymbol,
    probes: SmallVec<NamespaceProbe>,
}
~~~

Probe an existing formula and a newly entered formula after adding/removing the local Foo. Excel may preserve or rewrite particular already-bound references; do not infer that outcome only from lexical scope rules. The architecture must represent whichever rule the observation establishes.

#### Memo purity is transitive, and scenario identity is insufficient

~~~text
Inner = RAND()
Outer = Inner+1
~~~

Outer is not memo-safe just because its own syntax has no RAND token. Eligibility depends on transitive names, callees, captured values/references and context facilities. Editing Inner must invalidate that classification.

Even a pure SUM over absolute references needs read-certificate validation inside its scenario. ScenarioId alone is not a validity proof; it may still be a lookup key if certificates establish validity separately. An Arc<CompiledPlan> solves plan lifetime, but not captured environment lifetime, reference semantics or binding validity.

Finally, per-table Arcs do not make large-name edits cheap if changing one name clones a retained million-entry map. Measure name edits with live snapshots and closures before choosing the table representation. Use paged or structurally shared storage if copying the whole table appears. Include long chains, cycles, misses, shadowing and repeated notebook paging in the scale fixture.


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

### 7. Typed hosting must include semantic ABI, reentry and project-state lifetime

**Target: A §1.7, V1–V4.**

A fast Double-to-Double entry is necessary but insufficient to freeze calcvba-host-api.

The descriptors need argument passing, aliasing/writeback, omitted arguments, Empty/Null/Error distinctions, array bounds and element types, ownership of strings and objects, error propagation, permitted callbacks and suspension. VBA defaults to ByRef; FUNCDESC includes substantially more than a slot offset. [S10], [S11]

Define a callable lease over **both code and state**:

~~~rust
struct CallableLease {
    project: ProjectInstanceId,
    generation: CodeGeneration,
    signature: SignatureId,
    code: CodeLease,
    state: ProjectStateLease,
}
~~~

A code lease must not leave an old activation accessing newly reset module state. Test reload from F(Double) to F(String) while an invocation is suspended; retained callbacks must either remain coherently old or be rejected under a defined lifetime rule.

Native typed entry generation and browser call adapters may share semantic descriptors without pretending that a native pointer, COM ABI and wasm function-table entry are interchangeable. Keep the scalar path free of unnecessary conversions; measure string/array/object paths separately. "Allocation-free per call" is too broad for outputs that genuinely require owned storage.

#### The owner mailbox can deadlock without holding any lock

Engine waits for a VBA invocation; VBA calls a host method; the host enqueues a request behind the engine activation that is waiting for VBA. No lock is held, yet nothing can proceed.

Cached reads resolve some cases, but WorksheetFunction callbacks, external COM reentry and macro operations still need an invocation-aware path. COM STA dispatch requires message pumping and can reenter objects. [S12]

~~~rust
struct InvocationFrame {
    id: InvocationId,
    parent: Option<InvocationId>,
    view: ReadView,
    lease: ProjectLease,
    mode: InvocationMode,
    reentry: ReentryPolicy,
}
~~~

Prove one real typed callback through VM3 and JIT, including permitted nesting and reload exclusion, before freezing the API. OneCalc should supply the same abstract services from its fixture environment without acquiring an engine dependency.

#### An Excel IID is a complete interface contract

Matching a few DISPIDs and implemented slots is insufficient. COM interfaces are immutable contracts. [S13] Reusing an Excel IID requires the full inherited and declared layout, compatible signatures, ownership and identity behaviour, and correctly typed refusals for unsupported members. A truncated table can produce memory corruption instead of a useful refusal. Typed unsupported-member errors make a partial implementation memory-safe; they do not establish behavioural compatibility for clients that require those members.

Keep type-library compatibility separate from activation identity. A LIBID does not by itself redirect CreateObject; CLSID/ProgID policy and coexistence with Excel are distinct. Inspect real type information before V1/V3 descriptors freeze; external registration can remain later.

#### Read-only workbook access is not a sandbox

A trusted native extension can still perform arbitrary native effects or block indefinitely. WIT is an interface description; the runtime and supplied imports determine authority. Wasmtime explicitly treats host imports as the route outside the sandbox. [S14]

Specify trusted native versus sandboxed execution, granted host capabilities, memory/work limits and effect policy at registration. Do not infer these from a developer's "pure" annotation. A hard stop requires an isolation boundary that can actually terminate the work; it is not supplied by cooperative cancellation. Sandboxed instruction fuel also does not bound a blocking native host import.

### 8. Persistence has not caught up with partial publication or the XLSB promise

**Target: A §0, §1.3 and C2/C3.**

Saving must pin a model revision and a policy for formula caches. Walking "latest values" while units publish can serialize a combination never valid for the authored snapshot.

~~~rust
struct SaveRequest {
    model: Revision,
    results: ReadViewId, // pinned results and external-input frontier
    cache_policy: CachePolicy,
    format: SaveFormat,
}
enum CachePolicy {
    PreservePermittedCaches,
    RequireCalculatedScope(CalcScope),
}
~~~

Pin both the model revision and the result view: several calculation passes or external-update frontiers can exist at one authored revision. The appropriate policy depends on the requested Excel-compatible operation and calculation settings. It is not always "recalculate everything". A stale cache is allowed only with the intended cache/recalculation metadata; a mixed-revision cache must not be passed off as a fresh snapshot.

The described XLSB path converts binary formulas to templates/XML. C3 describes XML splicing, but the plan does not identify a corresponding binary writer. That is a **design gap**, not proof that unseen code has none. Same-format XLSB saving needs record/token reconstruction and all affected index mappings. If the actual deliverable is conversion to XLSX, say so explicitly in the capability surface.

Define ownership of opaque package parts and relationships. Preserving bytes for a part whose referenced names/sheets have changed may itself be incorrect. Every unsupported construct needs a preserve-when-safe, transform, explicit conversion or refusal policy. Macro parts and signatures make silent "best effort" especially dangerous.

Keep authored state, recoverable session/journal state, calculation caches and external-effect records distinct. File-format round-trip is not a substitute for session recovery, and a recovery journal must not rerun unknown completed effects.

### 9. Fact validation and function metadata do not yet define invalidation

**Target: A §§1.1, 1.3, 1.10 and S1.**

Recording a fact version when a calculation runs protects an in-flight commit. It does not automatically dirty an already-published reader when that fact later changes. Context facilities require an invalidation mechanism or an explicit policy to preserve old observations.

The "presentation-only" rule is too broad. Local repaint at another device scale is different from an Excel action that triggers calculation. Some divider actions are documented triggers; conditional formatting has refresh-driven evaluation; precision-as-displayed changes numeric semantics and may change stored values. [S6], [S8], [S9]

Keep distinct:

- Authored semantic changes, including number formats when precision-as-displayed applies.
- Changes to observable host facts.
- Conditional-format evaluation demand.
- Local rendering changes that only reuse committed values.

The shared resolver should expose calculation-relevant numeric precision separately from final display text. Do not let a conditional-format overlay accidentally create a numeric feedback loop through that service.

Early cutoff needs **observable facets**, even for pure formulas:

~~~text
A1: =1+1 -> =2          B1: =FORMULATEXT(A1)
A1: =2   -> constant 2  B1: =ISFORMULA(A1)
Hide a referenced row   B1: =SUBTOTAL(109,A1:A10)
~~~

Equal f64 values do not establish equal formula text, formula kind, shape, reference identity, row visibility or topology. Range/reference-valued results especially need identity-aware comparison. Use facet-aware invalidation and structural equality where needed; a hash alone is not an exactness proof.

#### Explicit context does not require expensive per-element dispatch

Construct a borrowed evaluation frame once. Keep hot scalar fields and pointers direct; put rare host operations behind narrow service interfaces. Provide span/range readers and batch read stamps so a SUM does not allocate or hash a certificate record for every element.

Benchmark the same evaluator with arithmetic-only formulas, range aggregation, name lookup, host facts and VBA callbacks. Measure allocations, cycles, bytes of read metadata and cold-service dispatches. The pack's per-cell memory and per-call allocation estimates are hypotheses until measured.

Catalogue entries should contain policies where behaviour depends on arguments, registration or context. Thread safety, volatility and permission to reorder are different properties. The MTR documentation has argument-sensitive restrictions; runtime volatility declarations also require an explicit update path. [S15]

### 10. Finite budgets are not yet compatible with every Exact kernel or foreign call

**Target: A §§1.3, 1.4, 1.8 and 3a.**

Counting completed cells does not bound latency. A single cell can scan a very large range, sort an array, recurse through LAMBDA or execute a long VBA loop. A budget checked only between cells leaves the worker unresponsive.

The x87 rule adds a second constraint: a continuation must not introduce an extra binary64 rounding point or change evaluation order. Either yield at an existing safe program boundary, or explicitly preserve the required extended-precision state and floating-point environment. Intel documents the control/status and register state that makes this more than saving an f64 accumulator. [S16]

Give long kernels resumable state, including their cursor and algorithmic/numeric state. Measure the longest non-yielding segment. JIT loops need safepoints if resumability is promised; arbitrary external COM/XLL calls cannot acquire a universal bound merely by being placed on a lane.

In a browser, returning through a chain of already-resolved promises is not sufficient evidence that queued worker messages are serviced. Test an actual task/event-loop handoff. The HTML event-loop model distinguishes tasks and microtask checkpoints. [S17]

Retain the owner's hardware-x87 decision. Record program identity, numeric backend and relevant floating-point environment alongside witness provenance. Backend selection alone does not establish that every function's algorithm is identified.

On the soft path, make refusal depend on the executed unsupported operation where the evaluator can do so safely. An untaken LN branch must not cause an otherwise evaluable expression to be refused. Preserve the distinction between engine refusal and an Excel worksheet error. Switching Fast/Exact must invalidate incompatible calculation caches and memo entries.

### 11. Binary tiles require transaction closure, bounded queues and a separate semantic projection

**Target: A §1.6 and U1–U3.**

Revision, publication sequence and dictionary generation are necessary, but do not specify the atomicity of a multi-tile result. A spill can occupy two tiles; installing only the first exposes half a committed spill to the renderer, hit testing or accessibility.

Choose either complete frames or a transaction marker:

~~~rust
struct TileFrame {
    base: PublicationSeq,
    next: PublicationSeq,
    revision: Revision,
    dictionary: DictionaryGeneration,
    transaction: PublicationUnitId,
    final_fragment: bool,
}
~~~

The mirror applies a complete unit coherently. Dictionary definitions must precede references; removal cells must be represented explicitly. Geometry and styles need version relationships too, otherwise a correct value tile can be painted or hit-tested against old row positions.

Distinguish intentionally omitted/coalesced delivery from missing required deltas. A sequence number over all engine publications cannot be assumed contiguous in each filtered subscription. Define per-subscription continuity or explicit coverage.

Backpressure must be bounded. A slow tab cannot retain unlimited tiles, snapshots and dictionaries. Coalesce only after recording the full logical event stream when replay needs it; on reset, atomically install a new mirror base.

#### One command model does not mean one undifferentiated message enum

~~~rust
enum ActorMessage {
    Command(CommandEnvelope),
    External(ExternalOperation),
    Interest(InterestUpdate),
    TransportAck(Ack),
}
~~~

The semantic command model can remain shared while separating edit authority, undo semantics, query traffic, external returns and UI-local intents. CommandEnvelope needs command identity, base revision and a resolved target. Revalidate availability/protection at execution time; a prior availability query is advisory.

Keyboard/focus and browser-authorized effects require a UI adapter. An old edit response must not focus a newly mounted skin or paste into a different target after a structural edit.

#### Accessibility cannot be delegated to RenderPlan snapshots

Define semantic cell identity, row/column coordinates and total counts, headers, spans, selection, editability and pending/refused descriptions independently of pixels. W3C documents the extra positional metadata required for virtualized grids. [S18]

Accessibility focus can demand cells beyond the visible pixel viewport. A screen reader's navigation must not depend on which tiles happened to be painted. Derive the accessible projection from the same committed units.

Keep a small real-browser test for focus, keyboard navigation and IME, plus a screen-reader check in renderer selection. Semantic and RenderPlan tests cannot establish that the actual accessibility tree is usable.

### 12. A runtime seed is not a workbook replay contract

**Target: A §§1.3–1.4 and Appendix G's deterministic-publication claims.**

Identical operations and interest changes do not guarantee identical publication streams when wall-clock budgets, worker completion timing, external returns and foreign code are uncontrolled.

Define two different replay goals:

1. Semantic replay: the same authored state and admitted semantic events produce the same permitted values and effect trace.
2. Publication replay: reproduce the chosen publication order and externally observed intermediate states.

The second needs recorded admission/publication decisions or a deterministic logical commit policy. A seeded runtime alone does not observe an external COM server, native CPU pool, OS entropy or unmediated I/O.

Record only what the chosen replay contract requires: accepted commands, semantic passes, external input values/sequences, volatile samples or deterministic draw identities, effect admissions/outcomes, relevant focus/interest events and publication choices. Keep diagnostic trace level from altering those choices.

Finally, fairness must be explicit. Retaining off-interest work does not ensure that it runs. Add aging or a finite-work service guarantee within permitted ordering, and report completion relative to a named command/frontier. An unending RTD stream need not reach one global quiescent state. A save or Calculate completion therefore needs a bounded admission frontier, not "wait until nothing can ever update again".


## 2. Implementation order

### The step most likely to be built twice

**3b-1: scalar certificates and publication.** Its later extension is not merely changing SchedUnit::Node into SchedUnit::Chunk. Scoped cached reads, observation frontiers, semantic passes, absence certificates and irreversible effect admission change what a certificate means.

Specify the common envelope first, then implement it narrowly:

~~~text
CalcCommandId / CalcPassId / InvocationId
EvaluationTarget / ReadPolicy / ObservationFrontier
ReadCertificate / ResultStamp / ObservableChange
PreparedTransaction / PublicationUnit / CompletionReceipt
~~~

These can be small types used by immediate callers. They are not a separate framework or a second engine.

**The step most likely to stall is 1b if "every duplicate table folded in, every consumer moved, help export" is one prerequisite change.** It spans parser, file writer, evaluator, function kernels, help and generated data. Establish the authoritative schema and first consumers early; move remaining consumers in working slices. All declarations should still have one owner. Do not block the calculation-contract experiment on handbook generation.

C2/C3 are the other rebuild risk: formula dialect, general transforms and same-format save must be decided before template/block storage hardens.

### Revised order

| Move | Reason |
|---|---|
| Bring the highest-risk Excel probes forward immediately | Cached UDF reads, Manual-range freshness, semantic invocation counts and reload determine API shape |
| Introduce a thin host command envelope before full U1 | The actor needs a real input path; it does not need all six skins extracted first |
| Combine the FEC split with read policy and invalidation facets | Otherwise the new explicit context carries the old ambiguous semantics |
| Bring one V3 typed callback/reentry proof alongside V1 | A scalar arithmetic entry does not prove hosting |
| Define formula dialect, operation algebra and save mode before C2 | Prevent lossy coalescing and one-way codec assumptions |
| Implement serial yielding plus narrow publication together | End-to-end behaviour is visible early; unsupported units retain explicit serial boundaries |
| Move a minimal spill/effect/async sentinel into that proving work | Their metadata should not be retrofitted after scalar publication |
| Add broad SCC/table support and optimized name memoization later | Their envelope is already demonstrated; their algorithms can grow incrementally |
| Add the native pool and block scheduling after the contract works | Executor changes should not alter read, invocation or publication semantics |
| Keep asupersync and renderer comparisons off the critical path | Both should be judged against the same working contract |
| Keep 0a, deletion work and research independent where possible | They need not become prerequisites for unrelated semantic corrections |

### Smallest first slice that proves the recalculation contract

Use one tiny loaded workbook and the existing UI/worker route, with a thin headless harness driving the same commands:

~~~text
A1 = 1
B1 = A1+1             // off-screen
C1 = B1*2             // visible
D1 = PeekB1()         // real typed callback, cached-observation probe
F1 = 1
E1 = SEQUENCE(F1)
G10 = E10+1           // visible blank-reader before spill growth
~~~

Keep the execution serial. Do not implement a worker pool, all skin commands, general iteration or a full macro object model for this slice.

The end-to-end trace must demonstrate:

1. A finite work slice returns to the actual worker event loop.
2. An interest change affects eligible pure scheduling without changing authorized calculation scope.
3. A command receipt precedes the values caused by the command; C1 can publish before unrelated work completes.
4. An edit arriving between slices invalidates conflicting pure work without losing reusable validated results.
5. A typed callback sees the permitted read view. An activation paused at a supported VM/JIT safepoint resumes without repeating effects; an external native call that cannot suspend follows the declared non-preemptible policy.
6. Manual calculation of C1 completes without falsely claiming global freshness.
7. Changing F1 from 1 to 10 prevents G10 from being labeled settled against an unproven blank.
8. Save with CalculateBeforeSave disabled preserves the chosen cached-state policy, and reopen is coherent.

PeekB1 alone does not prove suspension. Add a counting UDF with a controllable safepoint between incrementing module state and reading B1; resumption must increment only once. If the JIT does not yet support that safepoint, make it the immediately following hosting sentinel; cell-boundary yielding has not proved this invariant.

An asynchronous test provider should then deliver a result after overwrite/undo and after provider reload. It can be a deterministic provider fixture initially; a real XLL lifecycle adapter is a subsequent integration test. The stale result must be rejected by invocation/target identity.

Use a bounded, conservative serial boundary for unsupported units, with a reason and counter. This is an honest initial implementation of the shared contract. An uncounted whole-workbook fallback would hide whether visible-first is actually delivered.

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

## 4. Concurrency foundation and asupersync

**Agree with decision 7, with a narrower claim about what the adapter contributes.** The owner has settled dependency acceptability. The technical decision is whether asupersync improves provider/task lifecycle and debugging enough to justify its integration cost.

DNA Calc must own:

- Command demand and compatible ordering.
- Ready queues and interest priority.
- The decision to admit an effect.
- Work-budget checkpoints inside evaluators/kernels.
- Invocation identity and stale-return rejection.
- Atomic publication, freshness and replay semantics.

The adapter can own task execution, channels, timers, cancellation delivery, resource draining and provider supervision. It must not leak Cx, Region, Outcome or runtime-specific cancellation meanings into calc-fml, calc-func or the stable scheduler interface.

| Option | Appropriate role | What still belongs to DNA Calc |
|---|---|---|
| Owned serial stepping executor | Reference execution and browser worker driver | All calculation policy and actual event-loop yielding |
| Small bounded pool or Rayon | Coarse independent CPU chunks | Priority, checkpoints, admission and deterministic commit rules |
| Tokio | Native asynchronous I/O/providers | Bounded CPU execution and non-abortable foreign-call policy |
| asupersync adapter | Structured provider lifetime, cleanup, deterministic lab experiments | The same spreadsheet policies and boundaries |
| Shuttle / Loom | Exploring controlled schedules and small concurrency kernels | Excel semantics and external-event capture |

Tokio documents that an already-running spawn_blocking closure cannot be aborted. [S19] asupersync similarly cannot force noncooperative foreign work to stop; its lifecycle guarantees do not turn arbitrary external effects into database transactions. [S20] Neither runtime solves a blocking native UDF or preserves cached VBA observations automatically.

A particularly bad mapping is one runtime region per recalculation that must fully close before the next edit begins. A provider that ignores cancellation could then hold interactive editing hostage. Let edits advance the authoritative model while obsolete, revocable work drains under its own lifetime. Reject late publications; retain only leases genuinely needed by admitted effects. Truly conflicting edits may need to wait for a non-replayable invocation, and that exception must be explicit.

Do not cancel and respawn calculation tasks merely to reprioritize a viewport. Maintain interest in DNA Calc's own ready queue; wake or reschedule bounded chunks. Region lifetime should describe ownership, not be synonymous with "the workbook is fresh".

The library survey is not a permanent build contract. The currently retrieved asupersync documentation specifies a nightly default feature set and a stable subset with defaults disabled, differing from the survey's stable-default wording. Pin commit/version, feature set and toolchain for the spike, and verify an actual downstream consumer. [S21]

The strongest adoption experiment is provider churn with delayed async returns, RTD bursts, cancellation, reload and deterministic failure reproduction. Compare against the serial driver plus a modest native I/O/pool arrangement. Measure actual workbook latency, retained memory, cleanup failures and developer diagnosis—not author-reported spawn/join ratios.

For testing, Shuttle provides reproducible randomized scheduling; Loom explores modeled concurrent executions with documented limitations. Neither constitutes a proof over foreign code or a complete workbook. [S22], [S23] asupersync's lab has the same boundary problem: controlled scheduler events cannot make unrecorded outside behaviour deterministic.

Salsa, Arrow and differential/relational systems should remain narrowly evaluated tools or references. Formula parsing/binding caches, columnar interchange and streaming table features are different workloads from Excel's effect-sensitive calculation semantics. Do not replace the engine contract with a library's query or collection model to obtain a scheduler.

## 5. Owner decisions

| # | Position | Reason |
|---|---|---|
| 1 | Agree, with ABI conditions | A neutral hosting API and one app composition edge fit the boundary. Specify semantic signatures, code-and-state leases, reentry and platform entry adapters before freezing it. |
| 2 | Agree in principle; change sequencing | Retire workbook-wide atomic publication, but define scoped freshness and observation/effect semantics before 3b-1. Pure scalar certificates alone are the wrong first contract. |
| 3 | Agree, with prerequisites | Start codec blocks now. Formula dialect, edit operation semantics, member attributes and save identity must precede authoritative C2/C3 storage. |
| 4 | Mixed | Agree with definition/evaluation separation and cells/names only. Reject unconditional "inspection adds no evaluations" for newly evaluated names; make evaluation explicit. Memoization requires transitive eligibility and read validation. |
| 5 | Agree with the declared platform policy | Keep hardware x87 as ruled and label soft-path limitations. Exact refusal must be structured and follow the executed program; backend choice alone does not establish function correctness. |
| 6 | Agree | tools/calc-func-lab keeps search programs and witness machinery outside the product closure; xtask can forward without acquiring product dependencies. |
| 7 | Agree | Keep asupersync optional, native-only and measured. Core calculation semantics and the maintained serial path must remain independent of its lifecycle API. |
| 8 | Agree; promote the contract-changing probes | Add cached dirty reads, scoped calculation, volatile spill passes, Formula/Formula2, scenario effects, retained objects and same-format XLSB save. These inform signatures, not just later regression coverage. |
| 9 | Qualified agreement | Observe real type libraries early. Reuse IIDs only for complete compatible interface contracts; keep type-library identity, activation identity and coexistence policy distinct. |
| 10 | Agree, with a concrete exit | Freeze the two specified feature-growth fronts until command ownership and the neutral typed callback proof land. Do not let a broad catalogue or renderer project prolong the pause. |
| 11 | Agree with incremental migration | One source of function facts, including argument/context-dependent policies. Establish ownership now; migrate consumers without making every export/help conversion a prerequisite to the calculation slice. |

## 6. Missing decisions worth making now

| Area | Minimum decision |
|---|---|
| Authoritative state | Authored model, calculated cache, external inputs and per-skin view state have distinct ownership and persistence |
| Transactions | Atomic authored edits, base-revision checks, command identity, inverse authored patches and validation before commit |
| Collaboration hooks | A server-sequenced operation log with revisions and target transforms is sufficient now; no CRDT or stable axes are required |
| Command retry | Repeated delivery of a command ID cannot apply an edit or launch an effect twice |
| Irreversible effects | Record started/completed/indeterminate outcomes; no automatic replay of arbitrary effects after a crash |
| Application scope | Calculation policy and cross-workbook coordination have an owner above an individual document |
| Recovery | Journal/checkpoint schema, cache invalidation on restore and external-effect recovery policy |
| File preservation | Same-format versus conversion/export; ownership of unknown parts and signatures |
| Security | Trusted native and sandboxed profiles, capability enforcement, and honest stop/timeout guarantees |
| Observability | Explain "why dirty", "why waiting", "which read view", "why this refusal" and "why this completion was rejected" |
| Debugging | Observing a name or stepping a formula must declare whether it creates an evaluation |
| Corpus testing | Retain failing workbooks and operation traces; shrink deterministically where possible; compare state, shape, text, effects and persistence as well as values |
| Memory lifecycle | Bound retained snapshots, obsolete compiled plans, captured closure environments, dictionaries, queues and object handles |
| Progress | Report a named command's admitted frontier and pending reasons, rather than one perpetual global "calculating" flag |

The immediate deliverable should be an executable instance of the corrected calculation contract. It should accept an edit mid-calculation, publish a valid visible result, preserve a cached VBA observation, reject a stale completion, and save a coherent authored revision. Crate splits and executor substitutions can then be evaluated against those behaviours.

## Sources

[P] User-supplied 3-architecture-redteam-prompt.md, Appendices A–H. All statements about the current code and owner rulings are attributed to this pack.

- [S1] Microsoft: Excel Commands, Functions, and States.
- [S2] Microsoft: Excel performance—Improving calculation performance.
- [S3] Microsoft: Application.CalculateBeforeSave.
- [S4] Microsoft: How to correct a #SPILL! error, indeterminate-size section.
- [S5] Microsoft: Asynchronous user-defined functions.
- [S6] Microsoft: Change formula recalculation, iteration, or precision.
- [S7] Microsoft: Formula vs Formula2.
- [S8] Microsoft: Excel performance—Tips for optimizing performance obstructions.
- [S9] Microsoft: Excel Recalculation.
- [S10] Microsoft: Function statement, VBA.
- [S11] Microsoft: FUNCDESC structure.
- [S12] Microsoft: Single-Threaded Apartments.
- [S13] Microsoft: Interface Design Rules.
- [S14] Wasmtime: Security.
- [S15] Microsoft: Multithreaded recalculation in Excel.
- [S16] Intel: Intel 64 and IA-32 Software Developer's Manual, Volume 2A, FXSAVE.
- [S17] WHATWG: HTML Standard, event loops.
- [S18] W3C WAI: Grid and Table Properties.
- [S19] Tokio: spawn_blocking documentation.
- [S20] asupersync: Runtime lifecycle and cancellation documentation.
- [S21] asupersync 0.5.0 documentation, toolchain and stable-subset sections as retrieved.
- [S22] AWS Labs: Shuttle.
- [S23] Tokio project: Loom.
- [S24] Microsoft: Move or copy a formula in Excel.
- [S25] Microsoft: Create a 3-D reference to the same cell range on multiple worksheets.
- [S26] Microsoft: Range object.
- [S27] Microsoft: Names in formulas.

[S1]: https://learn.microsoft.com/en-us/office/client-developer/excel/excel-commands-functions-and-states
[S2]: https://learn.microsoft.com/en-us/office/vba/excel/concepts/excel-performance/excel-improving-calculation-performance
[S3]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculatebeforesave
[S4]: https://support.microsoft.com/en-us/excel/how-to-correct-a-spill-error
[S5]: https://learn.microsoft.com/en-us/office/client-developer/excel/asynchronous-user-defined-functions
[S6]: https://support.microsoft.com/en-us/excel/change-formula-recalculation-iteration-or-precision-in-excel
[S7]: https://learn.microsoft.com/en-us/office/vba/excel/concepts/cells-and-ranges/range-formula-vs-formula2
[S8]: https://learn.microsoft.com/en-us/office/vba/excel/concepts/excel-performance/excel-tips-for-optimizing-performance-obstructions
[S9]: https://learn.microsoft.com/en-us/office/client-developer/excel/excel-recalculation
[S10]: https://learn.microsoft.com/en-us/office/vba/language/reference/user-interface-help/function-statement
[S11]: https://learn.microsoft.com/en-us/windows/win32/api/oaidl/ns-oaidl-funcdesc
[S12]: https://learn.microsoft.com/en-us/windows/win32/com/single-threaded-apartments
[S13]: https://learn.microsoft.com/en-us/windows/win32/com/interface-design-rules
[S14]: https://docs.wasmtime.dev/security.html
[S15]: https://learn.microsoft.com/en-us/office/client-developer/excel/multithreaded-recalculation-in-excel
[S16]: https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf
[S17]: https://html.spec.whatwg.org/multipage/webappapis.html#event-loops
[S18]: https://www.w3.org/WAI/ARIA/apg/practices/grid-and-table-properties/
[S19]: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
[S20]: https://asupersync.com/
[S21]: https://docs.rs/crate/asupersync/0.5.0
[S22]: https://github.com/awslabs/shuttle
[S23]: https://github.com/tokio-rs/loom
[S24]: https://support.microsoft.com/en-us/excel/move-or-copy-a-formula-in-excel
[S25]: https://support.microsoft.com/en-us/excel/create-a-3-d-reference-to-the-same-cell-range-on-multiple-worksheets
[S26]: https://learn.microsoft.com/en-us/office/vba/api/excel.range(object)
[S27]: https://support.microsoft.com/en-us/excel/names-in-formulas

