# Risk lens: skins and app-shell drift

Audit of `C:\Work\DnaCalc\DnaCalc-review` at master `5a9348b`, read-only. Paths below are relative to
`apps/` unless noted. Baseline for growth figures: `git diff --stat ead996d~1 HEAD -- apps/`
(the dc-971p.2 to .15 slices, 2026-10-30/31): controller.rs +2,373, server.rs +2,106,
grid/mod.rs +1,228, wire.rs +366, 15,480 insertions across apps/.

Verdict: the owner's model (a functional model beneath, skins that only present and route) exists
only as a transport. There is no command layer, the controller is Grid-shaped and DOM-coupled, the
protocol has one bespoke request per skin, and Excel command rules live in dna-calc's server, not
calc-host. A new skin needs edits to six or more core files. Ranked by risk to future work.

## R1 (highest). No command layer: functionality is reachable only through the skin that wired it

Plan §8.1 (docs/plan/DNA_CALC_REIMPLEMENTATION_PLAN.md:485-491) asks for `CommandId` plus typed
payloads, a capability descriptor, and "every ribbon item, shortcut, menu, gesture and dialog
confirmation invokes a named command". None of this exists: no `CommandId`, no capability
descriptor, no `calcctl` anywhere in the tree (grep). What exists instead:

- Keyboard commands are bound inside the Grid skin's keydown handler: Ctrl+F/H/G/F5
  (dna-calc/src/grid/mod.rs:695-706), Ctrl+Z/Y (747-754), Ctrl+C/X (759-764), Ctrl+D/R (770-775),
  Ctrl+plus/minus (781-786), F9 (792-794). Canvas re-binds F9 and Escape itself (canvas.rs:515-518).
  Atlas, Ledger, Tree and Notebook have no undo/redo/find/recalculate keys at all; the shell's
  app-level handler (app.rs:284-330) only does F6 region cycling and Alt+digit skin switching.
  So "every skin presents full functionality" is already false for the keyboard.
- Menu commands are string-dispatched in a view: `match command { "copy" => ..., "insert" => ...,
  "find" => controller.find_open.set(Some("find".into())) ... }` (commands.rs:245-256).
- Mode and command arguments are stringly typed in the model: `find_open: RwSignal<Option<String>>`,
  `paste_mode: RwSignal<String>`, `fill_series: RwSignal<String>` (controller.rs:166-200),
  `freeze_panes(&self, choice: &'static str)` matching `"first-row" | "first-column" | "active"`
  (controller.rs:1971, 1986-1988).
- Grid-only command surfaces: Paste Special and fill tools (`clipboard::controls`), axis
  size/hide (`axis::controls`), shift-cells (`structure::controls`), freeze panes and
  Insert/Delete buttons are all mounted inside the grid view (grid/mod.rs:1740-1760). The shell
  mounts Find and Review for every skin (app.rs:483-484, 665-666), so coverage is accidental, not
  designed.

Risk: each slice adds per-skin bindings; other skins fall further behind and an agent customising
a skin must rediscover and re-wire every command. Root cause of most findings below. Seam: a typed
`Command` (or `CommandId` + payload) in the model, an availability query returning a reason, and
one shell-level keymap for all skins; skins add only skin-local gestures.

## R2. The controller is Grid-shaped and DOM-coupled, and holds per-skin state

`Controller` is the de facto functional model (skin.rs:12-14 says skins reach the workbook only
through it), but:

- It imports the Grid skin's module: `use crate::grid::selection::{GridExtent, NavAction, next_active, opposite}`
  (controller.rs:26).
- It focuses the Grid's DOM after model operations: `crate::grid::focus_grid()` at
  controller.rs:798, 1018, 1520, 2192, 2223, 2389, 2570, and queries `[data-dnacalc=grid]` /
  `[data-dnacalc=formula-bar]` directly (432, 532, 2233). Escape or Ctrl+Enter in the Notebook's
  editor runs `editor_key` (notebook.rs:358-362 delegates to controller.rs:2181) which focuses a
  grid that is not mounted; `focus_grid` silently falls back to `.dc-main` (grid/mod.rs:1777-1787).
- It installs a document-level capturing keydown listener, re-synthesises `KeyboardEvent`s and
  re-implements caret movement, Backspace/Delete and Home/End over UTF-16 for the text field
  (controller.rs:340-647, about 300 lines). This is editor logic, keyed to grid/bar DOM
  attributes, inside the model.
- The edit session enumerates skins: `EditOrigin { Cell, Bar, Notebook }` (controller.rs:48-56).
- It owns the persistent view state of three skins: `ledger: crate::ledger::ViewState`,
  `tree: crate::tree::ViewState`, `notebook: crate::notebook::ViewState` (controller.rs:221-223),
  plus Grid's `grid_views` and `grid_geometry` (173-174), and resets them by name on open
  (controller.rs:726-738). Canvas and Atlas have none, so Canvas depth/zoom resets on remount while
  Ledger filters survive: the persistence rule depends on whether the slice author edited the
  controller.
