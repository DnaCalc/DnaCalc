# Planning prompt — DNA Calc re-implementation (for GPT-6 Pro)

> Draft v0.6, 2026-10-05. Owner decisions so far are in §2; §5 lists what is deliberately left to you.

---

## Your role

You are the lead architect producing the **implementation plan** for a re-implementation of DNA Calc, an Excel-compatible calculation stack written mostly in Rust. You have read access to the existing repositories, which sit side by side under one `DnaCalc` root folder (listed in §4). **Every path in this prompt is relative to that root**, e.g. `OxCalc/src/oxcalc-core/src/consumer.rs`. The owner has fixed the target decomposition; your job is to turn it into a plan that engineers (human and AI coding agents) can execute, and to decide the questions the owner has deliberately left open.

Read the code; do not trust the docs blindly. This program has a recorded history of documentation, status ledgers and "evidence" overstating what the code does (see *Lessons* below). Where a doc and the code disagree, the code wins, and say so.

## Why a re-implementation

About ten months of work produced strong pieces and a tangled whole. Rather than keep refactoring in place, the owner wants a re-implementation under new names with a cleaner decomposition. Some libraries carry over almost unchanged, some are restructured, and some are rewritten from scratch. The plan decides which, per library (see §4). The biggest single simplification: **the engine drops the invented "tree" reference language and the dual-profile model. There is only Excel.**

---

## 1. Target decomposition (fixed by the owner)

