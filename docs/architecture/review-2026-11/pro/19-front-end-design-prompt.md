# Pro 19: DNA Calc front-end design: views, interaction, the conceptual UI model and the skin API

Brief for GPT-6 Pro and the other top models (2026-11-20). Each model answers on its own. A synthesis follows, and its result becomes the requirements for the internal implementation plan (the separate one-page brief, `docs/plan/FRONT_END_IMPLEMENTATION_BRIEF.md`).

## Your role and the deliverable

You are the lead designer of the DNA Calc front end. Write a **design of about 20 pages**. It should cover:
- the conceptual model of the user interface;
- each view's interaction design;
- the skin API and its intermediate representation (the "Skin IR");
- what the inner implementation must provide to make all this work.

Use plain language and numbered sections. Mark every assumption as an assumption. End with two numbered lists that implementation planning can take over directly:
- **R-1 ...**: front-end requirements, each one testable;
- **I-1 ...**: what the engine, document and host layers must provide, such as queries, events and budgets.

Sequence any plan by milestones, never by calendar dates or effort estimates.

## What DNA Calc is

- **An Excel-compatible spreadsheet system in Rust.** The engine aims to match Excel bit for bit, including its oddities. The file format is Excel's, and every workbook must still open and work in Excel.
- **The engine does no more than Excel.** Views may use only what Excel itself exposes (precedents, the calc chain, calc events, RTD), plus presentation of their own.
- **The aim: "super Excel-compatible in the engine, modern in the presentation."** A person or an agent building an Excel model can present it far better than a grid, and the file is still plain Excel.
- **Apps:**
  - the DNA Calc desktop app (a WebView2 skin today, a native UI later);
  - the web app (wasm, dnacalc.com);
  - OneCalc, a small values-only host for functions and add-ins.

  VBA, XLL and COM add-ins are supported.

## What exists today (status review, 2026-11-20)

**The contract has two tiers.**
- **Host side.** `HostSession` and `SkinIntent`: enter a cell, edit, select, calculate, subscribe.
- **Page side.**
  - A skin plugs in through `Skin` and a `Controller`.
  - They talk over a neutral wire protocol: 70 intents, 74 requests, 49 replies, events, streamed tile batches of grid cells, and a semantic RenderPlan for the grid.
  - A build rule stops page code from reaching engine or document code.

**Six skins use it:** grid (DOM and Canvas2D painters), notebook, ledger, atlas, tree and canvas.

**The leaks:**
- The shared Controller is shaped for the grid (5,400 lines).
- Skins build raw protocol messages rather than shared commands. A neutral `Target` type exists, but nothing uses it.
- Each skin's projection, the chart SVG and the canvas wire drawing are made in the worker, behind the boundary.
- Only one view can show at a time.
- The desktop shell decodes some protocol messages itself.
- OneCalc has a separate message set and shares only the formula editor.

**Not built:**
- a formula presenter or interpreter;
- name-backed blocks;
- the model map (two static HTML previews exist);
- simultaneous views;
- UI automation (VBA has the workbook object model, but no Selection, ActiveCell or ActiveWindow);
- a skin manifest or SDK.

**Persistence.** A view manifest exists as CustomXML (`urn:dnacalc:view-manifest`), but only the notebook writes it. Whether Excel keeps it on a re-save is unproven.

**Corpus for testing views.** These workbook sets exist:
- the Enron spreadsheets: 15,929 files, 1.9 GB, 20.7 million formula cells, 13.6 million of them shared-formula members;
- EUSES (4,652 files, not yet readable);
- seven library test sets (3,331 files);
- about 5,100 fixtures of our own.

Enron's files hold 436 names that call functions (OFFSET in 366 of them), 723 embedded OLE objects, 1,178 form controls and 230 ActiveX controls. A view design has to cope with this real material, not only with tidy examples.

## The owner's direction (settled; build on it)

1. **Five views by persona:**
   - **Classic:** Excel-familiar, with our own visual design, and a strong focus on keyboard and mouse gestures as an explicit conformance list.
   - **Inspector:** diagnostic, in the manner of Phyphox. It exposes precedents, the calc chain, traces, dirty and volatile state, errors and their origins, add-in and RTD activity, and timings.
   - **Notebook:** Jupyter-like layout with live recalculation in the style of Pluto and Excel. Dynamic arrays and tables serve as data frames, with inline charts and shapes, RTD, small control panels, horizontal and multi-sheet pipeline layouts, and a live tree as an extension.
   - **Model map:** blocks and lines, hierarchical, with optional sheet pages.
   - **App** (proposed): the model as an application for people who use a model but don't build it.

   **Thin formula rewriting** is a lens any view can switch on: names, LET and lambda structure, with exactly the same meaning.
2. **Shell and files:**
   - All views share one shell: files, print and page setup, add-ins, settings, macro security, the command palette and undo history.
   - Views are skins on one engine, and are views of an Excel model.
   - Extra view details live in CustomXML.
   - A file names its preferred and supported views, and views can be mixed per sheet.
