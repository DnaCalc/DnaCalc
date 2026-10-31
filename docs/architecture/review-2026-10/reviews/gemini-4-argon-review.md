# Target Architecture Review — Round 1 (Gemini 4 Argon)

**Reviewer:** Gemini 4 Argon (author of `drafts/gemini-4-argon.md`)  
**Baseline verified:** `DnaCalc-review` at `5a9348b`; `OxVba` at `c7ce1349` (read-only).

---

## 1. Review of the Other Three Drafts

### 1.1 Opus 5.5 (`drafts/opus-5-5.md`)

- **Strongest ideas to adopt**
  1. **Facet-tagged dependency footprints (`opus-5-5.md:50-51, 78`)**: Tagging edges with facets (`Value`, `Format`, `Existence`, `Shape`, `Visibility`, `NameBinding`, `SheetOrder`, `CallableGeneration`) so format edits dirty only `Format` readers (`CELL("format")`, precision-as-displayed) while single-concern decorators (`WhatIfReader` in `crates/calc-engine/src/data_table.rs:308-388`) wrap only `ValueReader` and cannot drop `WorkbookFacts`.
  2. **Axis-key order-statistic trees with tight split bounds (`opus-5-5.md:63-65, 234-240`)**: Keying `256x16` pages and `FormulaBlock` extents by `AxisKey(u64)` via per-axis order-statistic trees, and splitting a block with $r$ relative references of offset $d$ into at most $1 + 2r$ pieces across a row/col insertion at $p$.
  3. **Upper-left quadrant filter for `may_spill` (`opus-5-5.md:112-115`)**: Because Excel dynamic arrays spill only down and right, an empty cell $(r, c)$ can only be covered by dirty `may_spill` anchors at $(r' \le r, c' \le c)$ on that sheet.
  4. **Link-time split between `dna-calc` (UI) and `dna-calc-worker` (`opus-5-5.md:267-269, 346-347`)**: Today `apps/dna-calc/src/bin/dna-calc-web.rs:5-15` compiles UI and worker into one Wasm module, pulling `calc-host` and `calc-engine` into the page bundle. Separate crates make the UI ceiling a link-time invariant.
  5. **Splitting `ObjectModelBackend` into UDF-time (`calc-vba`) vs. macro-time (`calc-host-vba`) (`opus-5-5.md:295-299, 341-342`)**: Lets OneCalc run VBA UDFs with `Range` reads inside its ceiling while macro mutations route through `calc-host` commands.

- **Wrong or unverified claims**
  1. **Broken `calc-doc` save dependency (`opus-5-5.md:331, 334, 336`)**: Opus puts the prefix table in `calc-fml` (`:334`), deletes `calc-doc`'s table (`:278`), and drops `calc-doc -> calc-fml` so `calc-doc` depends only on `calc-doc-model` (`:336`, which depends only on `calc-value` and `calc-fml-syntax`, `:335`). In the code, `crates/calc-doc/src/save/file_form.rs:39-42, 63-74` requires both syntax trees and `FunctionFileForm { name, prefix, first_count, bases, positions }` (`save/file_form_table.rs:1-3571`) to write `_xlfn.`/`_xlpm.`/`_xleta.` prefixes and decide whether a saved formula gets dynamic-array metadata (`cm="1"`). Furthermore, `crates/calc-func/src/file_prefix.rs:37` (`FILE_PREFIXES`) only has `(&str, FilePrefix)` and lacks `first_count`/`bases`/`positions`. Under Opus's crate graph, `calc-doc` cannot save formulas.
  2. **False claim on Excel UDF reload dirtying (`opus-5-5.md:283-285`)**: Opus claims: *"Reload bumps the callable generation; plans rebind lazily; values are not dirtied (Excel keeps old UDF results until a recalc reaches them)"*. Our own Excel oracle in `crates/calc-vba/tests/fixtures/excel-recalculation/observations.json` and `docs/plan/VBA_UDFS.md:54-62` proved the opposite: editing a VBA module in Excel under Automatic mode immediately recalculates dependent UDF cells without `F9`.
  3. **Deferring `architecture.json` checks out of `check` (`opus-5-5.md:363-364`)**: While `tools/xtask/src/check.rs:18-20` currently leaves `architecture.json` to `sweep`, checking `Cargo.toml` dependency edges in a thin `xtask` takes <50 ms. Leaving layering checks only to multi-hour `sweep` runs lets worker lanes stack commits on forbidden edges.

