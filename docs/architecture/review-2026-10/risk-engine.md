# Risk lens: engine data representations and the UI <-> engine path

Repo `C:\Work\DnaCalc\DnaCalc-review`, master 5a9348b. Read-only; nothing built or run. Byte figures are
estimates from struct layouts, not measurements (the bench in `tools/xtask/src/bench.rs` has no recorded
results). Goal measured against: dc-8abf (Webs-class: billions of nodes, work proportional to change,
distribution) plus viewport-priority, cancellable recalculation.

## Verdict

The authored cell store (`calc-doc-model` pages) and the resumable `RecalcWork` are sound foundations.
Most of the rest is built for one Excel-sized workbook in one process, recalculated to completion and
published all at once. Nothing in the engine knows what is visible. Ranked below by how expensive each is
to change later, most expensive first.

## 1. Formulas are per-cell A1 text everywhere (most expensive to change)

- Authored formula = `CellInput::Formula(FormulaText(ExcelText))`, one `Arc<[u16]>` per cell
  (`crates/calc-doc-model/src/input.rs:85-117`). There is no shared or relative (R1C1/template) form in the model.
- Shared formulas are expanded on load: "Each member's input is the master's text with its relative
  references moved" (`crates/calc-doc/src/load/mod.rs:268-271`). The group survives only as a side
  record in `LoadedSheet.formulas` (`load/mod.rs:110`).
- Engine binding: each node decodes its text into a `Vec<char>` and builds a template-key `String`
  (`crates/calc-engine/src/recalc.rs:1128-1135`). It gets its own `FormulaInstance` with a boxed
  `areas` slice (`crates/calc-fml/src/eval/mod.rs:407-411`) and its own `Vec<Prerequisite>`
  (`calc-engine/src/state.rs:22-27`, `recalc.rs:1207`). Plans are shared per template, but nothing else is.
- Plan 6.2 promises "compressed interval/rectangle edges" for fills. A 1M-row fill of `=A1*2`
  instead becomes 1M nodes, 1M prerequisite vectors and 1M reverse-index cell entries
  (`graph.rs:342`, `BTreeMap<CellKey, NodeSet>`).
- Structural edits rewrite every formula's text through a `FormulaRewriter`
  (`calc-doc-model/src/structural.rs:1-8, 23-31`). Every anchor in the workbook moves
  (`calc-engine/src/edit.rs:479-560`) and all templates are dropped (`edit.rs:609`). Inserting one
  row in a big sheet is O(all formulas), with a re-parse of each.
- Undoing a structural edit rebuilds the engine with every formula pending
  (`calc-host/src/command.rs:447-459`, `EngineState::rebuilt`). That costs a full recalculation.
- Why this is expensive later: formula text is what the op vocabulary, the undo inverses, the wire
  (`wire::Cell.input`), the save writer and many tests carry. Adding a template-plus-offset or
  run-length "formula block" representation touches doc-model, codec, engine, host and app together.
  It is cheapest to decide before the large function-family ports and the save path settle further.

## 2. Recalculation: runs to completion, publishes all at once, sees no interest (expensive)

- Scheduling is the dirty closure (pending + volatile + cycle members), ordered completely up front in
  `begin_recalc` (`recalc.rs:480-620`). There is no target or priority parameter. Full recalc happens
  only when the numeric mode changes, on `calculate_full`, or on `set_environment` (`state.rs:782-788`).
- A budget exists (`WorkBudget::evaluations`, `recalc.rs:216-229`), but the product never uses it.
  `HostSession::recalculate` loops `step(.., WorkBudget::UNLIMITED, ..)` to the end
  (`calc-host/src/session.rs:679-700`). Budgeted steps appear only in tests and the bench.
- Publication is all-or-nothing: `finish` refuses `Incomplete` and `Superseded`
  (`recalc.rs:1001-1013`). Until then, every cell in the closure projects as `Stale`. Nothing is
  published while the recalc runs.
- Cancellation is supersession only: a commit drops `host.work` (`command.rs:460`, `recalc.rs:233`),
  and all staged work is thrown away. The worker cannot see a cancel request in any case (see 5).
- Interest exists, but only for delivery. `Interest::{Area, Areas, Sheet, Selection}`
  (`session.rs:56-68`) decides which re-projected cells a sink receives. It never reaches the engine.
- Parallelism and budgeting exclude each other. Parallel runs only when the budget is UNLIMITED and on
  the first step. It also needs no spills, no data tables, no iteration, no cycles and a silent
  observer (`recalc.rs:644, 739-757`). It is native-only, and spawns scoped threads per level
  (`recalc.rs:968-990`).
- What is already in place for a priority/cancellable design:
  - `Calculation` is stored per result (`result.rs:30-40`), so per-cell "settled at tick T" is representable.
  - `RecalcWork` is owned and resumable.
  - Readiness (a node waits on a not-yet-evaluated prerequisite) exists (`recalc.rs:2078`).
