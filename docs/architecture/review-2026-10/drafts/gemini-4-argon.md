# DNA Calc Target Architecture: Extreme Flexibility and Extreme Optimisation

**Model:** Gemini 4 Argon  
**Date:** 2026-10-31  
**Baseline inspected:** `DnaCalc-review` at `5a9348b`; `OxVba` at `c7ce1349`.

---

## 1. Abstract Architecture

### 1.1 Core Concepts and Invariants

1. **One Evaluator, One Graph, Provenance Before Value.** Every value exposed to a skin, comparison harness, or file writer carries an unforgeable `ValueProvenance`:
   - `Calculated { rev: AuthoredRevision, epoch: CalcEpochId, wave: WaveId, mode: NumericMode }`
   - `FileCached { source_part: SourcePartHash }` (never scores as calculated; never satisfies a fresh dependency read)
   - `Stale { last: Option<CalcEpochId>, cause: StaleCause }`
   - `NotModeled { owner: &'static str, diagnostic: DiagnosticId }`
2. **Three-Way Separation of Authority.**
   - **Authored Authority (`DocModel`)**: Immutable, copy-on-write, page- and interval-segmented representation of what the user or file authored at `AuthoredRevision(u64)`. Holds zero calculated values and zero engine node IDs.
   - **Derived Calculation State (`CalcEngine`)**: A partitioned, demand- and priority-driven dependency graph of *Calculation Participants* whose outputs advance across monotonic `WaveId`s within a `CalcEpoch`.
   - **Functional Session Model (`WorkbenchModel`)**: A headless, DOM-free state machine that accepts typed `Command`s (`CommandId` + payload) and serves reactive `Projection`s over `Interest` subscriptions. Skins only present projections and route user gestures to `Command`s.
3. **Conservation of Explicit Context.** No layer reads ambient globals, thread-locals, or piggybacked data readers for locale, clock, caller coordinates, or presentation metrics. Every downward call passes a narrow, statically typed context struct owning the exact data and callbacks required by that layer.
4. **Work Proportional to Change and Visibility.** Structural edits, formula binding, dirty propagation, and viewport projection scale with O(changed or visible blocks + log N), never O(workbook formulas) or O(grid capacity).

### 1.2 Layer Stack and Explicit Boundary Contexts

```
[ Skins: Grid | Notebook | Atlas | Tree | Canvas | Ledger ]   [ OneCalc Surface ]
        |  Command / SkinKeymap            ^  Binary Tile &        |
        v  & FocusTarget                   |  Graph Projections    v
[ WorkbenchModel (DOM-free functional model, ViewStateStore) ] [ OneCalcController ]
        |================== Worker / RPC Transport ================|
        v                                                          |
[ L4: CalcHost (Authoritative Session, Command Executor, Style/Metric Resolver) ]
        |  EngineExecContext                                       |
        v                                                          |
[ L3: CalcEngine (Block Graph, Priority Wave Scheduler, Spill/SCC/Table Solver) ]
        |  FormulaEvaluationContext  <-----------------------------+
        v                            (with CalcVbaHost / CallableTrampolineTable)
[ L2: CalcFml (CST, R1C1 Templates, Binder, Bytecode VM, Number-Format Engine) ]
        |  FunctionEvalContext (with injected NumberTextService & HostInfoQueries)
        v
[ L1: CalcFunc (Stateless Kernels, Coercions, Static Catalogue Metadata) ]
        |
        v
[ L0: CalcValue (Values, Striped Arrays, Coordinates, Generational/Shard IDs) ]
```

| Boundary | Context Type Passed Down | Who Supplies What (Data & Callbacks) |
|---|---|---|
| **L1 <- L2** (`CalcFunc <- CalcFml`) | `FunctionEvalContext<'a>` | Supplied by `CalcFml` once per call. Carries: `numeric_mode: NumericMode`; `locale: &'a LocaleProfile` (format + collation unified); `date_system: DateSystem`; `caller: CallerAnchor` (sheet, row, col, vector lane, eval role); `reader: &'a mut dyn ValueRangeReader` (pure chunked value/blank/subtotal-exclusion access—**no** environment or host facts); `number_text: &'a dyn NumberTextService` (implemented by `CalcFml` for `TEXT`/`FIXED`/`DOLLAR`/`VALUE`); `ref_resolver: &'a mut dyn ReferenceTextResolver` (implemented by `CalcFml` for `INDIRECT`); `volatile: &'a mut dyn VolatileServices` (`now_serial()`, `next_random_f64()`); `host_info: &'a dyn HostInfoQueries` (pre-resolved abstract answers for `CELL`/`INFO`/`SHEET`/`FORMULATEXT`); `callable: &'a mut dyn CallableInvoker`. |
| **L2 <- L3/Host** (`CalcFml <- CalcEngine` or `OneCalc`) | Compile: `BindCompileContext<'a>`<br>Eval: `FormulaEvaluationContext<'a>` | **Compile**: `catalog: &'a dyn SymbolCatalog` (sheets, scoped names with `BindVersion`, tables, external books); `callables: &'a dyn CallableResolver` (resolves UDF/`LAMBDA` name to `(CallableSlotId, Signature, ThreadAffinity)` at `RegistryGeneration`); `policy: CompilePolicy`.<br>**Eval**: `anchor: FormulaAnchor` (authoritative caller—never queried from host); `semantic_env: &'a SemanticEnvironment`; `volatile: &'a mut dyn VolatileServices`; `reader: &'a mut dyn ValueRangeReader`; `catalog: &'a dyn SymbolCatalog`; `callable_table: &'a CallableTrampolineTable`; `host_info: &'a dyn HostInfoQueries`; `effects: &'a mut EvaluationEffectsSink` (records dynamic reads, runtime `Application.Volatile` flags, and provenance). |
| **L3 <- L4** (`CalcEngine <- CalcHost`) | `EngineExecContext<'a>` | Separates **`semantic_env: SemanticEnvironment`** (changes dirty dependent plans/nodes: `DateSystem`, `LocaleId`, `NumericMode`, `IterationSettings`, `PrecisionAsDisplayed`, `RegistryGeneration`) from **`tick_services: &'a mut dyn TickServices`** (per-epoch `clock_serial`, `rng_seed`, RTD/external snapshots—updating these **never** dirties non-volatile nodes). Also supplies `presentation: &'a dyn PresentationFactProvider` and `demand: RecalcDemand { priority_targets: InterestSet, mode: CalcPolicy, cancel: CancelToken, budget: WorkBudget }`. |
| **L4 <- L5** (`CalcHost <- Worker/App`) | `HostPlatformServices` | Clock provider, OS/release `ApplicationFacts`, resolved host `LocaleId`, worker executor handle, and binary extension loader. Returns `CommandReceipt`, `ResolvedCellPresentation`, and typed `ProjectionDelta`s. |

```rust
// L2 (calc-fml) -> L1 (calc-func): explicit, non-defaulted context; ReferenceReader is pure data.
pub struct FormulaEvaluationContext<'a> {
    pub caller: CallerAnchor, // From FormulaInstance::anchor(), never HostFacts
    pub semantic_env: &'a SemanticEnvironment, // DateSystem, LocaleProfile (format+collation), NumericMode
    pub volatile: &'a mut dyn VolatileServices, // fn now_serial(&mut self) -> f64; fn rand_f64(&mut self, call_idx: u32) -> f64
    pub reader: &'a mut dyn ValueRangeReader,   // read_area, reduce_area, subtotal_exclusions (NO env/host methods)
    pub catalog: &'a dyn SymbolCatalog,         // Sheet/Name/Table lookup with unified Unicode folding
    pub callables: &'a CallableTrampolineTable, // Verified against PlanKey.registry_generation
    pub host_info: &'a dyn HostInfoQueries,     // Pre-resolved CELL/INFO/SHEET/FORMULATEXT answers
    pub effects: &'a mut EvaluationEffectsSink, // Records dynamic reads, Application.Volatile, provenance
}
```

