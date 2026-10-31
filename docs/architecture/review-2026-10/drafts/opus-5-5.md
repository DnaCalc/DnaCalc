# DNA Calc target architecture: Opus 5.5 draft

Code read at DnaCalc-review 5a9348b and OxVba c7ce1349 (`git show`). Stance: most of the stack
stays; four things get dearer every week and are fixed at the root: (1) formulas are per-cell text,
not Excel's own template-plus-offset form; (2) recalc is an atomic generation with no interest
input; (3) context reaches calc-func through defaulted methods on a value reader; (4) the app shell
and the VBA bridge are late-bound and stringly typed.

## 1. Abstract architecture

### 1.1 Core invariants

- **I1 One authority per fact.** Authored state: the model, owned by the host. Calculated state:
  the engine. Presentation facts: derived once, by one query. Nothing above the host caches an
  answer it did not get from a projection.
- **I2 Explicit context, complete by construction.** One context value per boundary; context
  traits have no default methods, so a new fact is a compile error in every supplier, never a
  silent refusal in one route (`calc-engine/src/data_table.rs:308-388` forgot `host_facts`).
- **I3 Excel's representation.** A formula is a template (canonical tokens, relative offsets,
  absolute positions) placed over an extent; A1 text is rendered at the edges. Excel does the
  same: xlsb stores token streams, shared formulas use relative `ptgRefN`, FORMULATEXT renders.
- **I4 Order independence.** At quiescence every result is a function of inputs, semantic
  context and tick seeds only, never of interest, priority, threads or partitioning.
  Order-observable participants are named and run in the reference order.
- **I5 Cancel means yield.** Interest changes reorder; edits merge into the dirty set; only what
  an edit invalidates is discarded.
- **I6 Per-result atomic publication.** A result publishes with its footprint and spill
  reservation; nobody sees a value with old edges or a spill with old ownership.
- **I7 Commands are data.** Every user, agent or macro action is a typed command resolved by a
  DOM-free model. Skins route gestures to commands and own no rule.
- **I8 Early-bound across language boundaries.** Formula-to-UDF and VBA-to-host calls resolve to
  slots and typed signatures at bind time; names matter only at bind time and for late-bound
  receivers.
- **I9 Exact means the same program**, transcribed over an explicit precision substrate; program
  search is outside the product's dependency closure.
- **I10 OneCalc is the proof.** Everything below the engine runs under OneCalc with a fixture
  supplier; if OneCalc needs an engine type, the boundary is wrong.

### 1.2 Layers and the context each boundary carries

| Boundary | Supplied by | Carries (all required, no defaults) | Callbacks |
|---|---|---|---|
| func <- fml: `FunctionServices` | calc-fml, built in one place from the FEC | numeric mode; `LocaleContext` (format profile + collation, one value); date system; caller identity; argument access (eager/lazy per catalogue); `WorkbookFacts` answers | `clock()`, `random(stream)`, `number_text(value, code, purpose)`, `resolve_reference_text`, `invoke(CallableId, args)` |
| fml <- engine/host/OneCalc: `FormulaEvaluationContext` | the evaluating caller (engine node, detached CF/DV rule, data-table scenario, OneCalc fixture) | `CallerIdentity` taken from the instance (cell, block offset, rule, scratch); `SemanticEnv`; `ValueReader` (values, shapes, spills, subtotal exclusions); `ReferenceCatalog` (sheets, names, tables, one sheet-name matcher); `WorkbookFacts`; `CallableTable` (generation-stamped); `NumberText`; `EvalControl` (budget, cancel checkpoint) | `VolatileServices` (clock per tick, counter-keyed random), `EffectSink` (footprint with facets, volatility requests, UDF effects) |
| engine <- host: `CalcContext` | calc-host | `SemanticEnv` (changing it dirties nodes); `VolatileServices` (never dirty anything); facts snapshot; `CallableTable` + thread affinity; typed CF/DV rule records; `InterestSet`; `OrderPolicy`; `CancelToken` | publication sink (pull-based), effect providers (RTD, external links) |
| host <- app/worker: `HostEnvironment` | the platform adapter | clock source; `ApplicationFacts` (INFO); file identity; resolved locale (no `CurrentExcelHost` below this line); font metrics tables; thread policy; registered add-ins and VBA projects | none; host answers commands and projection requests |
| model (UI) <- skins | the model crate | command registry, availability, neutral selection, view-state store, projection subscriptions | skin registers gestures, focus target, view-state schema |