- What is missing:
  - (a) `begin_recalc(targets)`, which needs the backward cone of the visible dirty cells. Forward
    prerequisites exist per node, so this is feasible.
  - (b) publishing settled nodes before the end, which breaks the single `generation` check in
    `step`/`finish` and the "one generation" contract in plan 2.3.
  - (c) a cancel/yield token.
  - (d) a host scheduler loop outside `HostSession`.
- The engine and host acceptance suites pin the atomic semantics (`Superseded`/`Incomplete`).
  So does the `StaleCause` vocabulary (`project.rs:21-47`). Every month of tests written against
  "stale until the whole generation publishes" raises the cost.

## 3. The wire and the grid carry formatted strings per cell; tests check DOM structure (expensive)

- The transport is JSON text through `postMessage` (`apps/dna-calc/src/wire.rs:830-838`).
- Each cell crosses as `wire::Cell` with about 8 owned strings: text, kind, input, prose provenance
  `why`, colour, owner, error meaning (`wire.rs:730-745`). `why` is a sentence per cell, for example
  "calculated by DNA Calc's engine at revision N" (`cells.rs:321-350`).
- Each delta is fully formatted on the server (`server.rs:1496-1535`). Estimate: 300-500 B of JSON per
  non-empty cell.
- Scrolling: the interest is rounded to 32x8 blocks (`grid/mod.rs:52-53, 133-152`). Each block crossing
  sends `SetInterest`, which delivers a full reset of the whole new window (`session.rs:735, 752`),
  not the cells that came into view. The page mirror is cleared and refilled (`controller.rs:97-117`).
- The render-plan `Memo` depends on `viewport`, so it rebuilds on every scroll event, converting every
  mirrored cell back to `CellShown` (`grid/mod.rs:355-372`). `cells_view` then rebuilds every cell
  `<div>` with freshly formatted inline-style strings, without keying (`grid/mod.rs:1216-1281`).
- The selection subscription follows Select-All. After Ctrl+A, every commit re-projects, diffs and
  JSON-encodes the whole sheet's occupied cells (`controller.rs:764`, `session.rs:758-770`).
  `Capture` builds a `Vec<CellKey>` of every cell in the area (`server.rs:100-118`).
- Why this is expensive later: the browser harness `tools/xtask/src/app.rs` has 97 references to the
  per-cell DOM attributes (`data-cell`, `data-provenance`, ...). Five skin stylesheets also key on them.
  A canvas/WebGL grid, or a binary columnar wire (value kind + f64 + string-table index + style id),
  would invalidate that harness and the `wire::Cell` round-trip tests. This matches the owner's point
  that rendering speed depends on suitable representations all the way through: today the
  representation is a string per field per cell from the host to the DOM.

## 4. Memory footprint and storage shapes (moderate to expensive)

- Per formula cell, summed across layers (estimate): 600-900 B, at least 6 heap allocations.
  - Model: an `Arc` of text in a page.
  - `LoadedSheet` per-cell `BTreeMap`s: `styles`, `formulas`, `value_caches`, held for the whole
    session next to the retained source part (`calc-doc/src/load/mod.rs:105-121`).
  - Engine:
    - `by_anchor: BTreeMap<CellKey, NodeId>`, duplicated by `AnchorIndex` (`state.rs:358-366, 179-189`).
    - `nodes: Vec<Option<FormulaNode>>` at about 80 B, plus the prerequisite `Vec`.
    - `Binding`/`FormulaInstance` plus a boxed areas slice.
    - `Results.by_slot` with a `FormulaResult`: `CalcValue` (~48 B, including `Option<Arc>`) plus a
      `Calculation` (~32 B, with an `Arc` refcount bump per result).
    - A reverse-index entry per cell read, and a readers-cache `Box<[NodeId]>` per node
      (`recalc.rs:1377-1391`).
- Styles have two layers, both per-cell maps:
  - The file layer is `LoadedSheet.styles: BTreeMap<(u32,u32),u32>`, reached through
    `opened_position`, which replays every axis edit since open on each lookup
    (`calc-host/src/state.rs:649-657`, `facts.rs:36-47`).
  - Authored overrides are `Definitions.formats: BTreeMap<CellKey,u32>` (`definitions.rs:315`).
  - Neither is a run or range representation, so formatting a whole column means 1M entries.
- `Definitions` is cloned whole on any format, CF, pane, iteration or name edit (`root.rs:657-683`).
  It holds every authored format, array, unloaded cell and hidden row.
- ID widths:
  - `NodeId`/`SheetId` = u32 slot + u32 generation (`calc-value/src/id.rs:150-160`), at most 4.29e9
    slots per allocator. Memory runs out long before that.
  - Slot-indexed dense `Vec`s (`nodes`, `Results`, `Progress`, `ReadersCache`) are sized by the
    largest slot ever used, with no compaction.
  - `CellKey` is 16 B, of which the `SheetId` is 8.
  - The grid is fixed at 1,048,576 x 16,384 and baked in (`calc-value/src/grid.rs:9-13`; page index
    const-asserts in `calc-doc-model/src/cells.rs:29-35`; 183 `MAX_ROWS`-style uses).
  - `ArrayShape` uses `usize`, which is 32-bit on wasm.
  - No node has a document-stable key (already noted in dc-8abf).