- `crate::clipboard::grid_owns_clipboard` decides clipboard ownership by
  `document().active_element().closest("[data-dnacalc=grid]")` (clipboard.rs:458-469), called from
  the controller (controller.rs:346-352).

Plan §8.1 separates four state classes (workbook data, calculated facts, durable per-skin layout,
ephemeral selection/editor state). Here all four share one `Copy` struct of ~40 public signals.
Seam: (a) a DOM-free `Model` (selection, edit session, history, clipboard snapshot, command
dispatch) that `tests/workbook.rs` can drive natively; (b) a `ViewStateStore` keyed by skin id
that skins register into; (c) a thin `dom` adapter for focus/keyboard replay that asks the active
skin for its focus target instead of naming the grid.

## R3. The worker protocol is a list of per-skin view models, and it depends on skin modules

`wire::Request` (wire.rs:251-345) mixes three kinds of thing: generic model operations
(`Intent`, `Subscribe`, `Save`), Grid-specific queries (`Geometry`, `UsedArea`, `Charts`,
`ReviewMarkers`, `Validation`, `Capture`, `FitContents`), and one request per non-Grid skin.
The per-file mapping is one-to-one:

| Skin file | Requests/intents it uses |
|---|---|
| atlas.rs | `Atlas`, `AtlasRefusals` |
| canvas.rs | `Canvas` |
| ledger.rs | `Ledger` |
| tree.rs | `Outline`, `Dependents` |
| notebook.rs | `Notebook`, `BlockDetail`, `Intent::NotebookLink`, `Intent::NotebookOrder` |

The replies are presentation, not model facts:
- `wire::Canvas` carries pixel `x`/`y` per chip, `width`/`height`, and `wires_svg: String`
  (wire.rs:643-685) produced from layout constants `COLUMN_WIDTH = 230.0`, `CHIP_HEIGHT = 110.0`
  (canvas.rs:41-44) and `format!("<svg ...>")` (canvas.rs:195-211). A second canvas-like skin
  cannot reuse the dependency trace without inheriting this layout.
- `Request::Ledger` takes `filter`, `sort` as free strings (wire.rs:335-345) parsed by
  `filter_for_choice("errors" | "refused" ...)` (ledger.rs:138-160); the option values in the
  `<select>` (ledger.rs:531-533) are the protocol.
- `Intent::Navigate { nav: crate::grid::selection::Navigation, .. }` (wire.rs:147): the wire type
  is defined in the Grid skin.

Dependency direction is inverted: the worker-side server calls into skin modules:
`crate::canvas::view_model` (server.rs:352-361), `crate::ledger::view_model` (362-376),
`crate::grid::selection::navigate` (1339), `crate::grid::sheet_metrics` (619, 1338, 1996),
`crate::grid::autofit::FIT_CELLS` (630, 640), `crate::grid::axis::stored_size` (742),
`crate::grid::chart::chart_svg` (2057), and even the page-side `crate::controller::memory_bytes`
(server.rs:311). Plan §8.1 asks for projections that "name interests: worksheet windows, block
IDs, dependency neighborhoods, object data or diagnostics". Seam: a `projections` module in the
server with skin-neutral queries (dependency neighbourhood to depth N as keys and edges; formula
cell listing with a typed filter/sort; layout blocks and placements per `ViewKind`; census) and
move pixel layout back into the skins. Navigation semantics (`selection::navigate`,
`sheet_metrics`) belong in the model, not in `grid/`.

## R4. Excel command semantics live in dna-calc's server, behind calc-host's `Edit { edits }` escape hatch

calc-host's `SkinIntent` (crates/calc-host/src/session.rs:121-170) has a generic
`Edit { edits: Vec<CellEdit> }`. dna-calc's server composes the edits itself for:
worksheet add/rename/move/remove/hide, including the "History is reserved" rule and sheet-key
allocation (server.rs:1085-1160); frozen panes (1217-1245); axis size/hide and fit (1161-1216,
702-777); conditional-format rule edits (872-1013); find/replace (1041-1068 with
`search::replace_edits`); paste plans (405-519, 1246-1257); notebook link/order, including
block-id and placement-id allocation by `max + 1` (1636-1815). None of this is reachable without
dna-calc's server, so a headless `calcctl`, an automation route (plan §8.8) or another front end
would have to copy it. `host_intent` (server.rs:2087-2104) still has arms that refuse these
intents with "needs the workbook worker"; they are unreachable because `intent()` handles them
first, and the messages are now misleading.

Risk: each UI slice adds Excel rules to the app layer. Seam: typed calc-host commands (sheet,
pane, axis, rule, layout) with validation in calc-host; dna-calc's server becomes a translator.

## R5. Duplication and drift between skins

- "Show this cell in skin X" is implemented five times with string skin ids, a 20 ms timer and a
  hard-coded focus selector: atlas.rs:134-144, ledger.rs:441-458 (chooses `"[data-dnacalc=canvas]"`
  or grid by string compare), notebook.rs:289, canvas.rs:525, tree.rs:647. All bypass
  `skin::switch_skin` and `SkinId` (skin.rs:157-165) by writing `active_skin` directly.