| New name | Was | Kind | Scope |
|---|---|---|---|
| **CalcFunc** | OxFunc | library | Excel worksheet functions and operators "pretty much as is". Function metadata; stateless implementations over structures passed in from CalcFml; testing plus the "smart fuzzer" Excel-differential campaigns that drive toward bit-exact Excel results. **No Lean or other formal methods at this stage.** Each function evaluation is single-threaded (or presents a single-threaded interface even if faster inside), but the library is stateless, so many threads may call it at once. |
| **CalcFml** | OxFml | library | The formula **and** number-format library. Internally stateless: all state is passed in and returned as structures. First version is as fast as possible single-threaded; multi-threaded later. Lifecycles as today: typing support (intellisense, completion, signature help, diagnostics), parsing, binding, evaluation. **Must cache and share structures across repeated "similar" formulas on a sheet** (R1C1-equivalent formulas, shared formulas, fill-down). Two callers: CalcEngine, and DNA OneCalc (single formula, no engine). |
| **CalcEngine** | OxCalc | library | The dependency-based calculation engine. **One unified model covering the full Excel scope, with no tree language and no second profile.** Full support for defined names (workbook and sheet scope), multiple sheets, the full 1,048,576 × 16,384 grid, and the other dependency-graph participants Excel has (see §3.3). Manipulation exposed as clearly as possible: a COM-like object model goes on top later; for now the client is CalcHost. Stateless in the same sense as CalcFml. **Single-threaded recalc for now**, as the reference implementation for a future optimised (incremental/parallel) version. Full Excel compatibility is the goal. |
| **CalcDoc** | OxDoc | library | Full representation of Excel `.xlsx` / `.xlsm` files: read, model, edit, write, with round-trip fidelity. |
| **CalcHost** | extracted from DnaTreeCalc | library | The application model and front-end wrapper. **Owns the authoritative, editable workbook state.** Defines the **skinning interface**; owns the workbook lifecycle: load and save via CalcDoc, calculation via CalcEngine, manipulation of workbooks. Presents the model to skins. Later it hosts VBA and XLL add-ins. |
| **DNA OneCalc** | DnaOneCalc (now folded into DnaTreeCalc's bench crates) | application | Front end over CalcFml + CalcFunc **only**. It must not reference CalcEngine. It is the test harness and proof of concept for the whole formula language and every function, without the dependency engine in the loop. |
| **DNA Calc** | DnaTreeCalc app | application | Lightweight skins plus an execution scaffold over CalcHost. Initial version ships at least these skins: (1) **Excel**, reproducing Excel's full UI (§3.5); (2) a **Jupyter-notebook-like** skin with the Excel engine underneath; (3) a **tree-based editor**; (4) a **free-form calculation canvas**. |

**Next phase, not in this plan's build scope but the architecture must accommodate it:** OxVba (VBA 7 compiler/runtime), OxForms (MSForms runtime) and OxIde (VBA IDE) fit on top to make a VBA-enabled version, and a version that hosts `.xll` add-ins. §3.6 lists the seams the first phase must leave room for.

---

## 2. Decisions already made

- **Re-implementation strategy is mixed, decided per library.** Some carry over almost verbatim (expected: CalcFunc), some are restructured (expected: CalcFml, CalcDoc), some are rewritten fresh from the old code's designs and tests (expected: CalcEngine, CalcHost, DNA Calc skins). Confirm or change these expectations per library, with reasons from the code.
- **Excel only.** The `treecalc-v1` profile, the tree reference language, its lane in OxCalc, and the host-reference syntax in OxFml are dropped. The tree *editor skin* survives as a view over an Excel workbook (see §3.5).
- **Process is lean:** tests, CI from the first milestone, the Excel differential oracle, beads (`br`) for task tracking. Drop W-numbered worksets, implementer/refuter ceremony, prompt/synthesis "runs", and formal-methods artifacts. Keep the honesty rules in *Lessons* below; they were learned the hard way.
- **State ownership: CalcHost owns the authoritative, editable workbook**, built on CalcDoc's model types. CalcDoc is the file format plus those model types: read, write, round-trip fidelity. CalcEngine holds only the state calculation needs, kept in step with CalcHost through **one stream of operations**. UI, macros, add-ins and scripts all mutate through that same stream. This reverses the current doctrine, in which OxCalc owns the single mutable model and OxDoc is a codec.
- **Phase-1 calculation scope is the full Excel set, including the hard cases.** The first version must actually *calculate*, not preserve and label typed-unsupported:
  - circular references with iterative calculation;
  - data tables (what-if `TABLE()`);
  - conditional-formatting and data-validation formulas, evaluated and taking part in dependencies;
  - chart series (rendered in the Excel skin) and pivot tables (refreshed).

  They are in scope for the first version, but see the next point: charts and pivots are separate areas whose detailed planning comes later.
- **Two levels of planning detail.**
  - **Core: plan to bead level now.**
    - CalcFunc, CalcFml, CalcEngine (including iterative calculation, data tables and CF/DV), CalcDoc, the CalcHost core, DNA OneCalc
    - the Excel skin's core (grid, formula bar, core editing, command model)
    - **the notebook, tree and canvas skins, with their persistence design: top priority.** They are what distinguishes DNA Calc from Excel. They are never deferrable. Sequence them as early as CalcHost and the engine allow, not after the Excel skin, and make sure the CalcHost skin interface is shaped by all four skins from the start, not by the Excel grid alone.
  - **Major separable areas: identify them now, plan them in detail after the initial engine implementation.** The owner names **charting** and **printing/page layout**. Propose others that fit the same test: large; separable behind a clear interface; and better planned once the engine and CalcHost exist. Likely candidates: pivot tables and the long tail of Excel UI dialogs. The non-Excel skins are explicitly excluded (see above). Say which you include and why.

  For each separable area, deliver:
  - **Scope:** what it covers and what it leaves out.
  - **What it depends on:** what it needs from the core.
  - **Attachment points:** what the core must provide so the area slots in without rework. Examples: dependency-graph participation for chart series and pivot sources, page-setup data in CalcDoc and CalcHost, the command-model slots in the ribbon.
  - **Risks** that could force a core change if discovered late.
  - **Where its planning bead sits:** one bead for the area's detailed plan, placed in the graph after the milestone it depends on.

  The attachment points themselves are core work and get full beads now.
- **Platforms: Windows, Linux, and web (wasm in the browser)**, all from the first version. All libraries and both applications build and pass their tests on all three, and CI runs on all three. Windows-only pieces (COM, `.xll` hosting, the live-Excel oracle) belong to phase 2 or to tooling, behind platform-specific crates. **Numeric consequence to resolve:** OxFunc's last-bit Excel matching (`OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs`) executes real x87 instructions via `asm!` on x86_64. That is unavailable in wasm (and on ARM).
- **Two numeric modes: Exact and Fast.**
  - **Exact** reproduces Excel's results bit for bit, including its x87 extended-precision intermediate behaviour. It is what the bit-exact campaign and the differential oracle measure against.
  - **Fast** uses plain IEEE double arithmetic and ordinary math routines. It is close to Excel but not bit-exact, and it is quicker.

  The plan must design the following:
  - Where the mode lives: per workbook, per session, or per calculation. Whether it is saved in the file, given that Excel has no such setting, so it is CalcHost metadata.
  - How CalcFunc expresses it: which functions and operators actually differ between modes, declared in function metadata once rather than as scattered branches.
  - How CalcFml and CalcEngine carry it through evaluation, and what a mode switch invalidates (a full recalc).
  - Whether Exact mode is available on every platform via a software 80-bit implementation (cross-checked against the native x87 path, with the performance cost measured on wasm), or only where x87 exists.
  - Whether Fast mode must be deterministic across Windows, Linux and wasm (one portable math library, no platform libm), and what that costs.
  - How the oracle scores each mode. Exact is scored bit for bit against Excel. Fast is scored against Exact with stated per-function tolerances, so Fast-mode drift is measured rather than discovered.
  - The default mode for each app (DNA Calc, DNA OneCalc, the web build).
- **Correctness first; no performance targets.** Faster is better, but this phase aims at a correct engine. The next phase will focus on performance, with the correctness of this phase as its anchor. So:
  - do not set performance targets or performance milestones;
  - do choose data structures and asymptotics that later optimisation can build on rather than replace: interned IDs, no whole-sheet clones or re-scans per edit, delta publication, shared formula plans;
  - keep the single-threaded reference engine as the permanent correctness oracle for the optimised engine to come;
  - provide a benchmark harness with named workloads, so the next phase starts from measured numbers.
- **No timeline.** Plan in terms of work sequencing and dependencies only. Give no dates, durations or calendar estimates.
- **The Excel skin covers Excel's full UI**, not just the grid. An Excel user should find the commands, dialogs, keyboard shortcuts and behaviours where they expect them. See §3.5 for the scope.
- **Clean-room rule stays:** public documentation and black-box observation of Excel only; no disassembly, no leaked source.
- **Language, repo layout, and UI stack are yours to recommend** (see §5). The current product code is Rust throughout (including OxVba); the UI is Leptos (wasm) + Tauri v2 + a web worker + Canvas2D grid; repo tooling has been .NET/PowerShell.

---

## 3. What the plan must design

### 3.1 State, ownership and "stateless"

The owner wants CalcFml, CalcFunc and CalcEngine to be *stateless*: no hidden state; all state passed in and returned as explicit structures. For a 17-billion-cell address space with tens of thousands of formulas, define exactly what that means in practice: immutable/persistent structures with structural sharing, caller-owned `&mut State` handles, `apply(state, ops) -> (state', facts)`, or another shape. Specify the cost model (an edit must not clone or re-scan the sheet; see the baseline in *Lessons*).

Ownership is decided (§2): CalcHost owns the authoritative workbook, built on CalcDoc's types; CalcEngine holds calculation state kept in step through one operation stream. Design the details:
- exactly what each of CalcDoc's model, CalcHost and CalcEngine holds;
- how one operation is applied to CalcHost's model and CalcEngine's state atomically;
- how a failed recalc or a refused save leaves both consistent;
- how undo/redo and transactions work across the two;
- how CalcEngine's results (values, spill shapes, recalc facts) flow back for display and save without CalcHost re-reading whole sheets;
- how opening a 100 MB workbook avoids holding two full copies of every cell.

### 3.2 CalcFml

- Public compiled-plan API keyed by an R1C1-normalised template identity and instantiated per caller cell, so 100k fill-down formulas share one parse/bind/plan. Today the plan is recompiled on every evaluation (`OxFml/crates/oxfml_core/src/eval/mod.rs` around `EvaluationContext::new`), and OxCalc works around it with its own second evaluator (`OxCalc/src/oxcalc-core/src/grid/machine/r1c1_plan.rs`). That evaluator duplicates semantics and must not exist in the new design.
- **Move Excel A1/R1C1 grid reference binding into CalcFml.** Today it lives in OxCalc (`OxCalc/src/oxcalc-core/src/grid/reference_engine.rs`), which is why DNA OneCalc cannot do references without the engine.
- Replace `BTreeMap<String, CalcValue>` cell inputs with a typed reference/provider interface that CalcEngine implements efficiently and DNA OneCalc implements trivially.
- Number formatting: today it is split between OxFunc (`OxFunc/crates/oxfunc_core/src/locale_format.rs`: profiles, traits) and OxFml (`OxFml/crates/oxfml_core/src/format/`: engine). Decide the final home and the shared number-to-text policy, which is still unresolved (`BLK-W111-NUMERIC-TEXT-SEAM`).
- Lifecycles: editing/intellisense (lossless incremental parse), parse, bind, plan, evaluate. Lazy forms (IF, IFS, CHOOSE, SWITCH, IFERROR, IFNA, LET, LAMBDA) are keyed on CalcFunc's laziness metadata.
- Threading: single-threaded first, but no `Rc`/`RefCell` across API boundaries, so a later parallel engine can evaluate independent cells on several threads.

### 3.3 CalcEngine

Design the reference single-threaded engine for full Excel semantics:

- One workbook-wide dependency graph with interned integer IDs for cells, sheets and names. No per-sheet federation with cross-sheet rounds, and no string-keyed addresses.
- Participants: cells, ranges, whole rows and columns, defined names (static and dynamic), sheet spans (3D references), structured references and tables, spilled dynamic arrays (spill ranges, blockers, `#SPILL!`), volatile functions, INDIRECT/OFFSET-style dynamic dependencies, data tables (what-if), conditional formatting and data validation formulas, chart series, pivot tables and GETPIVOTDATA, external workbook links. State for each how it enters the graph and how it is recalculated. Iterative calculation, data tables, CF/DV, charts and pivots are in first-version scope (§2). Anything else the first version cannot calculate must be typed-unsupported and counted, never served silently from the file's cached value.
- Calculation semantics: Excel's recalc order as observed, circular references and **iterative calculation** (SCC-based), manual and automatic calculation modes (F9, Shift+F9, Ctrl+Alt+F9), the 1900 and 1904 date systems, `calcPr` settings, the precision-as-displayed flag, and full-recalc-on-open behaviour.
- One reified **op vocabulary**: cell values and formulas, range fills, row/column insert and delete, sheet add/rename/move/delete, names, tables, settings, recalc. Op batch in; result plus typed **recalc facts** out: what was evaluated, what was escalated and why, value deltas.
- Publication is a **delta**, not an O(sheet) readout.
- Every published value carries **provenance** (`Calculated{tick}` vs `FileCached`/`Stale`/`NotModeled`/…), so the conformance instrument can never score an un-evaluated cell as a pass.
- A reference engine plus, later, an optimised engine, with the reference kept as a differential twin that fails loudly.

### 3.4 CalcDoc

- Full `.xlsx`/`.xlsm` model: cells, shared strings, styles, number formats, defined names, tables, CF, DV, comments, drawings and charts, pivots, `vbaProject.bin`, customXml, richData/`cm=`/`vm=`, calcChain, external links.
- Round-trip fidelity: untouched parts copied byte for byte, a fidelity ledger, typed refusals (OxDoc has 62 kinds).
- **Authoring as well as editing**: add cells, add/change/remove formulas, sheet lifecycle, Save As / fresh export that Excel opens without repair. OxDoc cannot do these yet (beads `oxdoc-1vg.16`–`.19`), and it is the main reason only value edits round-trip today.
- No engine-specific names in the model (OxDoc's model today carries `OxCalcIngestSink`, `OxCalcCell`).

### 3.5 CalcHost and the DNA Calc skins

- The skin interface: intents in, receipts and deltas out, a projection the skin renders, a versioned schema. The existing `DnaTreeCalc/src/dnacalc-skin-ir` is the precedent: about 80 intent verbs in one enum, a tree-first projection. Redesign it, organised by capability, with a profile-neutral projection.
- Undo/redo and transactions belong in CalcHost. The workbook path has none today.
- The four skins: say what each needs from CalcHost, and what the first version of each contains. **The notebook, tree and canvas skins are top priority and planned to bead level now** (§2). Design each one fully:
  - its cell model and editing interactions;
  - how it shows formulas, values, errors, spills and LAMBDAs;
  - how it navigates dependencies;
  - how it coexists with the Excel skin over the same workbook.
- **The Excel skin is the full Excel UI.** Build an inventory of Excel's desktop UI surface and plan it to completion. The inventory covers at least:
  - **Chrome:** ribbon tabs and groups, key tips (Alt sequences), the File/backstage view, the quick access toolbar, the name box and formula bar (multi-line, with function tooltips and intellisense from CalcFml), sheet tabs, the status bar with selection aggregates, and zoom.
  - **Grid behaviour:** selection models (ranges, multi-area, entire rows and columns, Ctrl/Shift extension), the fill handle and AutoFill series, drag-move and copy, edit mode vs enter mode vs point mode, F2/F4 reference cycling, and spill-range outlines.
  - **Rows, columns and panes:** freeze and split panes, hide and unhide, grouping and outlines, row heights and column widths.
  - **Clipboard:** reference adjustment on paste, Paste Special, and clipboard interchange with real Excel and other apps (HTML, text, and Excel's native formats where publicly documented).
  - **Dialogs:**
    - Format Cells (all tabs), plus formatting by cell styles and themes
    - Name Manager
    - Conditional Formatting rules manager
    - Data Validation
    - Find & Replace, Go To and Go To Special
    - Sort and filter (AutoFilter, advanced filter)
    - Table tools
    - Insert/Delete cells
    - Calculation options (Exact/Fast mode lives here as a DNA addition)
    - Evaluate Formula, Trace Precedents/Dependents, Watch Window and Error Checking
  - **Objects:** chart insertion, editing and formatting; the PivotTable field list and pivot layout; comments and notes; hyperlinks.
  - **Review and output:** sheet and workbook protection, page layout and print preview/print, undo/redo with Excel-equivalent granularity, and the keyboard shortcut set.

  Phase 2 adds the Developer tab, the VBA editor (OxIde), forms (OxForms) and the add-ins manager. Reserve their places in the ribbon and command model now.
- **Requirements for the Excel skin:**
  - **One command model.** Every UI command (ribbon, menu, shortcut, dialog OK) is a named command that turns into CalcHost operations. It must be invocable without the UI, so it can be tested headlessly and later driven by VBA. Plan a command-parity ledger against the inventory, with each entry marked as done, partial or not started, and verified by a test.
  - **Clean-room presentation.** Reproduce Excel's layout and behaviour, not Microsoft's icons, artwork or trademarks. Say how the icon set and visual design are sourced.
  - **Scale and platform.** The UI must stay responsive at 1M rows on all three platforms, including the browser.
  - **Sequencing.** Order it sensibly: grid, formula bar, core editing and the command model early. The full inventory is still first-phase scope, but parts of it belong to the separable areas in §2 (charting, printing, and any others you identify). For those parts, the inventory names the commands and reserves their command-model slots; their detailed beads come with that area's later planning.
- **Persisting non-Excel skins (open; design it from first principles).** The notebook, tree and canvas skins must save to `.xlsx` and reopen meaningfully in real Excel.
  - **Compare at least three designs on their merits:** (a) every skin cell is a defined name (static value or formula); (b) skin cells are a structured cell list in an ordinary or hidden grid sheet, with names as optional aliases; (c) a hybrid.
  - **For each, say where these live:** block order, prose and markdown, tree structure, canvas positions, and per-skin view state.
  - **Test each against these cases:**
    - the same workbook opened in two different skins;
    - a user editing it in real Excel (adding rows, renaming names, deleting the hidden sheet) and reopening it in DNA Calc;
    - names whose formulas reference other skin cells;
    - LAMBDA-valued cells;
    - spill results;
    - 10,000-cell notebooks.
  - Earlier work chose a hybrid on 2026-07-05: defined names over a hidden append-only `_names` sheet, plus an embedded CustomXML manifest, no sidecar file (`DnaTreeCalc/docs/ux/FRONTEND_UI_DESIGN_AND_ROUTEMAP.md` F.2/F.4). Treat it as one candidate among others, not a default.
- DNA OneCalc: must use CalcFml + CalcFunc only, with the dependency rule enforced mechanically (the existing `DnaTreeCalc/src/dnacalc-arch-gates` crate checks crate tiers via `cargo tree`; reuse the idea).

### 3.6 Seams for the next phase (VBA, forms, XLL)

Do not build these now, but the phase-1 architecture must not need rework to admit them:

- A source-neutral **UDF registry** with register, unregister and invalidate; volatility, thread-safety and async declarations; caller context. Contract already exists: `OxFunc/docs/function-lane/OXFUNC_UDF_REGISTRATION_AND_REGISTRY_MUTATION_CONTRACT.md` (W093).
- A COM-like **object model** (Application/Workbook/Worksheet/Range) as a late-bound dispatch projection over CalcHost ops. OxVba consumes it via host callbacks (`OxVba/crates/oxvba-host/src/engine.rs`), and the object model is explicitly host-owned.
- An **event bus** (Workbook_Open, SheetChange, Calculate, …) and an exclusive, serialised **macro transaction** mode.
- **External/RTD updates** as ops, distinct from volatile invalidation.
- `.xll` hosting: XLOPER12 marshalling and `Excel12v` callbacks into CalcEngine/CalcHost. OxFunc's `xlcall` symbols and codes already exist in the metadata; `OxFunc/tools/xll-addin/` is the reverse direction (OxFunc inside Excel), useful only as reference.
- Range picking for forms (RefEdit), and a panel slot for an embedded IDE (OxIde).

---

## 4. Existing code: where to look, what to carry

Read these first. Sizes are approximate.

**OxFunc → CalcFunc** (`OxFunc/`, ~140k lines)
- `OxFunc/crates/oxfunc_value_types/src/lib.rs`: `CalcValue`/`CoreValue`, already `Send + Sync` (`Arc`).
- `OxFunc/crates/oxfunc_core/src/function.rs` (`FunctionMeta`), `OxFunc/crates/oxfunc_core/src/function_call.rs`, `OxFunc/crates/oxfunc_core/src/registry.rs`, `OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs`.
- Call contract: `FunctionCallTarget::invoke(args, &mut FunctionExecutionContextBundle)` with reference, now, random, locale, host-info, callable, RTD and external providers. It is effectively stateless already (one `OnceLock` registry).
- Parity: `OxFunc/docs/function-lane/EXCEL_PARITY_LEDGER.csv` has 527 rows: 219 consistent, 260 divergent, 48 unverified.
- Oracle tooling: `OxFunc/smart-fuzzer/` (PowerShell COM runners, Rust racer), `OxFunc/tools/xll-addin/`, `OxFunc/crates/oxfunc_core/src/excel_numeric/` (x87 emulation).
- Regret to fix: `OxFunc/docs/decisions/ODR-FN-004-declarative-function-spec-and-generic-executor.md`: three hand-maintained dispatch paths drifted. Adopt "declare once, derive every path".
- **Provenance check required:** `OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs` cites an NT source-tree file. Confirm it complies with the clean-room rule before porting.
- **Data at risk:** the oracle cache and runs (`OxFunc/smart-fuzzer/cache/` 1.4 GB, `OxFunc/smart-fuzzer/runs/` 2.0 GB) are not in git. The plan must say how Excel ground truth is stored and versioned.

**OxFml → CalcFml** (`OxFml/`, ~34k lines plus ~35k lines of tests)
- Pipeline:
  - `OxFml/crates/oxfml_core/src/syntax/` (lossless incremental parser)
  - `OxFml/crates/oxfml_core/src/binding/`, including `OxFml/crates/oxfml_core/src/binding/profile.rs` (W077 template identity)
  - `OxFml/crates/oxfml_core/src/semantics/`
  - `OxFml/crates/oxfml_core/src/eval/mod.rs`
  - `OxFml/crates/oxfml_core/src/format/`
  - `OxFml/crates/oxfml_core/src/language_service/`
- Overlapping facades to collapse:
  - `SingleFormulaHost` in `OxFml/crates/oxfml_core/src/host/`
  - `RuntimeEnvironment`
  - `OxFml/crates/oxfml_core/src/consumer/`
  - `OxFml/crates/oxfml_core/src/session/`
  - `OxFml/crates/oxfml_core/src/seam/`
  - `OxFml/crates/oxfml_core/src/publication/`
  - `OxFml/crates/oxfml_core/src/scheduler/`

  See `OxFml/docs/spec/OXFML_CONSUMER_INTERFACE_REARCHITECTURE_PLAN.md`.
- Performance history: `calc-nhr4` ("one front end per formula") was reverted after a measured 2.2× regression; its prerequisite (the Arc migration) has since landed. W075 pooling lessons.

**OxCalc → CalcEngine** (`OxCalc/`, ~200k lines, of which ~85k are tests)
- Designs worth carrying:
  - the dependency-kind taxonomy and two-layer structural/overlay index with compressed ranges and a 1,024-cell block index (`OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs`)
  - the spill ledger (`OxCalc/src/oxcalc-core/src/grid/machine/spill_ledger.rs`)
  - the recalc tick and per-node splitmix64 RNG (`OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs`)
  - the reference-twin differential and the typed escalation outcomes (`GridSeededLaneOutcome`), both in `OxCalc/src/oxcalc-core/src/grid/machine/differential.rs`
  - packed cell storage at 8 B/cell dense (`OxCalc/src/oxcalc-core/src/grid/machine/optimized_storage.rs`)
  - content-addressed input snapshots (`GridInputSnapshotId` in `OxCalc/src/oxcalc-core/src/grid/authored.rs`)
  - the workload catalogue in `OxCalc/src/oxcalc-core/src/grid/scale/`
- Design reading:
  - `OxCalc/docs/design/W062_D1_STRUCTURAL_MODEL.md`
  - `OxCalc/docs/design/W062_D2_REFERENCE_ARCHITECTURE.md`
  - `OxCalc/docs/design/W062_D3_WORKBOOK_CALCULATION.md`
  - `OxCalc/docs/design/W062_D4_DOCUMENT_SURFACE_AND_INGESTION.md`
  - `OxCalc/docs/worksets/W062_IDEAL_ENGINE_MODEL_REWORK.md`
- Anti-patterns to avoid:
  - `OxCalc/src/oxcalc-core/src/consumer.rs` (35k lines, one `&mut self` god object)
  - per-sheet engines federated by cross-sheet rounds
  - string-keyed addresses (`ExcelGridCellAddress` in `OxCalc/src/oxcalc-core/src/grid/coords.rs` carries two heap `String`s)
  - an O(P²) ready-list
  - mark-all that drops topological order above 10,000 formulas
  - a 100,000-cell materialisation cap
  - the R1C1 side evaluator
  - rebuild-from-input on Delete and on F9
- To drop: the tree lane and its reference machinery (roughly 75–85k lines), `OxCalc/src/oxcalc-tracecalc/`, and `OxCalc/formal/` (Lean/TLA+, tree-only).

**OxDoc → CalcDoc** (`OxDoc/`)
- `OxDoc/crates/oxdoc-model/` (types, serde only), `OxDoc/crates/oxdoc-xlsx/` (1.48 MB in a single `OxDoc/crates/oxdoc-xlsx/src/lib.rs`), `OxDoc/crates/oxdoc-conformance/`, `OxDoc/crates/oxdoc-cli/`.
- Keep: source preservation, the fidelity ledger, typed refusals, lazy load profiles.
- Rework: split the monolith, remove the engine-specific names, add authoring.

**DnaTreeCalc → CalcHost, DNA Calc, DNA OneCalc** (`DnaTreeCalc/`, ~28 crates under `DnaTreeCalc/src/`, 7 MB)
- Model/host:
  - `DnaTreeCalc/src/dnacalc-host-core/`: `WorkbookSession` in `DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs`, `HostCommand` in `DnaTreeCalc/src/dnacalc-host-core/src/command.rs`, `DnaTreeCalc/src/dnacalc-host-core/src/defined_names.rs`, `DnaTreeCalc/src/dnacalc-host-core/src/grid_publication.rs`
  - `DnaTreeCalc/src/dnacalc-skin-ir/`
  - `DnaTreeCalc/src/dnacalc-formula-ux-core/`
- Skins:
  - `DnaTreeCalc/src/dnacalc-shell/` (`StageSurface` in `DnaTreeCalc/src/dnacalc-shell/src/stage.rs`)
  - `DnaTreeCalc/src/dnacalc-stage-sheet/` (Canvas2D grid with a pure RenderPlan)
  - `DnaTreeCalc/src/dnacalc-stage-notebook/` (partial)
  - `DnaTreeCalc/src/dnacalc-stage-atlas/`
  - `DnaTreeCalc/src/dnacalc-strand/` (design tokens)

  The tree/model stage and the canvas are not built. Design: `DnaTreeCalc/docs/ux/redesign/REDESIGN_PROGRAM.md`, `DnaTreeCalc/docs/ux/redesign/MODEL_SPEC.md`, `DnaTreeCalc/docs/ux/SKINS.md`.
- Conformance: `DnaTreeCalc/src/dnacalc-conform/`, a provenance-first, cell-by-cell differential against the `<v>` values Excel cached in the files. Latest coverage 407/423 formula cells evaluated; 321 match.
- Legacy to drop: the `DnaTreeCalc/src/dnatreecalc-*` stack (16 skins, a 402 KB `DnaTreeCalc/src/dnatreecalc-host/src/app/session.rs`), and most of `DnaTreeCalc/src/dnacalc-bench-host/` (legacy UI; a 301 KB verification bundle that shells out to `cargo` with hard-coded paths).

**Background and evidence**
- `Foundation/notes/PROGRAM_INVESTIGATION_2026-09-10.md`: the last whole-program investigation and campaign plan. Read §1 and §2.
- `Foundation/ARCHITECTURE_AND_REQUIREMENTS.md`: §3.18 (FEC/F3E seam: prepare → open → execute → commit producing value, topology and shape deltas), §3.5 (external/RTD), §3.7 (macros).
- `ExcelFunctionsHandbook/data/functions/*.json`: 541 function entries projected from OxFunc metadata; 40-locale localisation table.
- `OxXlPlay/` / `OxReplay/`: an Excel capture and replay bundle format; thin. Four parallel Excel-COM capture lineages exist across repos; the plan should pick one.
- `DnaVisiCalc/`: a small, complete pathfinder engine with a C API and UDF registration (`DnaVisiCalc/docs/ENGINE_API.md`). A compact precedent for an engine API.

---

## 5. Open questions for you to decide (with reasons)

1. **Language and runtime.** Rust throughout, or otherwise? Consider the three platforms (Windows, Linux, wasm), the future COM/XLL hosting on Windows, and OxVba, which is already Rust.
2. **Repository layout.** One workspace/monorepo vs repo per library. The current many-repo layout with path dependencies has caused cross-repo compile breaks, shared-`target/` compiler crashes ("one cargo at a time"), and no CI anywhere except OxVba.
3. **UI stack** for DNA Calc and DNA OneCalc: keep Leptos + Tauri + Canvas2D, or change. Justify against the four skins, the 1M-row grid, and one codebase running as a desktop app on Windows and Linux and in the browser.
4. Per-library **carry / restructure / rewrite** decision (§2).
5. The **"stateless" contract** and the mechanics of the decided state ownership (§3.1).
6. **Non-Excel skin persistence** (§3.5).
7. **Excel oracle infrastructure**: one capture lineage, how ground truth is stored and versioned (the current 3.4 GB is untracked), and how the offline cached-`<v>` differential and live-COM capture share one verdict path.
8. **Conformance corpus**: the existing Excel-authored corpus is tiny (174 workbooks, 481 formula cells, none multi-sheet). Plan a real one (public, permissively licensed: Enron, EUSES, FUSE class), with licence and provenance recorded.
9. **Exact vs Fast numeric modes**: the design questions in §2.
10. **The separable areas** (§2): which areas, their boundaries, and the attachment points the core must provide. For charts and pivots, outline enough of the scope (chart types and rendering; pivot cache, layout, refresh, GETPIVOTDATA) to fix those attachment points correctly. The detail waits for their planning beads.

---

## 6. Lessons from the current program (treat as requirements)

1. **Provenance before value.** When the engine could not evaluate a formula, it published Excel's cached value from the file. A naive differential scored those cells as passes. Only values the engine actually calculated may be scored. The headline metric is *cells calculated / formula cells*, then match rate.
2. **Fallbacks must be counted.** "Conservative full rebuild is always legal" became the default path: every uncertain edit fell back to mark-all, nothing failed, and no number got worse. Every escalation is counted and reported, and must have a named owner for its incremental replacement.
3. **No seams without traffic.** Well-designed interfaces that no production code called rotted and misled audits. Every production enum variant and seam needs a non-test caller, checked mechanically.
4. **One of everything.** Two engine lanes, each with its own graph, scheduler, cache and publication, meant every feature landed in only one. Do not re-create a second evaluator, a second graph, or a second capture lineage.
5. **The machine is not the product.** The well-tested engine core was not what the app called; a 35k-line consumer layer was. Test the path the product actually runs.
6. **No faked behaviour.** A hard-coded iterative-calc table returning literal Excel values sat in the engine for months. Typed "not implemented" is always better.
7. **Measure, don't assert.** Baseline on the current stack (`DnaTreeCalc/docs/test-runs/w012/BASELINE_2026-09-15.md`):
   - 59.6 ms per cell entry at 10k rows (cost grows with authored cells)
   - cold recalc of 20k formulas: 81 s
   - a warm F9 that evaluates 2 cells: 53 s
   - a cross-sheet single edit: 4.4 s
   - peak memory: 2.2 GB

   These numbers show what an architecture that clones or re-scans on every edit costs. They are not targets: this plan sets no performance targets (see §2).
8. **Declare once, derive every path** (function dispatch, metadata, XLL export tables, help).
9. **Bit-exactness is a long tail.** OxFunc's own register says the last-bit campaign is "near its start" (260 divergent functions). Keep it as a continuous campaign. It must not block the engine's progress, and claims must not exceed the evidence.

---

## 7. What to deliver

A single plan document with:

1. **Architecture.** Library and application boundaries, dependency direction (mechanically enforced), state ownership, data flow for open → edit → recalc → save, and threading model now and later. Include a diagram.
2. **Public API sketches** in Rust signatures for CalcFunc, CalcFml (compiled-plan API), CalcEngine (op vocabulary, recalc facts, provenance), CalcDoc, and CalcHost (skin interface).
3. **Answers to every §5 question**, each with the alternatives considered and why.
4. **Per-library migration table.** Carry / restructure / rewrite; what is ported, from which files; what is deliberately discarded.
5. **Milestones**, in dependency order, each with an observable exit criterion: a test, a corpus number, or a demo. Not a document. The first milestone should be a thin end-to-end slice: open a real Excel workbook → recalc → differential against cached values → edit → save → reopen in Excel.
6. **Test and oracle strategy**: unit, property, reference-twin differential, corpus differential, live-Excel capture, round-trip, plus the benchmark harness, which measures and records numbers but sets no targets. Say where CI runs and what each layer proves.
7. **Phase-2 seam checklist** (§3.6), showing where each seam lives in the phase-1 architecture.
8. **Risks** and how the plan detects them early.
9. **Separable areas** (§2): for each one, its scope, dependencies, attachment points, risks, and where its detailed-planning bead sits in the graph.
10. **The complete bead graph** for the core, down to individual beads (see §8). For the separable areas, include only their attachment-point beads and one planning bead each. This is the main deliverable, not an appendix: the owner will import it with `br` and hand beads straight to coding agents.

Be concrete and opinionated. Where you are unsure, say what you would measure to decide, and make that measurement a bead. Do not pad the plan with process.

---

## 8. Bead graph format

The work will be done mostly by AI coding agents, several in parallel, each picking up one bead with no other context than the bead text and the repository. Write every bead so it can be done that way.

**Granularity.** One bead is one agent session of work, touching one library, landing as one or a few commits with its tests. Split anything larger. Epics group beads by milestone and library; they hold no work themselves.

**IDs.** Use provisional IDs with a prefix per library or app, e.g. `cfunc-`, `cfml-`, `ceng-`, `cdoc-`, `chost-`, `onecalc-`, `dnacalc-`, `infra-` (for repo, CI and oracle tooling). Hierarchical: `ceng-m2` (epic), `ceng-m2.4` (child). Cross-library dependencies refer to these IDs.

**Fields per bead:**
- `id`, `title` (imperative, specific)
- `issue_type`: `epic` | `task` | `bug` | `chore`
- `priority`: 0–4 (0 = most urgent)
- `labels`: milestone, library, platform where relevant
- `dependencies`: a list of `{ "depends_on_id": ..., "type": "blocks" | "parent-child" }`
- `description`, with these sections in order:
  - **Outcome**: what is true when done, in one or two sentences.
  - **Context**: why it matters, and which plan section it implements.
  - **Do**: the concrete steps, including the public API or types it adds.
  - **Port from**: exact files and symbols in the old repos to read, port or deliberately not port. Write "fresh" if nothing.
  - **Acceptance**: the exact commands (e.g. `cargo test -p calc-engine iterative::`) and the observable results. Name the tests that must exist and what each asserts. Acceptance must exercise the path the product runs, not a parallel harness, and must not rest on an outcome value the code assigns to itself.
  - **Platforms**: which of Windows / Linux / wasm the acceptance runs on.
  - **Out of scope**: the nearest tempting extra work, which belongs to another bead.

**Coverage rules:**
- Every milestone exit criterion in §7.5 maps to one or more beads whose acceptance proves it.
- Every phase-2 seam in §3.6 has a bead that creates its phase-1 attachment point and a test that calls it from non-test code.
- Every core entry in the Excel UI inventory (§3.5) maps to at least one bead. Entries belonging to a separable area map to that area's planning bead. Group them into command families so a bead is one family (e.g. "Format Cells: Number and Alignment tabs"), not one button.
- Measurements the plan relies on (corpus coverage, oracle captures, the benchmark harness's recorded numbers) are beads with a retained artifact as output.
- Beads that unblock others early (repo skeleton, CI on three platforms, value types, the conformance runner) are first in the graph, so agents can work in parallel as early as possible.
- Parallelism: after the skeleton milestone, at least four independent lanes (e.g. CalcFunc port, CalcFml, CalcDoc, CalcEngine core) should be able to run at once without editing the same files.
- The notebook, tree and canvas skins and their persistence get full bead coverage, scheduled at the earliest point their dependencies allow (§2).

**Output form.** Give the bead graph twice:
1. A readable outline: milestone → epic → bead titles with IDs and dependencies.
2. Machine-importable JSON Lines, one bead per line, with the fields above.

If the full graph does not fit in one response, deliver the plan (§7 items 1–9) and the outline first. Then send the JSONL milestone by milestone, ending each part with the list of milestones still to come. Do not shorten bead descriptions to make it fit.
