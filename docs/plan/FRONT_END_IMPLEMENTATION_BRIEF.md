# Front-end implementation: internal design, cleanup and refactoring (brief, 2026-11-20)

**Purpose.** This is the brief for the implementation plan behind the new front end. The 20-page design from Pro 19 (`docs/architecture/review-2026-11/pro/19-front-end-design-prompt.md`, with its synthesis) is added to this brief as the requirements: its R-list (front-end requirements) and C-list (capabilities the design needs, stated as plain needs). The plan maps each C item onto the layers below and turns both lists into code changes. It does not redesign the UI. The design brief deliberately leaves out implementation detail; this brief carries it.

**Ground rules.**
- The engine does no more than Excel.
- Page code never reaches engine or document code; the existing build rule stays and is extended.
- Every step keeps the six current skins working.
- No behaviour is lost without a bead that says so.
- Development happens on Linux, in batches of fixes with one check after each batch.

## Where the code is today (facts to plan from — refreshed 2026-11-22)

- **Contract.**
  - Host side: `HostSession` (`crates/calc-host/src/session.rs:385`) and `SkinIntent` (`crates/calc-host/src/session.rs:133`).
  - Page side: `Skin` (`apps/dna-calc/src/skin.rs:59`) and `Controller` (`apps/dna-calc/src/controller.rs:100`) over the wire protocol (`apps/dna-calc-model/src/projection.rs`, re-exported via `apps/dna-calc-protocol/src/lib.rs:6` and `apps/dna-calc-worker/src/wire.rs:2`): 70 intents (`projection.rs:295`), 74 requests (`projection.rs:964`; `dc-xfrhg.1` `8ccfcdf7` deleted unused `CancelSave` 74 → 73, and `dc-971p.68` `a16744d6` added `GoToSpecial`), 50 replies (`projection.rs:1717`; `a16744d6` added `SpecialCells`), 7 events (`projection.rs:2024`), 4 `FromServer` envelopes (`projection.rs:2056`), `TileBatch` and `RenderPlan` (`apps/dna-calc-model/src/tiles.rs`, `render_plan.rs`). See [SKIN_INTERNALS_MAP.md](SKIN_INTERNALS_MAP.md).
