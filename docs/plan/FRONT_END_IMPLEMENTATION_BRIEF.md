# Front-end implementation: internal design, cleanup and refactoring (brief, 2026-11-20)

**Purpose.** This is the brief for the implementation plan behind the new front end. The 20-page design from Pro 19 (`docs/architecture/review-2026-11/pro/19-front-end-design-prompt.md`, with its synthesis) is added to this brief as the requirements: its R-list (front-end requirements) and C-list (capabilities the design needs, stated as plain needs). The plan maps each C item onto the layers below and turns both lists into code changes. It does not redesign the UI. The design brief deliberately leaves out implementation detail; this brief carries it.

**Ground rules.**
- The engine does no more than Excel.
- Page code never reaches engine or document code; the existing build rule stays and is extended.
- Every step keeps the six current skins working.
- No behaviour is lost without a bead that says so.
- Development happens on Linux, in batches of fixes with one check after each batch.

## Where the code is today (facts to plan from)

- **Contract.**
  - Host side: `HostSession` and `SkinIntent`.
  - Page side: `Skin` and `Controller` over the wire protocol (70 intents, 74 requests, 49 replies, events, TileBatch, RenderPlan).
- **The Controller** is one grid-shaped file of about 5,400 lines that every skin shares.
- **Skins build raw protocol intents.** Only 14 shell commands are shared, and the neutral `Target` type is unused.
- **The worker** builds each skin's projection and produces finished chart SVG and canvas wire drawings.
- **One view at a time**, with no ViewId.
- **The desktop shell** decodes some protocol messages itself (Open, add-ins).
- The current standalone Formula Explorer has its own port and Link message set and shares
  `dna-formula-ui`. Item 9 describes how it moves to the shared host contract.
- **View manifest** (CustomXML `urn:dnacalc:view-manifest`): written only by the notebook. Its round trip through Excel is unproven (dc-1nbp.22).

## The work the plan must cover

1. **Contract v2.** Adopt `Target` everywhere, then add:
   - a typed command layer that maps every action to an Excel operation (it replaces raw intents in skins);
   - ViewId and multiple subscriptions;
   - view state separate from workbook state.
2. **Controller split.** A small view-neutral core (session, commands, subscriptions, undo) plus a grid module. The other skins depend only on the core.
3. **Projections on the page side.** The worker sends neutral data: values, formulas, regions, dependency summaries, chart series. Skins assemble their own projections and draw their own SVG.
4. **The shell decoupled** from the protocol. The shell issues commands only, and one shell serves the browser and the desktop.
5. **Inner-layer queries and events (from the C-list).** For example:
   - dependency queries at region and name level;
   - calc chain and order;
   - region and pattern detection (sheet-as-database, input blocks, time series and the rest);
   - scoped change events;
   - incremental, paged projections;
   - level-of-detail summaries for large sheets.

   Each one gets a stated latency and memory budget.
6. **Persistence.**
   - One manifest reader and writer in calc-doc for all views, with per-sheet view choice and preferred and supported views.
   - Anchors through defined names.
   - Reconciliation after the file has been edited in Excel.
   - A proven Excel round trip.
7. **The skin manifest and SDK:**
   - the manifest schema and its loader;
   - theme tokens;
   - keymaps;
   - the component library;
   - the boundary for extension code.
8. **The test harness.**
   - Open corpus workbooks in every view, with no crash or hang, the budgets met, and no value changed by any view.
   - The file round trip and the Classic gesture list.
   - Screenshots, with a large-workbook tier drawn from Enron (20.7 million formula cells).
9. **A layered host contract, with the Formula Explorer as skin 0** (owner, 2026-11-20).
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
10. **Cleanup.** Delete what v2 replaces: dead intents, per-skin worker projections, duplicated message handling, and the second VBA project reader (calc-doc's against CalcVBA's).

## What the plan must deliver

- **Beads under one epic.** Each bead has acceptance criteria that name a test, is grouped into batches, and is ordered by dependency.
- **Milestones**, not dates.
- **For each step:** what is deleted, how the six skins stay working, and which tests prove it.
- **Risks**, with the step that retires each one.
- **Open questions that need the owner.** Keep these few. The coordinator decides the rest.