### 1.3 Data Representations End to End

- **Formulas and `FormulaBlock`s**: Per-cell A1 `FormulaText` strings (`crates/calc-doc-model/src/input.rs:85-117`) are replaced by **R1C1 Templates + Contiguous `FormulaBlock`s**. At load or edit, formula text is tokenized once into an interned `TemplateId` (canonical R1C1 token stream with relative/absolute flags) plus an optional sparse `SpellingOverride` (only when whitespace/casing or legacy `_xlfn.` differs from canonical rendering). Contiguous cells sharing `(TemplateId, BindScopeKey)` coalesce into a single `FormulaBlock { id: BlockId, area: GridArea, template: TemplateId, plan: PlanHandle }`. A 1M-row fill of `=A1*2` is **one** `FormulaBlock` in `DocModel` and **one** vectorized node in `CalcEngine` with a single affine interval prerequisite `A1:A1000000`.
- **Values (Paged Striped-SoA Tiles)**: Keep `calc-doc-model`'s copy-on-write 256×16 `CellPage` tree (`crates/calc-doc-model/src/cells.rs:1-35`), but replace dense `CalcValue` structs with **64×16 Striped Columnar Tiles**: a dense `f64` array, a 4-bit `ValueTag` bitmap (`Empty`, `Number`, `Logical`, `Error`, `StringId`, `SpillChild`, `NestedArray`), and compact side vectors for interned `StringId(u32)` and `WorksheetErrorCode(u8)`. Each `CellPage` header caches a `PageSummary` (`occupied_bounds`, `numeric_sum`, `numeric_count`, `min`, `max`, `non_empty_count`, `error_mask`, `subtotal_mask`) maintained incrementally on write.
- **Styles and Formats (2D Interval Runs + `ResolvedStyleTable`)**: Replace per-cell `BTreeMap`s (`LoadedSheet.styles` and `Definitions.formats` at `crates/calc-doc-model/src/definitions.rs:315`) and O(edits) axis-replay (`crates/calc-host/src/facts.rs:36-47`) with per-sheet **`AxisStyleRuns`** (sorted interval lists for row/column formatting) plus **`RectStylePatches`** (R-tree of rectangular style overrides) that shift directly under structural edits. Every unique `(StyleId, FontMetrics, ColWidthPx, CompiledNumberFormat)` is interned into a host-owned `ResolvedStyleTable` queried in O(1) by both the Grid renderer and `CELL()`.
- **Names and Tables**: Each `DefinedName` (`NameId`) and `Table` (`TableId`) carries a monotonic `BindVersion(u32)`. `PlanKey` records the exact `(NameId, BindVersion)` pairs a plan bound against; editing one name invalidates only plans in its reverse subscriber list, never the entire `templates` map (`crates/calc-engine/src/edit.rs:609`).
- **Dependency Graph, IDs, and Partitions (Webs-Scale Ready)**:
  - **Addressing**: Two-tier node identity: `LocalNodeId { slot: u32, gen: u32 }` inside a partition, and `GlobalNodeId { shard: ShardId(u32), local: LocalNodeId }` across partitions, paired with a document-stable `LogicalNodeKey` (`WorkbookId`, `SheetKey`, `BlockId`). Grid coordinates support a `SheetLimitsPolicy` (`Excel2007` 1,048,576×16,384 default vs. `Unbounded64` for Webs-class programmatic tables). `ArrayShape` uses `u32` (not `usize`, which truncates to 32-bit on `wasm32` in `crates/calc-value/src/value.rs:108-111`).
  - **Dual-Edge Graph**: Static edges live in a compressed CSR + **2D Interval Directory** (mapping rectangular areas and whole rows/columns to dependent `NodeId`s or `FormulaBlock`s without per-cell reverse-index entries). Dynamic edges (`INDIRECT`, `OFFSET`, spill blockers) live in a replaceable `DynamicEdgeOverlay`. Node tables are segmented into 4,096-slot pages with free-list compaction.

### 1.4 The Recalculation Contract

- **Demand and Interest Sets**: `HostSession` passes the union of active skin `Interest`s (viewport `GridArea`s, subscribed `BlockId`s, watch cells) as `RecalcDemand::priority_targets`.
- **Backward-Cone Priority Scheduling**:
  1. Edits mark affected nodes in a paged `DirtyBitmap` (O(changed blocks), zero O(workbook) allocations).
  2. When recalculation starts or the viewport moves, the scheduler traces the **Priority Backward Cone** $C_{\text{vis}} = \text{ancestors}(\text{priority\_targets}) \cap \text{Dirty}$ plus any unresolved **Potential-Spill / Dynamic-Topology Roots** on the target sheets.
  3. Ready nodes in $C_{\text{vis}}$ run in **Lane 0 (Interactive)**; remaining dirty nodes run in **Lane 1 (Background Closure)**.
- **Cooperative Cancellation and Preemption**: `EvalFrame` holds a `CancelToken` and an instruction/stripe fuel counter checked at loop back-edges, vector stripe boundaries (every 64 rows), and UDF calls.
  - A *viewport scroll* updates `priority_targets` in place: Lane 0 pivots to the new backward cone while preserving all staged values already computed in the current epoch.
  - An *authored edit* advances `AuthoredRevision` to $R+1$: only in-flight or staged nodes in the forward cone of the new edit delta $\Delta_{R \to R+1}$ are invalidated; unaffected nodes calculated at $R$ carry forward via input-revision backdating.
- **Wave-Based Incremental Publication**: Within a `CalcEpoch` (which fixes `AuthoredRevision`, `clock_serial`, and `epoch_rng_seed`), the engine commits a `WavePublication` as soon as $C_{\text{vis}}$ (or a 4–8 ms worker time-slice stratum) settles, immediately streaming a binary `ViewportTilePatch` to the UI while background waves finish Lane 1.
- **Determinism Invariant**: `clock_serial` is fixed per `CalcEpoch`, and `RAND()`/`RANDBETWEEN`/`RANDARRAY` draw from a stateless counter-based PRNG `philox4x64(epoch_rng_seed, sheet_id, row, col, call_index)`. Therefore, the settled value of every cell is **strictly invariant** to viewport priority order, wave slicing, scroll preemption, and worker thread count.

```rust
pub struct FormulaBlock {
    pub id: BlockId,
    pub area: GridArea,                  // Contiguous 1D/2D fill region on one sheet
    pub template: TemplateId,            // Interned R1C1 token stream + rel/abs flags
    pub plan: Arc<CompiledPlan>,         // Bound once against (BindScopeKey, NumericMode, RegistryGeneration)
    pub spelling: Option<SpellingPatch>, // Sparse override only when authored text != canonical A1 render
}

pub struct RecalcDemand {
    pub priority_targets: InterestSet,   // Active viewport GridAreas + subscribed BlockIds + watch cells
    pub policy: CalcPolicy,              // Automatic | Manual | AutomaticExceptTables
    pub cancel: CancelToken,             // Checked at loop back-edges, 64-row block stripes, and UDF entry
    pub budget: WorkBudget,              // e.g. 4 ms time-slice / stripe quota on wasm32 worker
}

pub struct WavePublication {
    pub revision: AuthoredRevision,
    pub epoch: CalcEpochId,
    pub wave: WaveId,                    // Monotonic within epoch; Wave 0 is the visible backward cone
    pub settled_tiles: Vec<TileDelta>,
    pub spill_changes: Vec<SpillChange>,
    pub remaining_dirty_count: u64,
}
```

