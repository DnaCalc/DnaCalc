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
  only after RenderPlan, tile mirror and a screen-reader check. Both painters now consume the same plan;
  [measured comparison](RENDERER_COMPARISON.md) retains DOM as default. Provisional pending a real screen-reader check.
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

### Run 5 implementation status (2026-11-01, dc-hsa5.7)

The first formula-block slice retains an xlsx shared group's source once, with its actual members
as dense membership or sorted runs over plain coordinates. Member spellings are validated with the
existing codec mover, then discarded; text is materialized on request. Engine binding uses the
master's source and origin and places the existing Fml plan at each member. Per-cell caches and
file attributes stay in the codec records. A dependent carrying text uses its group's source.

`calc-syntax` now owns the neutral dialect and template-key types; it does not yet own the parser,
binder or edit implementation. Keys use Fml's existing conservative relative-reference normalizer;
unsupported syntax has no key, and broader spelling normalization remains open. The engine derives
its key from the source rather than trusting an authored block's supplied identity. Explicit legacy
and dynamic ordinary inputs remain distinct through wire encoding, evaluation and xlsx persistence;
loaded array definitions continue to select their evaluation and publication form.

Copy and row insertion/deletion use the existing reference-occurrence transforms. This slice
materializes affected groups and saves individual formulas; untouched eligible groups retain their
shared file form. It does not yet regroup edited formulas, coalesce unrelated writer output, compact
fragmentation, extract the parser/binder, or store engine results in block columns. The sparse test
uses a full-sheet extent (17,179,869,184 coordinates), two members, two runs and two authored pages.