- **The Controller (`dc-xfrhg.3`, `a9ef002b`).** Split from one 5,578-line grid-shaped `controller.rs` (`a9ef002b^`) into a view-neutral shared core (`apps/dna-calc/src/controller.rs`: 3,054 lines after `a9ef002b`; 3,181 lines total / 2,263 production lines before `#[cfg(test)]` at `:2264` after `dc-971p.67–.73`) plus extracted `controller/clipboard.rs` (509 lines), `controller/editing.rs` (530 lines), `controller/files.rs` (438 lines), and grid-only `apps/dna-calc/src/controller/grid.rs` (`:7`, 1,145 lines). `apps/dna-calc/src/skin.rs:255` tests that the core and all five non-grid skins (`atlas`, `canvas`, `ledger`, `notebook`, `tree`) never import `GridController` or `crate::grid`.
- **Typed commands and `Target` (`dc-xfrhg.2`, `a3955bc2`).** Production page code and skins build zero raw protocol `Intent` variants (82 → 0 sites; remaining `Intent::`/`SkinIntent::` matches in `apps/dna-calc/src/` are inside `#[cfg(test)]` in `controller.rs:2448-2464,2797`). All 67 workbook mutation operations route through `WorkbookCommand` over `dna_calc_model::Target` (`Cells`, `Name`, `Sheet`, `Object`, `LayoutItem`; `apps/dna-calc-model/src/command.rs:78`) in `apps/dna-calc/src/workbook_command.rs:35-499`, alongside 14 shared `dna_calc_model::Command` shell commands (`command.rs:91`). Non-intent `Request`s (`Geometry`, `Outline`, `Notebook`, `Canvas`, `Ledger`, `Atlas`, etc.) are still issued by page/skin modules.
- **The worker (`dc-xfrhg.5`, `75bf5aec`).** Still builds each skin's projection (`apps/dna-calc-worker/src/canvas.rs:8`, `ledger.rs:34`, `tree.rs`, `chart.rs`, `server.rs:4220,4380`) and produces finished chart SVG (`apps/dna-calc-worker/src/chart.rs:180`) and canvas wire SVG drawings (`apps/dna-calc-worker/src/canvas.rs:101`). Cell assembly across six projection paths (`workbook.rs`, `printing.rs:314`, and `server.rs:2067,4112,4292,4346,4477`) now shares `Workbook::wire_cell` (`apps/dna-calc-worker/src/workbook.rs:796`) and `wire::cell_of` (`apps/dna-calc-worker/src/wire.rs:49`), and canvas/ledger share `cells::value_or_refused` (`apps/dna-calc-worker/src/cells.rs:178`), verified byte-identical across 12 replies (164,559 bytes) in `apps/dna-calc-worker/tests/support/projection_wire.rs:100`.
- **One view at a time.** `Model::active_skin: Option<String>` (`apps/dna-calc-model/src/state.rs:165`) holds one active skin (`apps/dna-calc/src/skin.rs:5`), with no `ViewId`. Subscription plumbing (`Controller::subscribe`, `apps/dna-calc/src/controller.rs:1081,1992`; `apps/dna-calc/src/grid/mod.rs:535`) registers numeric mirror IDs for selection and the grid viewport.
- **The desktop shell (`dc-xfrhg.4`, `a7cf790c`, `bfa07497`).** Decoupled from wire protocol decoding: `apps/dna-calc-desktop/src/app.rs` has 0 wire request decoders (down from 3) and 0 named protocol request references (down from 14), sharing `PageMessage::decode`, `desktop_command`, `answer`, and `event` in `apps/dna-calc-worker/src/message.rs:6-118` with the browser worker (`apps/dna-calc-worker/src/worker.rs:45,84`). `DesktopCommand` (`message.rs:11,33`) still intercepts `OpenNative` and add-in requests for native dialogs and `calc-addin-host`. (`apps/dna-calc-desktop/src/engine.rs` is the separate COM/automation `HostSession` bridge, not the WebView2 page transport.)
- **Classic grid parity (`dc-971p.67–.73`, refreshed [UI_REVIEW.md](UI_REVIEW.md)).** The 31-row Classic grid parity table in [UI_REVIEW.md](UI_REVIEW.md) now stands at **8 Done** and **23 Partial** after `dc-971p.67–.72` (formula-bar arrow pointing, picked-range F4, `Alt+PageUp/PageDown`, Go To Special, `Ctrl+wheel` zoom, spill outlines and read-only child explanations, note/comment hover previews, workbook VBA completion refresh after enabling macros, and Flash Fill smoke timing). Merge authoring (`dc-971p.73`) and the remaining 23 Partial rows (`dc-971p.74`, `dc-971p.16`, `.43`, `.62`) remain open.
- **Formula Explorer (public name "DNA Calc - Formula Explorer", `dc-sdzi1` `7dd2f1ab`).** Still uses its own port and Link message set (`apps/dna-onecalc/src/port.rs:76,309`: 32 requests after `dc-xfrhg.1` `8ccfcdf7` deleted 3 unused variants, 17 replies; `apps/dna-onecalc/src/bridge.rs`), its own desktop shell and add-in surrogate (`apps/dna-onecalc-desktop`), and shares `apps/dna-formula-ui` (including shared catalogue help from `dc-zsn.5` `e8ca21ba`). RTD admission remains in `crates/calc-host/src/recalc.rs:444-520`. Item 9 describes how it moves to the shared host contract.
- **View manifest** (CustomXML `urn:dnacalc:view-manifest`): `calc-doc` reads and writes version 1 for `notebook`, `tree`, `canvas`, and `grid` (`crates/calc-doc/src/load/manifest.rs:54`, `crates/calc-doc/src/save/manifest.rs`), while UI/host placement commands (`prepare_notebook_link`, `prepare_notebook_order` in `crates/calc-host/src/presentation_commands.rs:21,139` via `apps/dna-calc-worker/src/server.rs:4316,4321`) are still written only by the notebook. Its round trip through Excel remains unproven (`dc-1nbp.22`, open).
- **VBA project reader (`dc-xfrhg.6`, `90b76ae8`).** One live parser remains: `bf5efa42` removed `calc-doc`'s internal directory parser (`crates/calc-doc/src/vba.rs:69`, 98 lines, handles opaque `vbaProject.bin` extraction/preservation) and `crates/calc-vba/src/office.rs` delegates storage/stream parsing to `calcvba-office`; `dc-xfrhg.6` verified 0 disagreements across 4,426 tracked files (4,311 projects, 16,978 modules, 115 without projects).
- **FerroUI evaluation (`dc-etcxb.1–.3`).** `dc-etcxb.2` (closed, `dce21890`, `838a3702`, [ferroui-eval.md](../design/front-end/ferroui-eval.md)) built a standalone FerroUI 0.5.0 grid prototype and Linux smoke harness; Windows measurements (`dc-etcxb.3`) and comparative verdict (`dc-etcxb.1`) are open.