### 1.5 Concurrency and Distribution

- **`RecalcExecutor` Seam**: Core scheduling in `CalcEngine` is a pure state machine emitting batches of independent ready tasks (`NodeTask::Scalar(NodeId)` or `NodeTask::BlockStripe { block: BlockId, row_offset: u32, len: u16 }`).
  1. **`SerialSteppingExecutor`**: Default for single-worker `wasm32` and the permanent deterministic reference twin. Yields to the worker message pump every `WorkBudget` (4 ms).
  2. **`WorkStealingNativeExecutor`**: Pool-backed multi-threaded executor (native and `wasm32` `SharedArrayBuffer` workers). Spills and data tables do **not** disable parallelism (`crates/calc-engine/src/recalc.rs:739-757`); instead, spill-shape resolution acts as a stratum barrier and data-table scenarios run in parallel over immutable `OverlayValueView`s.
  3. **`DistributedShardExecutor` (Webs-Class)**: Partitions a multi-billion-node graph into `Shard`s (by sheet, table, or sub-DAG). Cross-shard dependencies subscribe to Arrow IPC / RaptorQ delta frames keyed by `(ShardId, CalcEpochId, WaveId)`. Uses **early-cutoff backdating**: when a recomputed node produces a bitwise-identical value and shape, downstream shards receive `Unchanged(EpochId)` and skip subgraph evaluation.

### 1.6 Command and Functional Model Beneath Skins

- **Four Strict State Classes**: (1) Authoritative `ModelRoot` (`CalcHost`), (2) Derived `EngineState` (`CalcHost`), (3) Durable `SkinLayout` (`ModelRoot`) + per-skin ephemeral `ViewStateStore` (`AnyMap` keyed by `SkinId`), (4) DOM-free `WorkbenchModel` (selection, active `FormulaEditSession`, undo/redo state, command dispatcher).
- **One `Command` Vocabulary**: Every ribbon button, menu item, keyboard shortcut, context menu, CLI call (`calcctl`), or agent action invokes `WorkbenchModel::dispatch(Command { id: CommandId, payload: CommandPayload })`. `WorkbenchModel::capability(CommandId)` returns `CapabilityStatus::{Enabled, Disabled(Reason), UnavailableOnPlatform}`. One shell-level keymap binds global shortcuts (`Ctrl+Z/Y`, `Ctrl+F/H/G`, `Ctrl+C/V/X`, `F9`, `Shift+F9`) across all skins; a skin provides only local navigation overrides (`SkinKeymap`).
- **Five Composable, Skin-Neutral Projections**: The worker protocol contains **zero skin-named requests**. Skins subscribe to:
  1. `ViewportTileStream` (binary columnar cell tiles + differential scroll patches)
  2. `DependencyGraphQuery` (pure node keys + typed edges within $(d_{\text{back}}, d_{\text{fwd}})$; layout/SVG lives in the skin)
  3. `FormulaCensusQuery` (typed `FilterPredicate` + `SortField` + page window)
  4. `HierarchyOutlineQuery` (sheets, tables, names, row/col outline groups)
  5. `BlockLayoutQuery` (`ViewKind`, `BlockId` window, placements)

```rust
pub trait SkinPlugin: Send + Sync + 'static {
    fn id(&self) -> SkinId;
    fn title(&self) -> &'static str;
    fn keymap(&self) -> &'static [SkinKeyBinding]; // Local gestures only; shell owns Ctrl+Z/Y/F/C/V, F9
    fn mount(&self, api: WorkbenchApi) -> AnyView;
}

pub struct WorkbenchApi {
    pub dispatch: Callback<Command>,
    pub capability: Signal<Arc<dyn Fn(CommandId) -> CapabilityStatus + Send + Sync>>,
    pub projections: ProjectionHub,                // Subscribe to the 5 skin-neutral projection streams
    pub view_state: ViewStateStore,                // Keyed by SkinId; persists across skin switches
    pub focus: FocusTargetRegistry,                // Skins register FocusSlot::PrimarySurface, etc.
}
```

### 1.7 Extension Seams: LAMBDA, UDFs, CalcVBA, COM Object Library, Add-ins, Agents

- **Early-Bound `CallableSlotId`**: `CalcFml` resolves every non-intrinsic callable (`LAMBDA`, VBA `Public Function`, `.xll` UDF, Wasm/WIT extension) at bind time into a `CallableSlotId(u32)` stamped with `RegistryGeneration(u32)`. `Instruction::UserCall` carries `(CallableSlotId, u8 argc)`, never a `String` name (`crates/calc-fml/src/eval/mod.rs:235`).
- **Host-Neutral CalcVBA (`OxVba`) and DNA Calc Glue (`calc-vba-host`)**:
  - **OxVba** knows nothing about spreadsheets. It exposes `oxvba-host-api`: load/compile project against host-supplied type libraries (`TypeLibMetadata` / `.tlb`), enumerate public procedures and typed signatures, return direct JIT function pointers (`extern "C" fn`), and dispatch early-bound calls (`OxInst::ComCallEarly`) via a cross-platform **`#[repr(C)]` COM vtable ABI** (`OxComObject { vtable: *const VTable }` starting with `IUnknown::QueryInterface/AddRef/Release` and `IDispatch` slots).
  - **Formula -> VBA Fast Path**: For typed VBA signatures (e.g., `(f64, f64) -> f64`), `calc-vba-host` binds a specialized C-ABI shim `extern "C" fn(*mut VbaCallCtx, f64, f64, *mut f64) -> VbaStatus`. `CalcFml`'s VM pops arguments directly from its register/value stack and invokes the function pointer with **zero heap allocations, zero string lookups, and zero `Variant` conversions**.
  - **VBA -> Host Early-Bound COM Path**: `calc-vba-host` authors and registers `DnaCalc.tlb` (`Application`, `Range`, `Worksheet`, `WorksheetFunction`) with fixed IIDs, vtable slots, and DISPIDs. Both OxVba's Cranelift JIT and VM3 lower `ComCallEarly` to a direct indirect call through `vtable[slot]`. `Range` objects are `#[repr(C)]` structs refcounted via `IUnknown`, eliminating the 4,096-object cap (`oxvba-embed/src/spreadsheet.rs:151`).
- **Add-ins and AI Agents**: Native `.xll` add-ins (Windows) adapt `XLOPER12` into `CallableTrampolineTable`. Cross-platform plugins and agent-built tools/skins connect via a **Wasm Component Model (WIT)** world (`dnacalc:ext/host`) over the same `Command` and `Projection` contracts.

```rust
#[repr(C)]
pub struct OxComVtable {
    pub query_interface: unsafe extern "C" fn(*mut OxComObject, *const Guid, *mut *mut OxComObject) -> i32,
    pub add_ref: unsafe extern "C" fn(*mut OxComObject) -> u32,
    pub release: unsafe extern "C" fn(*mut OxComObject) -> u32,
    pub slots: [*const (); 0], // Fixed-offset typed vtable slots from DnaCalc.tlb
}

pub struct CallableEntry {
    pub slot: CallableSlotId,
    pub signature: SignatureId,
    pub affinity: ThreadAffinity, // AnyThreadSync | OwnerThreadPinned
    pub purity: CallablePurity,   // Pure | Volatile | StatefulSequential
    pub fast_f64x2: Option<extern "C" fn(*mut VbaCallCtx, f64, f64, *mut f64) -> VbaStatus>,
    pub generic_entry: extern "C" fn(*mut VbaCallCtx, *const RawVal, usize, *mut RawVal) -> VbaStatus,
}
```