- "Reads" means different things: Ledger counts precedent *areas* (`host.precedents(at).len()`,
  ledger.rs:347), Canvas lists area labels (canvas.rs:339-343), Tree fetches dependents through a
  separate `Request::Dependents` and shows a count (tree.rs:585-600). Three dependency
  inspections, three code paths.
- Value/reason wording is recomputed per view model: `"refused".to_owned()` substitution at
  canvas.rs:259, ledger.rs:263, cells.rs:230; the reason sentence is
  `"{provenance}; {meaning}"` in Canvas (canvas.rs:317-321) but `"Calculated worksheet error:
  {meaning}"` only when Calculated in Ledger (ledger.rs:332-336), and the subscription `wire::Cell`
  has its own `error_meaning` (wire.rs:766). The same cell can explain itself differently per skin.

## R6. What a new skin would have to touch (no documented API)

`Skin::mount(ctx: SkinContext)` (skin.rs:84-91) hands over the whole `Controller` (about 40 public
signals and 70 public methods, none documented as a stable surface) and a raw
`RwSignal<Option<String>>` for the active skin. To add a seventh skin an agent must edit:
(1) the closed `SkinId` enum (skin.rs:31-74); (2) the registry (app.rs:32-40); (3) a new
`Request`/`Reply` pair in wire.rs; (4) a `handle` arm and view model in server.rs:84-392;
(5) a state field and its reset in controller.rs:165-232, 726-738; (6) a new `EditOrigin`
(controller.rs:48-56) and the key-replay selector (controller.rs:432) if it edits cells;
(7) the stylesheet list in tools/xtask/src/app.rs:126-130; (8) the five "show in skin" copies
(R5). It also has to follow undocumented DOM conventions (`data-dnacalc=grid`, `.dc-main`) for
focus, which the smoke harness also keys on.

## R7. OneCalc: the boundary holds, but its front end has no model under it

Positive: dna-formula-ui stays at calc-fml/calc-func (lib.rs:3-6); OneCalc re-exports it
(dna-onecalc/src/display.rs:9, result.rs:7) instead of copying; DNA Calc uses it for labels,
results and the formula field. The sweep's dependency ceiling enforces this (AGENTS.md).
Risks:
- `surface()` is one ~740-line view function (dna-onecalc/src/surface.rs:408-~1150) holding all
  application state as local signals (formula, rows, scratch entries, locale, format, fixture,
  share link, VBA source/status, dirty baseline), with `recalculate` (184-355) in the view module.
  The plan's "OneCalc has its own small application controller" (plan §8.7) does not exist;
  OneCalc cannot be reskinned without extracting one first.
- Shell pieces are duplicated rather than shared: theme `<select>` (app.rs:512-515 vs
  surface.rs:904-907), dialog styling (shell.css:76-77 `.dc-replace` vs surface.css:92-93
  `.oc-session-dialog`, same shadow/backdrop), unsaved-work protection (controller
  `has_dirty_work` vs OneCalc `baseline`).
- The edit contract diverged: DNA Calc wraps `FormulaAssist` with its own commit, point mode and
  F4 (controller.rs:2107-2485, using `dna_formula_ui::editor::{cycle_anchors, point_reference_span}`),
  while OneCalc drives `FormulaAssist` directly. Key semantics for the shared formula field now
  depend on which app hosts it.

## R8. God-object growth and the seams to split along

| File | Lines | Shape |
|---|---|---|
| dna-calc/src/controller.rs | 2,817 | One struct, ~70 methods; input queue/key replay 305-648; selection/navigation 1092-1240; clipboard/paste/fill 1245-1640; sheets 1640-1760; axis/fit/panes 1767-2015; go-to 2015-2106; edit session 2107-2485; structure/clear/history/calc 2485-2680; subscriptions/save 2682-2772 |
| dna-calc/src/server.rs | 2,661 | `handle` router 84-392; Excel commands 597-1350; notebook layout 1538-1875; outline 1877-1992; charts 1993-2066; tests 2197+ |
| dna-calc/src/grid/mod.rs | 1,870 | `grid_view` is a single ~1,610-line function (165-1776) containing keymap, pointer, editor, validation list, review markers, toolbars |

Natural cuts, in order of payoff: (1) controller → `model::{selection, edit, clipboard, sheets,
layout, history}` plus a `dom` adapter (R2); (2) server → `router`, `commands` (moving toward
calc-host, R4), `projections` (skin-neutral, R3); (3) `grid_view` → keymap table (to the shared
command layer, R1), pointer, editor overlay, and toolbars as separate components. Do (1) before the
next skin slice: every dc-971p slice added to all three files at once, so merge conflicts and
regressions concentrate there.

**Lower risk.** Strand tokens are mostly respected; one raw amber `rgb(255 179 1 / 0.16)` (dna-calc/web/css/grid.css:18)
should be a token. The transport is sound: one JSON message type, the in-process fallback runs the
same `Server` (port.rs:43-50), and tests/workbook.rs drives `Server::handle` through `wire::encode`.