- What breaks first:
  - 10^6 formulas: works natively, at roughly 0.6-0.9 GB. In the browser (wasm32, 4 GB cap, with the
    retained XML and `LoadedSheet` maps) it is near the ceiling. Opening recalculates everything on
    one thread. One row insert or its undo is O(10^6) with a re-parse.
  - 10^8: does not fit in one process (tens of GB). The fixed grid forces about 6+ full sheets. The
    O(N) costs per keystroke in item 6 reach seconds.
  - 10^9: needs a partitioned graph. There is no seam for one. `EngineState` reads `&ModelRoot` pages
    in process. Results, ids and indexes are process-local dense `Vec`s.

## 5. Concurrency model (moderate)

- Browser: one dedicated worker with a thread-local `RefCell<Option<Server>>`
  (`apps/dna-calc/src/worker.rs:94-97`). `handle` runs each message to completion: commit, then a full
  recalc, then delta projection, then posting (`worker.rs:60-90`). Scroll `SetInterest` messages queue
  behind a long recalc, and a cancel message cannot arrive. There are no wasm threads.
- `HostSession` is not `Send`: `DeltaSink = Box<dyn FnMut>` (`session.rs:120`), and `Workbook` keeps
  `Rc`/`RefCell` caches (`workbook.rs:210-217`). `EngineState` is `Sync` (it is shared across scoped
  threads), which is good.
- What a structured-concurrency runtime would plug into:
  - The only resumable seam is `calc_host::recalc::step(host, budget, observer)` (`calc-host/src/recalc.rs:68-125`).
  - It would need:
    - a scheduler that owns the session (actor style) and yields to the message pump between budgeted steps;
    - a cancellation and priority token passed into `begin_recalc`/`step`. Today `ReadObserver` can
      only stop a recalc by failing it, which counts as a failure, not a cancellation;
    - per-level task spawning reusing a pool instead of `std::thread::scope` per level;
    - a `Send` session or an explicit message boundary.
  - Delta delivery happens inside `dispatch` through a callback. It would have to become pull- or
    channel-based.

## 6. O(workbook) costs on every recalc and projection (cheap to fix now, internal)

- Every recalc, whatever the closure size:
  - `Progress::new(state.nodes.len())` (`recalc.rs:552`, `read.rs:358-364`);
  - `vec![false; state.nodes.len()]` in `finish` (`recalc.rs:1018`);
  - the readers cache is dropped whole whenever any edge, anchor or `Definitions` pointer changes
    (`recalc.rs:1407-1427`, `ptr_eq` at `1363`), and then ranks are rebuilt over the whole `by_anchor` map.
  - So typing one formula, or one format edit, costs O(all formulas).
- `project()` recomputes `engine.stale()`, the full dirty closure, sorted, on every call
  (`calc-host/src/project.rs:211-214`; `state.rs:670-698`). Conditional formatting calls `project` once
  per cell per rule (`conditional.rs:656-658`). In Manual mode, or while anything is pending, each
  scroll costs O(closure x cells shown).
- Any CF rule anywhere in the workbook turns every delta of every subscription into a full reset
  (`session.rs:852-866`). `snapshot` walks every spill in the workbook per subscription
  (`session.rs:808`).
- CF is evaluated in host projection at render time (`conditional.rs:145-162`), not as engine
  participants as plan 6.4 describes. Its statistics re-project whole rule areas (`conditional.rs:849-875`).

## What to keep

- The doc-model store: 256x16 `Arc`-shared pages under a 4-level index, so a one-cell edit copies a
  bounded path (`calc-doc-model/src/cells.rs:1-10`).
- Generational IDs.
- Slot-indexed results with a dense anchor grid (`state.rs:170-210`).
- The owned, resumable `RecalcWork`, with readiness and waits.
- `Calculation` provenance per result.
- Geometry by prefix sums and binary search, with `ScrollScale`.
- A real worker boundary, with formatting done off the main thread.

## Suggested order of decisions (not work)

1. Settle the formula representation: a template-and-offset or block form in the model and engine,
   with A1 text produced at the edges. Shared/filled regions get one node per block and interval edges.
2. Specify the recalc contract now:
   - `begin_recalc` takes a priority set (the viewport and subscriptions);
   - settled nodes are published before the whole generation finishes;
   - a cancellation token is checked within `step`;
   - the host scheduler yields between budgeted steps.
   After that, stop adding tests that assume one atomic generation.
3. Choose a viewport wire format: columnar or binary, a style-id table instead of per-cell strings,
   provenance as a code with prose on demand, and differential window moves. Decouple the browser
   harness from per-cell `<div>`s, for example by querying an accessibility or semantic layer.
4. Fix the O(N) costs per recalc and per projection in item 6 cheaply now.
5. Introduce run- or range-based style storage before formatting features grow.