### 1.8 Exact Numeric Path and Function Research Lab

- **Product (`calc-func`)**: Ships only clean, deterministic algorithms: `NumericMode::Fast` (pinned portable IEEE binary64) and `NumericMode::Exact(NumericTargetId)` (Excel's exact expression tree, association order, binary64 store barriers, ROM constants, and x87 80-bit extended arithmetic via hardware x87 on x86_64 or verified software `ext80` elsewhere). No fitted lookup tables or match-ratio switches.
- **Research Home (`tools/calc-func-lab`)**: Isolated outside the product workspace closure. Houses Calculation Maps, the `calc_graph_racer` candidate DSL (`RN53`, `RN64`, `DR`, x87 instructions), the active-learning disagreement scheduler, and the build-sharded Excel Oracle Cache (`OxFunc/docs/SMART_SEARCH_AND_ACTIVE_LEARNING_LOOP.md`). Only a candidate that passes 100% of discovery, adversarial, and held-out probes is promoted to clean Rust in `calc-func` with a `RESEARCH_SOURCES.md` provenance citation.

### 1.9 Platform Profiles

| Capability | Windows (`x86_64`) | Linux (`x86_64` / `aarch64`) | Browser (`wasm32-unknown-unknown`) |
|---|---|---|---|
| **Numeric modes** | `Fast` + hardware x87 & `ext80` `Exact` | `Fast` + x87 (`x86_64`) & `ext80` `Exact` | `Fast` + software `ext80` `Exact` |
| **CalcVBA execution** | Cranelift JIT + VM3 fallback | Cranelift JIT + VM3 fallback | Runtime Wasm-emitter JIT (`wasm-encoder`) + VM3 |
| **COM & TypeLibs** | Internal `DnaCalc.tlb` vtable + OS COM (`windows-rs`) | Internal `DnaCalc.tlb` `#[repr(C)]` vtable ABI | Internal `DnaCalc.tlb` `#[repr(C)]` vtable ABI |
| **Add-ins** | Native `.xll` + VBA + Wasm/WIT | VBA + Wasm/WIT | VBA + Wasm/WIT |
| **Recalc execution** | `WorkStealingNativeExecutor` | `WorkStealingNativeExecutor` | `SerialSteppingExecutor` (4 ms wave yield) + optional `SharedArrayBuffer` pool |

---

## 2. Seam Pressures

### 2.1 Function Context vs. Presentation and Circular Layering
- **Evidence**: `calc-func/src/format/mod.rs:1052-1126` owns `cell_text` and `cell_color` in violation of Plan §4.4, and `apps/dna-calc/src/cells.rs:187,209` and `dna-formula-ui/src/result.rs:430` call it directly (hard-coding `DateSystem::System1900`). Meanwhile, `calc-fml` has no `FormulaEvaluationContext`: six `ReferenceReader` wrappers manually forward `environment()` and `host_facts()`, every method on `HostFacts` defaults to `None` (`calc-func/src/call.rs:113-169`), and forgetting to forward in `WhatIfReader` (`calc-engine/src/data_table.rs:308-388`) silently refuses `ROW()`, `COLUMN()`, `SHEET`, and `CELL` inside data tables. `ROW()` is refused in OneCalc because `lookup.rs:1751` asks `HostFacts::caller` instead of `FormulaInstance::anchor()`. Grid and `CELL("width")` compute column width by different formulas (`apps/dna-calc/src/grid/render_plan.rs:140-143` vs. `calc-host/src/facts.rs:53-71`).
- **Design Response**: Move `format/` to `calc-fml` and inject `&dyn NumberTextService` into `calc-func`. Introduce a mandatory, non-defaulted `FormulaEvaluationContext` in `calc-fml` that always populates `CallerAnchor` from `FormulaInstance::anchor()`. Strip `environment()` and `host_facts()` from `ReferenceReader`. Compute style, alignment, and column width in one `calc-host::ResolvedPresentationQuery` shared by the Grid and `WorkbookFacts`.

### 2.2 Shared Formulas and Formula Blocks vs. Per-Cell Identity and Structural Edits
- **Evidence**: `calc-doc/src/load/mod.rs:268-271` expands shared formulas into 1M separate `Arc<[u16]>` strings. Each cell gets its own `FormulaNode`, `FormulaInstance`, `Vec<Prerequisite>`, and reverse-index entry (`calc-engine/src/recalc.rs:1128-1207`, `graph.rs:342`). Inserting one row rewrites and re-parses every formula string in the workbook (`calc-doc-model/src/structural.rs:23-31`, `calc-engine/src/edit.rs:479-609`), and undoing it rebuilds the entire engine from scratch (`calc-host/src/command.rs:447-459`).
- **The Non-Obvious Trap**: If a 1M-row fill is collapsed into a single `FormulaBlock`, what happens when (a) a user overwrites cell `B500` in the middle of `B1:B1000000`, (b) a formula inside the block calls `ROW()`, `CELL("address")`, or `Application.Caller`, or (c) a relative reference wraps or hits `#REF!` only on edge rows?
- **Design Response**:
  1. **Split-on-Write Interval Blocks**: `FormulaBlock`s are stored in a 2D interval tree. Editing `B500` splits `B1:B1000000` in O(log B) into `B1:B499`, scalar `B500`, and `B501:B1000000`—all three retaining the same `TemplateId` and `CompiledPlan` with zero re-parsing.
  2. **Structural Shift without Re-Parse**: Inserting a row at $r$ shifts the `GridArea` of blocks below $r$ and adjusts only relative references that cross $r$ at the `TemplateId` level (at most O(unique templates), typically <100 per workbook, never O(cells)). Undo applies the inverse `BlockPatch` directly to `EngineState`.
  3. **Edge-Clipping & Vector Lane Context**: During block instantiation, reference offsets that clip at sheet bounds automatically split the block along the boundary line so interior rows are guaranteed bounds-valid. Vectorized evaluation passes `CallerAnchor { block_origin, lane_idx }` so `ROW()` evaluates as an affine vector `[r..r+64]`.

### 2.3 Visible-First Recalc and Incremental Waves vs. Excel's Global Calculation Semantics
- **Evidence**: `calc-engine/src/recalc.rs:480-620` schedules the entire dirty closure up front and `finish` (`recalc.rs:1001-1013`) publishes all-or-nothing. Parallelism is disabled whenever any spill, data table, or cycle exists (`recalc.rs:739-757`). Changing the clock invalidates every formula in the workbook (`calc-engine/src/state.rs:783-789`).
- **The Non-Obvious Traps**:
  1. *Phantom Blank Reads from Unresolved Spills*: Visible cell `Z1` reads `A500`. `A500` has no authored formula, so static backward-cone tracing from `Z1` sees `A500` as a blank literal. In reality, off-screen cell `A1` holds `=SEQUENCE(1000)` which will spill into `A500`! Evaluating `Z1` before `A1` produces the wrong answer.
  2. *Order-Sensitive Volatility and Stateful UDFs*: If scrolling changes evaluation order, a shared mutable PRNG (`RandomSource::next_unit` at `calc-func/src/call.rs:80`) or a stateful VBA UDF (`Private n As Long: n = n + 1`, `docs/plan/VBA_UDFS.md:78-84`) will yield different values depending on scroll history.
  3. *Partial Cycles and Data Tables*: Publishing mid-iteration of a circular SCC or mid-scenario of a What-If `TABLE` exposes transient un-converged values.
- **Design Response**:
  1. **Potential-Spill Shadow Zones**: Any dirty formula node whose `CompiledPlan` has `can_spill() == true` (or `has_dynamic_ref() == true`) registers a sheet-level `UnresolvedTopologyBarrier` bounding its potential reach. Backward-cone extraction from `InterestSet` automatically pulls overlapping `UnresolvedTopologyBarrier` roots into a pre-wave topology pass before evaluating cells that read their target sheet.
  2. **Coordinate-Keyed PRNG & Epoch-Pinned Clock**: Separate `TickServices` (clock, RNG seed) from `SemanticEnvironment`. Replace sequential `RandomSource::next_unit()` with `philox4x64(epoch_seed, sheet_id, row, col, call_idx)`, making random draws 100% invariant to evaluation order. If a workbook registers a stateful VBA UDF (`CallablePurity::StatefulSequential`), nodes calling it are scheduled in canonical `(SheetIndex, Row, Col)` order on the owning thread.
  3. **Atomic Super-Nodes**: Condense each SCC and each What-If `TABLE` into an atomic scheduling super-node; wave publication boundaries never split a super-node. Implement `TABLE` scenarios via a copy-on-write `OverlayValueView` that preserves the full `FormulaEvaluationContext` (fixing `data_table.rs:308`).

### 2.4 Worker Boundary vs. Fine-Grained Reactivity and Browser Smoke Harness
- **Evidence**: `apps/dna-calc/src/wire.rs:730-745` serializes ~8 owned strings per cell (including prose `why` sentences, `cells.rs:321-350`) as JSON over `postMessage`. Every 32×8 scroll block boundary triggers `SetInterest`, which sends a full window reset (`calc-host/src/session.rs:735`), clears the client mirror (`controller.rs:97-117`), invalidates the render-plan `Memo` (`grid/mod.rs:355-372`), and rebuilds every unkeyed cell `<div>` (`grid/mod.rs:1216-1281`). Meanwhile, `tools/xtask/src/app.rs` has 97 selectors asserting on per-cell DOM `data-*` attributes.
- **Design Response**:
  1. **Binary `ViewportTilePatch` + Differential Scroll**: Worker sends binary columnar tile patches (`ValueTag`, `f64`, `StyleId(u16)`, `ProvenanceCode(u8)`, string table slice) only for newly entered 32×8 tiles; prose provenance sentences are formatted on the client from `(ProvenanceCode, CalcEpochId)` on hover/inspection.
  2. **Tile-Granular Signals + `Canvas2D` + Bounded A11y DOM Overlay**: Client stores tiles in a `ViewportTileStore` where each 32×8 tile has its own Leptos `RwSignal<TileGeneration>`. The Grid renders cells at 60 fps onto `Canvas2D` while mounting a lightweight **Semantic A11y DOM Overlay** only for the active selection/focus neighborhood (plus an explicit `window.__dnacalc_inspect(cell)` test bridge), preserving screen-reader accessibility and smoke-test inspectability without O(viewport) DOM allocation on scroll.

### 2.5 Early-Bound VBA and Direct JIT Calls vs. Portability, Reload, and Threading
- **Evidence**: `crates/oxvba-embed/src/spreadsheet.rs:416-634` embeds DNA Calc's `Application`/`Range`/`WorksheetFunction` inside OxVba. Both VM3 (`oxvba-vm3/src/lib.rs:2175-2203`) and the JIT (`oxvba-jit/src/lowering.rs:497-540`, `helpers.rs:4359-4383`) lower `OxInst::ComCallEarly` to heap-allocated, name-keyed `DynamicCallRequest`s. Formula-to-VBA calls perform 4 name lookups, 3 value conversions each way, and 8–10 allocations per call (`crates/calc-vba/src/lib.rs:135-147`, `calc-fml/src/eval/run.rs:266-294`). `calc-vba` uses `thread_local!` and `Rc` (`calc-vba/src/lib.rs:11-13, 85`), blocking multithreaded engine evaluation.
- **The Non-Obvious Trap**: How do we keep direct register-level JIT calls (`f64 -> f64`) when (a) a user reloads a VBA module mid-session (`docs/plan/VBA_UDFS.md:11-12`), (b) `wasm32` cannot allocate executable memory for Cranelift, and (c) native recalculation uses a multi-threaded worker pool while VBA module state is single-threaded?
- **Design Response**:
  1. **Indirect `CallableTrampolineTable` with `RegistryGeneration`**: `CompiledPlan` stores `CallableSlotId(u32)` + `SignatureId(u16)`. On module reload, if a procedure's signature is unchanged, `calc-vba-host` swaps the function pointer in `CallableTrampolineTable[slot]` in place (zero formula recompilation!); if signatures or procedure sets change, `RegistryGeneration` increments and invalidates only plans whose `PlanKey` used the old generation.
  2. **Unified C-ABI Entry Signature Across Backends**: Each trampoline slot holds `extern "C" fn(*mut VbaExecCtx, *const RawArg, *mut RawRet) -> VbaStatus` plus typed fast-path pointers (`FastF64x1`, `FastF64x2`, `FastF64x3`). On native JIT, the pointer targets Cranelift machine code directly; on VM3 or `wasm32` (interpreter or runtime-emitted Wasm module), it targets a thin C-ABI wrapper over the interpreter/Wasm table—identical call site in `calc-fml`.
  3. **Portable `#[repr(C)]` `OxComVtable` ABI**: Host objects (`Range`, `Application`) start with a `*const OxComVtable` layout-compatible with Windows COM on `x86_64` and usable as a plain C struct of function pointers on Linux and `wasm32`. OxVba lowers `ComCallEarly` to `((*obj->vtable).slots[slot_idx])(obj, ...)` on every platform.
  4. **Explicit `ThreadAffinity`**: `CallableTrampolineTable` tags VBA slots as `ThreadAffinity::OwnerThreadPinned`. `WorkStealingNativeExecutor` executes pure built-in strata across worker threads and routes `OwnerThreadPinned` nodes to the session's STA thread queue, keeping `CallableTrampolineTable` `Send + Sync` without `Rc` in `calc-func`.

### 2.6 OneCalc's Dependency Ceiling vs. UDF Registration and Shared Formula Editing
- **Evidence**: `dna-onecalc` must never depend on `calc-engine`, `calc-doc`, or `calc-host` (`tools/xtask/architecture.json:21-26`). Because `UserFunctions` was placed in `calc-func` using `Rc` (`calc-func/src/udf.rs:53`) just so `calc-vba` could avoid `calc-fml`, compilation and evaluation take separate registries (`risk-layering.md` R2). Meanwhile, `dna-onecalc/src/surface.rs:408-1150` has no controller beneath its view, and `dna-calc/src/controller.rs:2107-2485` forked formula editing (`F4` anchor cycling, point mode) away from `dna-formula-ui`.
- **Design Response**: Place `CallableResolver`, `CallableTrampolineTable`, and the number-format engine in `calc-fml`. Make `calc-vba-host` depend **only** on `calc-value`, `calc-func`, `calc-fml`, and `oxvba-host-api` by implementing `Range` and `Application` over `calc-fml`'s `ValueRangeReader` and `HostInfoQueries` traits. Both `dna-onecalc` and `calc-host` then plug into `calc-vba-host` cleanly without touching OneCalc's ceiling. Move `FormulaEditSession` (`F4`, point mode, commit/cancel) into `dna-formula-ui` and extract `OneCalcController`.

### 2.7 Agent-Customizable Skins vs. One Command and Projection Model
- **Evidence**: Adding a skin today requires editing 8 places (`apps/dna-calc/src/skin.rs:31-74`, `app.rs:32-40`, `wire.rs:251-345`, `server.rs:84-392`, `controller.rs:165-232`, `EditOrigin`, `xtask/src/app.rs:126-130`, and five duplicated `"show in skin"` timers in `atlas.rs:134`, `ledger.rs:441`, etc.). `server.rs` imports skin modules (`crate::canvas::view_model`, `crate::grid::selection::navigate` at `server.rs:352, 1339`), and `controller.rs` calls `crate::grid::focus_grid()` in 7 places (`controller.rs:798-2570`).
- **Design Response**: Invert the dependency: `server.rs` and `WorkbenchModel` never import a skin module. Define a self-contained `SkinPlugin` trait (`id: SkinId`, `keymap: SkinKeymap`, `mount(api: &WorkbenchApi) -> AnyView`). Move all Excel command logic (`server.rs:405-1815`) into `calc-host::WorkbookCommand`. Provide one `WorkbenchApi::reveal_in_skin(SkinId, RevealTarget)` command that switches active skin, updates selection, and focuses the registered `FocusSlot::PrimarySurface` of the newly mounted skin without timers or DOM selector strings.

### 2.8 Bit-Exact Symbolic Research vs. Product Code Integrity
- **Evidence**: Tracker bead `dc-j9k` (99.6 KB of notes) and commits like `f937b23` ("retain full matches 3604716 to 3613585 (+8869/-0)") show agents optimizing for match-count deltas via training/control splits and error-based branch switches (`risk-inner-loop.md` §7), directly violating `BRIEFING.md` §4.4.
- **Design Response**: Isolate candidate search, witness banks, and `calc_graph_racer` into `tools/calc-func-lab`. In `calc-func`, enforce an admission rule via `cargo xtask check`: no function kernel commit may cite match-count deltas or introduce lookup/piecewise fix-up tables; every `Exact` kernel change must reference an identified Calculation Map in `calc-func-lab` with 0 discrepancies across the target profile's witness bank.

### 2.9 Billion-Node Partitioned Graph vs. Excel's Whole-Axis and Dynamic Range Semantics
- **Evidence**: `crates/calc-value/src/grid.rs:9-13` bakes in `1,048,576 x 16,384` (183 `MAX_ROWS` uses), and `calc-func/src/call.rs:36-40` visits occupied cells in an area without hierarchical summaries. At $10^9$ nodes across partitions (`BRIEFING.md` §2), `SUM(A:A)` or `MAX(Sheet1:Sheet100!A1:Z100000)` cannot scan pages linearly.
- **Design Response**: Maintain algebraic `PageSummary` and `ShardSummary` trees (Fenwick / segment trees over 256×16 page headers). Associative reductions (`SUM`, `COUNT`, `COUNTA`, `COUNTBLANK`, `MIN`, `MAX`) query `ValueRangeReader::reduce_area(area, ReductionOp)` which combines interior `PageSummary` nodes in O(log P) and visits individual cells only on boundary pages or when `subtotal_mask` / `error_mask` requires filtering.

---

## 3. Reflection onto Rust

### 3.1 Target Workspace Crate Map and Dependency Rules

All rules are enforced in a single `tools/xtask/architecture.json` (deleting the duplicate checks in `sweep.rs:46-47` and `crates/calc-func/tests/acceptance.rs:382-430`).

| Crate | Portability | Owns | Allowed Workspace Dependencies |
|---|---|---|---|
| `crates/calc-value` | Portable | `CalcValue`, striped `CalcArray` (`u32` shape), `ExcelText`, `CellKey`, `GridArea`, `SheetLimitsPolicy`, generational & shard IDs (`LocalNodeId`, `GlobalNodeId`, `CallableSlotId`), `NumericMode`, `LocaleId`, Unicode case folding | *(none)* |
| `crates/calc-func` | Portable | ~500 stateless function kernels, coercions, static catalogue metadata (`FunctionId`, `VolatilityClass`, `is_subtotal`, `FilePrefix`, `NumericSensitivity`), `FunctionEvalContext`, `ValueRangeReader`, `NumberTextService` & `HostInfoQueries` traits | `calc-value` |
| `crates/calc-fml` | Portable | Lossless lexer/CST, R1C1 `TemplateId` normalizer, binder, `CompiledPlan`/`PlanCache`, explicit-frame VM, **Number-Format engine (`format/`)**, `FormulaEvaluationContext`, `CallableTrampolineTable`, sheet-name Unicode matcher | `calc-value`, `calc-func` |
| `crates/calc-vba-host` | Portable* | DNA Calc <-> OxVba glue: `DnaCalc.tlb` definitions, `#[repr(C)]` COM vtable implementations of `Application`/`Range`/`Worksheet`/`WorksheetFunction`, `IUnknown` refcounted handles, typed C-ABI UDF shim generator (*native enables `jit` feature) | `calc-value`, `calc-func`, `calc-fml`, `oxvba-host-api` |
| `crates/calc-doc-model` | Portable | `ModelRoot`, `AuthoredRevision`, 256×16 striped-SoA `CellPage`s + `PageSummary`, **`FormulaBlock` table**, **`AxisStyleRuns` + `RectStylePatches`**, typed `ConditionalRule` & `ValidationRule` ASTs, segmented `Arc` `Definitions`, `WorkbookOp`/`OpBatch`, `SkinLayout` | `calc-value` |
| `crates/calc-doc` | Portable | XLSX/XLSB package reader, preservation ledger, authoring writer, structural shifter, shared-formula -> `FormulaBlock` loader, CF/DV rule parser | `calc-value`, `calc-doc-model`, `calc-fml` |
| `crates/calc-engine` | Portable | `EngineState`, `FormulaBlock` & scalar nodes, Dual-Edge Graph (CSR + 2D Interval Tree + Dynamic Overlay), `DirtyBitmap`, Backward-Cone Priority Wave Scheduler, `PotentialSpillWatch`, Atomic SCC/Data-Table Super-Nodes, CF/DV participants, `RecalcExecutor` | `calc-value`, `calc-func`, `calc-fml`, `calc-doc-model` |
| `crates/calc-host` | Portable | `HostState`, `HostSession`, **complete `WorkbookCommand` executor**, **`ResolvedPresentationQuery` (`ResolvedStyleTable`)**, wave scheduler driver, binary `ViewportTilePatch` & 5 skin-neutral `Projection` builders | `calc-value`, `calc-func`, `calc-fml`, `calc-vba-host`, `calc-doc-model`, `calc-doc`, `calc-engine` |
| `apps/dna-strand` | Portable | Design tokens and CSS variables | *(none)* |
| `apps/dna-formula-ui` | Portable | Shared **`FormulaEditSession`** (`F4`, point mode, commit/cancel), syntax editor, completion/signature UI, **function help catalogue (`help.rs`)**, value/provenance badges, shared dialog/theme primitives | `calc-value`, `calc-func`, `calc-fml`, `dna-strand` |
| `apps/dna-onecalc` | Portable | `OneCalcController` (DOM-free functional model + scratch sheet reader) + thin Leptos `surface.rs` view | `calc-value`, `calc-func`, `calc-fml`, `calc-vba-host`, `dna-strand`, `dna-formula-ui` |
| `apps/dna-calc` | Portable | `WorkbenchModel` (DOM-free: `Command`, `CommandId`, `CapabilityStatus`, `ViewStateStore`, `ViewportTileStore`), `DomAdapter`, thin worker `Server`, `SkinPlugin` registry, and 6 isolated skins (`grid`, `notebook`, `atlas`, `tree`, `canvas`, `ledger`) | `calc-value`, `calc-fml`, `calc-host`, `dna-strand`, `dna-formula-ui` |
| `tools/xtask` | Native-only | Fast repo checks (`check`, `sweep`, `arch`, `licence`, `word-scan`)—**zero dependencies on any `calc-*` or `dna-*` crate!** | *(external CLI/serde crates only)* |
| `tools/calc-harness` | Native-only | Corpus runner, Excel capture driver, witness compare, benchmarks, browser smoke runner (split out of `xtask`) | `calc-*`, `dna-strand`, `calc-verdict`, `calc-oracle-excel`, `calc-platform-desktop` |
| `tools/calc-func-lab` | Native-only | Symbolic algorithm search (`calc_graph_racer`), Calculation Maps, active-learning disagreement scheduler, oracle cache | `calc-value`, `calc-func`, `calc-verdict`, `calc-oracle-excel` |
| `tools/calc-verdict` | Portable | Capture records, hex-bit numeric comparison, provenance-first verdict scoring | `calc-value` |
| `platform/calc-oracle-excel` | Native-only | Isolated Windows COM Excel runner with UIA modal watchdog | `calc-value`, `calc-verdict` |
| `platform/calc-platform-desktop` | Native-only | Tauri / native desktop shell, file I/O, `.xll` and OS COM adapters | `calc-host`, `dna-calc` |

### 3.2 Disposition of Current Crates and Modules

- **Move**: `calc-func/src/format/` -> `calc-fml/src/format/`; `calc-func/src/help.rs` -> `dna-formula-ui/src/help.rs`; `oxvba-embed/src/spreadsheet.rs` -> `calc-vba-host/src/excel_om.rs`; Excel commands in `apps/dna-calc/src/server.rs:405-1815` -> `calc-host/src/command.rs`; `CALC_CASES`/`ONECALC_CASES` in `tools/xtask/src/app.rs:989-7253` -> `apps/dna-calc/tests/` and `apps/dna-onecalc/tests/`; harness commands in `tools/xtask` -> `tools/calc-harness`.
- **Rewrite**: `calc-func/src/udf.rs` -> `calc-fml/src/callable.rs` (`CallableTrampolineTable`); `calc-doc-model/src/conditional.rs` (raw XML -> typed AST); `apps/dna-calc/src/controller.rs` (2,817 lines) -> `model/{command, selection, edit, view_store, tile_store}.rs` + `dom.rs`; `apps/dna-calc/src/wire.rs` (skin-specific requests & per-cell JSON -> 5 neutral projections & binary `ViewportTilePatch`).
- **Delete**: `accept-receipts/` (10.2 MB); frozen `docs/plan/{BEAD_DETAILS.md, beads.*.jsonl, build_plan.py}` (2.6 MB); `migration/source-manifest.json` (3.3 MB); `tools/xtask/src/{ci.rs, ci_audit.rs, evidence.rs, evidence_labels.rs}`; duplicate `calc-doc/src/save/file_form_table.rs` (3,571 lines); `ReferenceReader::environment()`/`host_facts()` fallbacks (`calc-func/src/call.rs:385-398`).
- **Keep As Is**: `calc-doc-model` 4-level `Arc` page index (`cells.rs:1-35`); generational ID allocator (`calc-value/src/id.rs`); `Calculation` provenance stamps; prefix-sum `ScrollScale` geometry; `calc-verdict` and content-hash-keyed section captures (`tests/captures/catalogue/sections/`).

---

## 4. Implementation Strategy

### 4.1 Phased Order of Work (Shippable at Every Step)

1. **Phase 0 — Unblock the Inner Loop and Prune Ceremony (Immediate, 1–2 commits)**
   - Split `tools/xtask` (zero product deps, builds in default `target/`) from `tools/calc-harness` (eliminating the double build in `risk-inner-loop.md` §2). Add `[profile.dev.package.{calc-value, calc-func}] opt-level = 2` to `Cargo.toml`.
   - Delete `accept-receipts/`, frozen planning JSONLs, `source-manifest.json`, and retired `ci`/`evidence` modules. Unify all layering rules into `tools/xtask/architecture.json`. Print `ACCEPT-NOT-RUN` counts in `check`.
   - Extract browser smoke cases to `apps/*/tests/`, add `--case <filter>` and fresh-page isolation, and strip brittle refusal-total/crate-name strings.
2. **Phase 1 — Explicit Context, Formatting Move, and Presentation Unification**
   - Move `format/` to `calc-fml`; inject `NumberTextService` into `calc-func`.
   - Land `FormulaEvaluationContext` in `calc-fml` and `FunctionEvalContext` in `calc-func`; pass `LocaleProfile` into `text_order`; split `SemanticEnvironment` from `TickServices` in `calc-engine` (fixing `set_clock` full-invalidation).
   - Move volatility and `is_subtotal` to `FunctionId` catalogue metadata; delete `calc-doc`'s duplicate prefix table.
   - Build `calc-host::ResolvedPresentationQuery` (`ResolvedStyleTable`) and wire both Grid and `WorkbookFacts` (`CELL()`) to it.
3. **Phase 2 — Command Model, Host Consolidation, and Strict Skin Architecture**
   - Move worksheet, pane, axis, CF, find/replace, paste, and block-layout execution from `dna-calc/src/server.rs` into `calc-host::WorkbookCommand`.
   - Replace skin-named `wire::Request`s with the 5 skin-neutral projections; move Canvas coordinate/SVG layout back into `apps/dna-calc/src/canvas.rs`.
   - Split `controller.rs` into DOM-free `WorkbenchModel` (`Command`, `CommandId`, `CapabilityStatus`, `ViewStateStore`), `DomAdapter`, and unified shell keymap. Refactor all 6 skins to implement `SkinPlugin` over `WorkbenchApi`. Extract shared `FormulaEditSession` into `dna-formula-ui` and `OneCalcController` into `dna-onecalc`.
4. **Phase 3 — O(1)/O(changed) Engine Fixes, `FormulaBlock`s, and Priority Wave Recalculation**
   - *Quick O(N) kills*: Replace `Progress::new(nodes.len())` and `vec![false; nodes.len()]` (`recalc.rs:552, 1018`) with `DirtyBitmap`; wrap `Definitions` sub-tables in individual `Arc`s (`root.rs:657-683`); parse CF/DV rules into typed model structs at load and evaluate them as engine participants instead of full-reset projection hooks (`session.rs:852-866`).
   - *Block & Tile Storage*: Introduce `FormulaBlock` (R1C1 `TemplateId` + `GridArea`) in `calc-doc-model`, stop expanding shared formulas in `calc-doc/src/load/mod.rs:268-271`, and add 2D interval edges + split-on-write structural edits in `calc-engine`. Upgrade cell pages to striped SoA tiles with `PageSummary` and `AxisStyleRuns`.
   - *Wave Recalc & Binary Viewport*: Implement backward-cone priority scheduling (`InterestSet`), `PotentialSpillWatch`, coordinate-keyed `philox4x64` PRNG, and monotonic `WavePublication` with 4 ms worker yields. Switch Grid transport to binary `ViewportTilePatch` + `Canvas2D` + bounded Semantic A11y DOM Overlay.
5. **Phase 4 — Host-Neutral CalcVBA, Early-Bound COM Vtables, and Typed JIT UDF Shims**
   - Extract `oxvba-host-api` in OxVba; lower `ComCallEarly` to direct C-ABI vtable slot calls in Cranelift JIT and VM3; delete `oxvba-embed/src/spreadsheet.rs`.
   - In `calc-fml` and `calc-vba-host`: replace string-keyed `UserCall` with `CallableSlotId` + `RegistryGeneration` in `PlanKey`; implement `DnaCalc.tlb` (`Application`, `Range`, `WorksheetFunction`) with `IUnknown` refcounting in `calc-vba-host`; generate typed C-ABI shims (`f64 -> f64`) into JIT entry points.
6. **Phase 5 — Webs-Class Partitioned Shards**
   - Enable `GlobalNodeId(ShardId, LocalNodeId)`, `SheetLimitsPolicy::Unbounded64`, and `DistributedShardExecutor` with early-cutoff backdating and Arrow IPC batch streams.

### 4.2 Library Choices

- **`asupersync`**: **Borrow the architecture; ring-fence the crate as an optional native adapter behind `RecalcExecutor`.** Even with the 2026-10-31 owner licence ruling, `asupersync` 0.5 is pre-1.0, requires nightly for `?` on `Outcome`, has 12–23× tokio's task-spawn cost (too heavy for fine-grained cell strata), and its `wasm32` runtime is preview-only without a scheduler loop or drain-on-cancel (`research-libraries.md` §1). We implement its best ideas—region-scoped waves, cooperative fuel checkpoints, cancel-lane priority, two-phase reserve/commit publication, and seeded deterministic schedule replay—directly in `calc-engine`'s portable wave scheduler, and keep an optional `asupersync` feature in `calc-platform-desktop` for coarse multi-shard orchestration.
- **Other Libraries**: **`arrow-rs`** (with `default-features = false`) for Webs-class cross-shard/IPC columnar batches and external data tables (using our lighter 64×16 striped SoA tiles for mutable worksheet pages); **`msft-typelib`** (vendored and verified against Excel/Office `.tlb`s) + **`windows-core`** (`#[interface]`/`#[implement]` on Windows) for cross-platform `DnaCalc.tlb` early binding; **Cranelift** (native JIT) paired with **`wasm-encoder`** (browser runtime Wasm-module JIT emission) behind OxVba's OxIR; **Wasm Component Model / WIT** (`wit-bindgen` + `wasmtime` native / `jco` browser) for sandboxed third-party add-ins and agent extensions; **Salsa / DBSP**: do not adopt for core cell recalc (per-query memo overhead in Salsa and relational mismatch/no-wasm in DBSP), though DBSP remains a candidate for Phase-2 Power Query streaming tables.

### 4.3 Redirecting the Active Codex Worker Lanes

1. **UI Slices Lane (`dc-971p.*`)**: **Halt feature additions to `controller.rs`, `server.rs`, and `xtask/src/app.rs`.** Redirect immediately to Phase 0 (moving/splitting `CALC_CASES` with `--case` filtering) and Phase 2 (moving Excel commands from `server.rs` to `calc-host`, splitting `WorkbenchModel` + `CommandId`, and extracting skin-neutral projections).
2. **Bit-Exact Function Lane (`dc-j9k` / `dc-zmk`)**: **Halt match-count delta commits in `calc-func`.** Redirect to Phase 1 (`FormulaEvaluationContext`, moving `format/` to `calc-fml`, `LocaleProfile` collation) and stand up `tools/calc-func-lab` for symbolic Calculation Map identification.
3. **CalcVBA Lane (`dc-snz.5` / `bd-59co.2.10.6`)**: **Halt adding `Range`/`WorksheetFunction` methods to `oxvba-embed/src/spreadsheet.rs`.** Redirect to Phase 4: move `spreadsheet.rs` into DNA Calc's `crates/calc-vba-host`, build the `#[repr(C)]` `OxComVtable` lowering for `ComCallEarly` in OxVba's VM3 and JIT, and wire `CallableSlotId` + typed C-ABI shims in `calc-fml`.

---

## 5. Decisions for the Owner

1. **CalcVBA <-> DNA Calc Boundary Topology (`BRIEFING.md` §4.5)**
   - *Question*: Does "no code cross-dependencies" require a pure binary/C-ABI boundary with zero Cargo edge, or may `crates/calc-vba-host` alone depend on a spreadsheet-free `oxvba-host-api` Cargo crate?
   - *Recommendation*: **Allow `crates/calc-vba-host` alone to depend on a tiny, spreadsheet-free `oxvba-host-api` crate whose public types are all `#[repr(C)]` vtables and function pointers** (enforced in `architecture.json`). This gives us zero-overhead C-ABI/vtable calls and dynamic `.dll`/`.so`/`.wasm` loadability by construction, while letting Cargo compile native and `wasm32` bundles cleanly in one workspace build.
2. **Epoch-Pinned Wave Publication vs. Whole-Generation Atomic Publication (`BRIEFING.md` §2)**
   - *Question*: Should recalculation publish settled visible cells incrementally in waves before the full dirty closure completes, relaxing Plan §2.3's whole-generation atomicity?
   - *Recommendation*: **Yes—adopt Epoch-Pinned Monotonic Wave Publication.** Visible-first priority over large graphs only improves perceived latency if the visible cone publishes as soon as it settles (Wave 0) while off-screen dirty nodes finish in background waves. Pinning the `CalcEpoch` (authored revision, clock, coordinate-keyed PRNG) and keeping SCCs and Data Tables as atomic super-nodes preserves 100% of Excel's calculation semantics.
3. **`asupersync` Role (`BRIEFING.md` §4.7)**
   - *Question*: Should `asupersync` be a core dependency of `calc-engine`, or ring-fenced behind `RecalcExecutor`?
   - *Recommendation*: **Ring-fence `asupersync` as an optional native backend behind `calc-engine`'s `RecalcExecutor` trait**, and implement the portable wave/checkpoint/cancellation state machine directly in `calc-engine` so `wasm32` and native share identical scheduling semantics without nightly Rust or per-task spawn overhead on cell strata.
4. **Grid Rendering Surface (`Canvas2D` + Bounded A11y DOM vs. Per-Cell `<div>`s)**
   - *Question*: May we replace the current per-cell `<div>` grid rendering (and update the 97 `data-*` selectors in the browser smoke harness) with `Canvas2D` backed by `ViewportTileStore` and a bounded Semantic A11y DOM Overlay?
   - *Recommendation*: **Yes.** Re-allocating thousands of DOM nodes with inline style strings on scroll is incompatible with 60 fps viewport-priority recalculation and Webs-scale grids.
5. **Portable `Exact` Numeric Mode via Verified Software `ext80` (`BRIEFING.md` §4.4)**
   - *Question*: Once `calc-func-lab` identifies an Excel function's exact x87 calculation graph and store barriers, should `NumericMode::Exact` execute on `wasm32` and ARM64 via deterministic software `ext80` (`RN64`/`DR`), rather than restricting `Exact` to x86_64 hardware?
   - *Recommendation*: **Yes.** Because Excel's results follow the x87 ISA + explicit binary64 stores, executing the identified calculation graph over a verified software `ext80` substrate brings bit-identical `Exact` results to the browser and Linux/ARM while keeping hardware x87 on x86_64 as a fast native path and verification oracle.