## The work the plan must cover

1. **Contract v2.** *[Partly done: `dc-xfrhg.2` `a3955bc2` adopted `Target` and typed `WorkbookCommand` (`workbook_command.rs:35`), reducing raw skin `Intent` sites 82 → 0; `ViewId`, multiple subscriptions, and separating view state from workbook state remain open.]* Adopt `Target` everywhere, then add:
   - a typed command layer that maps every action to an Excel operation (it replaces raw intents in skins);
   - ViewId and multiple subscriptions;
   - view state separate from workbook state.
2. **Controller split.** *[Done: `dc-xfrhg.3` `a9ef002b` split `controller.rs` (5,578 → 3,054 lines; 3,181 total / 2,263 prod lines today) into the view-neutral core + extracted modules and `controller/grid.rs` (1,145 lines), verified for all five non-grid skins by `apps/dna-calc/src/skin.rs:255`.]* A small view-neutral core (session, commands, subscriptions, undo) plus a grid module. The other skins depend only on the core.
3. **Projections on the page side.** *[Partly done (prep only): `dc-xfrhg.5` `75bf5aec` unified worker cell assembly 6 → 1 (`Workbook::wire_cell`) without changing the protocol; projections and chart/canvas SVG drawing remain worker-side.]* The worker sends neutral data: values, formulas, regions, dependency summaries, chart series. Skins assemble their own projections and draw their own SVG.
4. **The shell decoupled** *[Partly done: `dc-xfrhg.4` `a7cf790c`, `bfa07497` decoupled `apps/dna-calc-desktop/src/app.rs` from protocol decoding (3 → 0 decoders) via shared `apps/dna-calc-worker/src/message.rs`; command-only decoupling across the shell and retiring OneCalc's separate shell remain open.]* from the protocol. The shell issues commands only, and one shell serves the browser and the desktop.
5. **Inner-layer queries and events (from the C-list).** *[Not started.]* For example:
   - dependency queries at region and name level;
   - calc chain and order;
   - region and pattern detection (sheet-as-database, input blocks, time series and the rest);
   - scoped change events;
   - incremental, paged projections;
   - level-of-detail summaries for large sheets.

   Each one gets a stated latency and memory budget.
6. **Persistence.** *[Partly done (codec only): `crates/calc-doc/src/{load,save}/manifest.rs` read/write `v1` for `notebook`, `tree`, `canvas`, `grid`, and `prepare_notebook_link`/`order` write notebook placements; per-sheet view choice, preferred/supported views, Excel edit reconciliation, and the Excel round trip (`dc-1nbp.22`) remain open.]*
   - One manifest reader and writer in calc-doc for all views, with per-sheet view choice and preferred and supported views.
   - Anchors through defined names.
   - Reconciliation after the file has been edited in Excel.
   - A proven Excel round trip.
7. **The skin manifest and SDK:** *[Not started.]*
   - the manifest schema and its loader;
   - theme tokens;
   - keymaps;
   - the component library;
   - the boundary for extension code.
8. **The test harness.** *[Partly done: `dc-971p.67–.72` and `dc-xfrhg` expanded Chromium/Firefox Classic gesture smoke (`apps/dna-calc/tests/smoke/cases.rs`) and projection byte-replay (`apps/dna-calc-worker/tests/support/projection_wire.rs`), and `dc-etcxb.2` added the standalone FerroUI scenario harness; cross-view corpus opening with budget/unchanged-value checks, file round-trip harness, and Enron-scale screenshots remain open.]*
   - Open corpus workbooks in every view, with no crash or hang, the budgets met, and no value changed by any view.
   - The file round trip and the Classic gesture list.
   - Screenshots, with a large-workbook tier drawn from Enron (20.7 million formula cells).
9. **A layered host contract, with the Formula Explorer as skin 0** (owner, 2026-11-20). *[Partly done (naming only): `dc-sdzi1` `7dd2f1ab` applied the public name "DNA Calc - Formula Explorer" while retaining internal `OneCalc` identifiers; the level-0/level-1 split, `calc-host` decision, moving RTD admission out of `calc-host/src/recalc.rs`, and retiring `dna-onecalc`'s port/Link/shell remain open.]*
   - **Level 0, the formula host.** Formula entry and evaluation, the function catalogue and help, completion, value display, number formatting, conditional formatting, and add-in loading and UDF registration (.xll, COM, VBA).
   - **Level 0's dependency closure** is CalcFml, CalcFunc, calc-value, the formatting kernels, the add-in hosts, and calc-doc for its constrained file. It never includes calc-engine.
   - **calc-host's role is the plan's first decision.** There are two options:
     - (a) level 0 lives inside calc-host, as a formula tier that does not pull in the engine;
     - (b) level 0 sits beside or below calc-host, and the higher layer avoids calc-host when it needs only level 0.

     Weigh them on four things: how the separation is checked (a crate boundary holds, while a cargo feature unifies across the workspace and erodes); where level-0 session state lives (registered UDFs, RTD topics, options); how many session objects a skin must handle; and what moves out of calc-host today, for example RTD admission in `calc-host/src/recalc.rs`.
   - **Level 1, the workbook host.** Cells, ranges, recalc, tiles and subscriptions. It is built on level 0 and uses level 0's services; it never copies them.
   - **Every skin declares the level it needs.** DNA Calc's formula bar, help and formatting use the same level-0 calls.
   - **The Formula Explorer is a tool, not a view.** It is built like a skin, on level 0 only, and appears in two places:
     - as a tool inside the DNA Calc app. Opened on a cell, it receives the precedents' current values as a snapshot (level 1 passes values down; level 0 never reads the workbook). Writing a changed formula back to the cell is a level-1 command issued by DNA Calc.
     - standalone as **DNA Calc - Formula Explorer** (formerly DNA OneCalc): a stripped build of DNA Calc with access to skin 0 only. That build contains only level 0.
   - **The honest-layering rule moves to the right place.** Today the sweep's ceiling step checks the OneCalc app. Instead it checks level 0's closure, and the Formula Explorer build (desktop and wasm) carries no engine code.
   - **What goes:** OneCalc's own port and Link message set, the desktop shell's dependency on `dna-onecalc`, and OneCalc's separate shell. Public naming is tracked by dc-sdzi1. Internal OneCalc identifiers are retained.
10. **Cleanup.** *[Partly done: `dc-xfrhg.1` `8ccfcdf7` deleted unused `CancelSave`, `SetForm`, `RemoveInput`, `ClearInputs`; `dc-xfrhg.4` `a7cf790c` removed duplicated desktop message decoders; `bf5efa42` and `dc-xfrhg.6` `90b76ae8` confirmed the second VBA project reader in `calc-doc` is already gone. Remaining cleanup (dead intents, per-skin worker projections, Formula Explorer port/shell) awaits items 1, 3, and 9.]* Delete what v2 replaces: dead intents, per-skin worker projections, duplicated message handling, and the second VBA project reader (calc-doc's against CalcVBA's).

## What the plan must deliver

- **Beads under one epic.** Each bead has acceptance criteria that name a test, is grouped into batches, and is ordered by dependency.
- **Milestones**, not dates.
- **For each step:** what is deleted, how the six skins stay working, and which tests prove it.
- **Risks**, with the step that retires each one.
- **Open questions that need the owner.** Keep these few. The coordinator decides the rest.