- **Seam pressures missed or mishandled**
  1. **Block-grain nodes without affine sub-block invalidation (`opus-5-5.md:76-82, 130-131`)**: Opus makes `FormulaBlock` the graph node and scheduling grain ("blocks are the grain, never cells", `:131`) with static edges `block -> AxisRect` (`:78`) and offset edges only for *self-referential* recurrences (`:80-82`). If `B1:B1000000` contains `=A1*2` and the user edits `A500`, `Block(B) -> A1:A1000000` dirties all 1,000,000 rows of `B` instead of row 500.
  2. **Dynamic `may_spill` blindness (`opus-5-5.md:58, 112-115`)**: Opus derives `may_spill` statically from catalogue metadata (`:58`). Under `FormulaForm::Dynamic` (`crates/calc-fml/src/eval/mod.rs:107-113`), formulas with no dynamic-array function—e.g. `=INDIRECT(B1)`, `=OFFSET(A1,0,0,H1,W1)`, `=IF(C1,A1:A10,1)`, or any Variant/array UDF—can spill at runtime.

- **Conflicts with owner principles (`BRIEFING.md` §4)**
  1. **`BRIEFING.md` §4.4**: Opus defines `NumericTargetId = Excel build x CPU family x compatibility version` (`opus-5-5.md:196`). `BRIEFING.md` §4.4 states that *"Excel versions are assumed stable for function results; every reading records build, platform, CPU."* Putting Excel build into semantic `NumericTargetId` conflates capture provenance with calculation semantics.

---

### 1.2 Fable 5.1 (`drafts/fable-5-1.md`)

- **Strongest ideas to adopt**
  1. **`Affine { block, drow, dcol }` edges + per-block dirty bitset (`fable-5-1.md:63-66, 204-205`)**: Representing block nodes as `Block(block, offsets bitset)` with explicit `Affine` edges in `ReverseIndex` (`crates/calc-engine/src/graph.rs:341-356`) gives one edge for a 1M-row fill while dirtying only offset $k$ when input $A_k$ changes.
  2. **Explicit distinction between `BlockId` (formula storage) and `NotebookBlockId` (layout manifest) (`fable-5-1.md:207-208`)**: Prevents conflating split-on-edit R1C1 formula blocks with durable user-authored Notebook/Canvas layout blocks (`crates/calc-doc-model/src/manifest.rs`).
  3. **Per-function numeric `status` rows in catalogue & corpus output (`fable-5-1.md:168-171, 268-270`)**: Tracking `Identified | Candidate | Fitted(reverting) | Unverified` in the catalogue and reporting it in `cargo xtask corpus` prevents fitted kernels from masquerading as done.

- **Wrong or unverified claims**
  1. **Misstating `calc-doc`'s dependency on `calc-func` (`fable-5-1.md:314`)**: Fable claims `calc-doc` drops its "direct calc-func dependency (lexer via calc-fml)". In `crates/calc-doc/Cargo.toml:12-36`, `calc-func` is only in `[dev-dependencies]` (`:35`); `calc-doc` pulls the 77k-line `calc-func` crate *transitively through `calc-fml`* (`Cargo.toml:15`, `risk-layering.md` R9). Because Fable keeps `calc-doc -> calc-fml` (`fable-5-1.md:294`), `calc-doc` still compiles all of `calc-func`.
  2. **Same `save/file_form_table.rs` omission (`fable-5-1.md:278-279, 314`)**: Deleting `save/file_form_table.rs` without moving its `first_count`/`bases`/`positions` dynamic-array tables (`crates/calc-doc/src/save/file_form.rs:63-74`) into shared metadata breaks `file_form` on save.
  3. **Ignoring existing portable `soft87` in `calc-func` (`fable-5-1.md:164-165, 178, 320`)**: Fable claims `Exact` is native `x86_64`-only (`calc-exact-x86`) and sets Browser `Exact` to `no` (`:178`). In `crates/calc-func/src/ported/excel_numeric/mod.rs:9-15, 74-99` and `Cargo.toml:15`, `soft87` (`Ext80`, `CW_PC64_RN`) already exists and is bit-exact on `wasm32` for all algebraic x87 operations (`fadd`, `fmul`, `fdiv`, `fsqrt`, `fprem`, `store64`), differing only where un-modeled hardware transcendental microcode (`f2xm1`, `fyl2x`, `fpatan`, `fsin`, `fcos`, `fptan`) rounds a wider internal significand.

