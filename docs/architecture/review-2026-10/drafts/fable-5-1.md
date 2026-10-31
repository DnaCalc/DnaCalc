# DNA Calc target architecture: draft by Fable 5.1

Date 2026-10-31. Code read at DnaCalc-review 5a9348b and OxVba c7ce1349 (`git show`). Every
claim about the current code was checked against the file cited or against the six audit files
(cited as `layering R1`, `engine 2`, `skins R3`, `vba 2`, `loop 1`, `libs`). Nothing was built.

## 1. Abstract architecture

### 1.1 Core concepts and invariants

Six things exist; everything else is a view of them.

1. **Authored document** at an `AuthoredRevision`: immutable shared pages of inputs, formula
   templates and blocks, style runs, names, typed rules, objects, retained package parts.
2. **Formula template and instance**: `TemplateId` (R1C1-normalised syntax plus form) and
   `(TemplateId, anchor)`. A **formula block** is one template over an area. A1 text is a
   rendering produced at the edges, never the stored form.
3. **Calculation state**: one graph per workbook, partitionable; per-node results each stamped
   `Calculation { revision, tick, mode }`; dirtiness per node; spills, cycles, volatile set.
4. **Interest**: what some consumer wants now (viewport windows, skin subscriptions, a save, a
   query). Interest drives *order*; it never changes *what* a tick computes.
5. **Command**: a typed, named operation on the functional model, available or unavailable
   with a reason, invoked identically by a skin, a shortcut, `calcctl`, VBA or an agent.
6. **Context**: the facts and callbacks a layer needs, built by the layer above and handed down
   whole. No layer asks a neighbour-of-a-neighbour; no layer fills a missing fact with `None`.

Invariants: one evaluator, one graph, one formatting engine, one presentation query; provenance
before value; no presentation or host type in calc-func; results never publish a mix of ticks; an
unrelated one-cell edit does work proportional to its cone, not to the workbook; a skin cannot
install a value; every refusal is typed and carries an owner.

### 1.2 Layers and the context each boundary carries

