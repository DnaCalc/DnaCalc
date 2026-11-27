# Part XII. Customisation and the Skin API

Clusters: `CL-12-01`–`CL-12-12` · Governing decisions: `D-07`, `D-09`, `D-10`, `D-16`, `D-81`–`D-85` · Authoritative architectural specification: [SKIN_ARCHITECTURE.md](../SKIN_ARCHITECTURE.md) (`dc-xnsw4`) · Mockups: [Pro 21 U01](http://127.0.0.1:8720/p21/customise.html#U01)–[Pro 21 U05](http://127.0.0.1:8720/p21/customise.html#U05)

---

## 12.1 Centrality of the Skin Interface, Code Disposition, and Layer Rules (`CL-12-01`, `CL-12-02`, `D-07`, `D-09`, `D-10`, `D-81`)

Owner Rulings #9 and #10 (`D-09`, `D-10`) make the skin architecture the central foundation of DNA Calc's front end: Fable's and Opus's independent designs were merged in [SKIN_ARCHITECTURE.md](../SKIN_ARCHITECTURE.md) into **one single portable contract (`crates/calc-skin`)** so every built-in view, tool, user/agent skin, signed extension, CLI command, and automation bridge operates over neutral data with zero engine coupling (`Layer 1` `calc-host`/`calc-doc`/`calc-engine` is never reachable from UI crates, `R-91`, *"no more than Excel"*) `[FRAMING-0002, FRAMING-0003, FRAMING-0020, FRAMING-0046, FRAMING-0174, FRAMING-0175, P19-FABLE-0524]`.

### 12.1.1 From Prototype Leaks to a Clean Switch (No Micro-Step Migration)

The 2026-11-20/22 reviews (`dc-xfrhg`) split `controller.rs` (`5,578 -> 3,054` core lines) and eliminated raw skin `Intent` construction (`82 -> 0`), yet left per-skin projections and wire SVG inside `dna-calc-worker`, a 72-intent / 77-request / 53-reply wire protocol (`dna_calc_model::projection`), single-view mounting, and a separate 49-message `dna-onecalc` port and shell `[FRAMING-0021, FRAMING-0022, FRAMING-0023, FRAMING-0029, FRAMING-0030]`.

Honouring `D-07` (`WebView2 + Leptos 0.8 CSR` baseline kept; FerroUI `dc-etcxb.3` closed as not now) and `D-10` (no micro-step migration across the six old skins) `[FRAMING-0032, FRAMING-0033, FRAMING-0034, FRAMING-0044, FRAMING-0172, FRAMING-0175]`:
- **Kept & moved once (`~26,700` lines + `51,264` in `calc-host`):** DOM-free grid geometry/viewport/`RenderPlan`/`tile_mirror` (`-> dna-grid` & `dna-skin-runtime`), `printing.rs` (`1,285` lines `-> calc-host`, fixing `calc-host`'s upward dependency on `apps/dna-calc-model`), grid painters/panes/axis (`-> dna-grid`), worker tile `publication.rs` & `CommandJournal` (`-> calc-skin-host`), Explorer evaluator/CF/RTD/trace/session (`5,940` lines `-> calc-formula-host`), `dna-formula-ui` (`4,001` lines), `dna-strand` (`1,271` lines), `calc-chart-render` (`3,723` lines + `2.7 MB` Carlito TTF fonts kept **host-side** so `Chart.Export`, PDF/print, and worksheet `Objects` share one renderer without adding 2.7 MB to the UI wasm bundle), and **832 Classic/shell + 121 Explorer browser smoke cases**.
- **Frozen then deleted in one Milestone 3 switch commit (`~33,050` lines):** Old `Controller`, 5 retired skins (`atlas`, `canvas`, `ledger`, `tree`, v1 `notebook`), wire-coupled dialogs, `dna-calc-model` projections, `dna-calc-protocol`, worker per-skin arms, OneCalc's separate `port.rs`/`surface.rs`/`panels.rs`, `SkinIntent`, and v1 `urn:dnacalc:view-manifest`.

### 12.1.2 Vocabulary and Eight-Layer Workspace Stack

| Layer | Crates / Packages | Role & Mechanical `tools/xtask/architecture.json` Closure Rule |
|---|---|---|
| **L7 Compositions** | `apps/dna-calc`, `dna-calc-worker`, `dna-calc-desktop`, `dna-onecalc`, `dna-onecalc-desktop`, `dnacalc` | Thin entrypoints calling `View::install(&mut Registry)`; `dna-onecalc*` ceiling-checked in `sweep` for zero `calc-engine`/`calc-host`. |
| **L6 Views & Tools** | `apps/dna-view-{classic,inspector,notebook,map,app,explorer}` | Isolated presentation crates (`-app` registers `"app"` + `"forms"`); forbid `dna-shell`, peer views, and all host/engine/doc crates. |
| **L5 Shell** | `apps/dna-shell` | Shared Mast, Deck, Bridge, Stage + Companion, Panels, Sheet Tabs, Strip, Seam, Trust Cards, Try-On, Classic dialogs. |
| **L4 Kit, Grid & SDK** | `apps/dna-kit`, `dna-grid`, `dna-formula-ui`, `dna-strand`, `dna-ext-sdk` | Strand Leptos components (views write **zero raw CSS**), `PageRenderer`, `GridSurface` (`"sheet"`), formula field, guest `.dnaext` SDK. |
| **L3 Client Runtime** | `apps/dna-skin-runtime` | Portable, DOM-free, Leptos-free `Frame` store, `Live<T>`, `InputMode`, `EditSession`, `InputQueue` type-ahead, `Keymap`, `SkinValidator`, `__dnaProbe()`. |
| **L2 Skin Contract** | `crates/calc-skin` | **Single portable contract:** depends *only* on `calc-value`, `calc-syntax`, `calc-macro-security`, `serde`, `serde_json` (zero `calc-doc-model` or engine). |
| **L1 Serving** | `crates/calc-skin-host` | Multi-connection Level-1 server: neutral query handlers, footprint invalidation, atomic `Frame`s, binary tiles, `Preview`, `Proposal` staging. |
| **L0 Hosts & Render** | `crates/calc-formula-kernel`, `calc-formula-host` (L0); `calc-host` (L1), `calc-chart-render` | Level-0 pure kernel + stateful session (`D-81`, zero `calc-engine`); Level-1 `HostSession` owning `FormulaHost`, `History`, `ViewPart`, `PrintPlan`. |

---

## 12.2 The `crates/calc-skin` Contract Summary

Every type across the wire is portable, deterministic, and versioned (`Envelope` + binary tile frames over **four identical `Carrier`s**: browser `Worker` transferable `ArrayBuffer`s, `WebView2` shared binary buffers retiring base64-after-`U+0001`, `dnacalc serve` loopback WebSocket, and in-process Rust channels):

1. **Identity & Selection:** `SourceRef` (`Cells`, `Spill`, `Name`, `Table`, `Object`, `Sheet`, `Workbook`), `Target` (`Source | Anchor(AnchorId) | Item(ViewItemRef)`), dual-checked `Bound { anchor: AnchorId, address: String }` (`U01`, `U05`), and optimistic `Selection` (`client_seq` vs `SelectionCause` preventing cursor snap-back under type-ahead or macro `Select`).
2. **Stamps, Provenance & Evidence:** `Stamp { workbook, model, view, calc }`, `Shown<T> { value, provenance }`, `Coverage` (`Complete` vs `Partial`), `Answer<T>`, and `Evidence<T>` (`Observed` vs `Derived` vs `Unavailable`).
3. **Mutations & The Seam:** `Change` (`Model(ModelChange)` covering all Excel cell, formula-rewrite `ReplaceReference`/`RenameName`/`RewriteFormula`, structure, format, table, pivot, scenario, object, and settings edits; `Presentation(PresentationChange)` covering `CreateDoc`, RFC-6902 `PatchDoc { expect_view_rev }`, `RemoveDoc`, `Anchor`, `Prefs`, `Skin`, `Reading`, `Note`, `Trail`), atomic `Batch { id, base, expects, selection_seq, label, changes }`, `Preview` (`WritesOnly` via non-committing `prepare`, or budgeted `WithEffects` over a pinned `ModelRoot` when free of volatiles/RTD/VBA), and `Operation` (`Undo`, `Redo`, `Revert`, `Calculate`, `RunMacro`, `TryOn`, `Decide`, `RestoreCheckpoint`, `ViewRequest`).
4. **Neutral Budgeted Queries, Atomic `Frame`s, & `consistent()`:** Twenty-two view-neutral `WorkbookQuery` types (`Summary`, `Census`, `Inventory`, `Cells { Interest::Anchors }`, `Geometry`, `Formula`, `Deps`, `Structure`, `Issues`, `Sources`, `CalcState`, `CalcEvents`, `Changes`, `Explain`, `Contributions`, `Rows`, `Objects`, `Rules`, `Placement`, `TrialFor`, `PrintPlan`, `ViewPart`, `History`, `Proposals`, `Find`/`Special`/`Stats`/`Resolve`) plus Level-0 `FormulaQuery` (`Evaluate`, `EvaluatePart`, `Trace`, `ConditionalFormat`, `Catalogue`, `Providers`). Watch updates arrive in atomic **`Frame { stamp, events, updates }`** bundles (eliminating cross-watch tearing and honouring `ScreenUpdating = False`), while `runtime.consistent()` pins a single completed `CalcGeneration` for print/export/App views.
5. **Unforgeable Ports & Capabilities:** `FormulaPort` (cannot name workbook reads/writes) vs `WorkbookPort`, connection-fixed `Origin` and `WritePolicy` (`Commit | ApplyThenReview | Propose | Look`), upfront `Capabilities`, and typed `Refusal`s (`NotModelled`, `NotAvailableHere`, `Conflict`, `OverBudget`, `Validation`, `Protected`, `PolicyRefused`).

---

## 12.3 Declarative Manifests (`dna.skin/1` & `dna.page/1`), Contrast Checks, and Keymaps (`CL-12-03`–`CL-12-05`, `CL-12-07`–`CL-12-09`, `D-16`, `D-82`–`D-84`)

See [Pro 21 U01 — Layout declaration & live preview](http://127.0.0.1:8720/p21/customise.html#U01), [Pro 21 U02 — Theme tokens & sRGB contrast](http://127.0.0.1:8720/p21/customise.html#U02), and [Pro 21 U03 — Keymap & trial cell](http://127.0.0.1:8720/p21/customise.html#U03).

1. **Pure JSON declarations (`dna.skin/1` & `dna.page/1`, `D-83`, `D-84`, `R-91`):** Skins (`.dnaskin.json` or `<skins>` inside `urn:dnacalc:views:v1`) are 100% code-free JSON manifests specifying: (a) `theme` non-semantic overrides over `cockpit-light`, `cockpit-dark`, or `high-contrast` (`D-16`), (b) `shell` dock panels, Deck compactness, `Alt+1..9` quick slots, and extra Strip pins, (c) `keymap` chords, (d) `commands`—named sequences of built-in UI verbs that run **strictly on explicit user invocation, never on workbook open**, and route through normal Undo (`D-84`), (e) `views` settings, and (f) `pages` using one unified flow-layout `dna.page/1` component tree (`Stack`, `Row`, `Grid`, `Section`, `Tabs`, `Part { component, bind: BTreeMap<Slot, Bound>, props }`) shared across App pages, reader reports, skin panels, notebook controls, and Inspector benches `[FRAMING-0018, P19-CARBON-0086, P19-FABLE-0184, P19-FABLE-0185, P19-FABLE-0186, P19-FABLE-0188, P19-FABLE-0189, P19-FABLE-0190, P19-FABLE-0191, P19-FABLE-0508, P19-NEON-0079, D-83, D-84]`.
2. **Travelling skin safety & Try-On banner (`R-92`, `U01`):** Per-view layouts and `dna.page/1` pages in a workbook apply on open, whereas **file-embedded theme overrides, keymap changes, and shell layout overrides always open under the non-persistent Try-On banner** (`Trying "<name>" · Keep for file · Keep for me · Edit · Discard [Esc]`) so opening a colleague's `.xlsx` never hijacks a user's keyboard or contrast settings unasked `[P19-CARBON-0089, P19-FABLE-0193, P19-FABLE-0509, P19-GPT-0072, P19-NEON-0232, P21-0100]`. Dual `Bound { anchor, address }` mismatches block Apply with zero nearby-cell guessing (`U01` exception) `[P21-0101, D-82]`.
3. **Strand v0.2 Token Resolution & Blocking Contrast Check (`CL-12-03`, `CL-12-04`, `D-16`, `D-82`, `U02`):** `dna-strand::resolve` layers non-semantic overrides over Strand v0.2's three themes (including petrol-800 `--c-*` chrome inks, darkened light-paper inks, and WCAG AA `--dna-amber-ink` `#8A5A00`/`#FFD04D` alongside Recursive / Source Serif 4 / Recursive Mono and `0/5/9/14 px` radii) and computes unrounded sRGB relative luminance across **all 61 `SANCTIONED_PAIRS`** (surfacing the 9 headline pairs in Studio `U02`: `>= 4.5:1` text, `>= 3.0:1` boundaries and dual-stroke focus); **any contrast or immutable-core violation blocks Review/Apply** with exact ratios, line-numbered diagnostics, and an optional explicit `Apply suggested accessible value` button—never silently mutating author tokens `[P19-CARBON-0001, P19-FABLE-0011, P19-FABLE-0037, P19-GPT-0007, P19-NEON-0001, P19-NEON-0018, P19-NEON-0112, P21-0002, P21-0003, P21-0004, P21-0102, D-16, D-82]`.
4. **Immutable Core & Keymap Departure Counter (`CL-12-07`, `CL-12-08`, `R-61`, `R-94`, `U03`):** No skin may alter calculation, cell formatting, the five semantic signal hues/shapes, Strip safety readouts, Trust Cards, `Esc`/`Ctrl+Z`/`Ctrl+Y`/`F4`-edit/`Ctrl+Enter`/`Alt+Q`, or the return path to Classic `[P19-CARBON-0090, P19-FABLE-0194, P19-GPT-0071, P19-GPT-0291, P19-NEON-0082, P21-0104]`. Remapping any Appendix A Classic chord (`U03`: Ready-grid `F2 -> Ctrl+Shift+E`) increments a visible badge (**`Departs from Excel in N keys`**, `R-94`), blocks draft collisions (`Ctrl+E` Flash Fill), provides an isolated **single-cell Trial Bench** (`Inputs!B3`), and offers 1-click **`Restore Classic`** `[P19-FABLE-0187, P19-FABLE-0511, P21-0103, P21-0104, P21-0105]`.

---

## 12.4 The Extension Boundary: Signed Wasm-in-Worker `.dnaext` Over Bound Slots (`CL-12-06`)

Workbooks carry **zero executable UI code** (`R-31`): opening any `.xlsx` with custom views, pages, and skins runs zero scripts and fires zero network requests `[FRAMING-0067, P19-CARBON-0114, P19-CARBON-0202, P19-NEON-0107, P19-NEON-0117]`. Genuinely new visualization widgets (`dnakode.live-tree`, `NC04`, `E01`) ship as **Ed25519-signed `.dnaext` archives** (`extension.json` + Rust-compiled `module.wasm` built against `dna-ext-sdk`) installed behind a host Trust Card `[P19-FABLE-0192, P19-NEON-0080]`:

- Each active extension runs in an isolated **Web Worker** with **zero DOM, CSS, or raw `Query` access**; `dna-kit` feeds it only `Shown<T>` values on its declared `Bound` read slots and renders its returned Strand **`RenderTree`** primitives, routing write intents through cell validation and the Preview Seam (`Origin::Extension`).
- If a workbook references an uninstalled `.dnaext`, `dna-kit` renders a plain **`Needs extension <id> · Show bound source as table`** fallback card without fetching remote code (`SKIN_ARCHITECTURE.md §9`).

---

## 12.5 Skin SDK, Agent Verification Harness, and Two Canonical Worked Examples (`CL-12-10`–`CL-12-12`, `D-85`)

See [Pro 21 U01 — Customisation Studio](http://127.0.0.1:8720/p21/customise.html#U01), [Pro 21 U04 — Agent dark Classic restyle](http://127.0.0.1:8720/p21/customise.html#U04), and [Pro 21 U05 — Agent 3-line Loan App](http://127.0.0.1:8720/p21/customise.html#U05).

The published Skin SDK (`calc-skin`, `dna-kit` component catalogue, `dna.skin/1` & `dna.page/1` JSON schemas, and Customisation Studio `Alt,W,U`) pairs with a two-speed verification harness (`dnacalc skin validate <skin.json>` for sub-second schema/61-pair contrast/keymap/anchor checks + `cargo xtask app smoke calc --skin <skin.json>` driving `__dnaProbe()`, Light/Dark/High-Contrast screenshots, and `before.model == after.model` assertions in headless Chromium and Firefox) `[FRAMING-0019, FRAMING-0038, FRAMING-0127, FRAMING-0156]`.

### 12.5.1 Worked Example 1: Agent Restyles Classic (`U04`, `D-85`, `R-60`)

Prompt: *`"Make Classic calmer in dark mode for an audit session: larger formula text and a docked trace rail."`*
The agent submits a pure `dna.skin/1` manifest via `Operation::TryOn`—preserving Working density (`22 px` rows; refusing to mutate worksheet row heights, which would be a `ModelChange`), setting `formula.font.size: "15.5px"`, docking `inspector.trace` at `320 px` right, using `#A4B8C2` secondary ink on `#22343C` (`6.29:1` sRGB pass vs failing `#7D97A0` `4.19:1`), keeping `0` keymap departures and author cell formatting intact, and computing **`Changes: presentation only · 0 cells, formulas, or names`** (`Ctrl+Z` reverts in one step) `[P19-FABLE-0195, P19-GPT-0073, P19-GPT-0290, P21-0106, D-85]`:

```json
{
  "schema": "dna.skin/1",
  "id": "calm-dark-audit",
  "name": "Calm Dark Audit",
  "extends": "dna:default",
  "theme": {
    "base": "cockpit-dark",
    "density": "working",
    "overrides": { "ink.secondary": "#A4B8C2", "surface.panel": "#22343C", "formula.font.size": "15.5px" }
  },
  "shell": { "deck": "compact", "panels": { "right": [{ "id": "inspector.trace", "width_px": 320 }] } },
  "keymap": { "profile": "excel-365-win-en-us", "bindings": [] },
  "views": { "classic": { "provenance_tints": true } }
}
```

### 12.5.2 Worked Example 2: Agent Builds `Loan` App from a Three-Line Brief (`U05`, `D-85`)

Prompt: *`"Turn Loan.xlsx into a clean planner for Jo: principal, rate, and term inputs; monthly payment and total interest cards; the balance chart and expandable schedule."`*
The agent inspects `Structure` + `Rules`, never invents a second `PMT` formula, discloses exact Read/Write `Bound { anchor, address }` slots (`Loan!B2:B4` Write reusing workbook `Stop` validation, `Loan!B6:B7` Read, `Loan!"Balance"` chart, `Loan!A12:F371` windowed schedule), runs trial edits (`12% -> R10,286.13`) strictly in isolation without dirtying `Loan.xlsx`, blocks Apply if any binding is unresolved (`Loan!B8` exception), and sends any optional `Define Name` suggestions as a separate `Model` proposal `[P19-CARBON-0092, P19-FABLE-0196, P19-GPT-0074, P21-0107, P21-0108, P21-0109, D-85]`:

```json
{
  "schema": "dna.page/1",
  "id": "loan-repayment-app",
  "title": "Loan Repayment Planner",
  "input": "live",
  "print": { "default": "summary-1p", "schedules": ["none", "annual", "full"] },
  "root": {
    "kind": "Grid", "columns": 2,
    "children": [
      { "kind": "Section", "title": "Assumptions", "children": [
        { "kind": "Part", "id": "principal", "component": "input", "bind": { "value": { "anchor": "a001...", "address": "Loan!B2" } } },
        { "kind": "Part", "id": "rate",      "component": "input", "bind": { "value": { "anchor": "a002...", "address": "Loan!B3" } } },
        { "kind": "Part", "id": "term",      "component": "input", "bind": { "value": { "anchor": "a003...", "address": "Loan!B4" } } },
        { "kind": "Part", "id": "scenarios", "component": "scenario-picker", "props": { "baseline_pin": true } }
      ]},
      { "kind": "Section", "title": "Repayment Summary", "children": [
        { "kind": "Row", "children": [
          { "kind": "Part", "id": "payment",  "component": "value", "bind": { "value": { "anchor": "a004...", "address": "Loan!B6" } } },
          { "kind": "Part", "id": "interest", "component": "value", "bind": { "value": { "anchor": "a005...", "address": "Loan!B7" } } }
        ]},
        { "kind": "Part", "id": "chart",    "component": "chart", "bind": { "object": { "anchor": "a006...", "address": "obj:Loan/Balance" } } },
        { "kind": "Part", "id": "schedule", "component": "data-frame", "props": { "window": 12, "annual_rollup_opt_in": true },
          "bind": { "rows": { "anchor": "a007...", "address": "Loan!A12:F371" } } }
      ]}
    ]
  }
}
```

### 12.5.3 Engineering Milestones (`M0`–`M8`) and Honest Hard Parts (`SKIN_ARCHITECTURE.md §16–17`)

1. **Milestones:** **M0** Contract (`calc-skin`), `printing -> calc-host`, `ViewPart` & anchor transforms, `architecture.json` rules -> **M1** Level 0 (`calc-formula-kernel` + `calc-formula-host`) & standalone **Formula Explorer** teaser (`D-02`, 121 smoke cases) -> **M2** `calc-skin-host`, `dna-skin-runtime`, WebView2 binary buffers, `dna-shell` & thin Classic -> **M3** Full Classic (`>=832` smoke cases) & **one-commit deletion of ~33,050 legacy lines (`D-10`)** -> **M4** Inspector *(parallel after M2)* -> **M5** Notebook (`D-06`, *parallel after M2*) -> **M6** App, Forms maker, Skins & Agents *(parallel after M2)* -> **M7** Model map *(parallel after M4)* -> **M8** `.dnaext` Wasm-in-Worker host (Live Tree #1), VBA `Application.Run` view verbs & macro undo experiments.
2. **Ten hard parts stated plainly:** (1) 7-point macro-undo runtime/side-effect/event/replan hazard (`D-04`), (2) 300k–20.7M formula memory & background run indexing, (3) Windows-observed Excel key-tip/gesture/AltGr/IME long tail (`RUN-ON-WINDOWS.txt`), (4) snapshot-only non-volatile `WithEffects` value preview, (5) dynamic-array widening past lane widths (`#SPILL!` + explicit move), (6) unnamed external-edit `Orphan` anchors, (7) slow-carrier optimistic-cursor vs macro `Select` interleaving, (8) Windows STA thread blocking on slow native `.xll` calls, (9) Worker `RenderTree` hop bounds & Canvas2D ARIA trees, and (10) pinning the latest completed `CalcGeneration` under continuous high-rate RTD feeds in `consistent()`.