- **Seam pressures missed or mishandled**
  1. **Up-front full dirty-closure sort in `begin_recalc` (`fable-5-1.md:84-87`)**: Fable keeps `begin_recalc` computing and ordering the full dirty closure up front (`crates/calc-engine/src/recalc.rs:480-620`) before labelling the demanded backward cone. At $10^7$–$10^9$ dirty nodes, up-front closure sorting costs $O(\text{closure})$ before the first visible cell can evaluate (`foundation-ideas.md` R6).
  2. **Per-node publication without spill barriers (`fable-5-1.md:88-93`)**: Fable claims topological evaluation guarantees a node's prerequisites are settled before it publishes. When visible `Z1` reads empty cell `A500`, and off-screen `A1` holds un-evaluated `=SEQUENCE(1000)`, there is no static edge `Z1 -> A1`. Topological order on static edges lets `Z1` evaluate against blank `A500` and publish a wrong `Settled(tick)` value before `A1` spills.
  3. **No `AxisMap` (`fable-5-1.md:44-50, 68`)**: Keeping absolute coordinates in `CellPage` and `FormulaBlock` leaves row insertion $O(\text{pages + blocks below insert})$ instead of $O(\log N)$ (`foundation-ideas.md` R10).

- **Conflicts with owner principles (`BRIEFING.md` §4)**
  1. **`BRIEFING.md` §2 (Webs-class non-grid/programmatic scale)**: Fable locks sheets to `1,048,576 x 16,384` with no non-grid participant or unbounded table key (`fable-5-1.md:68-69`).

---

### 1.3 GPT-Astra 6 (`drafts/gpt-astra-6.md`)

- **Strongest ideas to adopt**
  1. **Separating base `CellPresentationFacts` from `ConditionalFormatOverlay` (`gpt-astra-6.md:39, 153`)**: Prevents a circular dependency where a CF rule or `CELL()` query asks for resolved presentation that recursively triggers CF evaluation.
  2. **`calc-function-meta` + `calc-syntax` + `calc-object-model` crates (`gpt-astra-6.md:195-204`)**: Putting data-only `FunctionId`, signatures, and file-form rules in `calc-function-meta` (depending only on `calc-value`) and syntax/templates in `calc-syntax` cleanly breaks `calc-doc -> calc-fml -> calc-func`. Putting DNA's COM object model descriptors and `ObjectServices` traits in `calc-object-model` decouples it from both `calc-host` and `calc-vba`.
  3. **Negative-lookup watches in `PlanKey` (`gpt-astra-6.md:51, 125, 214`)**: Recording unresolved symbol watches so adding a new `DefinedName` or VBA UDF rebinds only formulas that failed that lookup (`#NAME?`).
  4. **Preserving 80-bit `Ext80` state across kernel yield checkpoints (`gpt-astra-6.md:93, 245`)**: Yielding mid-loop in an `Exact` kernel must spill full 80-bit registers, never a `binary64` store that introduces extra rounding.

- **Wrong or unverified claims**
  1. **Flat contradiction of the owner's `asupersync` ruling (`gpt-astra-6.md:259`)**: GPT-Astra 6 declares `asupersync` *"Unusable pending counsel under the briefing's ruling."* `BRIEFING.md` §4.7 (and `research-libraries.md:1`) explicitly ruled that the licence rider is acceptable with a third-party notice and instructed us to evaluate `asupersync` on technical merit behind a ring-fenced boundary.
  2. **Did not inspect `OxVba` (`gpt-astra-6.md:8, 167-168`)**: Admits `OxVba` was not accessed and treats verified backend defects in `oxvba-vm3/src/lib.rs:2175-2203` and `oxvba-jit/src/lowering.rs:497-540` as unverified second-hand reports.
  3. **Overlooking `crates/calc-engine/src/random.rs:19-37` (`gpt-astra-6.md:94`)**: Warns *"do not invent per-cell random streams"*, missing that `RandomStream::new(seed, tick, at)` in `crates/calc-engine/src/random.rs:19-28` already keys `RAND()` per `(seed, tick, CellKey)` today so schedule order cannot mutate `RAND()` outputs (`foundation-ideas.md` R15).