3. **Agentic customization.** The front end is a customization surface where users and agents change display and behaviour without affecting the engine. A skin is mostly declarative: theme tokens, layout regions, keymap, commands and panels. Code is used only for new components. A skin holds no engine handle.
4. **A DNA Calc views COM library for VBA** is possible later.
5. **A layered host contract.**
   - **Level 0, the formula host,** goes only to the formula and function layers and never to the engine. It covers formulas, functions and their help, value display, number formatting, conditional formatting, and add-in UDFs.
   - **Level 1, the workbook host,** is built on level 0.
   - **Skins declare a level.**
   - **The Formula Explorer is a tool on level 0 only, not a workbook view.** It exposes and tests functions, formatting, conditional formatting and add-ins. DNA Calc hosts it as a tool, opened on a cell, with the precedents' values passed in as a snapshot. "DNA Calc - Formula Explorer" (formerly DNA OneCalc) is a stripped build of DNA Calc with access to skin 0 only.
   - **Enforcing honest layering** is a requirement of the design, not only a property of one app.
   - **Open: calc-host's part in this.** Level 0 may live inside calc-host as a formula tier, or the higher layer may avoid calc-host when it needs only level 0. Recommend one, with the reasons.

## Topics the design must cover

**A. A reified catalogue of spreadsheet models.** This is the grounding for everything else.
- **Name the recurring patterns** of real spreadsheet models, and of systems models built in spreadsheets. Examples:
  - sheet-as-database: a table of records used from elsewhere through lookups, SUMIFS and FILTER;
  - an assumptions or inputs sheet;
  - calculation blocks;
  - period-across time series;
  - scenario and sensitivity tables (data tables, the scenario manager);
  - reference and lookup tables;
  - staged pipelines across sheets;
  - accumulators and state (iteration, deliberate circular references, running balances);
  - simulation (random and volatile functions);
  - control panels built from form controls;
  - pivots;
  - external data and RTD;
  - VBA-driven models;
  - reports and dashboards;
  - stocks and flows, feedback and delays;
  - units.

  Add the patterns we have missed.
- **For each pattern, say:**
  - how to recognise it from the file: names, tables, shared-formula runs, the shape of the dependency graph, formatting;
  - with what confidence;
  - how a user or agent confirms or corrects that reading;
  - where the reading is stored (CustomXML, anchored by defined names);
  - how each of the five views presents and edits it.

**B. The conceptual UI model.** Cover:
- targets: cell, range, region, name, table, object and view item;
- selection and navigation that carry across views;
- what an edit means in a view that is not a grid, and how every edit maps back to Excel operations only;
- undo across views;
- errors, volatility, and calc state as first-class things a user can see.

**C. The interaction design of each view.** Cover:
- the Classic keyboard and gesture conformance list in full: key tips, navigation, F2, F4 and F9, Ctrl+Enter, fill handle, drag-move, picking ranges while typing a formula, and the rest you would add;
- for each other view: its gestures, its keyboard model, and how it is entered and left.

**D. Large workbooks and busy sheets.** For each view, decide when to **show everything**, **simplify** and **refuse**:
- show everything: Classic must virtualise Excel's full grid of 1,048,576 rows and 16,384 columns;
- simplify: regions as blocks, a shared-formula run as one item, level of detail, lazy expansion, sampling with a visible notice;
- refuse: a plain explanation and a way out.

Give the rules and thresholds as a policy, with the budgets each view must meet (open, first paint, interaction, memory). Also say what is never acceptable: a hang, silent truncation, or a view that quietly misstates the model.

**E. The skin API and IR.** Cover:
- the view contract: targets, one command vocabulary (Excel's commands plus view-layout commands), projections built from neutral data, subscriptions, view state, multi-view and ViewId;
- the manifest schema;
- the component library, theming tokens and keymaps;
- the boundary for extension code;
- versioning;
- how an agent writes, tests and ships a skin.

Show the IR concretely: types and a few message or manifest examples. Say which parts of today's contract to keep, change or drop.

Also design the contract levels:
- what belongs to level 0 and what to level 1;
- how a level-1 skin uses level-0 services without duplicating them;
- how the Formula Explorer looks and behaves:
  - standalone (DNA Calc - Formula Explorer), with no workbook: inputs, a scratch area of constants, conditional formatting previews, and add-in browsing;
  - as a tool inside DNA Calc: opened on a cell, values passed in, a formula written back through a level-1 command.
- the general idea of tools (panels built like skins) next to views.

**F. What the inner implementation must provide (the I-list).** Examples:
- dependency queries at region and name level;
- the calc chain and order;
- region and pattern detection;
- change events with scope;
- incremental projections;
- paging;
- performance budgets;
- persistence of view data through an Excel round trip, including how stale view data is reconciled after someone edits the file in Excel.

**G. Shell, printing and automation.** Cover:
- what each view prints;
- page setup;
- the shell commands;
- the missing UI members of the VBA object model;
- the outline of the views COM library.

**H. Testing.** Design a harness that opens corpus workbooks in every view. It checks:
- no crash or hang;
- the budgets are met;
- no view ever changes a value;
- the file round-trips;
- the gesture list.

It also gives screenshots for review and a large-workbook tier. Say how this scales across the Enron set.

**I. Risks, open questions and a milestone order.**

## Constraints

- No Microsoft icons, fonts, ribbon artwork or styling. The structure may be familiar, but the visual design is our own.
- The engine does no more than Excel.
- Nothing may require a file that Excel cannot open.
- Prefer what can be inferred from the file to anything the user must declare, but always allow correction.