Decorators override one concern: a data-table scenario wraps only `ValueReader`, so it cannot
forget facts it never forwards. Footprint entries name a facet (`Value`, `Format`, `Existence`,
`Shape`, `Visibility`, `NameBinding`, `SheetOrder`, `CallableGeneration`); a format edit dirties
only `Format` readers (CELL("format"), precision as displayed), so CELL stays right and cheap.

### 1.3 Data representations

- **Templates and plans.** `TemplateId(u32)`: interned tokens, relative offsets, absolute
  positions, literals, whitespace for faithful rendering. `PlanId(u32)`: template plus binding
  assumptions (plan 4.1). Plans carry static flags from catalogue metadata: `volatile`,
  `may_spill`, `user_calls`, `order_observable`, `facets_read`, an inferred result type.
- **Formula blocks.** `FormulaBlock { id, extent: AxisRect, plan, kind: Fill | LegacyArray |
  Single }`. A filled or shared region is one block; per-cell identity `(BlockId, offset)` is
  derived, never allocated. Editing one cell punches it out (at most 4 pieces plus a 1x1).
- **Axes.** Per sheet axis, an `AxisMap` (order-statistic tree of runs, position to stable
  `AxisKey(u64)`); an untouched sheet is one run. Extents, style runs and the 256x16 `Arc` pages
  (kept, `calc-doc-model/src/cells.rs:1-10`) are keyed by axis keys; an insert splits one page.
- **Results.** Per block, a `ResultColumn`: `tags: Vec<u8>`, `nums: Vec<f64>`, a side table for
  text, errors with payload, arrays and rich values, a run-length `settled_tick` and one shared
  `Calculation` provenance per run. About 9-10 bytes per numeric cell instead of today's
  600-900 B per formula cell (risk-engine 4).
- **Values.** `CalcValue` stays; strings crossing the wire are session-interned. A `Rich(RichRef)`
  arm is reserved for Excel data types and typed records, off the numeric hot path.
- **Styles.** Excel's precedence (cell over row over column over sheet) as run maps; a
  whole-column format is one run, not 1M entries (`calc-doc-model/src/definitions.rs:315`).
- **Names, rules.** `NameId` with scope and a binding version plans depend on. CF/DV rules are
  typed records parsed at load, source bytes kept (risk-layering R7), so they can be participants.
- **Dependency graph.** Nodes are participants (blocks, name instances, rule applicability,
  spill anchors, data tables, aggregates). Static edges are interval edges block -> `AxisRect`
  with a facet set; a per-sheet 2-D interval index answers "who reads this rectangle". Dynamic
  (INDIRECT/OFFSET) edges are replaceable footprints. Intra-block chains (a fill of
  `=A1*2+B2`-style recurrences) are an offset on the block's self-edge, so dirtying one row
  dirties a contiguous tail interval, and evaluation order inside a block follows the offset
  sign.
- **IDs.** Hot structures are sheet-scoped and use `CellPos { row: u32, col: u32 }` (8 B).
  `SheetId` stays generational but is checked at boundaries, not stored per cell.
  `NodeId` = u32 slot + u32 generation, local to a partition. `GlobalNode = (PartitionId u32,
  NodeId)`. Durable cross-session key: `DocKey { workbook: u128, sheet: u64, row: AxisKey,
  col: AxisKey }`. Grid bounds are a per-sheet `GridBounds` (Excel profile 1,048,576 x 16,384),
  not a crate constant asserted in 183 places.
- **Partitions.** A partition is a set of blocks of one sheet in a row-key band, plus their
  result columns. The engine reads inputs only through an `InputView` trait (today it holds
  `&ModelRoot`), so a remote partition is an `InputView` that answers from a replica. One
  process, one partition per sheet is the Excel-scope configuration.

### 1.4 The recalculation contract

```rust
pub struct RecalcRequest { pub kind: RecalcKind,        // Auto | CalcNow | CalcSheet | Full | RebuildFull
                           pub order: OrderPolicy }      // InterestFirst | ExcelOrder
pub struct InterestSet { pub visible: Vec<SheetRect>, pub subscribed: Vec<SheetRect>,
                         pub nodes: Vec<GlobalNode> }    // skins' named interests resolved to nodes
fn schedule(&mut self, req: RecalcRequest, interest: &InterestSet);   // opens tick T if needed
fn reprioritise(&mut self, interest: &InterestSet);                   // no work discarded
fn step(&mut self, budget: WorkBudget, cancel: &CancelToken) -> Progress;
fn drain(&mut self) -> Publication { tick, spans: Vec<(BlockSpan, ResultRun)>, quiescent: bool }
fn merge_edit(&mut self, change: &PreparedEngineChange);              // dirties, does not reset
```

- Priority: backward cones of visible dirty cells, then subscribed, then the rest; topological
  within a class. Cones come from interval edges, cached per interest version.