- **Seam pressures missed or mishandled**
  1. **Staging whole-calculation publication (Step 4) before partial publication (Step 5) (`gpt-astra-6.md:235-236`)**: Retaining `finish`'s all-or-nothing publication in Step 4 means viewport priority yields zero improvement in time-to-first-visible-value and continues entrenching atomic-generation tests (`risk-engine.md` §2).
  2. **Abstract prose instead of concrete data layouts (`gpt-astra-6.md:50, 86`)**: Leaves "compressed certificates as shared read-pattern descriptors" and "sparse pages for mixed cells; dense typed lanes for homogeneous tiles" without concrete struct layouts or byte budgets.

- **Conflicts with owner principles (`BRIEFING.md` §4)**
  1. **`BRIEFING.md` §4.7**: Refusing to evaluate `asupersync` as an optional ring-fenced dependency after the owner's explicit ruling.

---

## 2. Ideas from `foundation-ideas.md` to Take Up or Reject

### Take Up
1. **R1 + R2 (Per-value/per-span `settled_tick` + atomic per-node `DerivedBundle` publication)**: Every published unit must atomically install `(values, observed_dynamic_edges, spill_reservation)` so no reader ever observes a new value with stale edges or stale spill ownership.
2. **R3 (Replayable `VisibilityState` stream + `MAX_VISIBILITY_DEFER` starvation bound)**: All four drafts missed `MAX_VISIBILITY_DEFER` and replayable visibility streams. Recording viewport changes as a deterministic input stream makes wave publication sequences reproducible in tests, and `MAX_VISIBILITY_DEFER` guarantees background closure progress under continuous scrolling.
3. **R4 + R6 (Observer-driven `Necessary` backward cone + incremental Pearce-Kelly topological heights)**: Replaces `begin_recalc`'s $O(\text{closure})$ up-front sort (`crates/calc-engine/src/recalc.rs:480-620`) with an $O(\delta \log \delta)$ incrementally maintained height-ordered min-heap over the `Necessary` cone.
4. **R5 (Early cutoff / backdating conditioned on O1 purity metadata)**: Bring early cutoff from Phase 5 into the local engine scheduler: if a recomputed node's value, type, and shape are bit-identical, skip dirtying downstream readers unless they carry `order_observable` (stateful VBA/volatile) flags.
5. **R10 + R11 (Roslyn-style `AxisMap` stable `AxisKey`s + `FormulaBlock` affine/interval edges)**: Key pages, style runs, and blocks by `AxisKey(u64)` so structural edits are $O(\log N + \text{straddling blocks})$.
6. **O1 + R9 + O2 (Declared per-function `fec_dependency_profile` + `FormatDepToken`s + bound call handles)**: Static per-`FunctionId` capability bitmasks enforce Principle 4.1, drive facet invalidation (R9), and supply purity flags for early cutoff (R5).
7. **O5 (Deterministic operation-counter log-log slope checks in per-commit `cargo xtask check`)**: All four drafts missed O5. Fitting log-log slopes of deterministic counters (`nodes_visited`, `plans_bound`, `pages_copied`, `tiles_projected` over geometric sizes $N \in \{10^2, 10^3, 10^4\}$) runs in milliseconds with zero timing noise and catches every $O(\text{workbook})$ regression (`risk-engine.md` §6) per commit.
8. **R17 + O11 (Zero-cost value-only vs. trace-rich eval mode + a small offline TLA+ model of the epoch/wave/spill/cancel protocol)**: A 150-line standalone TLA+ spec checking `INV-NoStaleCommit` and `INV-SnapshotConsistency` (kept off the inner loop) guards the subtler wave/cancel/spill interleavings without reviving February's ceremony.
9. **R18 recast (Non-grid ID-keyed participants in the one graph)**: Support non-cell `LogicalNodeKey` participants (named calculations, table columns, Rust-defined nodes) in the same graph without a second formula language.

### Reject
1. **R16 (DBSP-style inverse-update aggregates, `SUM = old - prev + next`)**: Violates IEEE-754 left-fold bit-exactness (`BRIEFING.md` §4.4). Use hierarchical algebraic `PageSummary` segment trees (`gemini-4-argon.md:251-253`) instead.
2. **O9 ("DNA Prove" as a separate shipped product/MCP server now)**: Keep the mechanism (`numlab` / `calc-func-lab` workspace + witness check), reject the extra product surface until core DNA Calc ships.
3. **Dual Red/Blue engines, Lean/OCaml checks, universal CRDT OpLogs, and full SAC runtimes**: Keep rejected per OCT §6.

