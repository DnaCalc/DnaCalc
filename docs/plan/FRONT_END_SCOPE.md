# DNA Calc front end: scope (draft for the owner, 2026-11-20)

**Aim.** Excel under the hood, modern on the surface. The engine and the file stay Excel. Every workbook opens and works in Excel. The front end is a set of **views** over the same model: each one shows the workbook in a different way, and none of them changes how it calculates. An agent building an Excel-compatible model can present it far more attractively than a grid, and it is still Excel.

## 1. Layers

1. **Engine and document**: calc-host, calc-doc and the Excel model. Views never reach in.
2. **The view contract**, the Skin IR made real:
   - neutral **targets**: cells, ranges, names, tables, objects, layout items;
   - one **command vocabulary**: Excel's own commands, plus layout commands for views;
   - **projections** built from neutral data;
   - **subscriptions**: viewing is subscribing.

   A view can do only what a user could do in Excel, plus arrange its own layout.
3. **The shell**, shared by every view: files, recent and templates, print and page setup, add-ins, settings, search, a command palette, undo history and macro security. It is the same shell in the browser and on the desktop.
4. **Views**: the five modes below, plus any skin a user or agent writes.

## 2. The views (persona first)

| View | For | What it is |
|---|---|---|
| **Classic** (Excel-familiar) | Excel users, finance, operations | Grid, formula bar, sheet tabs and a tabbed command bar with Excel's structure but our own visual design (no Microsoft icons, fonts or styling). Keyboard and mouse gestures are an explicit conformance list: Alt key tips, Ctrl/Shift navigation, F2/F4/F9, Ctrl+Enter, fill handle and double-click fill, drag-move, range-picking while typing a formula. |
| **Inspector** (diagnostic, after Phyphox) | Model reviewers, auditors, developers, agents debugging | Everything systematically exposed: precedents and dependents, the calc chain and order, evaluation traces, volatile and dirty state, names, errors and their origins, add-in and RTD activity, timings, file parts. Read-mostly. |
| **Notebook** | Analysts, educators, anyone writing up a train of thought | Narrative and formula cells in reading order, with live recalculation (Pluto- and Excel-style, not run-once). Dynamic arrays and tables are the data frames, with inline charts and shapes, live RTD and small control panels built from sheet controls. Layouts run vertical or horizontal (a pipeline across the top, with sub-calculations on other sheets). Live tree is an extension. |
| **Model map** (blocks and links) | Modellers, engineers, systems thinkers | Names and blocks as boxes, references as wires, hierarchical blocks (a sheet, a table or a group shown as one block, opened to its inside), state and feedback (z⁻¹, iteration) drawn as in the 11-12 previews. Optional sheet pages. |
| **App** (proposed fifth) | People who *use* a model but don't build it | The model as an application: an input form, results, charts and scenario controls, laid out by the author. Nothing else is visible. This is the missing "face": today Excel users build these with locked sheets and buttons. |

**Thin formula rewriting** (the presenter/interpreter) is a lens any view can switch on: formulas shown in a more readable form, such as names, LET and lambda structure, with exactly the same meaning.

## 3. Views in a workbook

- **Stored in the file.** View layout and metadata live in the workbook as CustomXML (the existing `urn:dnacalc:view-manifest`), with hidden names as anchors. Excel ignores them and keeps them on save; that round trip is to be proven (dc-1nbp.22).
- **View rules.** A workbook may name a **preferred view** and **supported views**. Every view must open every workbook *correctly*, but a view a file wasn't designed for may look plain.
- **Mixed views.** Views can be mixed per sheet: a notebook sheet feeding a grid sheet, with a map sheet over both.
- **Automation.**
  - VBA and add-ins keep Excel's object model.
  - Classic gets the missing UI members: Selection, ActiveCell, ActiveWindow and the status bar.
  - Other views get a small **DNA Calc views COM library**, so a macro can switch views, select a block or show a panel. It is designed after the views settle.

## 4. Agents and users customise the front end safely

- **Declarative where possible.** A skin is a manifest that names its components:
  - theme tokens;
  - layout regions;
  - the keymap;
  - its commands and panels.

  Code is used only for new components.
- **A published skin SDK.** It has four parts:
  - the view contract;
  - the component library;
  - the manifest schema;
  - a harness that loads any workbook, screenshots each view and runs the keyboard and gesture list.

  An agent edits a skin, runs the harness, and ships it.
- **A guarantee.** A skin cannot change a calculation. It holds no engine handle: only the view contract, whose commands are Excel's. The engine rule holds: no more than Excel.

## 5. Where we are (status review, 2026-11-20)

- **Built.** There is a real two-tier contract:
  - **host side:** `HostSession` and `SkinIntent`;
  - **page side:** `Skin` and `Controller` over a neutral wire protocol (70 intents, tile batches, a semantic RenderPlan).
  - **Six DNA Calc skins:** grid (DOM and Canvas2D painters), notebook, ledger, atlas, tree and canvas. They all go through it, and none reaches the engine.
  - **Persistence:** the view manifest exists; only the notebook writes it.
- **Leaky:**
  - the Controller is grid-shaped;
  - skins build raw intents, and the neutral `Target` type is unused;
  - per-skin projections and chart and wire SVG are made in the worker, so a visual change touches worker code;
  - one view at a time;
  - the desktop shell decodes protocol messages.
- **Not built:**
  - the presenter/interpreter;
  - name-backed blocks and the model map;
  - multi-view;
  - UI automation;
  - a declarative skin manifest or SDK;
  - OneCalc on the shared contract (it shares only the formula UI).

## 6. Proposed order

1. **A design sweep.** It produces the view contract v2 (targets, one command vocabulary, page-side projections, multi-view and ViewId) and the skin manifest schema, with clickable prototypes of the five views and the owner's review.
2. **Contract work.** Split the grid out of the Controller, move SVG and projection assembly out of the worker, and adopt `Target`.
3. **Classic polish** against the keyboard and gesture conformance list.
4. **The skin SDK and harness.** Try them by having an agent restyle Classic and build an App view from a brief.
5. **Notebook v2, the Model map and Inspector**, in that order.
6. **DNA OneCalc becomes "DNA Calc - Formula Explorer", which gives access to skin 0 only** (owner, 2026-11-20). The host contract gets two levels:
   - **level 0, the formula host:** CalcFml and CalcFunc, formatting, conditional formatting and add-in UDFs, with no engine;
   - **level 1, the workbook host,** built on level 0.

   The honest-layering rule checks level 0's dependencies. Still to decide: whether level 0 lives inside calc-host as a formula tier, or the higher layer avoids calc-host for level-0 work. Skin 0 is not a workbook view on a par with the five views. It is a **tool**, the **Formula Explorer**, built the same way, on level 0 only. It appears in two places:
   - inside DNA Calc, as a tool opened from the shell or from a cell ("explore this formula"), where DNA Calc passes in the precedents' current values as a snapshot;
   - on its own as **DNA Calc - Formula Explorer** (formerly DNA OneCalc): a stripped build of DNA Calc with access to skin 0 only, for functions, formatting, conditional formatting and add-ins without a workbook.