- Cell states for skins: `Settled { tick }`, `Pending { last }`, `Refused(owner)`, `FileCached`.
  `Settled` at T means no dirty node in the backward cone at T (I4, I6).
- Spills: an empty cell (shown, or read by a formula) can be filled by any dirty `may_spill`
  anchor above-left of it; a per-sheet `may_spill` index keeps that set small and adds it to the
  cone. A formula cell's own value never changes by another's spill (the blocked anchor gets
  `#SPILL!`).
- Volatiles join tick T at open; random is counter-keyed by (seed, tick, cell), as
  `calc-engine/src/random.rs:1-8` already does, so priority cannot change RAND. The clock is a
  per-tick service and dirties only volatile nodes (`calc-engine/src/state.rs:782-788` today
  dirties all).
- Order-observable regions (side-effecting VBA UDFs, iterative SCCs, data-table runs) are units in
  the reference order; priority only decides when a unit runs. Manual mode publishes nothing
  until a calc command. Excel's interruptible recalc is this model: `step` returns at a
  checkpoint, the host serves messages, the next `step` resumes.

### 1.5 Concurrency and distribution

- **Layer 0 (ours, always present):** deterministic non-concurrent executor: one session actor per
  workbook, budgeted steps, message pump between steps. The reference order (plan 3.5, 6.5), the
  browser baseline, the path that must always work.
- **Layer 1 (native):** a pool evaluating ready blocks of a wave from an immutable snapshot
  (`EngineState` is already `Sync`); blocks are the grain, never cells. Thread-affine callables
  (VBA, non-thread-safe XLLs) go to an owner-lane mailbox. Spills and data tables no longer turn
  parallelism off (`recalc.rs:644,739-757`): reservations commit at publication, scenarios are units.