---

## 3. Where I Change Position vs. Hold Position

### Where I Change Position (Updating `drafts/gemini-4-argon.md`)
1. **Adopt `AxisMap` (`AxisKey(u64)`) for pages, blocks, and style runs (from Opus §1.3 & `foundation-ideas.md` R10)**: My draft (`gemini-4-argon.md:211`) shifted `GridArea` coordinates of blocks below an inserted row, which is still $O(\text{blocks below } r)$. Keying `CellPage`s and `FormulaBlock` extents by order-statistic `AxisKey`s makes row/column insert $O(\log N + \text{straddling blocks})$.
2. **Add `Affine { block, drow, dcol }` edges and per-block dirty bitsets (from Fable §1.3 & GPT-Astra 6 §1)**: My draft (`gemini-4-argon.md:72`) had one interval edge `A1:A1000000` for a 1M-row `FormulaBlock`. Without `Affine` edges and a per-block dirty stripe/offset bitmap, editing `A500` dirties all 1M rows of `B` instead of one 64-row stripe, and intra-block recurrences (`B2=B1+1`) cannot be distinguished from parallel vector fills.
3. **Extract `calc-function-meta`, `calc-syntax`, and `calc-format` below `calc-fml` (from GPT-Astra 6 §3, Opus §3.1, Fable §3)**: Moving `format/` into `calc-fml` (`gemini-4-argon.md:267`) still left `calc-doc` pulling `calc-fml` and `calc-func`. Extracting `calc-function-meta` (unifying `crates/calc-func/src/file_prefix.rs:37` with `crates/calc-doc/src/save/file_form.rs:63-74` `FunctionFileForm`), `calc-syntax`, and `calc-format` lets `calc-doc` drop `calc-fml` and `calc-func` completely.
4. **Split `dna-calc` into `dna-calc-model`, `dna-calc-protocol`, `dna-calc-worker`, and `dna-calc` (from Opus §2 S4)**: Keeping one `dna-calc` crate (`gemini-4-argon.md:276`) leaves UI and worker in one Wasm binary (`src/bin/dna-calc-web.rs:5-15`).
5. **Separate base `CellPresentationFacts` from `ConditionalFormatOverlay` (from GPT-Astra 6 §1)**: Prevents `CELL()` and CF evaluation from cycling through `ResolvedPresentationQuery`.
6. **Correct `RandomStream` baseline (`crates/calc-engine/src/random.rs:19-37`)**: My draft (`gemini-4-argon.md:217, 221`) missed that `random.rs:19-28` already keys streams by `(seed, tick, CellKey)`. The needed change is keying `RandomStream` by stable `(SheetId, AxisKey, AxisKey)` and providing it through FEC to OneCalc and detached evaluations.
7. **Incorporate `foundation-ideas.md` R3 (`MAX_VISIBILITY_DEFER` + recorded visibility stream), R4/R6 (Pearce-Kelly incremental heap), R5 (local early cutoff), and O5 (counter-slope checks in `check`)**.