| Boundary | Context handed down (who supplies) |
|---|---|
| func <- fml: `FunctionContext<'a>` | `numeric: &NumericMode`; `locale: &LocaleContext` (format profile **and** collation); `date_system`; `caller: FormulaAnchor` (from the instance, never from the host); `reader: &mut dyn CellReader` (values, shapes, blanks, subtotal exclusions, nothing else); `refs: &dyn ReferenceTextResolver` (INDIRECT/OFFSET text to area, through fml's binder and the one sheet-name matcher); `volatile: &mut dyn VolatileServices` (`now()`, `random()`, read per tick, never dirtying); `callables: &dyn CallableInvoker` (LAMBDA and user callables by `CallableSlot`); `host: &dyn HostQuestions` (CELL/INFO as *answers*: `prefix_char`, `width_characters`, `format_class`, `locked`, `file_name`, `sheet_index`, `is_formula`, `formula_text`, `application`); `number_text: &dyn NumberText` (TEXT/FIXED/DOLLAR/VALUE rendering implemented above func). Constructed by a builder whose every field is required; an absent service is `Absent(Owner)` and refuses typed. |
| fml <- engine/host: `FormulaEvaluationContext` | all of the above plus `catalog: &dyn ReferenceCatalog`, `callables: &CallableTable` (slot-indexed, generation-stamped), `observer`, effect sinks, `format: &FormatContext` (the same one the presentation query uses). fml derives `FunctionContext` in exactly one function. `ReferenceReader` shrinks to `read_area`, `shape_of`, `spill_of`. |
| engine <- host: `CalcContext` | `semantic: SemanticEnvironment` (date system, locale, compatibility, numeric mode; a change makes nodes pending) separate from `VolatileServices` (clock, seed; per tick); `facts: Arc<dyn WorkbookFacts>` snapshot; `callables: Arc<CallableTable>` with generation in the plan key; `threads: ThreadPolicy`; typed CF/DV rule records in the model. |
| host <- app/worker | clock source, application facts, file identity, locale choice (`CurrentExcelHost` resolved here), thread count, registered extensions. Back: `present(cell) -> Presentation { text, colour, alignment, width_px, style_id }`, used by grid, CF and `HostQuestions` alike. |
| model <- skins | `Command`, `availability`, projections by `Interest`, `ViewStateStore`. Skins supply nothing but gestures and a focus target. |

### 1.3 Data representations

- **Formulas.** `calc-doc-model` keeps `FormulaStore { templates: interned R1C1 text + form,
  blocks: Vec<FormulaBlock { id: BlockId, template, area }> }`. A cell input is
  `CellInput::Formula(FormulaRef)`, `FormulaRef = Single(TemplateId) | InBlock(BlockId)`; cells
  inside a block have no per-cell entry (the page says "covered by block B"). Loading a shared
  formula makes one block, not N expanded strings (today `load/mod.rs:268-271`, engine 1).
  Fill-down makes a block. Structural edits transform templates per block; an edit that lands
  inside a block splits it (at most three blocks). Original spelling is kept as a span override
  only where it differs from the rendering.
- **Values.** `CalcValue` stays. Results are columnar per node slot: `kind: u8`, `num: f64`,
  `payload: u32` into an `Arc` table for text/array/error detail, `stamp: u32` into a tick table.
  About 20 B per result instead of today's `Option<(NodeId, FormulaResult)>` with two `Arc`s.
- **Styles.** `StyleRuns` per sheet: column runs, row runs, rectangle overrides, cell overrides,
  resolved by precedence to a `StyleId`. A whole-column format is one run (today a `BTreeMap`
  entry per cell, engine 4). The file-layer map in `LoadedSheet.styles` is folded into it at
  load; axis edits move runs, never replay (layering R4).
- **Names.** `NameId` with scope and a binding-version node; caller-relative names instantiate.
- **Rules.** CF and DV parsed to typed records at load, source bytes retained for round trip;
  engine participants with applicability regions (plan 6.4; today raw XML, layering R7).
- **Graph.** Nodes are `Single(cell)` or `Block(block, offsets bitset)`. Edges are typed:
  `Cell`, `Rect`, `Affine { block, drow, dcol }` (every member of a block reads the cell at a
  fixed offset: one edge for a 1M-row fill), `Axis`, `Span`, `Name`, `Existence`. Reverse index
  keeps today's block-levelled rectangle filing (`graph.rs:336-350`) plus affine entries.
- **IDs.** Keep `slot u32 + generation u32` in-process (`calc-value/src/id.rs:150-160`). Add
  `PartitionId(u32)`; a cross-partition reference is `(PartitionId, NodeId)`. The Excel grid
  stays 1,048,576 x 16,384 per sheet (the A1 grammar needs it); scale is sheets x partitions,
  not a bigger grid. `ArrayShape` uses `u32`, not `usize` (wasm32).
- **Partitions.** A partition owns a contiguous set of sheets or sheet areas, its node slots,
  results and reverse index, and a **frontier**: the edges whose reader and read are in different
  partitions. A single-process workbook is one partition. The engine API takes a `&dyn Frontier`
  that in-process is a no-op. Only this seam is built now; distribution is placed later.

### 1.4 Recalculation contract

```rust
pub struct RecalcRequest { revision, mode, demand: DemandSet, policy: CalcPolicy, cancel: CancelToken }
pub enum DemandSet { All, Areas(Vec<GridArea>), Nodes(Vec<NodeId>) }   // from interests; a save is All
pub enum Lane { Demanded, Background }
pub enum StepOutcome { Settled(Vec<NodeId>), Yielded, Cancelled { settled: usize }, Failed(RecalcFailure) }
```

- `begin_recalc` computes the dirty closure as today (`recalc.rs:480-620`), then marks the
  backward cone of `demand` intersected with that closure as `Demanded`. The ready queue serves
  the Demanded lane first. Moving the viewport is `retarget(work, demand)`, which relabels, never
  recomputes the closure.
- **Per-node publication.** A node publishes when its prerequisites are settled for this tick
  and it has evaluated. Every published result carries its tick. The invariant replacing "one
  atomic generation" (`recalc.rs:1001-1013`): *a node's published value was computed from
  prerequisites all stamped with the same tick as itself*, which topological evaluation gives for
  free. A consumer sees `Settled(tick)` or `Stale { last_tick, cause }` per cell. Spill anchors
  publish shape and reservation together with the value; blockers are decided per tick.
- **Cancellation** is cooperative: `CancelToken` checked between nodes. Cancel keeps settled
  results (correct for this revision), drops in-flight ones, and leaves the rest pending. A new
  revision does not discard settled work outside the edit's cone, which is what dirtiness means.
  Today a commit discards all staged work (`command.rs:460`, engine 2).
- **Excel semantics are untouched by priority.** Volatile nodes evaluate once per tick against
  one effect tape (seed, clock sample) drawn at `begin_recalc`; an SCC with iteration on is one
  unit on one worker; a data table participant evaluates after its cone and defaults to the
  Background lane unless its cells are demanded; manual mode and automatic-except-tables stay
  decisions of the host policy, not of the lanes.
- **Determinism.** Given (revision, semantic environment, effect tape, callable generation), the
  final settled state is order-independent. The lab scheduler (seeded) replays interleavings:
  edit during tick, viewport move during tick, cancel mid-SCC.

### 1.5 Concurrency and distribution

A **session actor** owns `HostState` and runs `step(budget)` between messages; the browser's
worker pump and the native thread are the same loop (plan 3.5, today `session.rs:679-700` loops
unbudgeted). Evaluation workers pull ready nodes from the lanes; `EngineState: Sync` is kept and
the per-level `std::thread::scope` (`recalc.rs:968-990`) becomes a persistent pool. Thread-affine
callables (`ThreadAffinity::Owner`) are routed to the owning thread; a plan with such a call is
evaluated there. Browser: one worker now; a worker pool over `SharedArrayBuffer` is a profile
option, not a requirement. Distribution later: one actor per partition, a tick coordinator that
owns `revision` and `tick`, frontier values exchanged as `(NodeId, stamp, value)` messages.

The engine core names no runtime. A `Scheduler` trait in `calc-sched` carries the shape the
engine needs: `spawn_wave(region) -> WaveHandle`, `checkpoint(&Cx) -> Continue | Cancelled`,
`cancel(wave)` with request/drain/finalize, `Lane::{Cancel, Demanded, Background}`, `publish`
as reserve/commit. Two implementations: a serial one we own (the plan's permanent single-thread
oracle, also the browser's default) and an in-house pool. **asupersync** is evaluated as a third,
behind the same trait (section 4). Its region model, cancel protocol, lanes and seeded lab replay
match this shape closely (libs 1), which is why the trait is drawn to it; its per-task cost and
browser lane are why the engine must not depend on it directly.

### 1.6 Command and functional model

`calc-model` (DOM-free, toolkit-free): `Selection`, `EditSession` (one editor contract for cell,
bar and notebook), `Clipboard` snapshot, `History`, `ViewStateStore<SkinKey>`, a `Keymap` table,
`Command` (families: document, cells, structure, sheets, panes, axes, names, rules, formats,
calculation, find/replace, view, diagnostics) with `availability(&Command) -> Result<(), Why>`.
Skins are `fn mount(&ModelHandle) -> View` plus a `focus_target()`. Excel rules (sheet lifecycle,
panes, axes, replace, paste, layout ids) are typed `HostCommand`s validated in calc-host; the app
worker becomes a translator (today they live in `server.rs:597-1350`, skins R4).

### 1.7 Extension seams

- **LAMBDA** stays in calc-fml (closures, defined-name LAMBDAs). Unchanged.
- **Callables (UDFs, add-ins, VBA).** `CallableRegistry` lives in **calc-fml**, not calc-func
  (today `calc-func/src/udf.rs:53`, `Rc`, layering R2). `register(CallableDescriptor { name,
  signature: Signature { params: [ParamType], ret }, affinity, volatility }) -> CallableSlot`;
  the registry has a `generation` in the plan key; `Instruction::UserCall { slot }` replaces
  `UserCall { name }` (`eval/mod.rs:235`). Eval does no name lookup. A typed signature lets the
  glue hand fml a `extern "C" fn(*const Arg, *mut Ret)` entry; a Variant signature keeps a slow
  path. Reload = new generation; old code lives until in-flight calls return.
- **CalcVBA hosting.** CalcVBA exposes a neutral hosting API: load project; enumerate public
  procedures with signatures; obtain a typed entry (interpreter trampoline or JIT pointer);
  register host type libraries and host object implementations. The glue crate `calc-vba` owns
  Application, Range, WorksheetFunction (moved out of `oxvba-embed/src/spreadsheet.rs`, vba 1),
  generates the type library from one descriptor model, and registers shims as callables.
- **COM object library.** One `ObjectLibrary` descriptor (interfaces, members, dispids, vtable
  slots, types). From it: vtable tables of `extern "C"` fns for in-process early binding on every
  platform; a `.tlb` and real dual interfaces via `windows-core` on Windows for external clients.
  OxVba lowers `ComCallEarly` (`oxvba-oxir/src/inst.rs:230-254`) to a slot call in VM3 and JIT;
  `ComCallLate` stays for `Object`/`Variant` receivers. Host objects are refcounted handles, so the
  4,096 cap (vba 4) disappears.
- **Add-ins (XLL)**: native-only adapter over the same `CallableRegistry` and `HostQuestions`.
- **Agents and automation**: `calcctl` and a JSON command route over `calc-model`; a skin is
  data (keymap, layout, which projections) plus a small view, so an agent can write one.

### 1.8 Exact numeric path and function research

`NumericMode::Exact(target)` selects a substrate crate `calc-exact-x86` (native x86-64 only; x87
control word set and restored around a kernel). Function **research** (candidate programs,
precision models, witnesses, the symbolic search of OxFunc's SMART_SEARCH method) lives in its own
repository with a dependency ceiling of calc-value and calc-verdict. Its outputs into the product
are: a kernel (ordinary Rust in calc-func), a `witness.json` capture, and a status row
`{ function, target, status: Identified | Candidate | Fitted(reverting) | Unverified, build, cpu }`
generated into the catalogue. No fitted coefficient, bound or error switch enters calc-func
without an `Identified` row. Match-count deltas are research telemetry, not commits (loop 7).

### 1.9 Platform profiles

| | Windows | Linux | Browser |
|---|---|---|---|
| Threads | pool | pool | one worker; optional pool over SAB |
| Exact | x86-64 | x86-64 | no |
| VBA | interpreter + Cranelift JIT | same | interpreter; later wasm-emission backend |
| Object library | in-process vtables + real COM/.tlb | in-process vtables | in-process vtables |
| Add-ins | XLL, COM add-ins | none | none |
| Files | direct | direct | blobs through the page |

## 2. Seam pressures

Each: evidence that it is real, then the design response.

**S1. Function context vs presentation.** CELL("width") refuses any font but Calibri/Aptos
(`facts.rs:53-71`) while the grid uses `DIGIT_PX = 7.0` (`grid/mod.rs:65`); `cell_text` and
`DEFAULT_PALETTE` are in calc-func (`format/mod.rs:1052,1095`) and the apps call them
(`apps/dna-calc/src/cells.rs:187`; `result.rs:430` hard-codes System1900); SHEET and INDIRECT
fold sheet names differently (`read.rs:197-211` vs `1096-1108`). Pressure: TEXT and CELL need
*some* of the renderer, so the convenient place is calc-func. Response: a `calc-format` crate
beside calc-func (ceiling calc-value) owns formats, palette and locale rendering; calc-func gets
only the `NumberText` trait; calc-host's `present()` is the one route to shown text, colour,
alignment and width, and `HostQuestions` answers CELL from the same `Presentation`. One sheet-name
matcher in calc-fml. Plan 4.4's "CalcFml owns the formatting engine" is amended to "a crate below
CalcFml".

**S2. Shared formulas vs per-cell identity and structural edits.** A 1M-row fill is 1M nodes,
1M `Vec<Prerequisite>` and 1M reverse entries; a row insert rewrites every formula's text and
drops all templates (`edit.rs:609`; engine 1). Pressure: Excel's model *is* per cell (CELL, ROW,
ISFORMULA, per-cell CF, per-cell undo), so blocks must still answer per-cell questions, and an
edit anywhere inside a block must keep the others' identity. Response: blocks with per-member
result slots and a dirty bitset; `FormulaAnchor` for a member is `(block, offset)` resolved to a
cell on demand; a structural edit transforms the template once per block and splits blocks the
edit crosses; `BlockId` is stable through splits for the notebook's manifest but a notebook block
is a different concept (`BlockId` vs `NotebookBlockId`) and must not be conflated. Undo of a
structural edit restores pages and blocks by pointer, not by rebuilding the engine
(`command.rs:447-459`).

**S3. Visible-first recalc vs Excel's semantics.** `finish` refuses `Incomplete` and
`Superseded`; the product loops unbudgeted; `Interest` reaches only delivery (engine 2). Pressure:
volatile functions, iteration, data tables and spills all assume "a calculation" is one event; a
viewport-ordered evaluation can expose a half-state (a spill anchor settled, its blocker not; a
NOW() cell settled at tick T next to one at T-1). Response in 1.4: one effect tape per tick so
NOW/RAND agree across the tick; spill shape and reservation publish with the anchor and the
reservation is checked before any member settles; an SCC is one unit; data tables are Background
unless demanded; the shown state is honestly per cell `Settled(T) | Stale`, and the grid draws
stale cells as stale instead of blank. Tests that assert whole-generation atomicity stop now.

**S4. Worker boundary vs fine-grained reactivity.** A cell crosses as about eight strings
(`wire.rs:730-745`), a block scroll resets the whole window (`session.rs:735,752`), the render
`Memo` depends on the viewport and rebuilds every `<div>` (`grid/mod.rs:355-372, 1216-1281`),
and any CF rule turns every delta into a full reset (`session.rs:852-866`). Pressure: Leptos
reactivity wants one signal per changed thing, the worker wants few large messages, and the smoke
harness keys on per-cell DOM attributes (97 references, engine 3). Response: `WindowDelta
{ revision, tick, cols: ColumnarCells { at: [u32], kind: [u8], num: [f64], text: [u32 into a
string table], style: [StyleId], stamp: [u32] } }`, a `StyleTable` sent once per revision, prose
(`why`, `error_meaning`) fetched on demand by cell; differential window moves (cells entering the
window only); page-side mirror as typed arrays with one signal per 32x8 block; CF deltas carry
only cells whose decision changed. The harness checks a semantic layer (`present(cell)` through
the in-process `Server`) and a few real-browser cases.

**S5. Early-bound VBA vs portability and reload.** Both backends lower `ComCallEarly` to a
name-keyed `DynamicCallRequest` (`oxvba-vm3/src/lib.rs:2175-2203`, `oxvba-jit/src/lowering.rs:
497-540`); the only vtable path is Windows libffi per call; formula to VBA does four name lookups
and 8-10 allocations (vba 2-3). Pressure: a vtable call is a raw pointer, which fights reload
(module replaced while a call is in flight), portability (no COM on Linux/web) and the browser (no
executable memory). Response: `extern "C"` vtable tables generated from the `ObjectLibrary`
descriptor are plain Rust functions on every platform; reload is a generation bump in
`CallableRegistry` with old code kept alive by `Arc` until its last frame returns; in the browser
the same descriptors drive the interpreter's slot call now and a wasm-emission JIT later. The JIT
gives each public procedure a typed native entry (scalars in registers, no per-call `globals`
Vec, `oxvba-jit/src/runtime.rs:136-244`) and hands the glue a pointer plus `Signature`.

**S6. OneCalc's ceiling vs UDF registration.** The registry sits in calc-func only so calc-vba
can avoid calc-fml (layering R2); OneCalc calls `calc_vba::load_module` (`session.rs:338-348`)
and gets a `UserFunctions` it passes twice (compile and eval) with nothing checking they match.
Pressure: UDFs need a caller, ranges and `WorksheetFunction`, which smell like the engine.
Response: the registry moves to calc-fml (OneCalc's ceiling already includes calc-fml);
`calc-vba` depends on calc-fml and the neutral hosting API only; its Range reads go through a
`SheetAccess` trait that OneCalc implements over its fixture reader and calc-host over the
engine's reader. OneCalc stays the honesty check: if `calc-vba` ever needs calc-engine, the seam
has leaked.

**S7. Skins vs one command model.** Shortcuts live in the Grid's keydown handler
(`grid/mod.rs:695-794`), menus are string-dispatched (`commands.rs:245-256`), the controller
imports the Grid and focuses its DOM (`controller.rs:26, 798...`), a new skin touches eight places
(skins R1-R6). Pressure: each UI slice ships fastest by adding to controller/server/grid, and the
owner wants skins trivially writable by agents. Response: `calc-model` first (1.6), with the
smoke cases moved into the app and a 30-case core per commit (loop 1). A skin's whole contract is
`mount`, `focus_target`, skin-local gestures and registration in one table.

**S8. Bit-exact research vs product code.** dc-j9k's notes are a ledger of fitted variants scored
by match deltas; commits record match counts (loop 7); the owner is reverting fitted bounds
(brief 4.4). Pressure: the product's corpus harness is the most convenient scoreboard for the
research, so fitting leaks into kernels. Response in 1.8: separate repository, status rows, no
kernel change without an `Identified` witness; the corpus harness reports per-function status
alongside match counts so a "Fitted" kernel is visibly not a pass.

**S9. Clock and environment vs invalidation.** `set_clock` goes through `set_environment`, which
makes every formula pending (`state.rs:783-789`; layering R6). Response: `SemanticEnvironment`
vs `VolatileServices` (1.2). Cheap, do first.

**S10. Function identity by spelling.** Volatility is a nine-name list (`eval/mod.rs:190-201`),
SUBTOTAL detection is by spelling in the engine, three `_xlfn.` tables (layering R8). Response:
catalogue metadata `{ volatile, subtotal, file_prefix }` by `FunctionId`; calc-doc reads the one
prefix table through calc-fml and drops its 3,571-line copy.

**S11. Tooling that depends on the product.** xtask depends on nine product crates, so every
edit builds twice and xtask's tests run in every check (loop 2). Response: thin `xtask` (check,
sweep, scans) with no product dependency; `calc-harness` binary for corpus, capture, witness,
bench, smoke. `[profile.dev.package.calc-func] opt-level = 2`, measured.

## 3. Reflection onto Rust

Target crate map (arrows are allowed dependencies; `architecture.json` is the one rules file,
the hard-coded ceiling in `sweep.rs:46-47` and the source-text test `cfunc_m0_1` go):

```
calc-value ──> calc-format ──┐          platform/calc-exact-x86 (native-only, behind NumericMode)
     │                       │
     └──> calc-func ─────────┼──> calc-fml ──> calc-doc-model ──> calc-doc
                             │        │             │
                             │        └──> calc-vba (glue; + oxvba-host-api)
                             │        │
                             └────────┴──> calc-engine ──> calc-host ──> calc-model ──> calc-wire
calc-sched (Scheduler trait, serial + pool impls) <── calc-engine, calc-host
calc-sched-asupersync (native-only adapter; the only crate allowed to name asupersync)
apps: dna-formula-ui (ceiling calc-fml, calc-format)   dna-onecalc (ceiling calc-fml, calc-vba)
      dna-calc (calc-wire, calc-model, skins only)      dna-strand
tools: xtask (thin)   calc-harness (product deps)   calc-verdict (keep)
research: CalcFunc-research repo (calc-value, calc-verdict only)
```

| Crate | Owns | Moves in / out |
|---|---|---|
| calc-value | values, IDs, grid, `PartitionId`, `BlockId`, `StyleId` | `ArrayShape` to u32 |
| calc-format (new) | number formats, General, palette, locale format profile, `FormatContext`, `NumberText` impl | in: `calc-func/src/format/` (1,364 lines), the format half of `locale.rs`, `DEFAULT_PALETTE`; collation stays in calc-func |
| calc-func | catalogue with metadata (volatility, subtotal, prefix, demand), kernels, `FunctionContext`, `CellReader` (values only), `HostQuestions`, `NumberText` trait, `CallableInvoker` | out: `udf.rs`, `format/`, `CellFormatFacts`/`ColumnWidth`/`WorkbookFile`/`ApplicationFacts` (`call.rs:218-286`), `help.rs` (to a data file read by dna-formula-ui); `with_environment`/`with_host` and the services-to-reader fallback (`call.rs:385-398`) deleted |
| calc-fml | lexer, templates, plans, `FormulaEvaluationContext`, `CallableRegistry`/`CallableTable`, callables, the one sheet-name matcher, template rendering to A1, structural transform per template, `Instruction::UserCall { slot }` | in: registry from calc-func, volatility from the name list to catalogue metadata; `ReferenceReader` loses `environment()`/`host_facts()` (`reference/read.rs:47-55`) |
| calc-doc-model | pages (keep), `FormulaStore` + blocks, `StyleRuns`, typed CF/DV records, names, data tables, `WorkbookOp` | in: `FormulaBlock`; out: `NEXT_LINEAGE` global (`root.rs:18`), per-cell `formats` map |
| calc-doc | codec, fidelity, save | out: its own `Prefix`/`file_form_table.rs`, direct calc-func dependency (lexer via calc-fml); shared formulas load as blocks |
| calc-engine | block nodes, typed edges, lanes, per-node publication, `CancelToken`, `Frontier` seam, participants incl. CF/DV, data tables | in: `calc-host/conditional.rs` evaluation (as participants); out: spelling checks (`recalc.rs:1161-1166`), `templates: HashMap<(SheetId, FormulaForm, String), _>` (replaced by `TemplateId`), six reader wrappers collapse to one `RecalcReader` |
| calc-host | session actor, scheduler, `HostCommand` families with validation, `present()`, `WorkbookFacts`, projections by `Interest`, save | in: sheet/pane/axis/rule/replace/paste/layout rules from `apps/dna-calc/src/server.rs`; out: `LoadedSheet` as live state (`lib.rs:96`), CF evaluation |
| calc-model (new) | 1.6; Leptos-free; drives `tests/workbook.rs` natively | in: `controller.rs` minus DOM (selection, edit session, clipboard, sheets, history), `grid/selection.rs` navigation, `sheet_metrics` |
| calc-wire (new) | `Request::{Command, Project, Subscribe}`, `WindowDelta`, `StyleTable`, skin-neutral projections (`Window`, `Neighbourhood`, `FormulaListing { filter: Filter }`, `Census`, `Layout { view: ViewKind }`) | in: `wire.rs` generics; out: `Canvas` pixels and SVG, `Ledger` strings, `Intent::Navigate` typed from the Grid |
| calc-vba | object library (Application, Range, WorksheetFunction), descriptor model, type-library generation, typed shims, `SheetAccess` | in: `oxvba-embed/src/spreadsheet.rs` (rewritten over vtables); depends on calc-fml and `oxvba-host-api` only |
| calc-exact-x86 | x87 substrate, FP environment handling | new home for the native Exact adapter |
| dna-calc | skins, worker main, DOM adapter (`focus`, key replay asking the active skin) | out: everything above |
| dna-onecalc | a 300-line controller over calc-fml/calc-format/calc-vba, fixture | `surface.rs` split |
| xtask / calc-harness | see S11 | `ci.rs`, `ci_audit.rs`, evidence commands, `infra_m0_*` acceptance, `accept-receipts/`, `source-manifest.json` deleted |

Kept as is: the page store (`cells.rs:1-10`), generational IDs, `AnchorIndex`, `RecalcWork`
resumability and readiness, `Calculation` per result, prefix-sum geometry, the one-message
transport with in-process fallback (`port.rs`), `cargo xtask check`'s touched-plus-dependents
selection, content-hash captures, `calc-verdict`, dna-formula-ui's ceiling, dna-strand.

Dependency rules added: calc-func forbids calc-format (presentation never below functions);
calc-format forbids calc-func; calc-model forbids leptos/web-sys; calc-vba forbids calc-engine,
calc-doc*, calc-host; dna-onecalc forbids calc-model, calc-wire; research repo forbids calc-func.

**Open question 4.5 (Cargo edge to CalcVBA).** Recommend: *the glue crate alone* may depend on a
neutral `oxvba-host-api` crate (traits, descriptors, ABI types; no runtime, no calc-* dependency),
pinned by revision. Reasons: (1) the owner's rule is about content (no spreadsheet in CalcVBA, no
VBA in CalcFml/Engine), and a single edge plus `architecture.json` enforces that mechanically;
(2) a pure binary boundary would force two wasm modules talking through JS in the browser and a
dynamic-library load on every platform, which costs the early-bound call the owner wants and
makes the JIT's typed entry a cross-ABI call; (3) the same descriptor types can still *also* be
exposed as a C ABI and a WIT world for out-of-process hosts later. The current single edge
(`calc-vba/Cargo.toml:15`) is the right topology with the wrong contents.

## 4. Implementation strategy

Order chosen so each step leaves the product shippable and removes a coupling the next step
would otherwise fight. Durations are rough; every step ends with `cargo xtask check` green and
`cargo xtask corpus` unchanged or better.

0. **Inner loop (days).** Thin xtask + `calc-harness`; delete receipts, retired CI code, frozen
   planning files; `opt-level = 2` for calc-func/calc-value in dev; smoke tables move to the app
   with `--case`, 30-case core per UI commit, rest in sweep; check prints NOT-RUN counts. Nothing
   else should wait on a double build.
1. **Explicit context (1-2 weeks).** `FunctionContext` and `FormulaEvaluationContext` with no
   defaults; delete the six forwarders and the fallback; `CallableRegistry` to calc-fml with
   slot + generation; `SemanticEnvironment` vs `VolatileServices`; catalogue metadata for
   volatility/subtotal/prefix. Fixes ROW() in OneCalc, data-table refusals, the clock
   invalidation. dc-snz.6 lands on this, not on the `Rc` registry.
2. **Formatting and presentation (1 week).** `calc-format`; `present()` in calc-host; apps stop
   calling `calc_func::cell_text`; one width/alignment/sheet-name route. Plan 4.4 amended.
3. **Recalc contract (2-3 weeks).** Lanes, `DemandSet` from interests, per-node publication,
   `CancelToken`, session actor with budgeted steps in the worker, persistent pool; fix the
   O(workbook) costs (`Progress::new(nodes.len())`, readers-cache drop, `project()` recomputing
   `stale()`, CF full resets). Stop adding atomic-generation tests the day this starts.
4. **Command model and wire (2-3 weeks, UI lane).** `calc-model`, typed `HostCommand`s,
   `calc-wire` with columnar `WindowDelta`, skin-neutral projections, skins reduced to mount +
   gestures. Freeze new skin features until the Grid and one other skin run on it.
5. **Blocks, runs, typed rules (3-4 weeks).** `FormulaStore` + blocks through model, codec,
   engine, host, wire; `StyleRuns`; typed CF/DV as participants. Decide before more of the save
   path settles; the function ports do not touch this and continue in parallel.
6. **CalcVBA (parallel, CalcVBA lane).** OxVba: neutral hosting API, delete `oxvba-embed`'s
   spreadsheet surface, host-object vtable ABI, `ComCallEarly` lowered to slot calls in VM3 and
   JIT, typed procedure entries. DnaCalc: `calc-vba` owns the object library and shims. Early
   binding complete in interpreter and JIT before any late-bound DnaCalc path.
7. **Partition seam (design + types only).** `PartitionId`, `Frontier`, tick coordinator
   interface; one in-process test with two partitions. Implementation waits for the owner.

Test strategy: the engine gets a seeded lab scheduler (shuttle/loom for the small kernels) and
replay cases for edit/cancel/retarget interleavings; `calc-model` is tested natively without a
browser; browser smoke covers real events only; per-function status rows are generated and
published with the corpus summary; bead-named tests are renamed when touched, never as a project.

Library choices. **asupersync** (owner ruling 2026-10-31: the licence rider is not a bar;
ring-fence it) on technical merit, from the verified facts in libs 1: the fit is strong at the
*wave* and *actor* grain (regions closing on quiescence, request/drain/finalize cancel with
explicit checkpoints, a Cancel lane and EDF lane, two-phase publication, seeded replay with
virtual time) and poor at the *cell* grain (a task costs several times a tokio task, spawn+join
12-23x slower; priority is a budget field, so re-prioritising means cancel and respawn). Its
browser lane is single-threaded preview with no scheduler loop, cancel-by-drop instead of drain
and no browser CI, so native and browser would have different cancellation semantics, which
breaks "same engine everywhere". Pre-1.0, no stability promise, about 100 dependencies and 57 MB
of source, stable Rust for default features but nightly for `?` on `Outcome`, one maintainer who
merges no outside contributions. Recommendation: adopt it **only** behind `calc-sched` as the
native scheduler for session actors, partition actors and recalc waves (tens of tasks, not
millions), with our serial implementation always compiled and the browser on it; use its lab
runtime for the replay tests in 1.4 natively. Decide by measurement at step 3: wave spawn and
checkpoint overhead against the in-house pool on the 100k-formula fixture; drop the adapter if it
does not pay for its build weight. Third-party notices carry the rider. Sibling crates: none
needed (frankensqlite needs nightly and has memory-only wasm persistence; frankengraphdb has no
wasm; frankentui is a reference for virtualised lists only); `br`/`bv` stay as tooling.
No salsa for cells (per-query overhead); its early-cutoff idea is already in the plan key.
formualizer and IronCalc as benchmark comparators only. Arrow not now; the columnar result store
is small and ours. `windows-core` for Windows COM; `msft-typelib` vendored if we read Office
typelibs; Cranelift stays in OxVba; `wasm-encoder` for the browser JIT later. No rayon: the pool
must honour lanes, cancel and affinity.

Lane redirection: **UI lane** stops feature slices and does steps 0 and 4 (its own pain:
controller/server/grid conflicts). **Bit-exact lane** moves fitting to the research repository,
reverts fitted switches, produces status rows and witnesses; no match-count commits. **CalcVBA
lane** stops adding Range members to `oxvba-embed` and does step 6 in order. Function ports
continue unchanged.

## 5. Decisions for the owner

1. **Cargo edge to a neutral CalcVBA hosting-API crate, glue-only.** Recommend yes (3, last
   paragraph). The alternative is a binary boundary everywhere, which costs the early-bound call.
2. **Retire the one-atomic-generation contract for per-node, per-tick publication.** Recommend
   yes; it is the only route to visible-first and cancellable recalculation, and every month of
   atomic-generation tests raises the price.
3. **Formatting crate below calc-fml, amending plan 4.4.** Recommend `calc-format` beside
   calc-func rather than inside calc-fml, so OneCalc and dna-formula-ui use it without the
   evaluator and calc-func depends only on a trait.
4. **Formula blocks now, before the save path settles further.** Recommend now (step 5); it
   touches five crates together and only gets wider.
5. **Function research in a separate repository with import-by-witness.** Recommend yes;
   kernels enter calc-func only with an `Identified` status row.
6. **Freeze UI feature slices for about three weeks** for `calc-model` and the wire. Recommend
   yes; the smoke table grew 333 cases in ten hours on the shape being replaced.
7. **Browser worker pool (SharedArrayBuffer, COOP/COEP) as a profile option, not a requirement.**
   Recommend design for it, ship one worker with lanes first.
8. **Delete `accept-receipts/`, retired CI tooling, frozen planning files.** Recommend yes; 10 MB
   nobody reads and a second build on every edit.
9. **asupersync behind `calc-sched`, native only, decided by measurement at step 3.** Recommend
   this trial rather than basing the engine on it: the owner's ring-fence (serial path we own,
   one adapter crate) is exactly the shape the engine needs anyway, and the browser cannot use it
   today. If the trial fails, the trait and the serial path stay and nothing else moves.