- **Layer 2 (browser, optional):** the same pool over wasm threads when cross-origin isolated.
- **Layer 3 (distributed, designed only):** partitions exchange interval-dirty messages and
  `(span, tick)` publications with per-tick frontiers (timely-dataflow's idea). I4 makes any
  block-to-partition assignment give the same results.
- **asupersync (owner ruling: usable, ring-fenced).** On merit, its regions, request/drain/finalize
  cancellation, cancel-lane priority and seeded lab runtime fit host orchestration (session actor,
  I/O, RTD, async UDFs, a future partition coordinator) and deterministic tests of edit-during-recalc
  interleavings. It does not fit the evaluation loop: evaluation is synchronous CPU work, a task
  costs several tokio tasks (author's figures), the browser lane is single-threaded preview with
  cancel-by-drop, `?` on `Outcome` needs nightly, about 100 dependencies. So `calc-sched` defines
  `Executor`, `Region`, `CancelToken`, `Checkpoint`, `WorkBudget`; Layer 0 implements them; an
  asupersync backend is an optional native feature for host orchestration and lab replay. Nothing
  below calc-host names it. A spike measures build cost, stable-toolchain use and block-grain
  overhead before adoption.

### 1.6 The functional model beneath skins

- **Host commands** (calc-host, headless, authoritative): typed families `Cells`, `Sheets`, `Axes`,
  `Panes`, `Styles`, `Rules`, `Names`, `Clipboard`, `FindReplace`, `Layout` (manifest edits with
  id allocation), `Calc`, `History`, `File`; each validates and returns a receipt or typed
  refusal. The Excel rules in `apps/dna-calc/src/server.rs:597-1350, 1636-1815` move here.
- **UI model** (DOM-free, native-testable): neutral `Target::{Cells(AreaSet), Block, Name, Sheet,
  Object, LayoutItem}`, edit session, clipboard snapshot, navigation, `ViewStateStore` keyed by
  `SkinId`, subscriptions; UI commands (`"edit.undo"`, `"calc.now"`, `"view.reveal"`) with typed
  args and `availability(&Model) -> Enabled | Disabled(reason)`, mostly resolving to host commands.
- **Keymap and menus are data** at the shell (`KeyChord -> CommandId`); skins add only local
  gestures. F9, Ctrl+Z, Ctrl+F work in every skin because no skin binds them.
- **State classes:** document state, including Excel's persisted view facts (panes, widths), and
  DNA layout are host state; selection, editor and scroll are model state; gestures are skin state.
- **Skin contract:** `trait Skin { id; mount(SkinCtx); focus_target(); gestures(); projections();
  view_state_schema() }`; `SkinId` is an open registry; "show in skin X" is `view.reveal`.
- **Skin-neutral projections:** viewport frames, dependency neighbourhood to depth N, formula
  listing with typed filter/sort, layout placements per view kind, census, one-cell explanation
  on demand. Pixels, SVG and prose stay in skins.

### 1.7 Extension seams

- **LAMBDA:** callables are values in calc-fml; a named LAMBDA is a defined name (plan 4.3).
- **UDFs:** `CallableTable` in calc-fml; entry `{ id: CallableId, name, signature:
  TypedSignature, flags: volatile | thread_affinity | async, entry: Typed(fn ptr + ABI
  descriptor) | Dynamic(Arc<dyn Callable>) }`. Compile binds a name once; the plan holds
  `CallUser { id, generation }`, not `UserCall { name }` (`calc-fml/src/eval/mod.rs:235`).
  Built-ins win name conflicts, as in Excel.
- **CalcVBA hosting:** load project; enumerate modules and public procedures as type information
  (TKIND_MODULE-like: Excel's likely small non-COM step); typed entry points; register host type
  libraries and host object implementations.
- **COM object library:** one DNA Calc type library defined in Rust, generating `#[repr(C)]`
  vtables (IUnknown, IDispatch first). The same objects serve early-bound VBA everywhere and are
  real COM objects on Windows (windows-rs) for external clients.
- **XLL (Windows):** `xlfRegister` yields callable entries with XLOPER12 shims; `Excel12v`
  callbacks use a host service protocol. **WIT components:** sandboxed add-ins, batch calls only.
- **Agents:** command registry plus availability is the capability descriptor; projections are
  the read API; `calcctl` and an MCP adapter are thin shells over both.

### 1.8 The Exact numeric path and function research

- `calc-numeric` (product, portable) is the substrate: `X87` values with explicit precision
  control and `store64()` points, the instructions Excel programs use (`fadd`..`fsqrt`, `f2xm1`,
  `fyl2x`, `fpatan`, `fsin`, `fprem`), bit-pattern constants with derivation, `ExactDecimal`,
  digit generation. Backends: `Hardware` (native x86-64 x87, valid only when the running CPU
  family is the target's; AMD and Intel differ, 15 of 15 measured) and `Soft(VendorModel)`
  (portable; an instruction without an identified vendor model refuses under Exact).
- `NumericTargetId` = Excel build x CPU family x compatibility version. The catalogue declares
  `ProgramStatus::{Identified(spec_id), Unidentified}` per function; under Exact, `Unidentified`
  is a typed refusal, counted as a fail. A kernel reads like its program: straight-line code over
  calc-numeric, every constant sourced, no fitted bounds or error-based switches.
- Research lives in `numlab`, a separate workspace depending on calc-numeric and calc-verdict,
  never depended on: candidate-program DSL (DAG with precision, store and constant nodes), an
  interpreter on calc-numeric's own ops, search (enumeration, beam, e-graphs, constant
  recovery), the oracle cache fed by calc-oracle-excel, the active-learning scheduler. Output: a
  `ProgramSpec` plus witnesses; promotion adds the kernel and a witness-replay test to calc-func.
  Match counts are numlab diagnostics, never product commits.

### 1.9 Platform profiles

| | Windows native | Linux native | Browser |
|---|---|---|---|
| Recalc | Layers 0-1 | Layers 0-1 | Layer 0; Layer 2 if cross-origin isolated |
| Exact | Hardware x87 when CPU family matches target, else Soft | same | Soft models only, else refuse |
| VBA | interpreter + JIT; early-bound host objects; real COM refs | interpreter + JIT; in-process host objects only | interpreter (later wasm emission); no file I/O |
| Add-ins, files | XLL, COM add-ins, WIT; native I/O | WIT; native I/O | WIT via jco; blobs only |

## 2. Seam pressures

**S1. Function context vs presentation.** Pressure: CELL, TEXT, INFO and the grid need the same
facts (format, alignment, width, file name). Evidence: the format engine is in calc-func and apps
call it directly (`calc-func/src/format/mod.rs:1052`, `apps/dna-calc/src/cells.rs:187`); grid
width is `chars * 7 + 5` (`grid/render_plan.rs:140-143`) while CELL("width") refuses non-Calibri
fonts (`calc-host/src/facts.rs:53-71`); `dna-formula-ui/src/result.rs:430` hard-codes 1900 dates;
`WhatIfReader` lacks host facts. Response: `calc-format` (number formats, locale profiles) below
calc-fml and outside calc-func, injected into kernels as `NumberText`; `calc-present` derives
`ResolvedPresentation` (style -> format, alignment, font metrics -> width in characters and
pixels) once, from embedded metrics tables; the grid projection and `WorkbookFacts` both read
it. Metrics come from tables, never from browser `measureText`, so grid and CELL cannot diverge
by platform. Non-obvious part: CELL needs a `Format` facet edge or format edits leave it stale.

**S2. Shared formulas vs per-cell identity and structural edits.** Evidence: load expands groups
(`calc-doc/src/load/mod.rs:268-271`); 1M-row fill = 1M nodes; one row insert rewrites and
re-parses every formula (`calc-doc-model/src/structural.rs:1-31`, `calc-engine/src/edit.rs:479-609`);
undo of a structural edit rebuilds the engine with every formula pending
(`calc-host/src/command.rs:443-459`). Response: blocks over axis keys. Inserting k rows at p:
extents need no change (keys); a relative reference with offset d changes only for members whose
row and target straddle p, a contiguous run of at most |d| rows, so each block splits into at
most 1 + 2r pieces for r relative references; absolute references are rebound per plan, not
per cell; a block whose extent spans p splits there too (Excel leaves inserted rows empty).
Cost is O(log n + straddling blocks + affected plans). Undo is the inverse axis op
plus block merges, not a rebuild. Pressure that remains: per-cell overrides fragment blocks
over time, so the host coalesces adjacent blocks with equal plans after edits. Save writes
blocks back as shared groups; xlsb rgce loads straight to templates without text.

**S3. Visible-first recalc vs Excel semantics.** (a) Priority must not change values: RAND is
already counter-keyed; extend that to every effect provider (clock per tick, RTD per epoch).
(b) Off-screen anchors can spill into visible empty cells: the `may_spill` index (1.4). (c)
Iteration results depend on iterate order: iterative SCCs are reference-order units. (d) Data
tables are transient overlays on one snapshot, run as units; "automatic except tables" is a
request kind. (e) Side-effecting VBA UDF call order and count are observable and were measured
(dc-snz.5, `crates/calc-vba/tests/fixtures/excel-recalculation/observations.json`): their regions
run `ExcelOrder`; matching Excel's repeat-call counts needs its calc-chain behaviour, a fidelity
target of its own. (f) CF statistics rules need region aggregates while only the viewport shows:
aggregate participants join the visible class. (g) Suites pin the atomic generation
(`recalc.rs:1001-1013`, `StaleCause`); every new test of that shape raises the cost.

**S4. Worker boundary vs fine-grained reactivity.** Evidence: JSON per message
(`apps/dna-calc/src/wire.rs:830-838`); `wire::Cell` has eight strings including per-cell prose
(`wire.rs:730-745`); block crossings resend the whole window (`calc-host/src/session.rs:735-752`);
every scroll rebuilds every cell div unkeyed (`grid/mod.rs:355-372, 1216-1281`); the worker runs
each message to completion so cancel cannot arrive (`worker.rs:60-90`). Response: reactivity at
tile grain (32x8), never cell grain. `ViewportFrame` is binary: header (subscription, seq,
revision, tick), per-tile columns (tag u8, f64, string id u32, style id u32, state u8), string-
and style-table deltas; prose on request. The page keeps a `ViewportCache` with one version
signal per tile, renders recycled elements or canvas; window moves send entering tiles only. The
worker drains its inbox between steps, so `SetInterest` and edits never wait for a recalc. The
harness reads a semantic layer (`window.__dnacalc.cell(r,c)`, ARIA grid), not cell divs.
Non-obvious part: page and worker are one wasm module today (`src/bin/dna-calc-web.rs`), so the
page carries the engine; separate UI and worker crates make "skins cannot reach the host" a
link-time fact (the in-process fallback remains for native tests).

**S5. Early-bound VBA vs portability and reload.** Evidence: the Excel object model is in
OxVba (`oxvba-embed/src/spreadsheet.rs`, 635 lines); `ComCallEarly` is lowered to name dispatch
in VM3 (`oxvba-vm3/src/lib.rs:2175-2203`) and in the JIT, which passes `member.name` to the same
helper as `ComCallLate` (`oxvba-jit/src/lowering.rs:497-540`, confirmed at c7ce1349); `ComHal`
is dynamic by construction (`oxvba-hal/src/traits.rs:360`); a formula call costs 4 name lookups
and 8-10 allocations (risk-vba 3). Responses: (a) the host-object ABI is COM's binary shape
without needing Windows COM: `#[repr(C)]` tables of `extern "C"` pointers, IUnknown refcounting
(the 4,096-object cap goes), typed parameters from the type library; a COM vtable on Windows x64,
an in-process table on Linux, `call_indirect` indices in wasm. (b) `ComCallEarly` becomes
load-slot plus call-indirect in the JIT and a resolved slot call in VM3; `ComCallLate` stays for
`Object`/`Variant` receivers, as in Excel. (c) CalcVBA hands the glue a typed entry point per
public procedure; the glue registers a `Typed` callable with shims only for `Variant` and `Range`
parameters. (d) Reload bumps the callable generation; plans rebind lazily; values are not
dirtied (Excel keeps old UDF results until a recalc reaches them) but provenance says
`CallableChanged`. A type-library change is a new interface version (interfaces are immutable),
so VBA bound to old slots is recompiled, never patched. (e) No JIT in the browser (no executable
memory): VM3 with resolved slots, wasm emission later on the same IR. (f) VBA runs on its owner
lane; the scheduler routes `user_calls` plans there.

**S6. OneCalc's ceiling vs UDF registration.** Evidence: the registry sits in calc-func only so
calc-vba can avoid calc-fml (`calc-func/src/udf.rs:5,53`, `Rc`, not `Send`), with two supply
points (`calc-fml/src/eval/compile.rs:174-204`, `eval/mod.rs:507`); ROW() is refused in OneCalc
because the caller comes from host facts (`calc-func/src/catalogue/lookup.rs:1751`). Response:
`CallableTable` and `CallerIdentity` move to calc-fml with one supply point (the FEC); calc-vba
depends on calc-fml, inside OneCalc's ceiling. The object library splits by backend, not by type
library: one type library, an `ObjectModelBackend` trait; the UDF-time backend (reads through the
FEC: `Range.Value2`, `Application.Caller`, `WorksheetFunction`) is in calc-vba and refuses writes
as Excel does inside a UDF; the macro-time backend (writes as host commands, events) sits above
calc-host. OneCalc then runs VBA UDFs with `Range` arguments over its fixture, no engine.

**S7. Skins vs one command model.** Evidence: shortcuts bound in the Grid keydown handler
(`grid/mod.rs:695-794`); string-dispatched menus (`commands.rs:245-256`); controller imports the
grid and focuses its DOM (`controller.rs:26, 798...`); per-skin wire requests with pixels and SVG
(`wire.rs:643-685`); five copies of "show in skin X"; Excel rules in the app worker. Response:
1.6. Non-obvious part: "selection" differs per skin (a canvas card, a notebook block, a range),
so commands take a neutral `Target`, and each skin maps its gesture selection into it. Without
that, commands silently become Grid commands again.

**S8. Bit-exact research vs product code.** Evidence: bead dc-j9k records fitting ("gains 31385
but loses 2068"); commits for match deltas (risk-inner-loop 7); the x87 path is a module of
calc-func (`calc-func/src/ported/excel_numeric`, used by `kernel.rs:175-216`); a `soft-x87`
feature exists. Response: 1.8. The substrate is shared so a search result transcribes without
translation; the search is not, so product compile time and review never carry it.

**S9. Environment as identity vs service.** Setting the clock dirties every node
(`calc-engine/src/state.rs:782-788`), and DNA Calc never sets it, so NOW is refused. Response:
`SemanticEnv` (changes dirty everything) apart from `VolatileServices` (read per tick).

**S10. Engine reads the model directly.** `begin_recalc(state, inputs: &ModelRoot, ..)`
(`recalc.rs:480-485`) ties the engine to one in-memory model. Response: an `InputView` trait,
implemented in process by today's code; partition replicas implement it later.

## 3. Reflection onto Rust

### 3.1 Target crate map

| Crate | Owns | Depends on | From today |
|---|---|---|---|
| calc-value | values, ids, `CellPos`, `GridBounds`, error codes | - | keep; drop baked grid constants |
| calc-numeric (new) | x87 substrate, backends, `ExactDecimal`, digit generation | calc-value | split from calc-func `ported/excel_numeric`, `kernel.rs` arithmetic, `decimal.rs` |
| calc-func | catalogue (FunctionId, demand, volatility, subtotal flag, facets, thread safety, file prefix, numeric status), kernels | calc-value, calc-numeric | keep; remove `format/`, `udf.rs`, host/presentation types in `call.rs:218-286`, `help.rs` (to a data crate) |
| calc-format (new) | number format compile/render, locale profiles, `NumberText` impl | calc-value, calc-numeric | from calc-func `format/`, `number_text.rs`, `locale.rs` format parts |
| calc-fml | syntax, templates, plans, binding, FEC, evaluator (block-capable, checkpoints), `CallableTable`, reference transforms, the one prefix table | calc-func, calc-format, calc-fml-syntax | keep; add FEC; delete six forwarders |
| calc-fml-syntax (new) | lexer, tokens, template interning, A1/R1C1 rendering, reference transforms | calc-value | split from calc-fml `syntax/`, `reference/` |
| calc-doc-model | sheets, axis maps, pages, blocks, style runs, typed rules, names, manifest | calc-value, calc-fml-syntax | evolve |
| calc-doc | xlsx/xlsb codecs | calc-doc-model | drop calc-fml edge: emits shared groups as data, host interns |
| calc-sched (new) | executor traits, Layer 0 executor, cancel, budget, seeded test scheduler | - (asupersync optional, native) | new |
| calc-engine | graph, interval index, scheduler, result columns, participants | calc-fml, calc-doc-model, calc-sched | rewrite graph/scheduler/results; keep recalc state machine ideas, generational slots, spill and data-table logic |
| calc-present (new) | `ResolvedPresentation`, metrics tables | calc-doc-model, calc-format | from calc-host `facts.rs`, `state.rs:649-657`, grid heuristics |
| calc-host | session actor, host commands, journal, projections, subscriptions (pull), save, CF/DV wiring | engine, doc, present, sched | keep core; absorb server.rs Excel rules; make session `Send` |
| calc-vba | glue: shims, type library, UDF-time backend | calc-fml, calcvba-host-api | rewrite (551 lines) |
| calc-host-vba (later) | macro-time object backend, events | calc-host, calc-vba | new |
| dna-ui-core | command registry machinery, keymap, editor, formula field, theme, shared components | calc-fml, calc-format, dna-strand | rename/extend dna-formula-ui |
| dna-calc-model | DOM-free model, UI commands, view-state store | dna-ui-core, dna-calc-protocol | from controller.rs |
| dna-calc-protocol | requests, `ViewportFrame`, projections | calc-value | from wire.rs |
| dna-calc-worker | server: translator from protocol to host commands | calc-host, protocol | from server.rs, worker.rs |
| dna-calc | shell and skins | dna-calc-model, dna-ui-core | split from today's crate |
| onecalc-model, dna-onecalc | OneCalc model and front end | calc-fml, calc-vba, dna-ui-core | extract from `surface.rs` |
| xtask (thin) | check, sweep, words, arch, licence | no product crates | split |
| calc-harness | corpus, capture, witness, compare, bench, app smoke driver | product crates | from xtask |
| numlab (separate workspace) | program search | calc-numeric, calc-verdict | from OxFunc method |

calc-fml-syntax exists so the model can hold `TemplateId`s without the evaluator, and it removes
the codec's pull of the 77k-line function crate for the lexer (risk-layering R9).

### 3.2 Dependency rules (one file, `architecture.json`; four mechanisms become one)

calc-func and calc-numeric never see calc-format or calc-fml; calc-fml never sees the doc model,
codec, engine, host or calc-vba; calc-engine never sees calc-doc or calc-host; calc-vba never sees
calc-engine or calc-host; dna-calc (UI) never sees calc-host or calc-engine; OneCalc's closure has
no engine, host, doc or doc-model; nothing in the product closure depends on numlab or
calc-harness; only calc-vba depends on CalcVBA, and only on `calcvba-host-api`; asupersync only
behind calc-sched's optional native feature. Checked in sweep, not per commit, as the owner
asked.

### 3.3 What is kept as is

Doc-model `Arc` pages and copy-on-write path; generational slots; `RecalcWork`'s owned,
resumable shape with readiness waits; per-result `Calculation`; prefix-sum geometry and
`ScrollScale`; the worker boundary; counter-keyed random; the catalogue and its 500 kernels;
codec preservation machinery; `cargo xtask check`'s touched-plus-dependents selection;
content-hash captures; the corpus harness and verdict path.

## 4. Implementation strategy

Each step lands on master in working chunks and leaves both apps working.

1. **Inner loop (days).** Split xtask from calc-harness; opt-level 2 for calc-func and
   calc-numeric in dev; app smoke cases move to the app with `--case` and fresh-page groups, a
   30-case core for UI commits, the rest in sweep; `git rm` receipts and frozen plan files;
   check prints NOT-RUN counts.
2. **FEC and callables (weeks 1-2).** FEC with no default methods; caller from the instance;
   clock as service; `CallableTable` with generation in calc-fml; delete the calc-func registry
   and the `call.rs:385-398` fallback; volatility and subtotal as catalogue metadata; one prefix
   table. dc-snz.6 is re-scoped to this. Shippable: ROW() in OneCalc, CELL/SHEET in data tables,
   NOW in DNA Calc.
3. **Formatting and presentation (weeks 2-3).** calc-format, calc-present; apps stop calling
   `calc_func::cell_text`; one sheet-name matcher. dc-971p.6 builds on this route.
4. **Recalc contract (weeks 2-5, engine lane).** Per-node publication, interest classes,
   `reprioritise`, cancel-as-yield, `merge_edit`, host scheduler loop, worker inbox drained
   between steps. The single-thread order stays as the twin; a seeded schedule test (random
   priorities, interleaved edits, compare at quiescence) runs per commit in seconds.
5. **Commands and model (weeks 2-6, UI lane).** Host command families; the server becomes a
   translator; `dna-calc-model` from controller.rs; shell keymap; `ViewStateStore`; neutral
   `Target`; skins on projections; UI and worker crates split. Exit: a new skin is two edits and
   every skin has undo, find and F9.
6. **Viewport wire (weeks 5-7).** `ViewportFrame`, tile cache, keyed or canvas rendering,
   semantic test layer; per-cell DOM checks retire.
7. **Blocks and axis keys (weeks 4-10, engine lane, after 4).** calc-fml-syntax; templates in
   the model; codec keeps shared groups; xlsb rgce to templates; block nodes, interval edges,
   result columns; structural edits by splitting; style runs; undo by inverse ops. Bench before
   and after (bytes per formula cell, insert-row and open time on a 1M-row fill), kept in sweep.
8. **CalcVBA (OxVba lane, throughout).** (a) neutral `calcvba-host-api` restoring the May
   contract, `spreadsheet.rs` re-homed in calc-vba; (b) host-object ABI and `ComCallEarly` slot
   calls in VM3 then JIT, with Excel-measured tests, complete before any late-bound DNA Calc path
   (owner priority); (c) typed procedure entry points; (d) Windows COM exposure. No Range member
   is added to the string switch meanwhile.
9. **Partition seams (with step 7).** `InputView`, `GridBounds`, `DocKey`; no distributed runtime.

Libraries: asupersync behind calc-sched after a measured spike (1.5); salsa not adopted (per-query
memo cost at cell or block grain; our plan cache is simpler); Arrow not a dependency now (result
columns are Arrow-shaped for later IPC export); formualizer and IronCalc as benchmark
comparators on our corpus; windows-rs for COM on Windows; msft-typelib vendored for reading
.tlb files (writing our own type library is ours to generate); Cranelift stays CalcVBA's native JIT, wasm emission for the browser later; WIT
for sandboxed add-ins; shuttle or loom for scheduler kernels.

Lane redirection: the UI-slice lane stops feature slices and does step 5 then 6; the bit-exact
lane moves to numlab, reverts fitted switches (already under way), and promotes only identified
programs with witnesses; the CalcVBA lane follows step 8 in that order (its current stock-take
is step 8a's input); a new engine lane takes steps 2, 4 and 7.

Tests: engine tests assert quiescent state plus I4 and I6, not the atomic generation; UI
behaviour is tested natively on `dna-calc-model`, browser smokes keep real-event cases only;
Excel observations stay CalcVBA's oracle for both runtimes; kernels test by witness replay.

## 5. Decisions for the owner

1. **Cargo edge to CalcVBA.** Recommend: allow exactly one edge, calc-vba to an ABI-shaped
   `calcvba-host-api` (repr(C) tables, type descriptors, entry points; no runtime internals).
   Reasons: a binary boundary adds nothing to the per-call path (both end in a function-pointer
   call) but costs packaging; a wasm component boundary would copy arguments per call and stop
   `Range` objects sharing memory, which defeats the early-bound requirement in the browser.
   CalcVBA depends on nothing of DNA Calc and proves neutrality with its own non-spreadsheet
   test host; the ABI shape keeps a later dylib or COM-server split mechanical.
2. **Order policy.** Recommend `InterestFirst` by default with `ExcelOrder` forced for
   order-observable regions (side-effecting UDFs, iteration, data tables). Alternative: Excel
   order always, visible-first only as publication order; safer, slower to show.
3. **Formula representation now.** Recommend blocks plus axis keys now, accepting churn in wire,
   tests and save path, before the function ports and save path settle further.
4. **Browser threading.** Recommend: Layer 0 is the contract; serve dnacalc.com cross-origin
   isolated so the pool is possible, but never require it.
5. **ID widths.** Recommend `CellPos` u32/u32 in hot sheet-scoped structures, generational
   `SheetId` at boundaries only, partition-local `NodeId` (u32+u32), `GlobalNode` with a u32
   partition, `DocKey` with u64 axis keys and a u128 workbook id.
6. **Object library compatibility.** Recommend matching Excel's names, DISPIDs and interface
   order as observed through public COM introspection of a running Excel (`GetIDsOfNames`,
   `GetTypeInfo`), so compiled clients bind on Windows; needs your ruling that this is
   black-box observation, not binary inspection.
7. **Exact in the browser.** Recommend typed, counted refusal for any instruction without an
   identified vendor model, never an approximation labelled Exact.