### Where I Hold Position Against the Others
1. **64×16 Striped-SoA Tiles + Algebraic `PageSummary` Segment Trees (`gemini-4-argon.md:73, 251-253`)**: None of the other drafts addresses how `SUM(A:A)` or 1M-row range reductions avoid $O(N)$ cell scans once DBSP inverse-updates (R16) are rejected. Maintaining fixed-tree `PageSummary` aggregates on 256×16 pages gives $O(\log P)$ deterministic reductions.
2. **Upper-Left `PotentialSpillWatch` + Dynamic-Ref Topology Barrier before Wave 0 settles (`gemini-4-argon.md:220` + Opus's upper-left rule)**: Contra Fable (`fable-5-1.md:88-90`), static topological order cannot prevent a visible reader of an empty cell from publishing a stale blank before an off-screen `FormulaForm::Dynamic` spill or `INDIRECT` resolves.
3. **In-place `CallableTrampolineTable` pointer swap on signature-preserving VBA reload (`gemini-4-argon.md:234`)**: Contra Opus/Fable/GPT-Astra 6, editing a VBA function body without changing its `SignatureId` should swap the target pointer in `CallableTrampolineTable[slot]` (while old frames hold an `Arc` code lease) and dirty dependent nodes in Automatic mode—requiring zero `CompiledPlan` rebinding.
4. **Portable `NumericMode::Exact` via hardware x87 + `soft87` with vendor transcendental models (`gemini-4-argon.md:184, 348-349`, with Opus §1.8 against Fable §1.8)**: `crates/calc-func/src/ported/excel_numeric/mod.rs:74-99` (`soft87`) already runs algebraic x87 chains bit-identically on `wasm32`; only un-modeled transcendental microcode ops should refuse on non-x86 targets until vendor models (`Intel`/`AMD`) are identified.
5. **Fast in-memory `architecture.json` check in per-commit `cargo xtask check` (against Opus §3.2)**.

---

## 4. Proposed Consensus and Open Disagreements

### 4.1 Concrete Consensus Points (All Four Drafts Should Converge Here)
1. **Explicit, Non-Defaulted Boundary Contexts (`O1`)**: `FormulaEvaluationContext` in `calc-fml` and `FunctionContext` in `calc-func` with all fields required; `CallerAnchor` from `FormulaInstance::anchor()`; `ReferenceReader` stripped to pure value/shape/spill reads; `SemanticEnvironment` (dirties nodes) split from per-tick `VolatileServices` (clock, RNG seed—never dirtying non-volatiles); per-function declared `fec_dependency_profile` and facet tokens (`R9`).
2. **Crate Splits at the Lower & Upper Boundaries**: Extract `calc-function-meta` (data-only `FunctionId`, volatility, subtotal flag, `fec_dependency_profile`, and `FunctionFileForm` from `save/file_form.rs:63-74`), `calc-syntax` (lexer, R1C1 templates, reference shifts), and `calc-format` (number formats + `NumberText` impl) below `calc-fml` so `calc-doc` drops `calc-fml`/`calc-func`. Split `dna-calc` into `dna-calc-model` (DOM-free), `dna-calc-protocol`, `dna-calc-worker`, and `dna-calc` (UI). Split thin `xtask` (zero product deps) from `calc-harness`.
3. **R1C1 `TemplateId` + `FormulaBlock`s over `AxisMap` (`R10, R11`)**: Replace per-cell A1 `FormulaText` expansion (`crates/calc-doc/src/load/mod.rs:268-271`) with R1C1 `TemplateId`s and `FormulaBlock`s over `AxisKey(u64)` order-statistic trees; graph edges carry `Affine { drow, dcol }` and `Rect` intervals with per-block dirty bitsets; undo applies inverse ops instead of `EngineState::rebuilt` (`crates/calc-host/src/command.rs:447-459`).
4. **Incremental Per-Node/Wave Publication + Priority Backward Cone (`R1–R6`)**: Retire `finish`'s whole-generation atomicity (`crates/calc-engine/src/recalc.rs:1001-1013`). Pull ready work from an incrementally maintained height-ordered heap over the demanded backward cone (`Necessary`), bounded by `MAX_VISIBILITY_DEFER`, with early cutoff (`R5`) for pure nodes and atomic super-nodes for iterative SCCs and What-If data tables.
5. **One Command & Projection Surface (`O3, O4`)**: Move all Excel command rules from `apps/dna-calc/src/server.rs:405-1815` into `calc-host`. Skins implement a narrow `SkinPlugin` trait over a DOM-free model with a shell-owned global keymap, `ViewStateStore<SkinId>`, semantic `Target`, binary columnar `ViewportFrame` tile deltas, and a semantic test/A11y inspection layer replacing per-cell DOM `data-*` selectors.
6. **Early-Bound CalcVBA & Glue (`O2, O8`)**: Allow only `calc-vba` to depend on a spreadsheet-free `oxvba-host-api` crate (`#[repr(C)]` vtables and descriptors). Move `Application`/`Range`/`WorksheetFunction` out of `oxvba-embed/src/spreadsheet.rs` into DNA Calc (`calc-object-model` + `calc-vba`). Lower `OxInst::ComCallEarly` to direct vtable slot calls in VM3 and Cranelift JIT, and replace `Instruction::UserCall { name }` with `CallableSlotId` + typed C-ABI shims.
7. **Ring-Fenced Scheduler & Research Isolation (`O5, O9`)**: Ring-fence `asupersync` as an optional native backend behind `calc-sched` (keeping our portable Layer-0 stepping executor for `wasm32` and deterministic replay). Isolate symbolic function search in `numlab` / `calc-func-lab` outside the product workspace. Add deterministic counter-slope checks (`O5`) to `cargo xtask check`.

### 4.2 Genuine Open Disagreements (Stated as Choices with Trade-Offs)

| # | Disagreement | Option A | Option B | Trade-Off |
|---|---|---|---|---|
| **D1** | **Block-grain vs. Stripe/Member-grain scheduling & storage** | **Whole-block node & `ResultColumn` (Opus)**: Each `FormulaBlock` is one graph node and one parallel task; results live in per-block `ResultColumn`s. | **Block storage with 64-row stripe tasks & page-tiled results (Gemini / Fable / Astra)**: `FormulaBlock` holds a dirty bitset over 64-row stripes; results stay in sheet `CellPage` striped tiles with `PageSummary`. | **A** minimizes graph metadata for uniform fills, but a 1-cell edit in `A500` recomputes an entire 1M-row block `B1:B1000000` and complicates cross-block range reads (`SUM(A1:D100)`). **B** keeps single-cell edits $O(1\text{ stripe})$ and makes range reads uniform across formulas and literals, at the cost of a bitset per block. *(Recommend **B**)* |
| **D2** | **Publication unit: Per-Node Bundle vs. Time-Sliced Wave** | **Per-node `DerivedBundle` commit (Fable / Opus / `R2`)**: Each node/span commits to `Results` immediately upon evaluation; `drain()` streams settled spans. | **4–8 ms Wave Stratum commit (Gemini)**: Nodes stage into a wave buffer and commit in 4–8 ms dependency-closed waves aligned with worker yields. | **A** is simplest and has zero staging overhead, but requires every mid-tick reader to check per-cell `settled_tick` and spill barriers before reading. **B** amortizes reverse-index/tile-delta updates to once per 4–8 ms frame, at the cost of a small wave buffer. *(Recommend **A** internally in engine, batched into 4–8 ms `ViewportFrame`s at the host boundary)* |
| **D3** | **Portable `NumericMode::Exact` on `wasm32`/ARM64** | **Hardware x86_64 only (`calc-exact-x86`, Fable)**: Browser and ARM64 refuse `NumericMode::Exact` altogether. | **Hardware x87 + portable `soft87` with vendor models (Opus / Gemini / Astra)**: Run `soft87` (`Ext80`) everywhere; refuse on non-x86 only for transcendental instructions lacking an identified `Intel`/`AMD` microcode model. | **A** avoids maintaining software transcendental microcode models, but makes `Exact` unusable in the browser even for pure algebraic x87 functions (`fadd`..`fprem`, `store64`) that `soft87` already computes bit-identically (`excel_numeric/mod.rs:10-12`). **B** delivers `Exact` in the browser for all algebraic and modeled functions while refusing un-modeled transcendentals honestly. *(Recommend **B**)* |
| **D4** | **Grid rendering surface: Recycled Keyed DOM vs. `Canvas2D` + A11y Overlay** | **Keyed/pooled DOM `<div>`s or renderer-agnostic (Opus / Fable / Astra)**: Keep DOM cells (keyed by tile/pool) or leave `Canvas2D` optional. | **`Canvas2D` primary + bounded Semantic A11y DOM Overlay (Gemini / `foundation-ideas.md` O4)**: Render grid cells on `Canvas2D` via a testable `RenderPlan` IR; mount DOM nodes only for the active selection/focus neighborhood. | **A** is a smaller diff from today's `grid/mod.rs`, but CSS layout/paint of thousands of DOM nodes during 60 fps diagonal scrolling and zoom remains a browser bottleneck. **B** guarantees sub-millisecond frame paint and deterministic `RenderPlan` JSON snapshots (`O4`), requiring a rewrite of `grid/mod.rs` paint code. *(Recommend **B**)* |
| **D5** | **Order-observable UDFs & `RAND()` under viewport priority** | **Force `ExcelOrder` only for impure UDFs/SCCs; keep coordinate-keyed `RAND()` (Opus / Gemini / Fable)**: `RAND()` uses `RandomStream(seed, tick, key)`; only stateful VBA UDFs and iterative SCCs force reference order. | **Reference serial order wherever Excel's RNG/effect order is unverified (Astra)**: Do not claim coordinate-keyed `RAND()` is Excel-compatible; restrict reordering wherever observable effects could differ. | **A** gives full visible-first priority even on sheets with `RAND()` while preserving deterministic replay (`R15`). **B** is stricter about sequential PRNG draw order, but disables visible-first priority on any sheet using `RAND()`. *(Recommend **A**)* |
