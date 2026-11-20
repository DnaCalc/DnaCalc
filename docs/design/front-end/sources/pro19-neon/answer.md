# DNA Calc Front-End Design: User Experience, Views, Interaction, and Mockups

**Document purpose:** Unified front-end UX specification for DNA Calc (desktop and browser), grounded in the **Strand** design language (`STRAND.md` v0.1), the **Linked Boxes** architecture (`LINKED_BOXES_SKIN.md`), and the live-system sketches (`names-in-motion.html`, `watershed.html`).  
**Companion file:** All 17 mockups (`M-01` through `M-17`) are rendered as self-contained interactive HTML/SVG views in [mockups.html](mockups.html).

---

## 1. Design philosophy

### What "modern on the surface" means here

Most spreadsheet challengers confuse *modern* with *un-Excel*: they replace the grid with card walls, invent proprietary formula dialects, or hide cell addresses behind friendly labels until the user hits a wall and exports back to Excel. DNA Calc makes the opposite bet. **Excel's calculation semantics, file format, and addressability are the rock; everything above the grid is an instrument panel built for the task in front of the person.**

"Modern on the surface" therefore means four concrete things—and explicitly *not* decorative gloss:

1. **Right representation for the cognitive job.** Reading a model for a board memo wants narrative prose with live bound numbers (**Notebook**); auditing a `$118,420` variance wants a causal graph and structural anomaly probes (**Inspector**); tuning an inventory feedback loop wants blocks, wires, and a `z⁻¹` delay loop (**Model Map**); exploring mortgage payments wants sliders and scenario columns (**App**); building and editing 12-month projections at keyboard speed wants an uncompromising grid (**Classic**). All five views project the *same* `.xlsx` workbook live.
2. **Flight-deck instrumentation instead of modal dialog boxes.** In Excel, checking errors, inspecting precedents, evaluating sub-expressions, managing external links, and enabling iterative calculation trap the user inside stacked modal dialogs that hide the sheet. DNA Calc adopts Strand's flight-deck temperament: calm petrol chrome (`#193F49`) holds live readouts in the **Mast**, **Bridge**, and **Strip** while the model sits unobstructed on daylight paper (`#FFFFFF` / `#EDF1F2`).
3. **Preview before commit.** Destructive or structural edits—fill-handle drags, wire connections, range rewrites, and AI agent proposals—render first across a **Preview Seam** with live recalculated deltas before touching the workbook. Pressing `Esc` is always an exact, lossless revert.
4. **Human and agent parity.** Every object has one canonical address (`Monthly!D14`, `Orders[Amount]`, `Inv.OnHand`, `block:Monthly/Revenue`). What a person navigates, selects, or inspects is the exact grammar an AI agent addresses and proposes diffs against.

### Weighing familiarity, clarity, density, and honesty

When these four qualities collide, DNA Calc resolves them in a strict order:

> **Honesty > Clarity > Familiarity (in Classic) / Density (calibrated per view)**

* **Honesty never yields.** DNA Calc will never invent a number Excel would not compute, never silently truncate 304,218 formulas to make a diagram look tidy, never mask a cached value from a missing external workbook as live data, and never hide an error behind rounded formatting. If a view cannot truthfully show a sheet at full resolution, it says what it simplified or declines with a concrete way forward.
* **Clarity governs presentation.** Visual hierarchy comes from Strand's single block geometry, weight, tabular numerals, and semantic color signals—not from competing borders, rainbow category palettes, or ribbon clutter.
* **Familiarity governs muscle memory, not chrome artwork.** In **Classic**, Excel's keyboard and mouse choreography (`F2`, `F4`, `F9`, `Ctrl+Enter`, `Alt` Key Tips, double-click fill handle, range picking mid-formula) is sacred. The chrome look is strictly our own—no Microsoft icons, ribbons, or office-blue styling. Outside Classic, views stay free to choose natural layouts while preserving shared shell shortcuts (`Ctrl+K`, `Ctrl+G`, `Ctrl+Z`, `Ctrl+Alt+1..5`).
* **Density follows hand-vs-eye intent (two modes only).** Strand defines **Working** density (22 px rows, tight 4–6 px gaps, high information-per-pixel for Classic, Inspector, Model Map, Formula Explorer) and **Reading** density (68 ch measure, Source Serif 4 prose, generous spacing for Notebook and App). No third "medium" mode is introduced.

*Alternatives considered and rejected:*
* *An Excel-clone ribbon with light-grey chrome:* Rejected—violates the no-Microsoft-artwork constraint and wastes ~110 px of vertical height needed for split diagnostics and Formula Explorer.
* *Hiding the grid behind a "modern default" for every opened `.xlsx`:* Rejected—an analyst opening a 40-sheet legacy workbook expects immediate grid footing; forcing a graph or document view first triggers distrust.

### What every view shares in look and behaviour

Every view lives inside **one unified shell** so switching views changes the *lens*, never the *application*:

| Shell Region | Position & Height | Persistent Across All Views | Adapts Per View |
| :--- | :--- | :--- | :--- |
| **Mast** | Top, 36 px (`petrol-800` `#193F49`) | Base-pair profile badge, file name + save state, View Switcher (`Ctrl+Alt+1..5`), Formula Lens toggle (`Alt+L`), Formula Explorer (`Ctrl+E`), Agent Drawer (`Ctrl+I`), Command Palette (`Ctrl+K`), Print, Settings. | Non-modal trust/alert pill when workbook has macros, missing links, or circular-ref warnings. Collapses to minimal bar in App present mode (keeping an explicit exit/view chip). |
| **Bridge** | Below Mast, 34 px (`petrol-900` `#12303A` / contextual) | **Unified Address Box** (`Ctrl+G` / `Alt+D`), Back/Forward jump history (`Alt+←` / `Alt+→`). | Classic: compact Formula Bar + expandable single-row Action Band. Inspector: Trace/Filter controls. Notebook: Block insert + Vertical/Pipeline toggle. Model Map: Hierarchy breadcrumbs + Director pill + Run/Step. App: Scenario selector + Reset + Print. |
| **Stage** | Main viewport, 14 px rounded top corners on paper (`#FFFFFF` / `#1A262B`) | 1-bit `SELECTED` vs `EDITING` model, dual-stroke amber focus ring, **Continuity Halo** (150 ms) on view switch, **Preview Seam** for uncommitted edits/agent proposals, `Alt+Hover` quick peek. | The active view surface (and optional side-by-side split or docked Formula Explorer / Agent Review drawer). |
| **Strip** | Bottom, 26 px (`petrol-800` `#193F49`) | Sheet/Page tabs (with per-sheet view micro-glyphs), live **Calc State** readout, clickable **Error Counter Pill**, quick selection aggregates (`Sum`, `Avg`, `Count`), Density indicator, **Semantic Zoom** control. | Contextual telemetry: e.g., row-count notice on sampled tables, iteration convergence readout, or live RTD feed rate. |

### Refinements to `STRAND.md` (v0.1)

This design builds directly on `STRAND.md` v0.1 and proposes **six targeted refinements** (proposed for Strand v0.2) to close gaps exposed by real spreadsheet workflows:

1. **Add `amber-ink` and dual-stroke focus rings (Contrast Law fix).** `STRAND.md` uses raw `amber` (`#FFB301`) for focus and selection, but `#FFB301` on `#FFFFFF` light paper is only ~1.84:1—failing WCAG AA (3:1 non-text, 4.5:1 text). *Refinement:* Add `--dna-amber-ink` (`#8A5A00` on light paper, ~5.7:1; `#FFD04D` on dark paper, ~12.4:1) for amber text/badges, and render focus outlines on light paper as a **dual-stroke ring** (2 px `#FFB301` outer + 1 px `#193F49` inner keyline) so amber stays the sole saturated attention channel while passing AA contrast everywhere.
2. **Define the Preview Seam & Agent Proposal grammar without adding a new hue.** `STRAND.md` has no tokens for uncommitted AI proposals or partial-hunk diffs. Rather than inventing a decorative "AI purple," proposals use a **dashed `teal-600` border** over an **8% petrol-teal 45° hairline-hatched surface** (`--dna-proposal-bg`) paired with a `[⇄ Agent]` glyph badge; accepted hunks settle with a 150 ms `green-500` fresh pulse, and rejected hunks revert cleanly.
3. **Add a scoped Formula Reference Palette (6 functional tones, editing-only).** During formula editing (`EDITING` state) or precedent tracing, users must match 4–6 simultaneous range tokens to grid bounding boxes. *Refinement:* Introduce 6 WCAG-checked structural accents (`teal-ink`, `prov-const`, `green-ink`, `amber-ink`, `slate-ink #4A5D7A`, `plum-ink #6E4775`—explicitly excluding `#D02A23` error red), always paired with circled numeric corner indices (`① ② ③ ④`) matching superscripts in the Formula Bar. Active *only* while editing or tracing.
4. **Reconcile `STRAND.md` §6 (Themes) with declarative Skins.** `STRAND.md` v0.1 stated that three themes (`cockpit-light`, `cockpit-dark`, `high-contrast`) replace skins entirely, whereas the product direction supports declarative skins (theme + layout + keymap + commands + panels). *Refinement:* Strand's three themes remain the **canonical token baseline**; a declarative **Skin** may override non-semantic surface/accent tokens and arrange chrome/panels, subject to an automated **Strand Contrast & Safety Gate** that blocks any override violating WCAG AA or repurposing `red` (error) and `signal` (external/volatile).
5. **Reconcile Model Map previews (`names-in-motion.html`, `watershed.html`) into Strand.** Both previews are great interaction proofs, but they invent divergent palettes (emerald green directors, purple circular wires, cream/navy themes). *Refinement:* Port their rich structural mechanics (directors, `z⁻¹` delay chips, inline FSM state diagrams, event queues, fan charts) into Strand's actual palette: petrol chrome, daylight/charcoal paper, teal structure, amber active firing ripple, `signal` orange RTD event sources, and **dashed-double-stroke + `↻` badge** for circular iteration blocks (zero extra hues).
6. **Protect Excel author formatting in Classic via an Instrument Overlay Layer.** Real workbooks carry author fills, fonts, and conditional formatting. In Classic, author styling renders faithfully on paper; DNA Calc's structural signals (provenance micro-ticks, volatile corners, error triangles, selection halo) occupy a non-destructive overlay layer toggleable with `Ctrl+Shift+P` (Provenance Overlay).

### Four principles that decide hard cases

1. **Excel Truth Over Visual Tidiness.** Whenever a cleaner UI clashes with an Excel oddity (blank cells coerced to `0`, `OFFSET` volatility, `1900` leap-year serial dates, hardcoded range truncation next to a table), DNA Calc exposes the exact Excel truth—and explains it plainly—rather than "fixing" or smoothing over the model silently.
2. **Infer First, Declare Never (Unless Correcting).** Every view must produce a useful, honest first view from any plain `.xlsx` file zero seconds after open without asking the user to annotate cells or define schemas. Whenever the user or agent refines an inferred structure, that correction is saved quietly in the workbook's `customXml` extension part.
3. **Same Address, Continuous Place.** Switching views, opening Formula Explorer, or stepping through an agent diff must never teleport the user to `A1`. The selected entity remains anchored, centered, and briefly ringed by the 150 ms Continuity Halo; `Esc` always unwinds one layer cleanly (`Editing → Selected → Drawer Closed`).
4. **Never Trap, Never Block, Never Hang.** No modal security walls on open, no frozen scrollbars during 300k-formula indexing, and no dead-end App screens without an unmistakable path back to the underlying model (unless the workbook author explicitly locked sheet protection in Excel itself).

---

## 2. The experience, walked through

Every storyboard below uses the five shared workbooks (`Inherited`, `Budget`, `Sales`, `Loan`, `Stock`) and six personas (**Maria**, **Sam**, **Lee**, **Jo**, **Ade**, and the **AI Agent**). Every mockup uses real data, real formulas, and active states (mid-index, mid-edit, active error, preview seam), with monospace wireframes strictly ≤ 98 columns wide and matching visual renderings in [mockups.html](mockups.html).

---

### Storyboard 2.1 — Maria opens `Inherited` for the first time and finds her way around

**Cast:** Maria (finance analyst, Excel power user) · **Workbook:** `Inherited.xlsx` (`WestPower_Consolidation_FY24_FINAL_v7.xlsx`: 40 sheets, 6 hidden, 304,218 formulas, `Dispatch_Hourly` packed `A1:CZ8760`, 3 missing external links, unsigned VBA module `modRollForward`, 9 `OFFSET` names, 47 unfixed errors across 4 root causes).

#### Beat 1 — Instant first paint without a modal dialog gauntlet
1. Maria presses `Ctrl+O` (or drags `Inherited.xlsx` onto the empty welcome Stage, which shows recent files, `New Blank Workbook [Ctrl+N]`, `Open Formula Explorer [Ctrl+E]`, and a drag-drop target).
2. **Feedback while loading (< 220 ms to first paint):** In Excel, opening `Inherited` traps Maria behind three sequential modal popups ("Update links?", "Cannot find `FY24_ForwardCurves.xlsx`", "Security Warning: Macros") while the UI freezes for 9 seconds. In DNA Calc:
   - At **180 ms**, the **Stage** paints `Exec_Summary` (the workbook's active sheet) in **Classic** using cached OOXML values so Maria can immediately scroll, select, and read numbers.
   - No modal popups block her. Instead, a single calm petrol-tinted **Trust & Triage Banner** docks cleanly between Bridge and Stage (`③` in `M-01`), showing exact counts: `40 sheets (6 hidden) · 304k formulas · 47 errors (4 root causes) · 3 missing links · 1 macro blocked · 9 volatile OFFSET names`.
   - Meanwhile, the **Strip** (`⑦`) shows live non-blocking background indexing: `◐ Indexing dependency graph 64% (195k/304k)`—never stalling scroll or arrow-key navigation.
3. **Hover & focus feedback:** Maria taps `↓` four times to `D9` (`#N/A`). A dual-stroke amber focus frame settles in 140 ms. In the grid, cell `B6` carries a tiny `signal` orange corner tick (`▸`) denoting an unresolved external link (`FY24_ForwardCurves.xlsx`), and `D9:D19` carry crisp `red-ink` (`#D02A23`) left hairline bars paired with `⊗` icons so color is never the sole error signal.
4. **Accessibility:** Screen reader announces: *"Inherited.xlsx opened in Classic view, sheet Exec_Summary. Warning banner: 47 errors from 4 root causes, 3 missing external links, 1 macro disabled. Focus on D9, error hash-N-A, formula INDEX Curve_Lookup."*

#### Mockup M-01 — `Inherited.xlsx` first paint in Classic (mid-index, non-modal triage banner)

```text
+----------------------------------------------------------------------------------------------+
| [==] Inherited.xlsx  [Classic*|Inspector|Notebook|Map|App]  [Lens] ① | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Exec_Summary!D9  v | fx =INDEX(Curve_Lookup!$C$2:$C$500,MATCH(A9,Curve_Lookup!$A$2:$A$500,0))|
+----------------------------------------------------------------------------------------------+
| ② ⚠ Triage: 47 errors (4 root causes) · 3 missing links · 1 macro blocked (modRollForward)   |
|    9 volatile OFFSET names [Open Inspector Ctrl+Alt+2] [Inspect Macro] [Manage Links]     [x]|
+-----+----------------+--------------+--------------+--------------+--------------+-----------+
|     |       A        |      B       |      C       |      D       |      E       |     F     |
+-----+----------------+--------------+--------------+--------------+--------------+-----------+
|  4  | Region / Hub   | Fwd Curve    | Vol (MWh)    | Gross Margin | Var vs Plan  | Status    |
|  5  | SP15_Peak      | ▸$68.40      |   1,420,500  |  $14,820,100 |    +$412,000 | Fresh     |
|  6  | SP15_OffPeak   | ▸$41.15 ③    |   1,108,200  |   $7,210,450 |    -$108,200 | Fresh     |
|  7  | PaloVerde      | ▸$59.80      |     982,000  |   $9,105,300 |    +$225,100 | Fresh     |
|  8  | ERCOT_North    |  $54.20      |   1,840,000  |  ⊗#REF!      |  ⊗#REF!      | Broken    |
|  9  | NP15_Peak      |  $71.10      |   1,265,400  |[[⊗#N/A ④ ]]  |  ⊗#N/A       | Broken    |
| 10  | NP15_OffPeak   |  $43.80      |     940,100  |  ⊗#N/A       |  ⊗#N/A       | Broken    |
| 11  | MidC_Peak      |  $62.05      |     810,600  |   $6,442,800 |    +$189,400 | Fresh     |
| 12  | Dispatch_Sum   |  (volatile)▸ |   8,366,800  |  ⊗#DIV/0!    |  ⊗#DIV/0!    | Broken    |
+-----+----------------+--------------+--------------+--------------+--------------+-----------+
| ⑤ [Exec_Summary*][Region_West][Region_ERCOT][Dispatch_Hourly][Curve_Lookup] [+6 hidden ▾]    |
| ⑥ ◐ Indexing 64% (195k/304k) · Cached values active  | ⊗ 47 errors (4 roots) | Working 100%  |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Unified Mast: `[==]` Excel-strict base-pair badge, View Switcher (`Classic` active), `Lens` (`Alt+L`), Formula Explorer (`fx`), Agent (`Ag`), Command Palette (`Ctrl+K`), Print. **②** Non-modal Triage Banner replaces Excel's blocking popup cascade; macro `modRollForward` is quarantined until explicitly trusted. **③** `▸` (`signal` `#FF8001`) corner tick marks cached values from missing external link `FY24_ForwardCurves.xlsx`; hovering reveals source file path and last-cached timestamp. **④** `[[...]]` dual-stroke amber focus frame on `D9`; `⊗` red-ink icon ensures error state is legible in monochrome. **⑤** Sheet strip reveals `[+6 hidden ▾]` (4 `xlSheetHidden`, 2 `xlSheetVeryHidden`) without unhiding them in the workbook. **⑥** Strip telemetry reports live non-blocking graph indexing and a clickable `⊗ 47 errors (4 roots)` pill.

#### Beat 2 — Switching to Inspector to map all 40 sheets and collapse 47 errors to 4 root causes
5. Maria presses `Ctrl+Alt+2` (or clicks `[Open in Inspector Ctrl+Alt+2]`).
6. **View switch continuity (< 60 ms):** The Mast, Unified Address (`Exec_Summary!D9`), Sheet Strip, and Undo stack **stay put**. The Stage transitions into **Inspector** (`M-02`). Because `Exec_Summary!D9` was selected, Inspector opens straight to the **Error Root-Cause Triage** tab with `D9`'s causal chain expanded and highlighted by a 150 ms amber **Continuity Halo**.
7. Instead of hunting through 40 tabs, Maria sees the **Workbook Atlas** on the left (all 40 sheets clustered into four dependency tiers: *External & Inputs → Curves & Tariffs → Hourly Engine (`Dispatch_Hourly`) → Regional & Exec Outputs*, with the 6 hidden sheets dashed and labeled `[hidden]` / `[veryHidden]`), and the **Root-Cause Ledger** on the right.
8. All 47 error cells collapse cleanly to **4 root causes**:
   - **Root 1 (`11 × #N/A`):** `Curve_Lookup!A412` missing lookup key `"NP15_Peak"` (propagates to `Exec_Summary!D9:E10` + 7 summary cells).
   - **Root 2 (`29 × #REF!`):** `Tariffs!G:G` deleted column referenced by `Region_ERCOT!F14:F42`.
   - **Root 3 (`5 × #DIV/0!`):** `Dispatch_Hourly!Z4410` zero generation hours for Plant #4.
   - **Root 4 (`2 × #NAME?`):** `Legacy_Adj!C11:C12` calls unknown UDF `=CURVE_INTERP(...)` (missing add-in).
9. **Volatile & External Links rail:** Below the Root-Cause Ledger, Maria clicks `Volatile (9 OFFSET names)` and immediately sees why `Inherited` felt sluggish in Excel: `Dyn_Curve = OFFSET(Curve_Lookup!$B$2,0,0,COUNTA(Curve_Lookup!$B:$B)-1,12)` forces 142,000 downstream formulas to recalculate on every single edit. Inspector offers a non-destructive note: *"Tip: Replace with `INDEX` or Excel Table to eliminate volatility"* (without touching the file).

#### Mockup M-02 — `Inherited.xlsx` in Inspector (40-sheet Atlas + 4 Root Causes)

```text
+----------------------------------------------------------------------------------------------+
| [==] Inherited.xlsx  [Classic|Inspector*|Notebook|Map|App]  [Lens]   | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Exec_Summary!D9  v | [Health & Atlas*] [Trace Selected] [Calc Order] [Live / External]       |
+---------------------------------------------------+------------------------------------------+
| ① WORKBOOK ATLAS (40 sheets · 304,218 formulas)   | ② ROOT-CAUSE ERRORS (47 cells -> 4 roots)|
|  Zoom: L0 Atlas  [Filter: Errors | Hidden | All]  |                                          |
|                                                   | ▼ [1] #N/A · 11 affected (Active focus)  |
|  [External Links (3 missing) ▸]                   |   Origin: Exec_Summary!D9 (MATCH miss)   |
|    ├─ FY24_ForwardCurves.xlsx (142 cells)         |   Key "NP15_Peak" not in Curve_Lookup!A  |
|    ├─ HR_Rates_2023.xlsx (18 cells)               |   Closest match: "NP15_PK" (row 412) ③   |
|    └─ Q2_Actuals_Lock.xlsx (9 cells)              |   [Jump to Curve_Lookup!A412] [Explorer] |
|          │                                        |                                          |
|          ▼                                        | ▶ [2] #REF! · 29 affected                |
|  [Curves & Tariffs (8 sheets, 1 hidden)]          |   Origin: Region_ERCOT!F14:F42           |
|    Curve_Lookup (4k) · Tariffs (1.2k)             |   Deleted col ref in VLOOKUP(...,#REF!)  |
|    [_FX_Legacy · veryHidden] (dashed)             |                                          |
|          │                                        | ▶ [3] #DIV/0! · 5 affected               |
|          ▼                                        |   Origin: Dispatch_Hourly!Z4410 (=0 hrs) |
|  [Hourly Engine (1 sheet · 182,400 fmls)]         |                                          |
|    Dispatch_Hourly (A1:CZ8760 · 12 runs)          | ▶ [4] #NAME? · 2 affected                |
|          │                                        |   Origin: Legacy_Adj!C11 (=CURVE_INTERP) |
|          ▼                                        |------------------------------------------|
|  [Outputs (12 sheets)]                            | ④ VOLATILE & MACROS                      |
|    [[Exec_Summary]] ⊗11 · Region_ERCOT ⊗29        | ▸ 9 OFFSET names (142k downstream cells) |
|    Region_West ⊗5 · Legacy_Adj ⊗2                 |   Dyn_Curve, Rolling_12M, +7 more [List] |
|                                                   | ⚑ VBA: modRollForward (214 LOC · Blocked)|
+---------------------------------------------------+------------------------------------------+
| [Exec_Summary*][Region_West][Region_ERCOT][Dispatch_Hourly][Curve_Lookup] [+6 hidden ▾]      |
| ✓ Indexed 304,218 formulas (1.4s) · Cached values | ⊗ 47 errors (4 roots) | Working L0 Atlas |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Workbook Atlas compresses 40 sheets into four topological tiers using Strand semantic zoom (`L0 Atlas`); hidden/very-hidden sheets appear in dashed outline with honest status tags. **②** Root-Cause collapse groups 47 cascading errors into 4 actionable origins, preserving focus on Maria's selected `Exec_Summary!D9`. **③** Inspector performs a read-only fuzzy key check on `MATCH` misses (`"NP15_Peak"` vs `"NP15_PK"` at `Curve_Lookup!A412`) without mutating anything. **④** Volatile `OFFSET` names and quarantined VBA macros share the right instrument rail.

#### Beat 3 — Honest refusal and Block Summary when Maria tries Model Map on `Dispatch_Hourly`
10. Curious how `Dispatch_Hourly` works, Maria clicks `Dispatch_Hourly` in the Atlas (navigating focus to `Dispatch_Hourly!A1`) and presses `Ctrl+Alt+4` to try **Model Map**.
11. **Refusal with a way forward (`M-03`):** Model Map does *not* freeze trying to draw 182,400 boxes, nor does it silently drop 99% of cells. Within 45 ms, it renders an explicit **Scale Notice Banner** at the top of the Stage: *"Dispatch_Hourly holds 182,400 formulas across 8,760 hourly rows. Individual-cell map declined to preserve responsiveness and honesty. Showing 12 exact R1C1 formula-pattern runs instead (100% of formulas accounted for)."*
12. Each of the 12 boxes displays its exact grid span (`K2:K8761`, 8,760 identical formulas), its shared formula pattern, and a live sample readout (`Row 2` + `Min/Max/Sum` across all 8,760 rows). Clicking `[Inspect Row 4410 (#DIV/0!)]` jumps Maria back to `Classic` (`Ctrl+Alt+1`) scrolled directly to `Dispatch_Hourly!Z4410` with the amber Continuity Halo pulsing on the exact zero-hours cell.
13. **Copy/Paste out:** Back in Inspector (`Alt+←`), Maria clicks `Copy Health Summary` (`Ctrl+Shift+C`) and pastes a clean Markdown table of the 4 root causes and 3 missing links straight into her team's Slack/email thread.

#### Mockup M-03 — `Inherited.xlsx` Model Map honest refusal & 12-run Block Summary on `Dispatch_Hourly`

```text
+----------------------------------------------------------------------------------------------+
| [==] Inherited.xlsx  [Classic|Inspector|Notebook|Map*|App]  [Lens]   | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Dispatch_Hourly!Z4410 v | Inherited › Dispatch_Hourly  | [Recalc]  | LOD: L2 Formula Runs    |
+----------------------------------------------------------------------------------------------+
| ① ⓘ Cell-level map declined for Dispatch_Hourly (182,400 formulas / 8,760 rows).             |
|   Showing all 12 R1C1 formula-pattern runs (100% coverage · 0 cells omitted). [Open Classic] |
+----------------------------------------------------------------------------------------------+
|                                                                                              |
|  +-----------------------+      +-------------------------+      +------------------------+  |
|  | ② INPUT RUN: A2:E8761 |      | FORMULA RUN: F2:K8761   |      | ③ FORMULA RUN: Z2:Z8761|  |
|  | 8,760 rows · 5 cols   |----->| 52,560 formulas (6 cols)|----->| 8,760 fmls · ⊗5 #DIV/0!|  |
|  | Hour, Plant, Load...  |      | =Load * HeatRate * Fuel |      | =Rev_Hr / Gen_Hrs      |  |
|  | Sample R2: 412.5 MW   |      | Σ $284.6M (all 8,760)   |      | Min: ⊗#DIV/0! (R4410)  |  |
|  +-----------------------+      +-------------------------+      | [Jump R4410 Classic]   |  |
|               │                             │                    +------------------------+  |
|               │                             ▼                                 ▲              |
|               │                 +-------------------------+                   │              |
|               └---------------->| ④ LOOKUP RUN: L2:N8761  |───────────────────┘              |
|                                 | 26,280 fmls · ▸Volatile |                                  |
|                                 | Reads Dyn_Curve (OFFSET)|                                  |
|                                 +-------------------------+                                  |
+----------------------------------------------------------------------------------------------+
| [Exec_Summary][Region_West][Region_ERCOT][Dispatch_Hourly*][Curve_Lookup] [+6 hidden ▾]      |
| ✓ Clean (cached) · 12/12 formula runs shown (182,400 fmls) | ⊗ 5 on sheet | Working 65% L2   |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Honest scale refusal explains *why* cell-level rendering was declined and confirms 100% coverage via R1C1 pattern folding—zero silent omission. **②** Every block displays its exact Excel range and aggregate across all 8,760 rows alongside a labeled row sample. **③** The 5 `#DIV/0!` cells inside `Z2:Z8761` surface right on the folded block with a one-click jump to `Classic` row 4410. **④** Downstream dependency on the volatile `OFFSET` name `Dyn_Curve` carries the `▸` (`signal`) marker.

> **What an Excel user notices as different—and why it is a win:** Opening a broken 300k-formula legacy workbook takes under a quarter-second with zero modal popups to dismiss; 47 scary red errors immediately collapse into 4 root causes; hidden sheets and volatile `OFFSET` bottlenecks appear in plain sight without modifying a single byte of the file.

---

### Storyboard 2.2 — Maria turns `Budget` into a Notebook write-up for her director

**Cast:** Maria · **Workbook:** `Budget.xlsx` (3 sheets: `Inputs` with 12 assumptions across `Base` and `Stretch` columns, `Monthly` with 12 periods Jan–Dec across columns `B:M` + `FY Total` in `N`, and `Summary`).

#### Beat 1 — First switch to Notebook: smart auto-scaffold vs blank page
1. Maria opens `Budget.xlsx` in **Classic**, checks `Summary!B12` (`EBITDA = $290,311`, `10.0%` margin on `$2,889,851` revenue), and presses `Ctrl+Alt+3` to open **Notebook**.
2. **First-run empty state (feature discovery):** Because `Budget.xlsx` has no saved Notebook layout in its `customXml` part yet, the Stage presents a crisp **First-Run Scaffold Card** instead of a blank void:
   - Option A (default, `Enter`): **Draft from Workbook Structure** — DNA Calc's pattern inference has already recognized `Inputs` (12 assumptions + scenario switcher `Inputs!C2`), `Monthly` (12-period time series), and `Summary` (executive rollup), and shows a 3-item preview outline.
   - Option B: **Start Blank Notebook** — empty prose canvas where Maria inserts only the blocks she chooses via `/`.
3. Maria presses `Enter` to accept **Draft from Workbook Structure**. In 70 ms, the Stage populates in **Reading density** (`68 ch` measure, Source Serif 4 prose headings, Recursive Mono tabular numerals) with three collapsible live sections linked directly to `Budget.xlsx`'s real sheets—zero formulas duplicated or forked.

#### Beat 2 — Authoring live narrative prose and inline bound tokens
4. Maria clicks the top prose paragraph (entering `EDITING` state—single bit flips, border shifts to dual-stroke amber) and writes her executive takeaway for her director.
5. Whenever she wants a live workbook number inside prose, she types `=` (or presses `Ctrl+Shift+R` / `/ref`), triggering a lightweight inline autocomplete popover searching defined names, block titles, and A1 addresses. She types `=Summary!B6`, presses `Tab`, and DNA Calc renders an inline **Live Value Chip** `«$2,889,851»` tinted subtly with `green-ink` (`#2E6E5B`) and a hairline underline.
   - **Hover feedback:** Hovering `«$2,889,851»` shows a crisp tooltip: `Summary!B6 · =Monthly!N7 · Base scenario` and softly highlights the corresponding row in the Summary table below.
   - **No stale numbers:** Maria changes the inline **Scenario Control** bound to `Inputs!C2` from `Base` to `Stretch` (`①` in `M-04`). In **< 12 ms**, every chip inside her prose updates synchronously with a subtle 140 ms `green-500` fresh settle pulse: `«$2,889,851»` → `«$3,386,368»` (`+17.2%`), and EBITDA updates from `«$290,311»` to `«$322,626»` (`9.5%` margin). She flips it back to `Base` to finish writing.

#### Beat 3 — Copy/paste from Classic, inline formulas, and switching to Pipeline mode
6. Maria wants to show monthly gross margin progression alongside her commentary. She presses `Ctrl+Alt+1` (Classic), selects `Monthly!A4:N14`, presses `Ctrl+C`, presses `Ctrl+Alt+3` (Notebook), and presses `Ctrl+V` below her narrative.
7. **Smart cross-view Paste:** Instead of pasting dead static text, DNA Calc pastes a **Live Data Frame Block** linked to `Monthly!A4:N14` and shows an unobtrusive corner paste-option chip for 5 seconds: `[Linked Live Block* | Static Snapshot | Markdown Table]` (accessible by pressing `Ctrl` right after paste, echoing Excel's Paste Options key!).
8. Maria clicks `+ Chart` on the Data Frame's hover toolbar (or types `/chart` via Command Palette `Ctrl+K`), binding an inline stacked Revenue & EBITDA spark-bar directly to `Monthly!B7:M7` and `Monthly!B15:M15`.
9. Next, Maria wants to check one live sensitivity without cluttering `Summary`: she types `/calc` and enters a new **Notebook Formula Cell**:
   - Name: `Cash_Runway_Gain` · Formula: `=(Summary!C12 - Summary!B12) / Inputs!E11` (Stretch vs Base EBITDA lift measured in months of loaded salary).
   - **Honest Excel backing:** A subtle right-margin chip says `Backed at Summary!B19 (named Cash_Runway_Gain)`. DNA Calc never creates "floating Notebook-only math"; every formula block writes an ordinary cell and defined name in the `.xlsx` workbook!
10. Maria clicks the **Layout Toggle** `[Vertical | Pipeline]` (`②` in `M-04`, shortcut `Alt+P`) to preview the budget left-to-right (`Inputs → Monthly → Summary`) for a screen-share walkthrough, then returns to `Vertical` for her director's PDF/readout.

#### Mockup M-04 — `Budget.xlsx` in Notebook (live prose chips, scenario switcher, data frame, inline chart)

```text
+----------------------------------------------------------------------------------------------+
| [==] Budget.xlsx     [Classic|Inspector|Notebook*|Map|App]  [Lens]   | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Summary!B12      v | [+ Prose] [+ Formula] [+ Link Range] [+ Chart]  | ② [Vertical*|Pipeline]|
+----------------------------------------------------------------------------------------------+
|                                                                                              |
|   FY2027 Operating Plan — Executive Brief                                                    |
|   ════════════════════════════════════════════════════════════════════════════════════════   |
|   ① Active Scenario (Inputs!C2):  [ ● Base | ○ Stretch ]   (live workbook cell)              |
|                                                                                              |
|   Under the «Base» scenario, FY2027 revenue reaches ③ «$2,889,851» (MRR exits Dec at         |
|   «$248,017», up from «$185,000» in Jan). Gross margin holds steady at «78.0%», yielding     |
|   EBITDA of «$290,311» («10.0%» margin) and Net Income of «$229,346» after «21.0%» tax.      |
|   Switching to Stretch adds «+$496,517» top line but raises hiring to 2/qtr, holding         |
|   EBITDA margin at «9.5%».                                                                   |
|                                                                                              |
|   ┌─ ④ KEY ASSUMPTIONS (linked: Inputs!B4:E15 · 6 of 12 pinned) ──────── [Show all 12] ───┐  |
|   | Opening_MRR      $185,000  | Monthly_Growth  [  4.5% ]  | Logo_Churn      [  1.8% ]   |  |
|   | COGS_Pct         [ 22.0% ] | Headcount_Start [    10 ]  | Marketing_Pct   [ 18.0% ]   |  |
|   └───────────────────────────────────────────────────────────────────────────────────────┘  |
|                                                                                              |
|   ┌─ ⑤ MONTHLY TRAJECTORY (linked: Monthly!A4:N15 · Data Frame + Chart) ── [Classic ↗] ───┐  |
|   | Metric         Jan       Feb       Mar       ...       Dec       FY Total    Trend    |  |
|   | Total_Rev   $207,200  $212,794  $218,540     ...    $277,779  $2,889,851   ▁▂▃▃▄▅▆▇█  |  |
|   | Gross_Prof  $161,616  $165,979  $170,461     ...    $216,668  $2,254,084   ▁▂▃▃▄▅▆▇█  |  |
|   | EBITDA       $19,320   $20,641   $22,008     ...     $31,412    $290,311   ▂▃▃▄▅▅▆▇█  |  |
|   └───────────────────────────────────────────────────────────────────────────────────────┘  |
|                                                                                              |
|   ┌─ ⑥ Cash_Runway_Gain  (backed: Summary!B19) ─────────────────────── [fx Explorer] ─────┐  |
|   |  = (Summary!C12 - Summary!B12) / Inputs!E11          =>   3.51 months                 |  |
|   └───────────────────────────────────────────────────────────────────────────────────────┘  |
|                                                                                              |
+----------------------------------------------------------------------------------------------+
| [Inputs][Monthly][Summary* (Notebook)]   | ✓ Clean · 142 fmls · 4 ms | Reading 100%          |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Live Scenario Switcher directly edits `Inputs!C2` (an ordinary Excel cell) and recalculates all prose chips, tables, and charts synchronously. **②** Vertical reading page vs horizontal left-to-right Pipeline mode (`M-14`). **③** Live Value Chips (`«$2,889,851»`) embed live Excel values inside Source Serif 4 narrative prose; hovering highlights their source block. **④** Editable assumption cards map directly to `Inputs!C4:C15`—changing `Monthly_Growth` to `5.0%` here edits `Inputs!C5` in the `.xlsx` file. **⑤** Linked Data Frame summarizes wide time series cleanly with tabular numerals and inline sparklines (`[Classic ↗]` jumps straight to `Monthly!A4:N15`). **⑥** New Notebook formula cell explicitly displays its real backing Excel coordinate (`Summary!B19`) so Excel opens and evaluates it identically.

#### Beat 4 — Undo across views, exporting to Word/Email, and Accessibility
11. Maria edits `Monthly_Growth` inside the Notebook card from `4.5%` to `5.5%`, then switches to **Classic** (`Ctrl+Alt+1`) to inspect `Monthly!C5`. Deciding `5.5%` is too aggressive, she presses `Ctrl+Z` *while still in Classic*.
    - **Cross-view Undo:** `Inputs!C5` reverts from `5.5%` to `4.5%`; `Monthly!C5` immediately recalculates back to `$189,995`, and a subtle Strip toast reads `Undid [Model]: Inputs!C5 -> 4.5% (edited in Notebook)`.
12. Maria switches back to Notebook (`Ctrl+Alt+3`), clicks `Save` (`Ctrl+S`—saving `Budget.xlsx` with her Notebook prose and layout cleanly stored in `customXml`), selects the executive section, and presses `Ctrl+C` to paste into Outlook/Word: the clipboard carries clean rich HTML + plain Markdown with resolved formatted values and inline SVG charts—never `#REF!` or broken formula text.
13. **Accessibility:** At 200% browser/OS zoom, Notebook prose reflows smoothly at `100%` viewport width without horizontal scrolling; Data Frames gain clean keyboard-focusable horizontal scrollbars; every live value chip exposes `aria-label="FY2027 revenue, 2,889,851 dollars, formula cell Summary B6"`.

> **What an Excel user notices as different—and why it is a win:** Instead of screenshotting Excel tables into Word or PowerPoint and praying nobody changes an assumption at 11 PM, Maria writes the board memo *directly on top of the live `.xlsx` file*—and when her director opens `Budget.xlsx` in Excel, every single number and her new `Cash_Runway_Gain` formula work unchanged.

---

### Storyboard 2.3 — Sam traces why one number on `Sales`' `Report` is wrong

**Cast:** Sam (reviewer / auditor) · **Workbook:** `Sales.xlsx` (`Orders` sheet holding an Excel Table `Orders` of 25,000 rows `A1:K25001`; `Report` sheet aggregating it via `SUMIFS`, `XLOOKUP`, `FILTER`, and a PivotTable).

#### Beat 1 — Spotting the discrepancy and entering Inspector with anchor preserved
1. Sam opens `Sales.xlsx` in **Classic** on the `Report` sheet. Everything looks calm—no `#REF!`, no `#VALUE!`, no red cells anywhere. Yet Sam's auditor eye catches a reconciliation mismatch:
   - `Report!D8` (**Q3 EMEA Revenue**) shows **`$412,680`**.
   - Directly below, the PivotTable (`Report!A18:E24`) reports **Q3 EMEA = `$531,100`**, and the reconciliation check cell `Report!D14` (`=D8 - GETPIVOTDATA(...)`) flags **`-$118,420`** in amber conditional formatting.
2. Sam clicks `Report!D8` and presses `Ctrl+Alt+2` to open **Inspector** (`M-05`).
3. **View switch continuity (< 50 ms):** `Report!D8` stays anchored in the Unified Address Box, ringed by the amber Continuity Halo in Inspector's **Trace Selected** canvas.

#### Beat 2 — Phyphox-style causal trace + Formula Lens + Structural Coverage Probe
4. Inspector shows `Report!D8`'s live **Causal Strip**:
   - **Raw Excel formula:** `=SUMIFS(Orders!$K$2:$K$20000, Orders!$D$2:$D$20000, $A8, Orders!$C$2:$C$20000, D$4)`
   - **Formula Lens (`Alt+L` toggled ON by Sam):** Displays the exact same calculation semantically structured with `LET` labels and explicit row bounds (`②` in `M-05`):
     ```text
     LET(amounts, Orders!$K$2:$K$20000 [19,999 of 25,000 rows!],
         regions, Orders!$D$2:$D$20000,
         qtrs,    Orders!$C$2:$C$20000,
         SUMIFS(amounts, regions, "EMEA", qtrs, "Q3"))
     ```
5. **The diagnostic breakthrough (`③` in `M-05`):** Unlike Excel—which sees zero syntax errors and stays silent—Inspector's **Structural Anomaly Probe** lights up an unmistakable **amber-ink warning card** (`⚠ Partial Table Coverage`):
   - *"This formula references `Orders!$K$2:$K$20000` (19,999 rows), which stops short of Table `Orders` (`A1:K25001`, 25,000 rows). **5,001 trailing rows are excluded** (`Orders!20001:25001`)."*
   - *"Matched rows inside excluded slice (`Region="EMEA"`, `Quarter="Q3"`): **493 rows · Sum = `$118,420.00`**."*
6. **Downstream blast radius (`④` in `M-05`):** On the right side of the Trace canvas, Sam immediately sees the domino effect—`Report!D8` doesn't just understate revenue:
   - `Report!F8` (`Rebate Tier` via `=XLOOKUP(D8, $J$5:$J$8, $K$5:$K$8, "Tier 1", -1)`) dropped EMEA from **Tier 3 (4%, ≥$500k)** to **Tier 2 (2%)**.
   - `Report!G8` (`Partner Rebate`) is consequently understated by **`$12,990`** (`$8,254` instead of `$21,244`).
   - Meanwhile `Report!E8` (`Top EMEA Rep`), written with `FILTER(Orders[Rep], ...)` using structured table refs, already read all 25,000 rows—confirming the inconsistency!

#### Mockup M-05 — `Sales.xlsx` in Inspector diagnosing `Report!D8` (Coverage Probe + Formula Lens)

```text
+----------------------------------------------------------------------------------------------+
| [==] Sales.xlsx      [Classic|Inspector*|Notebook|Map|App]  [Lens*①] | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Report!D8        v | [Health] [Trace Selected*] [Calc Order] [Live / External] | [fx Explore]|
+----------------------------------------------------------------------------------------------+
| ② ACTIVE TARGET: Report!D8 (Q3 EMEA Revenue) · Value: $412,680 · Calc: 1.8 ms · Clean        |
|   Formula Lens (ON · Alt+L):                                         [Show Raw Excel Formula]|
|   LET( amounts = Orders!$K$2:$K$20000  ⚠ 19,999 / 25,000 table rows                          |
|        regions = Orders!$D$2:$D$20000  ⚠ 19,999 / 25,000 table rows                          |
|        qtrs    = Orders!$C$2:$C$20000  ⚠ 19,999 / 25,000 table rows                          |
|        SUMIFS(amounts, regions, $A8 ["EMEA"], qtrs, D$4 ["Q3"]) )                            |
+------------------------------------+---------------------------------------------------------+
| ③ STRUCTURAL ANOMALY PROBE         | ④ CAUSAL GRAPH (Precedents -> [[Report!D8]] -> Deps)    |
|                                    |                                                         |
| ⚠ PARTIAL TABLE COVERAGE           |  [Orders!$K$2:$K$20000]  ──┐                            |
|   Ref stops at row 20,000.         |    19,999/25,000 rows ⚠    │                            |
|   Table `Orders` ends row 25,001.  |  [Report!$A8 = "EMEA"]   ──┼──> [[Report!D8]]           |
|                                    |  [Report!D$4 = "Q3"]     ──┘      $412,680              |
|   Excluded slice (20001:25001):    |                                     │                   |
|   • 5,001 total rows excluded      |          ┌──────────────────────────┼───────────────┐   |
|   • 493 match EMEA + Q3            |          ▼                          ▼               ▼   |
|   • Excluded Sum: +$118,420.00     |  [Report!D14 Check]       [Report!F8 Tier]  [Report!D12]|
|   • Matches D14 variance exactly!  |   -$118,420 (Pivot diff)   "Tier 2" (2%)     Total Q3   |
|                                    |                            (XLOOKUP -1)     $1,904,210  |
| ⑤ PREVIEW FIX (uncommitted)        |                                 │                       |
|   Use structured table refs:       |                                 ▼                       |
|   SUMIFS(Orders[Amount], ...)      |                           [Report!G8 Rebate]            |
|   D8: $412,680 -> $531,100         |                            $8,254 -> $21,244 (if fixed) |
|   D14: -$118,420 -> $0             |                                                         |
|   [Test in Explorer] [Apply Fix]   |  [Copy Audit Trace (Md)]  [Inspect Excluded 493 Rows ↗] |
+------------------------------------+---------------------------------------------------------+
| [Orders (25,000 rows)][Report*]    | ✓ Clean · 18 ms recalc | ⚠ 1 anomaly | Working 100%     |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Formula Lens (`Alt+L`) active in Mast—rewrites raw references into readable `LET` structure with live evaluated argument chips (`["EMEA"]`, `["Q3"]`) without altering the workbook formula. **②** Header readout shows exact value, recalc duration (`1.8 ms`), and quick toggle back to raw Excel text. **③** Structural Anomaly Probe detects the hardcoded `$20000` ceiling inside a 25,000-row Excel Table and computes the exact excluded slice (`493` matching rows, `$118,420`). **④** Downstream graph reveals silent secondary corruption (`F8` Rebate Tier and `G8` Partner Rebate). **⑤** Preview Fix calculates speculative deltas across the preview seam *before* Sam commits anything; `[Inspect Excluded 493 Rows ↗]` opens `Orders` filtered to rows `20001:25001`.

#### Beat 3 — Verifying the excluded rows, applying the fix, and exporting the audit trail
7. Before touching the formula, Sam clicks `[Inspect Excluded 493 Rows ↗]`. DNA Calc jumps to `Orders` in **Classic**, scrolled to row `20001` with a non-destructive diagnostic highlight (NOT mutating Excel's AutoFilter!) showing the newly imported enterprise orders from `2026-09-14` onward.
8. Sam presses `Alt+←` (Back navigation) to return to Inspector, checks the **Preview Fix** (`⑤`), and clicks `[Apply Fix]` (or tests it first in **Formula Explorer** via `Ctrl+E`).
9. **Calc feedback (18 ms):** `Report!D8` commits `=SUMIFS(Orders[Amount], Orders[Region], $A8, Orders[Quarter], D$4)`—and Inspector notes *"Same `$20000` truncation found in 3 sibling cells (`Report!D6:D9`); apply to all 4?"* Sam confirms. In 18 ms, `D8` flips to `$531,100` with a 150 ms `green-500` fresh settle, `F8` updates to `"Tier 3"`, `G8` updates to `$21,244`, and `D14` drops to `$0`.
10. **Undo & Audit Copy:** Pressing `Ctrl+Z` cleanly reverts all 4 sibling cells in one atomic step (`Undid [Model]: Structured ref fix on Report!D6:D9`), and `Ctrl+Y` restores it. Sam clicks `[Copy Audit Trace (Md)]` to paste the root-cause findings, before/after values, and formula diff directly into the audit ticket.
11. **Accessibility:** Every causal graph node is keyboard-traversable (`←/→` walks upstream precedents / downstream dependents; `↑/↓` cycles siblings); screen reader announces live recalculation deltas via `aria-live="polite"`: *"Report D8 updated from 412,680 to 531,100 dollars. Variance D14 is now 0."*

> **What an Excel user notices as different—and why it is a win:** Excel's *Trace Precedents* draws fragile blue arrows that disappear on edit, cannot cross into tables meaningfully, and never warn that `$K$2:$K$20000` missed 5,001 new rows. Inspector turns auditing from detective guesswork into a live instrument—and the fix is plain structured-reference Excel.

---

### Storyboard 2.4 — Lee builds `Stock` on the Model Map, then checks it in Classic

**Cast:** Lee (systems engineer) · **Workbook:** `Stock.xlsx` (new workbook built from scratch: inventory with inflow and outflow, 3-day reorder delay line, deliberate circular self-reference solved by Excel iterative calculation, and a Form Control slider for `Supply.ReorderPoint`).

#### Beat 1 — First-run empty state on Model Map & creating linked-name boxes
1. Lee presses `Ctrl+N` (New Workbook), then `Ctrl+Alt+4` to switch directly into **Model Map**.
2. **Empty state:** Instead of an intimidating blank canvas, the paper Stage shows a calm centered prompt with three one-click starters and keyboard hints:
   - `[+ Input Box (I)]` · `[+ Calc / State Box (B)]` · `[+ Group Frame (G)]` · `[+ Form Control Slider (S)]`
   - Subtle footnote: *"Every box creates a standard Excel defined name on sheet `StockModel`. Excel opens and recalculates it unchanged."*
3. Lee presses `G` to draw three composite group frames—`Demand`, `Supply`, and `Inv`—and creates boxes inside them by double-clicking or pressing `I`/`B` (each box auto-allocates a tidy labeled row on backing sheet `StockModel` and registers a workbook-scoped dotted defined name per `LINKED_BOXES_SKIN.md`):
   - `Demand.Rate` (`StockModel!B4`, literal `85`).
   - `Supply.LeadTime` (`B7`, literal `3`), `Supply.ReorderPoint` (`B8`, literal `260`), `Supply.OrderQty` (`B9`, literal `400`).
   - `Inv.Initial` (`B16`, literal `350`), `Inv.Outflow` (`B17`), `Inv.OnHand` (`B18`), `Inv.Shortage` (`B19`).
   - `Supply.Pipeline` (`B10`), `Supply.Inflow` (`B11`), `Supply.Position` (`B12`), `Supply.Order` (`B13`), plus `Sim.Reset` (`B2`, `FALSE`) and `Sim.Tick` (`B3`).
4. **Wiring rewrites real Excel formulas:** Lee drags from the right output port of `Demand.Rate` to the left input port of `Inv.Outflow` (keyboard equivalent: focus `Demand.Rate`, press `W` for Wire, arrow to `Inv.Outflow`, press `Enter`). A smooth teal Bezier wire snaps with magnetic port feedback; inside `Inv.Outflow`, Lee types `=MIN(Demand.Rate, OnHand + Supply.Inflow)`. The **Formula-Name Interpreter** resolves local short names (`OnHand` → `Inv.OnHand`) via the parsed formula AST and writes `=MIN(Demand.Rate, Inv.OnHand + Supply.Inflow)` directly into `StockModel!B17`.
5. **Clipboard interop:** Lee copies a 3-column initial-pipeline vector (`0, 0, 0`) from a text note and pastes (`Ctrl+V`) onto `Supply.Pipeline`—Model Map previews the dynamic array spill `{0, 0, 0}` right inside the box face.

#### Beat 2 — Warning & refusal moment: closing the deliberate circular reference
6. Now Lee builds the inventory accumulator. Inside `Inv.OnHand`, Lee drags a feedback wire from `Inv.OnHand`'s right output port back into its own left input port (to compute `OnHand⁻ + Inflow - Outflow` across ticks).
7. **Preview Seam refusal (`②` in `M-06`):** Because Excel workbooks start with *Iterative Calculation = OFF*, DNA Calc **does not silently fail or silently flip workbook settings**. The ghost loop wire pauses over the box in dashed `red-ink` + `amber` with an inline action popover right at the drop point:
   - `↻ Circular reference detected on Inv.OnHand`
   - `Excel iterative calculation is currently OFF. Without it, this loop evaluates to 0 with a circular warning.`
   - `[Enable Iteration: 1 pass/tick (Ticks Director)]`  `[Custom Iteration Settings…]`  `[Esc: Cancel Wire]`
8. Lee clicks `[Enable Iteration: 1 pass/tick (Ticks Director)]`. Three exact things happen:
   - Standard Excel workbook calculation properties update (`Iterative = True`, `MaxIterations = 1`, `MaxChange = 0.001`).
   - The ghost loop snaps cleanly into a Strand-styled dashed feedback wire arched over the box carrying a crisp **`z⁻¹`** delay chip (`③` in `M-06`), and the Bridge switches its Director pill from `Recalc` to **`Ticks · 1 iter/step`** (`①`), surfacing `[▶ Run]`, `[Step (F9)]`, and `[Reset]` controls (each step is literally one workbook recalculation!).
   - Lee wires `Supply.Order → Supply.Pipeline` with the same `z⁻¹` delay (`=IF(Sim.Reset, SEQUENCE(1,Supply.LeadTime,0,0), HSTACK(DROP(Supply.Pipeline,,1), Supply.Order))`).
9. **Adding an Excel Form Control Slider:** Lee presses `S` (or `+ Slider`), binds it to `Supply.ReorderPoint` (`min=100`, `max=500`, `step=10`). DNA Calc creates a genuine OOXML Form Control slider on `StockModel` linked to `$B$8` and docks a Strand-styled interactive slider widget in the right Parameter Rail (`④` in `M-06`) as well as on the sheet.

#### Mockup M-06 — `Stock.xlsx` Model Map mid-wire Preview Seam (circular reference refusal → `z⁻¹` delay)

```text
+----------------------------------------------------------------------------------------------+
| [==] Stock.xlsx      [Classic|Inspector|Notebook|Map*|App]  [Lens]   | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Inv.OnHand       v | Stock › Top | ① [Ticks · 1 iter] [▶ Run][Step F9][Reset] | [Presented*] |
+----------------------------------------------------------------------+-----------------------+
|                                                                      | ④ PARAMETERS & INSPECT|
|  ┌─ Demand ──────────┐            ┌─ Inv ─────────────────────────┐  |                       |
|  | [Demand.Rate]     |            |        ┌───[ z⁻¹ ]───┐ ③      |  | Supply.ReorderPoint   |
|  |  85 /day          |----------->|        ▼             │        |  |   260 units ($B$8)    |
|  └───────────────────┘            | [Inv.Outflow]   [[Inv.OnHand]]|  |   100 ──●────── 500   |
|                                   |  85              265  (state) |  |   (Excel Form Slider) |
|  ┌─ Supply ───────────────────┐   |  min(Rate,       OnHand⁻ +    |  |-----------------------|
|  | [ReorderPoint] [OrderQty]  |   |   OnHand⁻+In)    In - Out     |  | SELECTED ACTOR        |
|  |  260 (slider)   400        |   |                               |  | Inv.OnHand            |
|  |       │            │       |   | ② ╔═════════════════════════╗ |  | Kind: State (z⁻¹)     |
|  |       ▼            ▼       |   |   ║ ↻ Circular ref detected ║ |  | Backed: StockModel!B18|
|  | [Supply.Order]  400        |   |   ║ Iteration is OFF.       ║ |  |                       |
|  |  Pos < RP ? OQ : 0         |   |   ║ [Enable 1 iter/tick*]   ║ |  | Excel formula:        |
|  |       │ (z⁻¹ delay)        |   |   ║ [Settings..] [Esc Undo] ║ |  | =IF(Sim.Reset,        |
|  |       ▼                    |   |   ╚═════════════════════════╝ |  |  Inv.Initial,         |
|  | [Supply.Pipeline] {0,0,400}|   └───────────────────────────────┘  |  Inv.OnHand +         |
|  |  shift(Pipe⁻) <- Order⁻    |                                      |  Supply.Inflow -      |
|  |       │                    |   ┌─ Live Plotter (Tick 0..12) ───┐  |  Inv.Outflow)         |
|  |       ▼                    |   | 450│ ╲     ╱╲                 |  |                       |
|  | [Supply.Inflow]  0  ───────┼──>| 260│--╲---╱--╲-- (RP=260)     |  | Reads: Sim.Reset,     |
|  └────────────────────────────┘   |   0└───╲─╱────╲────────────── |  |  Inv.Initial, Inflow, |
|                                   └───────────────────────────────┘  |  Outflow, OnHand⁻     |
+----------------------------------------------------------------------+-----------------------+
| [StockModel (Map)*]     | ↻ Circular preview · Iteration pending     | Working 100% L3       |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Bridge displays breadcrumbs, the Excel-backed **Ticks Director** pill (`Iterative=True, MaxIter=1`), playback buttons (`Step` invokes standard `F9`), and `Presented` vs `Excel formulas` toggle (echoing `names-in-motion.html`, restyled strictly in Strand petrol/teal/amber). **②** Inline **Preview Seam** popover intercepts the self-loop before commit and turns Excel's iterative calculation setting into a single clear decision. **③** Accepted self-references render with a dashed feedback loop and `z⁻¹` delay chip. **④** Right rail surfaces the real Excel Form Control slider (`$B$8`) and exact backing Excel formula for `StockModel!B18`.

#### Beat 3 — Live topological ripple, switching to Classic, and verifying Excel parity
10. Lee clicks `[Step F9]` six times (or clicks `[▶ Run]` at 2 ticks/s) while dragging the `Supply.ReorderPoint` slider down from `260` to `180`.
    - **Motion with meaning:** On each tick, Strand's sanctioned **Recalc Ripple** (140 ms) travels in topological dependency order (`Sim.Tick → Supply.Inflow → Supply.Pipeline → Inv.Outflow → Inv.OnHand → Supply.Position → Supply.Order`) in amber (`#FFB301`)—showing the exact causal wave. When `ReorderPoint = 180`, `Inv.OnHand` dips to `0` on Tick 8 and `Inv.Shortage` spikes to `55` (`amber` highlight in the Plotter). Lee drags the slider back to `260` and clicks `[Reset]` (`Sim.Reset` pulses `TRUE` then `FALSE` via one recalculation).
11. **Checking the truth in Classic (`Ctrl+Alt+1`, `M-07`):** Lee presses `Ctrl+Alt+1` to verify what Excel will actually see.
    - **View switch continuity (< 50 ms):** `Inv.OnHand` stays selected—the Continuity Halo pulses on **`StockModel!B18`**, the Formula Bar displays the exact standard Excel formula `=IF(Sim.Reset, Inv.Initial, Inv.OnHand + Supply.Inflow - Inv.Outflow)`, the real Excel Form Control slider sits cleanly anchored over `D8:F8`, and `StockModel!B10:D10` shows the live dynamic array spill `{0, 0, 400}` with its crisp teal spill border.
12. **Undo across views:** In Classic, Lee deletes `Supply.Order` by accident (`Del`), sees `Supply.Pipeline` go flat, and hits `Ctrl+Z`—restoring both the formula on `B13` and its wire on Model Map atomically.
13. **Accessibility:** Model Map supports full keyboard graph navigation (`Tab` enters groups, `↑/↓/←/→` steps along topological wires, `W` opens an accessible "Connect output port to..." picker, `Space` steps one tick). `prefers-reduced-motion` replaces wire dash animations with instant crisp amber border highlights.

#### Mockup M-07 — `Stock.xlsx` switched to Classic (`Ctrl+Alt+1`): exact Excel backing sheet & Form Slider

```text
+----------------------------------------------------------------------------------------------+
| [==] Stock.xlsx      [Classic*|Inspector|Notebook|Map|App]  [Lens]   | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Inv.OnHand (B18) v | fx =IF(Sim.Reset, Inv.Initial, Inv.OnHand+Supply.Inflow-Inv.Outflow) ①  |
+-----+----------------------+----------+----------+----------+----------+---------------------+
|     |          A           |    B     |    C     |    D     |    E     |          F          |
+-----+----------------------+----------+----------+----------+----------+---------------------+
|  2  | Sim.Reset            | FALSE    |          |          |          |                     |
|  3  | Sim.Tick             |       12 |          |          |          |                     |
|  4  | Demand.Rate          |       85 | units/d  |          |          |                     |
|  7  | Supply.LeadTime      |        3 | days     |          |          |                     |
|  8  | Supply.ReorderPoint  |      260 | units    | ② [ 100 ───●─────── 500 ] (Slider -> $B$8)|
|  9  | Supply.OrderQty      |      400 | units    |          |          |                     |
| 10  | Supply.Pipeline      | ┌─── 0   |     0    |   400 ───┐ ③ (3-cell dynamic spill)       |
| 11  | Supply.Inflow        |        0 |          |          |          |                     |
| 12  | Supply.Position      |      665 |          |          |          |                     |
| 13  | Supply.Order         |        0 |          |          |          |                     |
| 16  | Inv.Initial          |      350 |          |          |          |                     |
| 17  | Inv.Outflow          |       85 |          |          |          |                     |
| 18  | Inv.OnHand           |[[  265 ]]④          | ↻ Iter   |          |                     |
| 19  | Inv.Shortage         |        0 |          |          |          |                     |
+-----+----------------------+----------+----------+----------+----------+---------------------+
| [StockModel* (Map pref)]   | ✓ Converged · Iterative (1 max, Δ 0.001) | Working 100%         |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Unified Address Box shows both defined name and cell coordinate (`Inv.OnHand (B18)`) with the exact plain-Excel formula written by Model Map wiring. **②** Standard OOXML Form Control slider linked to `$B$8` works identically in Excel and Model Map. **③** `Supply.Pipeline` spills cleanly across `B10:D10` with Strand's teal dynamic-array frame. **④** Continuity Halo settles on `B18` (`265`) with a subtle `↻ Iter` overlay pill; Strip confirms Excel's iterative calc settings.

> **What an Excel user notices as different—and why it is a win:** Building a feedback loop in Excel normally triggers a confusing blue circular-reference warning bar and forces a dig into *File › Options › Formulas*. DNA Calc makes circular dynamics legible as a `z⁻¹` loop and puts an honest switch right where the loop closes—while producing a 100% clean `.xlsx` file.

---

### Storyboard 2.5 — Jo runs `Loan` as an App, tries scenarios, and prints the result

**Cast:** Jo (manager who uses models built by others) · **Workbook:** `Loan.xlsx` (inputs block `C3:C7`, 360-row amortization schedule using `PMT`, `IPMT`, `PPMT` and running balance, balance-vs-interest chart, saved with preferred view `App` in `customXml`).

#### Beat 1 — Opening straight into App view (consumer clarity, zero grid intimidation)
1. Jo double-clicks `Loan.xlsx` received from Finance. Because `Loan.xlsx` declares `App` as its preferred view in `customXml`, it opens directly in **App** (`M-08`, Reading density) in **< 140 ms**—no grid lines, no formula bar, no risk of Jo accidentally typing over `=IPMT(...)` in row 47.
2. **Mast restraint + honest escape hatch (`①` in `M-08`):** The Mast stays calm in petrol-800 showing only the workbook title, `[Print / PDF]`, `[Reset]`, and a clear unobtrusive **`[View: App ▾]`** switcher (so Jo—or Maria auditing Jo's copy—can always switch to Classic `Ctrl+Alt+1` or Inspector `Ctrl+Alt+2` to verify formulas; nothing is hidden or proprietary).
3. Jo sees three crisp sections on daylight paper:
   - **Left Control Panel (`②`):** Five validated input controls bound directly to `Loan!C3:C7` (`Principal $420,000`, `Annual Rate 6.25%`, `Term 30 yrs`, `Extra Monthly Payment $0`, `Start Date 2027-01-01`) plus Scenario Preset chips (`Base 6.25%` · `+$300/mo Extra` · `Refi 5.50%` · `+ Save Scenario`).
   - **Top KPI Strip (`③`):** Four tabular-numeral readout cards: **Monthly Outlay `$2,586.01`**, **Total Interest `$510,964`**, **Payoff Date `Dec 2056 (30 yr 0 mo)`**, **Total Paid `$930,964`**.
   - **Chart & Collapsible Schedule (`④`):** The workbook's actual Excel chart (Balance vs Cumulative Interest over 360 months, styled cleanly in Strand teal/petrol/amber) above the 360-row schedule **smartly grouped by Year (Years 1–30)** with expand chevrons `▸ Year 1 (12 months)` so Jo isn't confronted with a 360-row scroll wall.

#### Beat 2 — Validation refusal, side-by-side scenario comparison, and fast recalculation
4. Jo types `-2` into the `Annual Rate` box by mistake and presses `Enter`.
   - **Honest validation refusal (`⑤` in `M-08`):** Respecting Excel's Data Validation rule on `Loan!C4` (`0.01% <= rate <= 25%`), the field outlines in `red-ink` with an inline non-modal refusal message—never a jarring system beep or modal popup: `⊗ Must be between 0.01% and 25.00% (Excel validation on Loan!C4). Value unchanged (6.25%). [Esc to clear]`.
5. Jo presses `Esc`, clicks **`[+ Compare Scenario]`**, names it **`+$300/mo Extra`**, and drags the `Extra Monthly Payment` slider to **`$300`** (or types `300` and presses `Enter`).
   - **Instant feel (< 12 ms):** The entire 360-row amortization schedule recalculates on every slider tick without stutter. A compact **Scenario Comparison Table** (`⑥` in `M-08`) pins **Base (`6.25%, +$0`)** beside **`+$300/mo Extra`** (and Jo adds a third chip **`Refi 5.50%`**):
     - **Base (`6.25%`, `+$0`):** `$2,586.01/mo` · Interest `$510,964` · Payoff `30 yr 0 mo`
     - **`+$300/mo Extra`:** `$2,886.01/mo` · Interest **`$368,169` (`-$142,795` saved)** · Payoff **`22 yr 10 mo` (`7 yr 2 mo` earlier)**
     - **`Refi 5.50%`:** `$2,384.71/mo` · Interest **`$438,496` (`-$72,468` saved)** · Payoff `30 yr 0 mo`
   - The chart overlays the active comparison curve as a dashed teal trajectory so Jo immediately sees the debt cliff move left by 7+ years.
   - *How scenarios stay pure Excel:* Saved scenario snapshots map onto Excel's standard Scenario Manager / `customXml` view presets; switching the active scenario simply writes `Loan!C3:C7` and recalculates normally!

#### Mockup M-08 — `Loan.xlsx` in App view (scenario comparison, validation refusal, annual schedule)

```text
+----------------------------------------------------------------------------------------------+
| [==] Loan.xlsx — Mortgage Explorer       ① [View: App ▾]   |  [Reset] [Copy Summary] [Print] |
+----------------------------------------------------------------------------------------------+
| Scenarios: [Base 6.25%] [+$300/mo Extra*] [Refi 5.50%] [+ Compare]  | Active -> Loan!C3:C7   |
+---------------------------------+------------------------------------------------------------+
| ② LOAN INPUTS                   | ③ KEY OUTCOMES (+$300/mo Extra vs Base)                    |
|                                 | ┌──────────────┐ ┌──────────────┐ ┌──────────────────────┐ |
| Principal (Loan!C3)             | | Monthly Paid | | Total Inter. | | Payoff Horizon       | |
| [ $420,000                   ]  | |  $2,886.01   | |  $368,169    | |  22 yr 10 mo         | |
|                                 | |  +$300 vs Bs | | ▼ -$142,795  | |  ▼ 7 yr 2 mo faster  | |
| Annual Rate (Loan!C4)           | └──────────────┘ └──────────────┘ └──────────────────────┘ |
| [ -2%                        ]  |                                                            |
| ⑤ ⊗ Must be 0.01%–25% (C4 rule) | ④ REMAINING BALANCE & CUMULATIVE INTEREST (Loan Chart 1)   |
|   Kept previous: 6.25%          | $500k│ ── Base Balance       ── +$300 Extra Balance        |
|                                 |      │ ╲╲                                                  |
| Term (Years) (Loan!C5)          | $250k│   ╲ ╲___            .... Interest (Base $511k)      |
| [ 30 ]  15 ─────────● 30        |      │     ╲   ╲___     ...   Interest (+$300 $368k)       |
|                                 |   $0 └──────╲──────╲───────────────────────────────────    |
| Extra Monthly (Loan!C6)         |      Yr 1   Yr 10  Yr 23 (Paid!)             Yr 30         |
| [ $300 ]  $0 ───●────── $1,000  |                                                            |
|                                 | ⑥ SCENARIO COMPARISON                                      |
| Start Date (Loan!C7)            | Scenario        Monthly    Total Int.  Saved      Payoff   |
| [ 2027-01-01                 ]  | Base 6.25%     $2,586.01   $510,964         —    30y 00m   |
|                                 | +$300 Extra*   $2,886.01   $368,169   $142,795   22y 10m   |
|                                 | Refi 5.50%     $2,384.71   $438,496    $72,468   30y 00m   |
|                                 |                                                            |
| [Undo Last Edit Ctrl+Z]         | AMORTISATION SCHEDULE (360 rows · grouped by year)         |
|                                 | ▼ Yr 1 (2027)  Paid $34,632 · Int $25,840 · Bal $411,208   |
|                                 |   Mo 1  Jan-27  Pmt $2,886  Int $2,188  Prn $699  $419,301 |
|                                 | ▶ Yr 2 (2028)  Paid $34,632 · Int $25,264 · Bal $401,840   |
+---------------------------------+------------------------------------------------------------+
| App View (Loan.xlsx)            | ✓ Clean · 360 rows · 7 ms          | Reading 100%          |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Clean consumer Mast retains an explicit `[View: App ▾]` switcher—never trapping the user away from Classic or Inspector. **②** Validated inputs bind 1:1 to `Loan!C3:C7` (tooltip reveals exact Excel cell). **③** Tabular-numeral KPI strip highlights savings in `green-ink` paired with downward arrow `▼`. **④** Live Excel chart updates on every slider step (< 12 ms), overlaying Base vs active scenario curves. **⑤** Inline validation refusal mirrors Excel's exact cell validation rule without a modal popup. **⑥** Side-by-side comparison table and year-grouped collapsible 360-row schedule keep dense schedules scannable.

#### Beat 3 — Printing a clean 2-page brief, Undo, and Accessibility
6. Jo clicks **`[Print]`** (`Ctrl+P`). Rather than printing 9 ugly landscape pages of raw grid columns split across page breaks, App view opens a live **Print Preview Sheet**:
   - **Page 1:** Title header (`Loan.xlsx — Scenario Decision Brief`), Inputs & Scenario Comparison Table, and crisp vector Balance-vs-Interest Chart.
   - **Page 2:** 30-row **Annual Summary Table** (with a checkbox `[ ] Include all 360 monthly rows (adds 6 pages)` unchecked by default, transparently stating *"Showing 30 annual summaries of 360 schedule rows"* so nothing is silently omitted!).
7. Jo clicks `Save as PDF` (or prints), then clicks `[Copy Summary]` to paste the 3-row scenario table into an email, and tests `Ctrl+Z` to step back through her slider tweaks cleanly.
8. **Accessibility:** Full keyboard tab order moves logically from Scenario chips → Inputs → KPI summary (`aria-live="polite"` announces *"Monthly payment 2,886 dollars and 1 cent, total interest saved 142,795 dollars, payoff 22 years 10 months"*) → Expandable year rows (`Enter`/`Space` expands months). Contrast passes WCAG AA across `cockpit-light`, `cockpit-dark`, and `high-contrast`.

> **What an Excel user notices as different—and why it is a win:** Jo gets a safe, clean calculator and a 2-page executive printout instead of hunting for yellow input cells inside a fragile 360-row grid—yet the underlying `.xlsx` file needs zero VBA macros, zero web servers, and opens natively in Excel.

---

### Storyboard 2.6 — Ade loads an add-in in the standalone Formula Explorer, tests a function, then uses it from a cell in `Budget`

**Cast:** Ade (Excel add-in developer) · **Tool & Workbook:** Standalone **DNA Calc - Formula Explorer** (zero workbook loaded) → then embedded **Formula Explorer** inside `Budget.xlsx`.

#### Beat 1 — Empty state in standalone "DNA Calc - Formula Explorer" & loading an add-in
1. Ade launches the stripped desktop build **DNA Calc - Formula Explorer** directly from their terminal or app launcher.
2. **First-run empty state (`M-09`):** There is **no workbook, no sheet tabs, no grid**. The Strand cockpit frame holds:
   - **Mast:** OneCalc-style stripped badge (`dot-long` cyan+petrol), title `DNA Calc — Formula Explorer`, `[Load Add-in (.xll/.js)]`, `[Theme]`, `[Settings]`.
   - **Left Rail (Inputs & Function Library):** Empty **Local Test Inputs** table (`+ Add Input`, e.g., `x = 100`, `tbl = {1,2;3,4}`) and searchable built-in Excel function catalogue (`500+` functions + loaded add-ins).
   - **Center Stage:** Large Recursive Mono **Formula Editor** with placeholder ghost hint (`=SUM(1, 2, 3)  or load an add-in to test custom functions`) and four inspection tabs below: **`[Eval Tree*]` `[Number Format]` `[Cond. Format]` `[Add-in & Types]`**.
3. Ade clicks **`[Load Add-in]`** (or presses `Ctrl+Shift+A`) and picks their freshly compiled **`TaxPack.xll`** (an unsigned local dev build).
   - **Security warning (non-blocking, honest):** A `signal` orange (`#FF8001`) strip appears: `⚠ Unsigned local add-in: TaxPack.xll (3 functions: TAX.EFFRATE, TAX.BRACKET, TAX.CREDIT) · [Load in Dev Sandbox] [Cancel]`.
   - Ade clicks `[Load in Dev Sandbox]`. All three functions register immediately in the Function Library with their signature docs and `signal` add-in provenance tags.

#### Beat 2 — Testing arguments, catching a `#VALUE!` bug, and testing Number/Conditional Formatting
4. Ade creates three local inputs in the Left Rail: `ebitda = 290311`, `juris = "CA"`, `corp = TRUE`, and types in the Formula Editor:
   `=TAX.EFFRATE(ebitda, juris, corp)`
5. **Immediate live error diagnosis (`②` in `M-09`):** Within **3 ms**, the Result Banner displays **`⊗ #VALUE!`** (in `red-ink`), and the **`Eval Tree`** breaks open every argument and add-in return diagnostic:
   - `ebitda` → `290311` (`Number` ✓)
   - `juris` → `"CA"` (`Text` → **`⊗ Add-in diagnostic: Expected ISO-3166-2 region code e.g. "US-CA", got "CA"`**)
   - `corp` → `TRUE` (`Boolean` ✓)
6. Ade edits `juris` to `"US-CA"` right in the Left Rail. Instantly (`2.1 ms`), the Result Banner flips to `green-ink`: **`0.27984`** (`27.98%`, exact IEEE-754 value `0.2798412048...` visible in hover/type detail!).
7. Ade clicks the **`[Number Format]`** tab (`③` in `M-09`) to test how Excel displays their function's return value under custom format codes: typing `0.00%;"-"0.00%;"Zero"` renders live preview **`27.98%`** alongside edge-case test rows (`0`, `-0.05`, `#VALUE!`, `1.23456`), and Ade switches to **`[Cond. Format]`** to test `Cell Value > 0.25` rule triggering.
8. **Copy out:** Ade presses `Ctrl+Shift+C` (`Copy Excel Formula`), placing `=TAX.EFFRATE(ebitda, "US-CA", TRUE)` cleanly on the clipboard.

#### Mockup M-09 — Standalone `DNA Calc - Formula Explorer` testing `TaxPack.xll` (active `#VALUE!` -> live fix)

```text
+----------------------------------------------------------------------------------------------+
| [.==] DNA Calc — Formula Explorer (Standalone · No Workbook)   | [Load Add-in*] [Help] [Set] |
+----------------------------------------------------------------------------------------------+
| ▸ Add-in Loaded: TaxPack.xll (Dev Sandbox · unsigned) · 3 functions: TAX.EFFRATE, +2  [Rld]  |
+---------------------------+------------------------------------------------------------------+
| ① LOCAL TEST INPUTS       | ② FORMULA EDITOR                                 [Copy Ctrl+Sh+C]|
|   Name      Value   Type  | ┌──────────────────────────────────────────────────────────────┐ |
|   ebitda   290311   Num   | | =TAX.EFFRATE( ebitda, juris, corp )                          | |
|   juris    "CA"     Text  | └──────────────────────────────────────────────────────────────┘ |
|   corp     TRUE     Bool  |  RESULT: ⊗ #VALUE!  (2.1 ms · Add-in call · volatile: No)        |
|   [+ Add Input / Array]   |                                                                  |
|                           | [Eval Tree*] [Number Format] [Cond. Format] [Add-in & Types]     |
| ADD-IN FUNCTIONS (3)      |                                                                  |
| ▸ TAX.EFFRATE(inc,rg,c)   |  ▼ TAX.EFFRATE(ebitda, juris, corp)        =>  ⊗ #VALUE!         |
|   TAX.BRACKET(inc,rg)     |    ├─ arg 1  ebitda  ->  290311  (Number)               ✓ ok     |
|   TAX.CREDIT(inc,rd)      |    ├─ arg 2  juris   ->  "CA"    (Text)                 ⊗ ERR    |
|                           |    │    └─ Add-in msg: Expected "US-CA" (ISO-3166-2), got "CA"   |
| BUILT-IN SEARCH           |    └─ arg 3  corp    ->  TRUE    (Logical)              ✓ ok     |
| [ Search 512 functions ]  |                                                                  |
|                           | ③ QUICK FORMAT TEST (when juris -> "US-CA" => 0.2798412)         |
|                           |   Format: [ 0.00%;(0.00%);"—" ]  Preview:  27.98%                |
|                           |   CF Rule: [ > 0.25 -> amber-ink bold ]   Triggered: YES         |
+---------------------------+------------------------------------------------------------------+
| Standalone Sandbox · No file open | Add-in: TaxPack.xll (▸ signal) | Working 100%            |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Local Test Inputs replace workbook cells in the zero-workbook standalone build; supports scalars, dynamic arrays (`{10,20;30,40}`), and errors. **②** Live Eval Tree expands each argument with exact Excel coercion type and add-in diagnostic feedback (never modal like Excel's tiny *Evaluate Formula* box). **③** Built-in Number Format and Conditional Formatting preview lets Ade verify display parity before ever opening a workbook.

#### Beat 3 — Using `TAX.EFFRATE` inside `Budget.xlsx` via the embedded Formula Explorer
9. Satisfied, Ade opens `Budget.xlsx` in DNA Calc (Classic), navigates to `Summary!B13` (`Tax Rate`, currently `=Inputs!E15` = `21.0%`), and presses **`Ctrl+E`** (or clicks the `fx` button in the Bridge).
10. **Embedded Formula Explorer drawer (`M-10`):** The exact same Formula Explorer slides open docked to the right of Classic (38% width, resizable), **pre-populated from `Summary!B13`'s live workbook context**:
    - Inputs auto-bound to `Summary!B12` (`EBITDA = $290,311`), `Inputs!E15`, and available workbook names!
    - Ade enters `=TAX.EFFRATE(Summary!B12, "US-CA", TRUE)` inside the Explorer.
    - **Preview Seam before commit (`①` in `M-10`):** Before Ade clicks **`[Commit to Summary!B13]`**, Classic shows a live dashed preview on `Summary!B13` (`21.0% -> 27.98%`) AND live speculative ripple on `Summary!B14` (`Net Income: $229,346 -> $209,070`, `-$20,276`)—so Ade sees the workbook impact *before* touching the cell!
11. Ade clicks `[Commit to Summary!B13]` (or presses `Ctrl+Enter`). Cell `B13` commits cleanly with a `signal` orange add-in provenance tick (`▸`) noting dependency on `TaxPack.xll`, and `Esc` closes the Explorer drawer cleanly with focus anchored on `Summary!B13`.
12. **Accessibility:** `Eval Tree` implements standard WAI-ARIA `tree` semantics (`↑/↓` move nodes, `←/→` collapse/expand, `Enter` focuses an argument span in the Formula Editor). Keyboard-only users invoke, test, preview, and commit without ever touching a mouse.

#### Mockup M-10 — Embedded Formula Explorer drawer inside `Budget.xlsx` (`Summary!B13` preview seam)

```text
+----------------------------------------------------------------------------------------------+
| [==] Budget.xlsx     [Classic*|Inspector|Notebook|Map|App]  [Lens]   | [fx*][Ag][Ctrl+K][Prt]|
+----------------------------------------------------------------------------------------------+
| Summary!B13      v | fx =Inputs!E15  (Previewing Formula Explorer edit...)                   |
+-----------------------------------------------+----------------------------------------------+
|     |        A         |     B (Base)         | ② FORMULA EXPLORER · Summary!B13       [⤢][x]|
|-----+------------------+----------------------+----------------------------------------------|
|  6  | Total Revenue    |   $2,889,851         | =TAX.EFFRATE(Summary!B12, "US-CA", TRUE)     |
|  8  | Gross Profit     |   $2,254,084         |                                              |
| 11  | Total OpEx       |   $1,963,773         | Live Preview:  27.98%  (was 21.00%) · 1.9 ms |
| 12  | EBITDA           |     $290,311         | [Eval Tree*] [Fmt: 0.00%] [CF] [Add-in]      |
| 13  | Effective Tax    |[[ ▸ 27.98%  ]]①      |                                              |
|     |   (preview seam) |  (was 21.00%)        | ▼ TAX.EFFRATE(Summary!B12, "US-CA", TRUE)    |
| 14  | Net Income       |  ┄$209,070┄          |   ├─ Summary!B12 -> 290,311 (EBITDA)    ✓ ok |
|     |                  |  (was $229,346)      |   ├─ "US-CA"     -> Text                ✓ ok |
| 15  | Net Margin       |   ┄7.2%┄ (was 7.9%)  |   └─ TRUE        -> Logical             ✓ ok |
|     |                  |                      |                                              |
|     |                  |                      | Downstream impact (2 cells):                 |
|     |                  |                      | • Summary!B14 Net Income  -$20,276           |
|     |                  |                      | • Summary!B15 Net Margin  -0.7 pts           |
|     |                  |                      |                                              |
|     |                  |                      | [Esc Discard]    [Commit to B13 Ctrl+Enter]  |
+-----+------------------+----------------------+----------------------------------------------+
| [Inputs][Monthly][Summary*]  | Preview seam active (uncommitted) | Add-in: TaxPack.xll       |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Preview Seam shows both `Summary!B13` and its downstream dependents (`B14`, `B15`) in dashed speculative preview *before* committing the formula. **②** Embedded Formula Explorer reuses the exact standalone UI, pre-wired to `Summary!B13`'s workbook inputs, with `[⤢]` to pop out into a separate window if desired.

> **What an Excel user notices as different—and why it is a win:** Excel's *Evaluate Formula* dialog is modal, tiny, un-editable, and loses your place if you test an alternative. DNA Calc's Formula Explorer is a live workbench—identical standalone for add-in authors and embedded for analysts—with speculative impact preview before a single cell changes.

---

### Storyboard 2.7 — The agent proposes a change to `Budget`. Maria reviews it, accepts part of it, and undoes the rest

**Cast:** Maria + **AI Agent** · **Workbook:** `Budget.xlsx`.

#### Beat 1 — Maria prompts the agent; agent submits a Staged Proposal Batch (never silent overwrite)
1. Maria is reviewing `Budget.xlsx` in **Classic** (on `Summary`). Her CFO asked to find `+$70k` in FY2027 EBITDA without cutting headcount. Maria presses `Ctrl+I` to open the **Agent Drawer** and asks: *"Model a 5% seat-price increase starting in Q3 (Jul–Dec) and trim Marketing spend from 18% to 15% of revenue so we can see the EBITDA lift."*
2. **Feedback while working:** The Strip displays `◐ Agent drafting proposal (read-only snapshot)…`—Maria can keep scrolling `Budget` uninterrupted. The agent **never writes directly to workbook cells**.
3. **Preview Seam Proposal (`M-11`):** In 1.2 seconds, the agent returns a **Staged Proposal Batch** grouped into **2 semantic hunks** (addressing cells via the exact same unified grammar Maria uses):
   - **Hunk 1 — `Q3 Seat-Price Uplift (+5%)`** (7 cells: adds `Inputs!B16:E16` `Q3_Price_Uplift = 5.0%` + updates `Monthly!H5:M5` Jul–Dec MRR formula to `=G5*(1+Inputs!$E$5-Inputs!$E$6)*(1+IF(MONTH(H$4)=7,Inputs!$E$16,0))`).
     - *Speculative Impact:* Revenue `+$74,509` (`$2,889,851 → $2,964,360`), EBITDA **`+$58,117`** (`$290,311 → $348,428`).
   - **Hunk 2 — `Trim Marketing Spend (18% -> 15%)`** (1 cell: edits `Inputs!C14` from `18.0%` to `15.0%`).
     - *Speculative Impact:* Marketing OpEx `-$88,931`, EBITDA **`+$88,931`** (combined EBITDA: `$437,359`, `14.8%` margin).
   - **Refusal / Guardrail check:** Notice that if the agent had also attempted to edit a locked/protected cell or had introduced a `#REF!` error in speculative recalculation, DNA Calc would automatically quarantine that hunk with a **`⊗ Blocked: introduces #REF! in Summary!B14`** warning badge and disable `[Accept All]` until resolved.

#### Beat 2 — Reviewing across views, selective acceptance, and clean Undo
4. On the **Classic** Stage (`①` in `M-11`), every proposed cell renders inside Strand's **Preview Seam**: a dashed `teal-600` frame over an 8% petrol-teal diagonal-hatched fill with a `[⇄]` corner badge, displaying old value struck lightly beside speculative new value (`$290,311 → $437,359`).
5. **Cross-view review:** Maria presses `Ctrl+Alt+3` to peek at her **Notebook** write-up while the Proposal Batch is still staged! Her prose chips show live speculative preview pills (`«$290,311 -> $437,359»`) so she immediately reads how her board memo changes—zero committed mutations yet! She presses `Ctrl+Alt+1` to return to Classic.
6. Maria inspects **Hunk 1** (`Q3 Price Uplift`): she likes the clean `Inputs!E16` assumption and clicks **`[Inspect Formula Diff]`** to verify the exact Excel formula on `Monthly!H5` (confirming zero proprietary syntax). She clicks **`[Accept Hunk 1]`** (or presses `Alt+A`).
   - **Commit feedback:** Hunk 1's 7 cells settle into daylight paper with a 150 ms `green-500` fresh flash; `Summary!B12` commits to **`$348,428`** (`+$58,117`). An entry is written to the Undo stack: `[Agent ✓] Q3 Seat-Price Uplift (+5%) (7 cells)`.
7. Maria is unsure about **Hunk 2** (`Marketing 18% -> 15%`), so she clicks **`[Accept Hunk 2]`** briefly to test how Stretch looks in `Inputs!C2`, realizes `15%` starves Q4 growth, and presses **`Ctrl+Z`**.
   - **Exact surgical Undo:** `Ctrl+Z` reverts **only Hunk 2** (`Inputs!C14` returns to `18.0%`, `Summary!B12` settles cleanly back to **`$348,428`**), leaving Hunk 1 completely intact! (Had she declined Hunk 2 before accepting, clicking `[Reject Hunk 2]` or pressing `Esc` would discard the preview cleanly with zero Undo pollution.)
8. **Traceability:** Opening the compact **Audit Log** inside the Agent Drawer (`③` in `M-11`) shows a transparent session record: `16:14 · Agent Hunk 1 accepted by Maria (Inputs!B16:E16, Monthly!H5:M5) · Hunk 2 accepted then undone via Ctrl+Z`—exportable as plain text or cleared anytime, never hidden in binary engine state.

#### Mockup M-11 — AI Agent Staged Proposal Batch on `Budget.xlsx` (Preview Seam + selective hunk accept/undo)

```text
+----------------------------------------------------------------------------------------------+
| [==] Budget.xlsx     [Classic*|Inspector|Notebook|Map|App]  [Lens]   | [fx][Ag 1*][Ctrl+K]   |
+----------------------------------------------------------------------------------------------+
| Summary!B12      v | fx =Monthly!N15  | ⇄ Agent Batch: 1 Accepted · 1 Staged · [Accept][Esc] |
+-----------------------------------------------+----------------------------------------------+
|     |        A         |   B (Base)  | Δ Prev | ② AGENT REVIEW DRAWER                [Log][x]|
|-----+------------------+-------------+--------+----------------------------------------------|
|  6  | Total Revenue    |  $2,964,360 | +$74k✓ | Prompt: "Model +5% Q3 seat price & trim Mktg |
|  8  | Gross Profit     |  $2,312,201 | +$58k✓ |          from 18% to 15% to see EBITDA lift" |
| 10  | Payroll          |  $1,269,600 |    $0  |----------------------------------------------|
| 11  | Cloud            |    $174,000 |    $0  | ✓ HUNK 1: Q3 Seat-Price Uplift (+5%) [Done]  |
| 12  | Marketing        | ┄ $444,654┄ | -$89k⇄ |   Accepted (7 cells: Inputs!16, Monthly!H5:M)|
|     |  (Preview Seam①) | (was $534k) |        |   EBITDA +$58,117 · [Undo Hunk 1 Ctrl+Z]     |
| 14  | EBITDA           |[[┄$437,359┄]]+$147k⇄ |----------------------------------------------|
|     |                  | (now $348k) |        | ⇄ HUNK 2: Trim Marketing (18% -> 15%)        |
| 15  | EBITDA Margin    |   ┄14.8%┄   | +4.8pt |   1 cell: Inputs!C14  18.0% -> 15.0%         |
|     |                  | (now 11.8%) |        |   Formula diff: constant edit (0 errors)     |
| 16  | Net Income       |  ┄$345,514┄ | +$70k⇄ |   Impact: OpEx -$88,931 · EBITDA +$88,931    |
|     |                  |             |        |   [Jump to Inputs!C14]                       |
|     |                  |             |        |   [Accept Hunk 2 Alt+A]  [Reject Hunk 2 Del] |
|     |                  |             |        |----------------------------------------------|
|     |                  |             |        | ③ SESSION AUDIT LOG (2 entries)              |
|     |                  |             |        | 16:14 ✓ Hunk 1 committed (7 cells · Maria)   |
|     |                  |             |        | 16:15 ↺ Hunk 2 pending review                |
+-----+------------------+-------------+--------+----------------------------------------------+
| [Inputs][Monthly][Summary*]  | ⇄ 1 Agent hunk staged (uncommitted) · 0 errors | Working 100% |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Staged cells render across the Preview Seam (`┄...┄` dashed teal frame + diagonal-hatched tint + `⇄` badge) while already-accepted Hunk 1 values show clean committed numbers (`+$58k ✓`). **②** Agent Review Drawer groups cell mutations into meaningful business hunks with live speculative recalculation impact, formula-diff inspection, and per-hunk Accept/Reject/Undo controls. **③** Session Audit Log traces exactly which cells came from the agent and what Maria accepted or undid.

> **What an Excel user notices as different—and why it is a win:** Most AI spreadsheet sidebars overwrite 40 cells in a flash and leave the analyst frantically hitting Undo to guess what broke. DNA Calc treats AI edits like a live code review over the sheet: every proposed cell recalculates across the Preview Seam *before* commit, works identically across Classic and Notebook, accepts hunk-by-hunk, and produces clean plain-Excel formulas.

---

## 3. The conceptual model

DNA Calc's five views and Formula Explorer stay coherent because they operate on **one conceptual model**, split cleanly along a single seam: the **Excel Workbook Model** (calculation truth, persisted in standard OOXML parts) and the **Presentation Layer** (layout, prose, annotations, inferred-block overrides, skins—persisted in an Excel-preserved `customXml` extension part).

### 3.1 What a user points at (one unified address grammar)

Every pointer click, keyboard focus, hover peek, Formula Explorer target, and AI agent command targets one of **seven first-class entity kinds**, expressed in one unified address grammar (`Ctrl+G` / Bridge Address Box):

| Entity Kind | Canonical Address Syntax | Stored In | Concrete Cast Example |
| :--- | :--- | :--- | :--- |
| **Cell** | `Sheet!A1` (or R1C1 mode) | Excel Workbook | `Summary!B12` (`$290,311`) |
| **Range** | `Sheet!A1:D12` (or multi-area union) | Excel Workbook | `Monthly!B5:M5` (12-month MRR) |
| **Block** | `block:Sheet/BlockId` | Inferred + optional `customXml` label | `block:Monthly/Revenue` (`A4:N7`) |
| **Name** | `Name` or `Sheet!Name` (dots native) | Excel Defined Names | `Inv.OnHand`, `Dyn_Curve` |
| **Table** | `Table`, `Table[Col]`, `Table[#Totals]` | Excel ListObject | `Orders[Amount]` (25,000 rows) |
| **Object** | `obj:Sheet/ObjectName` | Excel Drawing / Control / Pivot / VBA | `obj:StockModel/Slider_RP`, `obj:Report/Pivot1` |
| **View Item** | `view:ViewId/ItemId` | Presentation `customXml` only | `view:nb/exec-lede`, `view:app/kpi-strip` |

**Iron distinction:** View Items carry a subtle dotted frame when focused and a `[View]` tag in the Address Box so a person never mistakes a Notebook prose paragraph or App card border for a workbook cell.

### 3.2 Selection and navigation that carry across views

Switching views (`Ctrl+Alt+1..5`) never resets focus to `A1`. The shell maintains a **Selection Anchor** (the primary entity address plus an optional multi-selection set) and a **Navigation History Stack** (`Alt+←` Back, `Alt+→` Forward):

* **Anchor translation rules:**
  * `Classic` (`Monthly!H5`) → `Inspector`: opens `Trace Selected` centered on `Monthly!H5`.
  * `Classic` (`StockModel!B18`) → `Model Map`: centers and focuses actor `Inv.OnHand` (since `B18` backs `Inv.OnHand`); if a cell has no name, focuses its containing Block or Formula Run.
  * `Model Map` (`Supply.Pipeline`) → `Classic`: focuses `StockModel!B10` (and outlines its spill `B10:D10`).
  * `Notebook` (`«$2,889,851»` live chip) → `Classic` / `Inspector`: resolves through the chip's binding straight to `Summary!B6`.
  * `App` (input control `Extra Monthly`) → `Classic`: focuses `Loan!C6` directly.
  * `View Item` (e.g., a Notebook prose block with no workbook backing) → `Classic`: retains the last workbook anchor on that sheet and displays a calm Strip notice (`Returned to last cell anchor: Summary!B12`).
* **Visual continuity:** On arrival, the target scrolls smoothly into the center third of the Stage and receives Strand's **150 ms amber Continuity Halo** (reduced to an instant crisp focus frame under `prefers-reduced-motion`).

### 3.3 What an edit means in a view that is not a grid

Every non-grid gesture maps deterministically to either a **plain Excel workbook mutation** or a **presentation-only `customXml` update**—never a hidden engine extension:

| Gesture & View | Exact Workbook Mutation (Excel-legal) | Presentation-Only Change (`customXml`) |
| :--- | :--- | :--- |
| Drag wire `A.Out -> B.In` (**Model Map**) | Rewrites `B.In`'s defined-name formula via the **Formula-Name Interpreter** (`=A.Out`) on the backing sheet. | Saves wire routing & box `(x, y)` coordinates. |
| Drag `Supply.ReorderPoint` slider (**Model Map / App**) | Writes numeric value to linked cell `StockModel!$B$8` and triggers standard recalc. | None. |
| Edit assumption in card (**Notebook / App**) | Writes constant to bound cell (e.g., `Inputs!C5 = 5.0%`), honoring Excel Data Validation. | None. |
| Type new `/calc` formula block (**Notebook**) | Allocates a clean cell on the active/designated sheet and defines its workbook name (e.g., `Summary!B19`, `Cash_Runway_Gain`). | Stores block position in Notebook reading order. |
| Click `[Apply Fix]` on range truncation (**Inspector**) | Rewrites `Report!D8` formula to structured refs `=SUMIFS(Orders[Amount], ...)`. | Dismisses resolved anomaly note. |
| Edit prose paragraph or reorder cards (**Notebook / App**) | **None** (workbook calculation graph untouched; dirty flag marks presentation save only). | Updates `customXml` prose/layout tree. |

The **Formula-Name Presenter / Interpreter** (adapted from `LINKED_BOXES_SKIN.md`) operates strictly on the parsed formula syntax tree—never string regex—so stripping group prefixes (`Inv.OnHand` → `OnHand` inside group `Inv`) or displaying self-references as `OnHand⁻` never collides with function names, strings, or A1 references.

### 3.4 Undo across views

DNA Calc maintains **one chronological Undo/Redo stack (`Ctrl+Z` / `Ctrl+Y`)** per workbook across all views, rather than separate per-view stacks that lose edits when switching views.

* **Dual-tagged transactions:** Every Undo entry is tagged **`[Model]`** (mutates Excel cells, names, formulas, validation, calc settings) or **`[View]`** (moves a box, edits Notebook prose, changes layout).
* **Predictable cross-view behavior:** Pressing `Ctrl+Z` always undoes the most recent action regardless of which view is currently open. If Maria edited `Inputs!C5` in Notebook and switches to Classic before hitting `Ctrl+Z`, `Inputs!C5` reverts immediately; the Strip shows `Undid [Model]: Inputs!C5 -> 4.5% (from Notebook) [Jump]`. It does **not** yank her back to Notebook (preventing view whiplash), unless the undone action deleted the exact object she is currently viewing.
* **Undo History Peek (`Ctrl+Shift+Z` / Mast history chevron):** Lets power users filter or inspect recent `[Model]` vs `[View]` entries and confirms atomic Agent commit checkpoints.
* **Crash recovery:** Both workbook deltas and `customXml` presentation deltas journal continuously to a local recovery snapshot; reopening after a crash displays a non-modal recovery bar (`Recover unsaved changes from 16:18 · [Restore] [Discard]`).

### 3.5 Errors, calculation state, and live data as visible things

1. **Errors as traceable objects:** Every Excel error (`#REF!`, `#DIV/0!`, `#VALUE!`, `#NAME?`, `#N/A`, `#NUM!`, `#NULL!`, `#SPILL!`, `#CALC!`) is classified as either a **Root Origin** (first formula producing the error from non-error inputs) or **Propagated** (receiving an upstream error). Classic renders root errors with a filled `⊗` glyph and propagated errors with a hollow `⊘` glyph (both `red-ink`), while the Strip's clickable `⊗ N errors (K roots)` pill jumps straight into Inspector's collapsed root-cause list.
2. **Calculation state as persistent flight-deck telemetry:** The Strip always reports exact engine state—never silent staleness:
   - `✓ Clean (12 ms)` — all formulas current under Automatic calculation.
   - `⚠ Dirty · Manual Calc [F9]` — workbook is in Manual calc mode and has uncalculated edits (`amber-ink` pill).
   - `◐ Calculating 42% (128k/304k)` — live progress bar during heavy recalculation (abortable via `Esc`).
   - `↻ Iterative · Converged (14 passes, Δ 0.0004)` vs `⚠ Iterative · Max 100 reached (Δ 0.12 — Unconverged)` — honest convergence telemetry.
3. **Live data & volatility as visible provenance:** Cells fed by RTD (`=RTD(...)`), external workbook links, async add-ins (`TaxPack.xll`), or volatile functions (`OFFSET`, `INDIRECT`, `NOW`, `RAND`) carry a `signal` (`#FF8001`) `▸` corner tick. Hovering reveals freshness (`RTD updated 0.4s ago · 2.0/s` or `External link unresolved · cached 2026-10-12`). Inspector's `Live / External` tab lets users pause/throttle RTD visual updates or inspect external link status without mutating formulas.

### 3.6 How an agent's actions are shown, reviewed, accepted, and traced

* **Same tools, zero backdoors:** The AI agent reads workbook structure and proposes changes strictly through the Unified Address Grammar and standard Excel edit primitives.
* **Staged Proposal Batch + Preview Seam:** Agent proposals land in a speculative staging buffer recalculated live against the engine—**never mutating the workbook directly**. Affected cells, Notebook chips, and Model Map boxes show Strand's dashed `teal-600` + diagonal-hatch Preview Seam with before/after deltas.
* **Semantic Hunks & Safety Gate:** Related mutations are grouped into named hunks (`Hunk 1: Q3 Seat-Price Uplift`). Any hunk that introduces a new formula error, touches a protected cell, or turns on iterative calculation automatically triggers a visible **Safety Warning badge** and cannot be batch-accepted blindly.
* **Selective Commit & Audit Trace:** The user accepts per hunk (`Alt+A`), accepts individual cells, rejects (`Del` / `Esc`), or accepts all safe hunks. Every accepted hunk commits as a single named Undo checkpoint (`[Agent ✓] ...`) and appends an entry to the exportable session Audit Log.

---

## 4. The views and the Formula Explorer

### 4.1 Classic

**Purpose:** Zero-compromise Excel grid for rapid data entry, financial modeling, formatting, and formula authoring, wrapped in Strand's calm petrol chrome and instrument overlay layer.

#### Full-screen Mockup M-12 — Classic on `Budget.xlsx` (`Monthly`, mid-edit picking ranges with numbered tokens)

```text
+----------------------------------------------------------------------------------------------+
| [==] Budget.xlsx     [Classic*|Inspector|Notebook|Map|App]  [Lens]   | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| ① [Home*][Insert][Formulas][Data][Review][View]  B I U | $ % , .0 | Borders | AutoSum | [▾]  |
| Monthly!H15      v | fx =SUM(①H5:H6) - ②H10 - SUM(③H12:H14)                                  |
+-----+----------------+-----------+-----------+-----------+-----------+-----------+-----------+
|     |       A        |     F     |     G     |     H     |     I     |     J     |     N     |
+-----+----------------+-----------+-----------+-----------+-----------+-----------+-----------+
|  4  | Line Item      |    May    |    Jun    |    Jul    |    Aug    |    Sep    | FY Total  |
|  5  | MRR            |  $205,803 |  $211,360 |╔①$217,067╗|  $222,927 |  $228,946 |$2,580,224 |
|  6  | Services Rev   |   $24,696 |   $25,363 |╚ $26,048 ╝|   $26,751 |   $27,474 |  $309,627 |
|  7  | Total Revenue  |  $230,499 |  $236,723 |  $243,115 |  $249,678 |  $256,420 |$2,889,851 |
|  9  |                |           |           |           |           |           |           |
| 10  | COGS (22%)     |   $50,710 |   $52,079 |╔②$53,485 ╗|   $54,929 |   $56,412 |  $635,767 |
| 11  | Gross Profit   |  $179,789 |  $184,644 |  $189,630 |  $194,749 |  $200,008 |$2,254,084 |
| 12  | Payroll        |  $101,200 |  $101,200 |╔③$110,400╗|  $110,400 |  $110,400 |$1,269,600 |
| 13  | Cloud Base     |   $14,500 |   $14,500 |║  $14,500║|   $14,500 |   $14,500 |  $174,000 |
| 14  | Marketing      |   $41,490 |   $42,610 |╚  $43,761╝|   $44,942 |   $46,156 |  $520,173 |
| 15  | EBITDA         |   $22,599 |   $26,334 |[[=SUM(H5:H6)-H10-SUM(H12:H14)|]]② |  $290,311 |
|     |                |           |           | ③ ┌─────────────────────────────────────────┐ |
|     |                |           |           |   | Live Preview: $20,969 (was $20,969)     | |
|     |                |           |           |   | SUM(number1, [number2], ...)  [F9 Eval] | |
|     |                |           |           |   └─────────────────────────────────────────┘ |
+-----+----------------+-----------+-----------+-----------+-----------+-----------+-----------+
| ④ [Inputs][Monthly*][Summary]  | EDITING (Point) · Sum $168,661 | ✓ Clean · 4ms | Working    |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Single-row collapsible Strand Action Band (32 px compact default, expandable to 68 px, full `Alt` Key Tips conformance—zero Microsoft ribbon artwork). **②** Mid-edit cell `H15` in `EDITING` state: active range picks use Strand's 6-tone Formula Reference Palette paired with non-color numbered corner chips `① ② ③` matching superscripts in the Formula Bar. **③** Non-obstructive inline signature & live preview tooltip shows current formula result (`$20,969`) before pressing `Enter`. **④** Strip reports `EDITING (Point)` mode (arrows pick ranges; pressing `F2` flips to `EDITING (Edit)` where arrows move the text caret).

* **Gestures & Keyboard Model:** Full Excel muscle-memory conformance (detailed exhaustively in **Appendix A**): `F2` (Enter↔Edit toggle), `F4` (absolute/relative `$` cycle / repeat), `F9` (calc / inline partial evaluation), `Ctrl+Enter` (fill selection), `Alt` Key Tips, Fill Handle & double-click auto-fill with live Preview Seam, drag-move / `Ctrl+Drag` copy / `Shift+Drag` insert-shift, `Ctrl+Shift+P` Provenance Overlay toggle.
* **Entry & Exit:** Default view for plain `.xlsx` files with no preferred view; enter anytime via `Ctrl+Alt+1` or `[Classic ↗]` links from any other view. Exit via `Ctrl+Alt+2..5` preserving exact cell/range anchor.
* **Cast Workbook Fit:**
  * *Suits:* **`Budget`** (fast modeling & editing), **`Sales`** (inspecting raw 25k-row `Orders` and editing `Report`), **`Inherited`** (instant cached open & surgical fixes on 40 sheets), **`Loan`** (authoring the 360-row schedule), **`Stock`** (auditing backing cells/names).
  * *Does not suit:* Executive presentation to non-builders (`Jo` using `Loan`), holistic 40-sheet root-cause triage (`Inherited` → Inspector), or visualizing circular feedback loops (`Stock` → Model Map).

---

### 4.2 Inspector

**Purpose:** Phyphox-inspired diagnostic instrument showing *what depends on what*, *in what order it calculates*, *where errors originate*, *structural anomalies*, and *live/external feeds*.

#### Full-screen Mockup M-13 — Inspector on `Sales.xlsx` (Full diagnostic layout: Graph, Calc Order, Live Probes)

```text
+----------------------------------------------------------------------------------------------+
| [==] Sales.xlsx      [Classic|Inspector*|Notebook|Map|App]  [Lens]   | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Report!D8        v | [Health] [Trace Selected*] [Calc Order] [Live & Volatile] | [Export Md] |
+-------------------------------------------+--------------------------------------------------+
| ① WORKBOOK DIAGNOSTIC SUMMARY             | ② ACTIVE TRACE · Report!D8 (Q3 EMEA Revenue)     |
|   Sheets: 2  ·  Tables: 1 (Orders, 25k)   |   Value: $412,680  |  Calc: 1.8 ms  | Depth: 2   |
|   Formulas: 418  ·  PivotTables: 1        |   [Precedents (3)]  [Dependents (4)]  [Full Path]|
|                                           |                                                  |
|   ⊗ Errors: 0                             |   [Orders!$K$2:$K$20000] ⚠ 5,001 rows missed     |
|   ⚠ Structural Anomalies: 2               |          │                                       |
|     • Partial Table Coverage (D6:D9)  <-- |   [Orders!$D$2:$D$20000] ──┐                     |
|     • Mixed hardcode in Total (G12)       |   [Report!$A8 = "EMEA"]  ──┼─> [[Report!D8]]     |
|   ▸ Volatile / External: 0                |   [Report!D$4 = "Q3"]    ──┘     $412,680        |
|                                           |                                     │            |
| ③ CALCULATION ORDER (Topological Wave)    |                  ┌──────────────────┼──────┐     |
|   Wave 0: Orders[A2:K25001] (constants)   |                  ▼                  ▼      ▼     |
|   Wave 1: Report!B6:E9 (SUMIFS · 11.4 ms) |          [Report!D14]       [Report!F8]  [D12]   |
|   Wave 2: Report!F6:F9 (XLOOKUP · 0.6 ms) |           -$118,420          "Tier 2"    Total   |
|   Wave 3: Report!G6:G9 (Rebates · 0.3 ms) |           (Reconcil. diff)     │                 |
|   Wave 4: Report!D14 (Pivot diff · 0.2ms) |                                ▼                 |
|   Wave 5: Pivot1 Cache (refresh · 5.5 ms) |                             [Report!G8] $8,254   |
|                                           |--------------------------------------------------|
| ④ LIVE DATA / PERFORMANCE PROBE           | ⑤ SELECTED PROBE DETAIL                          |
|   Last Recalc: 18.0 ms (Automatic)        |   Anomaly: Hardcoded stop row 20,000 inside      |
|   Slowest Block: Report!B6:E9 (63% time)  |   Table `Orders` (25,001). Excluded EMEA/Q3:     |
|   RTD Feeds: None                         |   493 rows = +$118,420.00.                       |
|   [Re-profile Full Recalc Ctrl+Alt+F9]    |   [Inspect 493 Rows ↗] [Test Fix fx] [Apply Fix] |
+-------------------------------------------+--------------------------------------------------+
| [Orders (25,000 rows)][Report*]           | ✓ Clean · 18.0 ms | ⚠ 2 anomalies | Working 100% |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Diagnostic ledger groups errors, structural anomalies (partial table coverage, inconsistent row formulas, hardcoded numbers inside formula blocks), and volatile/external counts. **②** Interactive causal graph centered on the selected anchor with live values on every node. **③** Topological Calculation Order shows evaluation waves and millisecond hotspots—vital for diagnosing slow workbooks like `Inherited`. **④** Live Data & Performance Probe monitors recalc cost, RTD event frequency, and iteration convergence deltas. **⑤** Actionable probe detail offers non-destructive inspection or Preview Seam fixes.

* **Gestures & Keyboard Model:** `←/→` walks upstream precedents and downstream dependents; `↑/↓` cycles sibling nodes; `Enter` opens Formula Explorer on the focused node; `Space` toggles branch collapse; `Ctrl+Alt+1` jumps straight to that node's cell in Classic; `Ctrl+Shift+C` copies the active trace as Markdown/CSV.
* **Entry & Exit:** Enter via `Ctrl+Alt+2`, clicking the Strip Error Pill (`⊗ 47 errors`), or the Triage Banner. Exit via `Ctrl+Alt+1/3/4/5` or `Alt+←` Back.
* **Cast Workbook Fit:**
  * *Suits:* **`Inherited`** (essential first stop for 40-sheet triage, root-cause error collapse, volatile `OFFSET` bottlenecks), **`Sales`** (auditing `SUMIFS`/`XLOOKUP`/`FILTER`/Pivot discrepancies), **`Stock`** (verifying circular convergence and dependency waves), **`Budget`** (pre-board audit check).
  * *Does not suit:* Daily data entry (`Classic`), executive storytelling (`Notebook`), or consumer scenario exploration (`Jo` on `Loan`).

---

### 4.3 Notebook

**Purpose:** Live reactive document interleaving Source Serif 4 prose, editable input strips, live value chips, data frames (tables/arrays), formula cells, and inline charts—recalculating live like Pluto and Excel itself, never run-once like Jupyter.

#### Full-screen Mockup M-14 — Notebook on `Budget.xlsx` in Horizontal Pipeline Mode (`Alt+P`)

```text
+----------------------------------------------------------------------------------------------+
| [==] Budget.xlsx     [Classic|Inspector|Notebook*|Map|App]  [Lens]   | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Monthly!N15      v | [+ Prose] [+ Formula] [+ Range] [+ Chart] | ① [Vertical | Pipeline*]    |
+----------------------------------------------------------------------------------------------+
|                                                                                              |
|  ┌─ STAGE 1: ASSUMPTIONS ─────┐     ┌─ STAGE 2: MONTHLY ENGINE ───────┐     ┌─ STAGE 3: OUT ┐|
|  | Linked: Inputs!B4:E15      |     | Linked: Monthly!A4:N15          |     | Summary!A4:C16||
|  |                            |     |                                 |     |               ||
|  | Scenario: [● Base|○ Str]   |     | Revenue & Cost Data Frame       |     | FY27 Executive||
|  |                            |     | 12 cols (Jan..Dec) -> FY Total  |     | Rollup        ||
|  | Opening_MRR      $185,000  |════>|                                 |════>|               ||
|  | Monthly_Growth   [  4.5% ] | ②   | MRR       $185k -> $248k  $2.58M|     | Revenue       ||
|  | Logo_Churn       [  1.8% ] |     | Services   $22k ->  $30k  $310k |     |  «$2,889,851» ||
|  | Services_Attach  [ 12.0% ] |     | Total_Rev $207k -> $278k  $2.89M|     |               ||
|  | COGS_Pct         [ 22.0% ] |     | ─────────────────────────────── |     | Gross Margin  ||
|  | Headcount_Start  [    10 ] |     | COGS      -$46k -> -$61k -$636k |     |  «78.0%»      ||
|  | Hires_Per_Qtr    [     1 ] |     | Payroll   -$92k ->-$120k -$1.27M|     |               ||
|  | Avg_Salary       [ $9,200] |     | Cloud     -$15k -> -$15k -$174k |     | [[EBITDA]]    ||
|  | Marketing_Pct    [ 18.0% ] |     | Marketing -$37k -> -$50k -$520k |     |  «$290,311»   ||
|  | Tax_Rate         [ 21.0% ] |     | ─────────────────────────────── |     |  (10.0% mgn)  ||
|  |                            |     | EBITDA     $19k ->  $31k  $290k |     |               ||
|  | Prose note:                |     |                                 |     | Net Income    ||
|  | "Base assumes 1 hire/qtr   |     | Inline Spark Chart:             |     |  «$229,346»   ||
|  |  and flat cloud infra."    |     | ▁▂▃▃▄▅▆▇█ Rev  ▂▃▃▄▅▅▆▇█ EBITDA |     |               ||
|  └────────────────────────────┘     └─────────────────────────────────┘     └───────────────┘|
|   ◀────────────────────── Horizontal scroll / Shift+Wheel / Alt+←/→ ──────────────────────▶  |
+----------------------------------------------------------------------------------------------+
| [Inputs][Monthly][Summary* (Notebook)]   | ✓ Clean · Live reactive · 4 ms | Reading 100%     |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** One click (`Alt+P`) flips between Vertical reading layout (`M-04`, ideal for written memos/PDF) and Horizontal Pipeline layout (`M-14`, ideal for showing left-to-right cross-sheet data flow `Inputs → Monthly → Summary` during live reviews). **②** Subtle teal dependency rails connect pipeline stages automatically from workbook precedents—hovering an input (`Monthly_Growth`) highlights every downstream row it drives across Stages 2 and 3.

* **Gestures & Keyboard Model:** `/` opens the slash-command block inserter (`Prose`, `Formula`, `Link Range`, `Data Frame`, `Chart`, `Control`); `=` inside prose inserts a live bound value chip; `Enter` edits focused block, `Esc` returns to block selection; `Alt+↑/↓` (Vertical) or `Alt+←/→` (Pipeline) reorders presentation blocks; `Alt+P` toggles Vertical ↔ Pipeline; `Ctrl+V` smart-pastes copied Classic ranges as Linked Data Frames.
* **Entry & Exit:** Enter via `Ctrl+Alt+3`. First run on a workbook without a saved Notebook offers `[Draft from Workbook Structure]` vs `[Start Blank]`. Exit via `Ctrl+Alt+1/2/4/5` or `[Classic ↗]` on any block.
* **Cast Workbook Fit:**
  * *Suits:* **`Budget`** (executive operating plan memo & live pipeline), **`Loan`** (annotated financing memo), **`Sales`** (`Report` sheet as a narrative analytical brief with a sampled 200-row preview of `Orders` + clear notice), **`Stock`** (explaining policy trade-offs alongside live charts).
  * *Does not suit:* **`Inherited`**'s raw 40-sheet sprawl (Notebook offers to scaffold *only* `Exec_Summary` or user-picked sheets and declines auto-dumping `Dispatch_Hourly`'s 8,760 rows).

---

### 4.4 Model Map

**Purpose:** Hierarchical blocks-and-wires schematic of the model's structure, dynamics, feedback loops, and directors—unifying `LINKED_BOXES_SKIN.md` with the Ptolemy-inspired director, delay (`z⁻¹`), modal-state, and live-event concepts from `names-in-motion.html` and `watershed.html`, strictly harmonized into Strand's palette and Excel-only engine rules.

#### Reactions to `names-in-motion.html` and `watershed.html` (what we adopt vs change)
* **Adopted:** Hierarchical composite boxes (`click/Enter` to drill in, breadcrumbs in Bridge to climb out); explicit **Director pills** mapped strictly to Excel mechanisms (`Recalc`, `Iteration`, `Ticks`, `Sweep` over Data Tables, `Events` over RTD); `z⁻¹` delay chips on self-referencing feedback loops; inline mini-state diagrams for `IFS`/`SWITCH` modal cells; side Parameter Rail + bottom Plotter; topological recalc ripple showing real causal propagation; and `Presented` vs `Excel formulas` toggle via the AST Formula-Name Presenter.
* **Changed for Strand & Excel honesty:**
  1. Replace the previews' divergent emerald-green/navy/purple palettes with Strand's petrol chrome, daylight/charcoal paper, teal structure, amber active firing ripple, `signal` orange RTD sources, and **double-dashed teal + `↻` badge** for circular iteration loops (eliminating unbudgeted hues).
  2. Replace `watershed.html`'s continuous animated turbine blades / water waves with crisp data-bearing sparklines and gauges (obeying Strand's *"don't animate to delight; if a motion carries no state information, cut it"*).
  3. Add **optional Sheet Pages** (each sheet can be either a top-level Composite box on the Workbook Map or its own Sheet Page tab) and **Block/Run Folding** so big sheets like `Dispatch_Hourly` render honestly at `L2 Formula Runs` instead of collapsing into spaghetti.

#### Full-screen Mockup M-15 — Model Map on `Stock.xlsx` (Hierarchical Composites, Director, Live Ripple & Plotter)

```text
+----------------------------------------------------------------------------------------------+
| [==] Stock.xlsx      [Classic|Inspector|Notebook|Map*|App]  [Lens]   | [fx][Ag][Ctrl+K][Prt] |
+----------------------------------------------------------------------------------------------+
| Supply.Order     v | ① Stock › Top  | [Ticks · 1 iter/tick] [▶ Run][Step F9][Reset] | [Prs*] |
+--------------------------------------------------------------------+-------------------------+
| ② WORKBOOK COMPOSITES (Click/Enter composite to drill inside)      | ④ PARAMETER RAIL        |
|                                                                    |  Supply.ReorderPoint    |
|  ┌─ Sim (Ticks) ──┐                                                |  260 ($B$8 · Slider)    |
|  | [Sim.Tick] 18  | ──(z⁻¹)──┐                                     |  100 ────●──────── 500  |
|  └────────────────┘          │                                     |  Demand.Rate            |
|        │                     ▼                                     |  85 ($B$4)              |
|        ▼           ┌═ Supply (Composite · 7 actors) ══════ [⤵] ═┐  |  40 ─────●──────── 150  |
|  ┌─ Demand ──────┐ | [ReorderPoint: 260]  [OrderQty: 400]       |  |-------------------------|
|  | Rate: 85 /d   | |         │                 │                |  | ⑤ INSPECTOR MINI-CARD   |
|  | (constant)    | |         ▼                 ▼                |  |  Supply.Order           |
|  └───────┬───────┘ | [[Supply.Order: 400]]━━━>(z⁻¹)━━>[Pipeline]|  |  Kind: Calculation      |
|          │         |   Position < RP ? OQ : 0     {0, 400, 0}   |  |  Cell: StockModel!B13   |
|          │         |        ▲                         │         |  |                         |
|          │         |        │ [Position: 235]         ▼         |  |  Presented:             |
|          │         |        │  OnHand + ΣPipe    [Inflow: 0]    |  |  Position < ReorderPoint|
|          │         └────────┼─────────────────────────┼─────────┘  |   ? OrderQty : 0        |
|          │                  │ (feedback)              │            |                         |
|          │                  │    ┌───[ z⁻¹ ]───┐      │            |  Excel:                 |
|          │                  │    ▼             │      ▼            |  =IF(Supply.Position<   |
|          │         ┌─ Inv ──┴──────────────────┴─────────────────┐ |   Supply.ReorderPoint,  |
|          └────────>| [Outflow: 85] ────> [OnHand: 235] (state)   | |   Supply.OrderQty, 0)   |
|                    |   min(Rate,In+OH⁻)    OH⁻ + In - Out        | |                         |
|                    |        └──────────> [Shortage: 0]           | | [Classic Ctrl+Alt+1]    |
|                    └─────────────────────────────────────────────┘ |  [Formula Explorer fx]  |
+--------------------------------------------------------------------+-------------------------+
| ③ LIVE PLOTTER (Ticks 1–18 · Stock OnHand ── · ReorderPoint ┄ · ● Order Placed)              |
| 500│  ╲     ╱╲      ╲                                                                        |
| 260│---╲---╱--╲------╲--- (RP = 260)      [● Tick 4: +400]  [● Tick 11: +400] [● Tick 18*]   |
|   0└────╲─╱────╲──────╲───────────────────────────────────────────────────────────────────── |
+----------------------------------------------------------------------------------------------+
| [Top Map*][Sheet: StockModel] | ◐ Ripple: Order->Pipeline (Tick 18) | Working 100% L3        |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Hierarchical breadcrumbs (`Stock › Top`) + Director pill (`Ticks · 1 iter/tick`) + Presented/Excel toggle. **②** Composite group `Supply` has a double-line Strand frame and `[⤵]` affordance to drill inside (or expand inline); `━━━>` marks the live 140 ms amber **Recalc Ripple** travelling from `Supply.Order` into `Supply.Pipeline` on Tick 18. **③** Collapsible bottom Plotter charts history collected across ticks (pure view state, zero workbook mutation). **④** Parameter Rail surfaces Excel Form Controls and constant inputs for live tuning. **⑤** Inspector Mini-Card shows both Presented and raw Excel formulas plus direct jump to Classic or Formula Explorer.

* **Gestures & Keyboard Model:** Drag port-to-port (or `W` key) wires named actors; drag box moves layout (presentation-only); `Enter` drills into a Composite or edits formula; `Backspace`/`Esc` climbs up one hierarchy level; `Space` steps one tick (`F9`) under Ticks/Iteration director; scroll wheel executes **Semantic Zoom** (`L0 Atlas → L1 Composites → L2 Formula Runs → L3 Individual Boxes`).
* **Entry & Exit:** Enter via `Ctrl+Alt+4`. Supports Sheet Pages toggle in Strip (`[Top Map | Sheet: <Name>]`). Exit via `Ctrl+Alt+1/2/3/5` with Continuity Halo on backing cell/range.
* **Cast Workbook Fit:**
  * *Suits:* **`Stock`** (ideal native fit: stocks, flows, `z⁻¹` delays, circular iteration, Form Slider), **`Budget`** (clean 3-stage architecture `Inputs → Monthly → Summary`), **`Sales`** (`Report` block-level flow feeding Pivot & checks).
  * *Does not suit:* Raw 25,000-row **`Orders`** database table (shown as a single compact `[Table: Orders · 25k rows]` source block, never expanded row-by-row) and **`Inherited`**'s 182k-formula `Dispatch_Hourly` (declines cell-level graph, offers honest 12-run `L2` summary as shown in `M-03`).

---

### 4.5 App

**Purpose:** A clean, distraction-free decision application for people who run a model without authoring it (`Jo`), binding validated inputs, scenario chips, KPI cards, charts, and grouped tables directly to the workbook's real cells.

#### Full-screen Mockup M-16 — App View on `Loan.xlsx` (Clean consumer state + Print Preview & Unlock affordance)

```text
+----------------------------------------------------------------------------------------------+
| [==] Loan.xlsx — Home Mortgage Planner       [View: App ▾]  | [Undo][Reset][Copy][Print ①]   |
+----------------------------------------------------------------------------------------------+
| Scenarios: [Base 6.25%*] [+$300/mo Extra] [Refi 5.50%] [+ New Scenario] | Bound: Loan!C3:C7  |
+--------------------------------+-------------------------------------------------------------+
| ② ASSUMPTIONS                  | ③ SUMMARY OUTCOMES (Base 6.25%)                             |
|                                | ┌───────────────┐ ┌───────────────┐ ┌─────────────────────┐ |
| Principal                      | | Monthly Pmt   | | Total Interest| | Payoff Horizon      | |
| [ $420,000                  ]  | |   $2,586.01   | |   $510,964    | |  Dec 2056 (30y 0m)  | |
| $100k ────────●─────── $1.0M   | └───────────────┘ └───────────────┘ └─────────────────────┘ |
|                                |                                                             |
| Annual Interest Rate           | ④ AMORTISATION TRAJECTORY (Live Excel Chart · Loan!Chart1)  |
| [ 6.25%                     ]  | $550k│                                        .. Total Int  |
| 2.0% ─────●─────────── 12.0%   |      │ ── Balance ($420k -> $0)          ....    ($511k)    |
|                                | $275k│      ╲╲                      .....                   |
| Term (Years)                   |      │        ╲╲╲             ......                        |
| [ 15 yr | 20 yr | ● 30 yr ]    |   $0 └───────────╲╲╲────────..────────────────────────────  |
|                                |      2027     2034     2041     2048     2056               |
| Extra Monthly Payment          |                                                             |
| [ $0                        ]  | ⑤ ANNUAL SCHEDULE (360 monthly rows grouped into 30 years)  |
| $0 ●────────────────── $1,000  | Year   Payments    Principal   Interest    Ending Balance   |
|                                | ▶ 2027  $31,032     $4,922     $26,110       $415,078       |
| Start Date                     | ▶ 2028  $31,032     $5,239     $25,793       $409,839       |
| [ 2027-01-01                ]  | ▶ 2029  $31,032     $5,576     $25,456       $404,263       |
|                                | ... (27 more years) · [Expand All 360 Months]               |
+--------------------------------+-------------------------------------------------------------+
| App View · Skin: "Loan Planner" | ✓ Clean · 7 ms | [Formulas Ctrl+Alt+1]           | Reading |
+----------------------------------------------------------------------------------------------+
```

*Callouts:* **①** Minimal petrol Mast removes formula/ribbon noise while keeping `[Print]` (`Ctrl+P`), `[Reset]`, `[Undo]`, and an honest `[View: App ▾]` escape hatch. **②** Validated inputs enforce Excel Data Validation inline (`M-08`) and support keyboard arrows on sliders/segmented terms. **③** KPI strip uses tabular numerals (`tnum`) and zero decorative colors. **④** Renders the workbook's actual Excel chart live. **⑤** 360-row schedule auto-groups by Year with instant expand-to-months and explicit row-count disclosure.

* **Gestures & Keyboard Model:** Standard accessible form navigation (`Tab`/`Shift+Tab` across controls, `←/→` on sliders and segmented pickers, `Enter` commits numeric edits, `Esc` reverts invalid input, `Ctrl+Z` undoes last input/scenario change, `Ctrl+P` opens executive Print Preview). Read-only to formulas: clicking a KPI card offers `Alt+Hover` peek (`=-PMT(Annual_Rate/12, Term_Years*12, Principal)`) without risking accidental overwrite.
* **Entry & Exit:** Opens automatically when a workbook declares `preferredView: "app"` in `customXml`, or via `Ctrl+Alt+5`. First run on a workbook without an App definition offers `[Auto-Generate App from Inputs & Summary]` (using Section 5's pattern inference) or `[Ask Agent to Build App]`. Exit via `[View: App ▾]` or `Ctrl+Alt+1..4`.
* **Cast Workbook Fit:**
  * *Suits:* **`Loan`** (textbook fit: 5 inputs, 3 KPIs, 1 chart, grouped schedule), **`Budget`** (executive scenario explorer for directors toggling Base/Stretch & hiring knobs), **`Stock`** (live interactive policy simulator with `ReorderPoint` slider & plotter).
  * *Does not suit:* **`Inherited`** (declines auto-App generation with a plain notice: *"40 sheets, 47 errors, and no clear input/output contract — clean up in Inspector or Classic first"*) and **`Sales`**' raw `Orders` sheet (only suits a curated `Report` filter front-end, never editing 25,000 rows).

---

### 4.6 Formula Explorer (Embedded & Standalone) and Formula Lens

**Purpose:** Dedicated laboratory for writing, decomposing, debugging, formatting, and add-in-testing a *single formula*—available both **embedded** inside any workbook view (`Ctrl+E`) and **standalone** as the stripped **"DNA Calc — Formula Explorer"** build with no workbook at all.

#### Full-screen Mockup M-17 — Formula Explorer in Both Forms (Standalone + Embedded with Format/CF Tabs)

```text
+----------------------------------------------+-----------------------------------------------+
| FORM A: STANDALONE BUILD (No Workbook)       | FORM B: EMBEDDED DRAWER (Inside Budget.xlsx)  |
+----------------------------------------------+-----------------------------------------------+
| [.==] DNA Calc — Formula Explorer   [Add-in] | [==] Budget.xlsx [Classic*|...] | [fx*][Ag]   |
+----------------------------------------------+-----------------------------------------------+
| ① LOCAL INPUTS     | ② FORMULA & RESULT      | ④ WORKBOOK CELL: Summary!B13 (Auto-bound)     |
| ebitda = 290311    | =TAX.EFFRATE(ebitda,    | =TAX.EFFRATE(Summary!B12, "US-CA", TRUE)      |
| juris  = "US-CA"   |    juris, corp)         |                                               |
| corp   = TRUE      | => 0.2798412 (2.0 ms)   | Preview: 27.98% (was 21.00%) · Δ NI -$20,276  |
| [+ Scalar / Array] | [Eval][Fmt*][CF][Types] | [Eval][Fmt][CF*][Add-in]                      |
|                    |                         |                                               |
| FUNCTIONS          | ③ NUMBER FORMAT LAB     | ⑤ CONDITIONAL FORMAT SIMULATOR                |
| ▸ TaxPack.xll (3)  | Code: 0.00%;(0.00%);"—" | Rules matching Summary!B13:                   |
|   TAX.EFFRATE      |                         |  [✓ ACTIVE] Rule 1: Value > 0.25              |
|   TAX.BRACKET      | Live Preview Matrix:    |    Style: amber-ink bold · border-left 2px    |
|   TAX.CREDIT       |  0.27984  ->  27.98%    |  [  idle  ] Rule 2: Value > 0.35              |
| ▸ Built-in (512)   | -0.04500  -> (4.50%)    |    Style: red-ink bold (Stop If True)         |
|   LET, LAMBDA,     |  0.00000  ->   —        |                                               |
|   XLOOKUP, SCAN... |  #VALUE!  -> ⊗#VALUE!   | Test override value: [ 0.38   ] -> Rule 2 ON  |
|                    |                         | (Sandbox only — workbook cell untouched)      |
| [Copy Formula]     | [Test Locale: en-US ▾]  | [Esc Discard]    [Commit to B13 Ctrl+Enter]   |
+----------------------------------------------+-----------------------------------------------+
| Standalone · Zero workbook · TaxPack.xll     | Docked over Classic · Preview Seam active     |
+----------------------------------------------+-----------------------------------------------+
```

*Callouts:* **①** Standalone replaces workbook references with a crisp Local Inputs sandbox (scalars, arrays, error literals) and add-in loader. **②** Identical recursive syntax-highlighted editor, instant evaluation banner, and four-tab workbench shared verbatim between standalone and embedded forms. **③** Number Format Lab lets users author and test Excel's 4-section format codes across positive/negative/zero/error edge cases and locales. **④** Embedded form binds straight to the active cell's precedents and feeds live speculative deltas back to the workbook Preview Seam. **⑤** Conditional Formatting Simulator lets users test threshold overrides without mutating the workbook.

* **Gestures & Keyboard Model:** `Ctrl+E` (or clicking `fx` in Bridge) opens/focuses Formula Explorer from any view; `F9` evaluates highlighted sub-expression; `Ctrl+Enter` commits to the target cell (embedded) or copies result (standalone); `Ctrl+Shift+C` copies clean Excel syntax; `Alt+1..4` switches `Eval Tree / Number Format / Cond. Format / Add-in & Types`; `Esc` closes drawer without modifying the cell.
* **Entry & Exit:** Launch standalone directly from OS/browser with zero workbook; launch embedded from any view (`Classic`, `Inspector`, `Notebook`, `Model Map`) with `Ctrl+E`.
* **Cast Workbook Fit:** Suits **every** workbook (debugging `Inherited`'s broken `VLOOKUP`/`INDEX/MATCH`, fixing `Sales`' `SUMIFS`/`FILTER`, building `Stock`'s `HSTACK`/`DROP` pipeline, testing `Loan`'s `IPMT`, or testing Ade's `TaxPack.xll` add-in before pasting into `Budget`).

#### Cross-cutting: Formula Lens (`Alt+L`)

Available as a single toggle pill in the Mast across **every view**:
* **What it shows:** Rewrites any displayed formula *readably* without changing its calculation meaning by (a) substituting workbook defined names and Table column headers where exact ranges match, (b) structuring deeply nested expressions (`INDEX(MATCH(...))`, multi-condition `SUMIFS`, nested `IF`) into indented `LET` bindings with live inline value chips (`$A8 ["EMEA"]`), and (c) rendering `LAMBDA` invocations with parameter names and group-stripped dotted names (`Inv.OnHand` → `OnHand`).
* **Strict honesty guardrails:** Whenever Formula Lens is ON, the Mast pill highlights (`[Lens*]`) and every rewritten formula bears a subtle `[Readable view · Hover/Alt for raw Excel]` tag. It never silently rewrites the stored `.xlsx` formula—unless the user explicitly invokes **"Refactor cell to real `LET` in workbook"** via Command Palette (`Ctrl+K`).

---

## 5. Real spreadsheet models

Most real-world `.xlsx` files contain zero metadata explaining what a sheet *is*. To make Inspector, Notebook, Model Map, and App immediately useful without making users annotate cells first ("Infer first, declare never unless correcting"), DNA Calc runs a lightweight, non-blocking **Pattern Inference Pass** over the parsed workbook structure and dependency graph.

### 5.1 How recognition, confidence, and correction work

1. **Structural signals (no guesswork, no hidden edits):** Rather than brittle regex over cell labels, the recognizer combines **six verifiable Excel signals**: (a) grid geometry (contiguous header/data regions, blank gutters, Excel Tables, PivotTables, Charts, Form Controls), (b) R1C1 formula signatures (identical relative formulas repeated across rows or columns), (c) cell provenance (pure constants vs formulas vs volatile/RTD/external), (d) graph topology (zero-in-degree input islands, fan-in summaries, cross-sheet pipelines, self-loops/strongly connected components), (e) number formats (dates/quarters/months along a header axis, currency/percent alignment), and (f) defined names & Data Validation lists.
2. **Three calibrated confidence tiers:**
   - **High (`≥ 0.85`):** Directly backed by explicit OOXML objects or unambiguous R1C1 structure (e.g., an Excel Table `Orders`, a PivotTable, a self-loop under iterative calc, a 12-month date header over uniform across-column formulas). Applied automatically across views.
   - **Medium (`0.50–0.84`):** Strong heuristic match (e.g., an unnamed input assumptions block, a reconciliation variance row, a scenario selector cell). Shown with a subtle dashed inference badge (`Inferred: Inputs Block · [Confirm / Change]`) that never interferes with editing.
   - **Low (`< 0.50`):** Ambiguous or heavily interleaved legacy grids (like messy sections of `Inherited`). Left strictly as raw grid/cells—never fabricated into a false abstraction.
3. **Two-click human or agent confirmation & correction:** Clicking any inferred block chip in Classic (Provenance Overlay), Inspector, Notebook, or Model Map opens a tiny **Classify Block** popover (`Reclassify as…`, `Adjust Range`, `Merge/Split`, `Dismiss Inference`). An AI agent can also propose block classifications through the exact same Preview Seam. Confirmed/corrected bindings persist cleanly inside the workbook's `customXml` extension part so Excel preserves them and DNA Calc never asks twice.

### 5.2 Catalogue of 18 recurring model patterns

The 15 patterns from real-world Excel practice fall into five structural families, to which we add **three ubiquitous patterns missing from the initial list** (`#16`, `#17`, `#18`) that appear constantly in financial, operational, and Enron-class workbooks:

| Family | Included Patterns (Full 5-View Matrix in **Appendix D**) | Cast Exemplars |
| :--- | :--- | :--- |
| **A. Data & Reference Foundations** | **1.** Sheet-as-database · **6.** Reference / lookup tables · **11.** Pivots (PivotTables & slicers) · **18. *(Added)*** External-link consolidation & staging hubs | `Sales` (`Orders`, `Pivot1`), `Inherited` (`Curve_Lookup`, 3 missing links) |
| **B. Parameters & Controls** | **2.** Inputs & assumptions · **5.** Scenarios & sensitivity tables (`=TABLE(...)`, scenario switchers) · **10.** Control panels (Form Controls, switches, flags) | `Budget` (`Inputs`), `Loan` (`C3:C7`), `Stock` (`Slider_RP`) |
| **C. Calculation Engines** | **3.** Calculation blocks · **4.** Time series with periods across · **7.** Pipelines across sheets · **17. *(Added)*** Dynamic array / `SPILL` pipelines & `LAMBDA` helper libraries | `Budget` (`Monthly`), `Sales` (`FILTER`), `Stock` (`Supply.Pipeline`), `Inherited` (`Dispatch_Hourly`) |
| **D. State, Feedback & Stochastic Systems** | **8.** Running balances & deliberate circular references · **9.** Simulations (Monte Carlo / Data Table sweeps) · **12.** Live data (RTD, async add-ins, volatile feeds) · **15.** Stocks, flows, feedback & delays | `Loan` (360-row balance), `Stock` (`Inv.OnHand` `z⁻¹`), `watershed.html` (`Outlook` sweep, RTD) |
| **E. Governance, Automation & Presentation** | **13.** Macro-driven models · **14.** Reports & dashboards · **16. *(Added)*** Reconciliations & check blocks (balance checks, variance-to-zero traps) | `Sales` (`Report!D14` check), `Budget` (`Summary`), `Inherited` (`modRollForward`) |

*Why the three additions matter:*
* **#16 Reconciliations & check blocks:** Nearly every serious finance/audit workbook contains explicit check rows (`Assets - Liab - Equity = 0`, `Report!D14 = D8 - GETPIVOTDATA(...)`). Recognizing them lets **Inspector** pin a live `Checks: 11/12 Pass · 1 Fail (D14)` instrument and lets **App/Notebook** show a verified green audit stamp.
* **#17 Dynamic array / `SPILL` pipelines & `LAMBDA` libraries:** Modern Excel models increasingly replace 500 copy-dragged formulas with a single `FILTER`/`SORT`/`SCAN` anchor formula and workbook-scoped `LAMBDA` names. Treating a spill as one atomic block (never 500 separate boxes) prevents clutter and exposes `#SPILL!` collisions clearly.
* **#18 External-link consolidation & staging hubs:** Real corporate workbooks (`Inherited`) routinely stitch together cached values from external workbooks and paste-special staging sheets. Recognizing external hubs explicitly lets **Inspector** and **Model Map** mark external boundaries in `signal` orange (`#FF8001`) instead of mistaking stale caches for clean local constants.

See **Appendix D** for the complete 18-row pattern table detailing *How a person recognises it*, *How DNA Calc recognises it & confidence*, *Confirmation/correction flow*, and *Presentation across all five views*.

---

## 6. Big and busy workbooks

Using **`Inherited.xlsx`** (`40 sheets`, `6 hidden`, `304,218 formulas`, `Dispatch_Hourly` packed `A1:CZ8760`, 3 missing links, 1 VBA module, 9 `OFFSET` names, 47 errors) as the stress benchmark, DNA Calc enforces an explicit **Show / Simplify / Decline** policy per view so huge workbooks stay fast and truthful.

### 6.1 Per-view policy on `Inherited.xlsx`

| View / Tool | Shows Everything | Simplifies (with Visible Notice) | Declines (with Plain Explanation & Way Forward) |
| :--- | :--- | :--- | :--- |
| **Classic** | Always renders the full virtualized grid (up to Excel's 1,048,576 × 16,384 bounds) on demand. | When zoomed out `< 55%` (Semantic Zoom), swaps tiny cell text for **L1/L2 Block Headers** (`Dispatch_Hourly · 8,760 rows · ⊗5`); zooming back in restores every cell. | Never declines—Classic is always the universal bedrock. |
| **Inspector** | Shows all 40 sheets (including hidden/very-hidden), all 4 error root causes, all 3 missing links, all 9 `OFFSET` names, and full trace of any selected cell. | Collapses 47 errors to 4 root causes (expandable to all 47); collapses `Dispatch_Hourly`'s 182,400 cells into **12 R1C1 formula runs** inside the Workbook Atlas (`L0`) and Calc Order waves. | Never declines; if a single cell has `> 500` direct precedents (e.g., `SUM` over 8,760 rows), shows the range as **one bounded range node** (`K2:K8761 · 8,760 cells`) with expand-on-demand instead of 8,760 arrows. |
| **Model Map** | Shows all nodes/wires when active scope has `≤ 120` actors/blocks (e.g., `Stock`, `Budget`, `Sales!Report`). | For `121–2,000` cells or large uniform sheets (`Dispatch_Hourly`), automatically folds contiguous identical R1C1 formulas into **Formula-Pattern Runs** (12 runs = 100% of 182,400 formulas) with a persistent top Scale Notice (`M-03`), min/max/sum across all rows, and row-sample badge. | **Declines cell-level graph** for `> 120` non-foldable bespoke cells on a single canvas or raw 25k-row database tables (`Orders`), displaying: *"Too dense for cell-level wiring (182,400 formulas). Showing 12 exact formula runs (100% covered) — or [Open in Inspector] / [Jump to Classic]."* |
| **Notebook** | Shows all curated prose, inputs, charts, and summary tables (`≤ 200` rows rendered directly). | Large linked ranges or tables (`Orders` 25,000 rows) display as **Sampled Data Frames** showing the first 200 rows + full-table aggregates (`Sum/Avg/Count over all 25,000 rows`) with an unmistakable footer notice (`Showing 200 of 25,000 rows · Aggregates exact over all rows · [Inspect Full Table]`). | **Declines auto-scaffolding** all 40 sheets of `Inherited` at once; presents a sheet picker pre-selecting `Exec_Summary` (*"40 sheets detected — pick which sheets to include in this Notebook"*). |
| **App** | Works whenever a clean Input/Output contract exists (explicit skin or high-confidence inference). | Groups schedules `> 50` rows automatically by Year/Category (`Loan`'s 360 months → 30 expandable annual rows) with explicit count disclosure. | **Declines auto-generation** on `Inherited`: *"This workbook has 40 sheets, 47 unresolved errors, and no clear input/output block. Triage in Inspector [Ctrl+Alt+2] or build an explicit App skin first."* |
| **Formula Explorer** | Always evaluates and displays the complete AST of any single formula. | Array arguments `> 50` elements render as scrollable virtualized previews with explicit dimensions (`8,760 × 1 array · showing 1..20 · [Inspect All]`). | Never declines a valid Excel formula. |

### 6.2 Five simplification techniques (and how they stay honest)

1. **Blocks instead of cells:** Contiguous semantic tables/sections render as named cards with exact A1 bounds (`A4:N15`).
2. **Formula-run folding (R1C1 clustering):** Contiguous cells sharing one identical R1C1 formula collapse into one item (`F2:K8761 · 52,560 identical formulas`), while **exceptions inside a run** (a hardcoded override or `#DIV/0!` cell like `Z4410`) are *never* averaged away—they surface explicitly as exception badges right on the folded run!
3. **Semantic Zoom Levels of Detail (LODs):** `L0` Workbook Atlas (sheets & links) → `L1` Sheet Districts/Composites → `L2` Formula Runs & Blocks → `L3` Individual Cells & Ports. Text never shrinks below 11 px legibility.
4. **Expand on demand:** Every folded run, year group, or error cluster expands locally on click/Enter without loading sibling clusters.
5. **Sampling with visible notice:** Any truncated table preview explicitly states exact row counts (`Showing 200 of 25,000 rows`), guarantees that summary stats and formulas compute over **100% of rows**, and links straight to the unsampled grid.

### 6.3 Performance budgets (Desktop & Browser)

| Interaction | Target Budget (Desktop) | Target Budget (Browser) | Hard Ceiling / Fallback |
| :--- | :--- | :--- | :--- |
| **File open → Interactive Cached First Paint** (`Inherited`, 304k formulas) | `≤ 250 ms` | `≤ 450 ms` | Cached values paint immediately; dependency indexing streams off main thread with Strip progress. |
| **Keystroke echo** (typing in cell / Formula Bar / Notebook) | `≤ 12 ms` (1 frame @ 60–120 Hz) | `≤ 16 ms` | Input buffer never waits on recalculation or inference. |
| **Grid arrow navigation / smooth scroll** (60 fps) | `≤ 8 ms / frame` | `≤ 16 ms / frame` | Strict viewport virtualization; zero DOM nodes for off-screen cells. |
| **View switch** (`Ctrl+Alt+1..5`, preserving anchor) | `≤ 60 ms` | `≤ 100 ms` | Target view paints skeleton/cached layout within 1 frame if heavy layout runs async. |
| **Interactive edit → Recalc feedback** (`Budget`/`Loan`/`Stock` < 5k cells) | `≤ 16 ms` | `≤ 30 ms` | Instantaneous; no spinner ever flashes for `< 150 ms` work. |
| **Heavy recalc progress indicator** (`Inherited` 304k formulas, volatile `OFFSET`) | Progress pill appears at `150 ms`; UI remains scrollable/cancelable (`Esc`) | Same (`150 ms` threshold) | Never freezes the main thread; stale cells subtly dim with `◐ Calculating…` in Strip. |

### 6.4 Three never-acceptable failure modes (and UI safeguards)

* **Never hang:** Parsing, dependency graph building, pattern inference, anomaly scanning, and full recalculation run off the UI thread. Any task exceeding `150 ms` surfaces live progress and an `Esc` abort in the Strip while keeping scrolling, switching views, and saving responsive.
* **Never silently drop content:** Every folding, grouping, sampling, or refusal step displays exact coverage counts (`12/12 runs · 100% covered`, `47 errors -> 4 roots`, `Showing 200 of 25,000 rows`, `+6 hidden sheets`).
* **Never misstate the model:** Cached values from missing external links always carry the `signal` `▸` mark; manual-calc dirty cells display `⚠ Dirty [F9]`; unconverged iterations never display as `✓ Clean`; exception cells inside folded runs always surface visibly.

---

## 7. Customisation

DNA Calc treats customisation as a **declarative skin layer** usable identically by people and AI agents—changing how a workbook looks and feels without *ever* altering what the model calculates.

### 7.1 What a skin can declare (and when code is used)

A skin is a readable JSON/YAML manifest (stored in the workbook's `customXml` part or exported as a portable `.dnaskin` file) containing five declarative sections:

1. **Theme overrides:** Customizations over Strand's `--dna-*` tokens (chrome ramp, paper tint, typography family/size scale, row height, corner radius). Automatically checked by the **Strand Contrast & Safety Gate** before applying.
2. **Layout & panels:** Which views are enabled/preferred (per workbook or per sheet), default split drawers, Mast compactness, Parameter Rail / Plotter visibility, and App/Notebook card compositions bound to workbook addresses.
3. **Keymap:** Additional or remapped keyboard chords (e.g., Bloomberg/financial-modeling border shortcuts, Vim-like navigation outside Classic editing).
4. **Commands:** Named macro-free UI shortcuts surfaced in the Command Palette (`Ctrl+K`) and Bridge (e.g., `"Format as Model Total": apply bold + top thin / bottom double border + $#,##0`—strictly invoking built-in formatting/navigation primitives).
5. **Panels & widgets:** Declarative cards (KPI tile, slider, segmented switch, chart card, grouped table, markdown prose block) bound strictly to standard Excel addresses (`Loan!C3`, `Loan!Chart1`).
6. **Custom code (strictly bounded):** Code is permitted *only* when registering genuinely new visualization/widget components (e.g., a specialized 3D yield-curve plot or interactive warehouse map) via an explicitly installed, signed extension/add-in—**never** needed for theming, layout, keymaps, or App views, and never able to mutate calculation rules.

### 7.2 Creating, trying, and sharing a skin

1. **Make:** A user edits in the visual **Skin Studio** (`Ctrl+K → Customize Skin`) or describes what they want to the AI Agent (`Ctrl+I`).
2. **Try (Zero-risk Live Preview):** Skins enter via the same **Preview Seam**: DNA Calc runs the **Strand Contrast & Safety Gate** (verifying every foreground/background token pair passes WCAG AA `≥ 4.5:1` text / `≥ 3:1` UI, and verifying `red`/`signal`/focus semantics are unharmed), applies the skin live with a floating `[Keep Skin] [Save into Workbook] [Revert Esc]` bar, and reports any auto-adjusted contrast token transparently.
3. **Share:** One click either embeds the declarative skin inside the workbook's `customXml` part (so anyone opening `Loan.xlsx` in DNA Calc gets the custom App while Excel ignores it cleanly) or exports a `.dnaskin` file for personal/team reuse across all workbooks.

### 7.3 What stays fixed (the inviolable core)

No skin—human or agent-authored—may ever alter:
* **Calculation semantics & Excel file parity** (exact Excel recalculation, types, errors, `.xlsx` round-trip).
* **Semantic safety signals** (`#D02A23` error red reserved exclusively for errors; `signal` indicator for external/volatile feeds; clear distinction between cached vs live values and model vs view items).
* **Accessibility floor** (WCAG AA contrast gate, keyboard reachability, screen-reader landmarks, `prefers-reduced-motion` respect).
* **Undo, Esc revert, and View escape hatch** (users can always inspect underlying Classic formulas unless locked by standard Excel sheet protection, and can reset to default Strand theme in one click via Command Palette).
* **Macro & Add-in security boundaries.**

### 7.4 Two short worked examples

#### Example 1 — An agent restyles Classic for Maria
*Maria's prompt:* *"Make Classic feel like a dense dark trading terminal—19 px rows, subtle petrol-slate gridlines, monospace bold totals, and bind `Alt+Shift+B` to apply a top-thin/bottom-double accounting underline."*

```yaml
# Declarative skin diff (personal user skin · passes Strand Contrast Gate)
skin: "terminal-classic-v1"
extends: "strand:cockpit-dark"
tokens:
  grid-row-height: "19px"
  grid-line-color: "#223840"           # 3.1:1 border check: PASS
  total-row-font: "Recursive Mono 650" # tabular numerals preserved
keymap:
  classic.normal:
    "Alt+Shift+B": "cmd:accounting-total-border"
commands:
  accounting-total-border:
    label: "Apply Accounting Total Border"
    actions: [{ borderTop: "thin" }, { borderBottom: "double" }, { fontWeight: "bold" }]
```

*Result:* Live preview activates immediately in Classic; Strand's Contrast Gate confirms all `-ink` pairs pass `≥ 6.0:1` on `#1A262B` paper; `red` error cells and `amber` focus remain untouched. Maria clicks `[Save as Personal Classic Skin]`.

#### Example 2 — An agent builds an App view of `Loan.xlsx` from a three-line brief
*Maria's three-line brief:*
> 1. Turn `Loan.xlsx` into a clean Mortgage Planner App for Jo with sliders for Principal, Rate, Term (15/20/30), and Extra Monthly Payment.
> 2. Show Monthly Payment, Total Interest, and Payoff Date as top KPIs with Base vs custom scenario comparison, plus the existing balance chart.
> 3. Group the 360-row schedule by Year and make Page 1–2 print cleanly as a decision brief.

```yaml
# Stored in Loan.xlsx -> customXml/dnacalc-views.xml (0 lines of code)
preferredView: "app"
app:
  title: "Home Mortgage Planner"
  scenarios:
    bindRange: "Loan!C3:C7"
    presets:
      - { name: "Base 6.25%",     values: [420000, 0.0625, 30, 0,   "2027-01-01"] }
      - { name: "+$300/mo Extra", values: [420000, 0.0625, 30, 300, "2027-01-01"] }
      - { name: "Refi 5.50%",     values: [420000, 0.0550, 30, 0,   "2027-01-01"] }
  inputs:
    - { bind: "Loan!C3", label: "Principal", widget: "currency-slider",
        min: 100000, max: 1000000, step: 5000 }
    - { bind: "Loan!C4", label: "Annual Interest Rate", widget: "percent-slider",
        min: 0.02, max: 0.12, step: 0.00125 }
    - { bind: "Loan!C5", label: "Term (Years)", widget: "segmented",
        options: [15, 20, 30] }
    - { bind: "Loan!C6", label: "Extra Monthly Payment", widget: "currency-slider",
        min: 0, max: 1000, step: 25 }
    - { bind: "Loan!C7", label: "Start Date",           widget: "date" }
  kpis:
    - { bind: "Loan!C9",  label: "Monthly Payment", format: "$#,##0.00" }
    - { bind: "Loan!C10", label: "Total Interest",  deltaPolarity: "lower-is-better" }
    - { bind: "Loan!C11", label: "Payoff Horizon" }
  body:
    - { widget: "excel-chart",   bind: "obj:Loan/Chart1", overlayBaseline: true }
    - { widget: "grouped-table", bind: "Loan!A14:G374",   groupBy: "Year", collapsed: true }
  print: { pages: ["kpis+chart+scenarios", "annual-summary"], includeFullScheduleOption: true }
```

*Result:* Produces the exact interactive experience shown in Storyboard 2.5 (`M-08`, `M-16`) with zero custom code and 100% Excel compatibility.

---

## 8. Printing and macros

### What each view prints

Every view prints cleanly via `Ctrl+P` using a shared live **Print Preview** overlay (PDF export or physical print), never forcing users back into Classic just to print a clean page:

| View / Tool | Default Print Output | Honesty & Parity Guarantees |
| :--- | :--- | :--- |
| **Classic** | Exact Excel Print Area, manual/automatic page breaks, repeating header rows/columns, scaling (`Fit to 1 Page Wide`), headers/footers, and author formatting. | Strict visual parity with Excel's printed page; Provenance Overlay pips default OFF on print (optional checkbox `[ ] Include diagnostic overlay`). |
| **Inspector** | Structured **Audit Report PDF**: Workbook Health Summary, Root-Cause Error Table (with cell coordinates & formulas), Structural Anomalies, Missing External Links, Volatile `OFFSET` Inventory, and Active Trace Diagram. | Includes file hash, timestamp, calculation state (`Clean` vs `Cached/Dirty`), and exact counts so an audit PDF is defensible evidence. |
| **Notebook** | Typeset publication PDF using Source Serif 4 prose, live resolved values, clean vector charts, and gracefully paginated Data Frames (repeating headers across page breaks). | Sampled Data Frames print a visible footer disclosure (`First 200 of 25,000 rows printed; aggregates exact over all 25,000`) or offer `[ ] Expand full table`. |
| **Model Map** | Vector **Engineering Drawing Sheet** with Strand title block (workbook, active scope, director settings, tick/state stamp, legend)—single-page fit or tiled multi-page poster, plus optional one-page-per-composite appendix. | Declined/folded sheets print their exact `L2 Formula Runs` summary with the coverage disclosure stamp (`12/12 runs · 182,400 formulas`). |
| **App** | Executive **Decision Brief** (typically 1–2 pages): active scenario assumptions, comparison table, KPI strip, live vector chart, and grouped summary schedule (e.g., `Loan`'s 30 annual rows, with checkbox to append all 360 monthly rows). | Discloses active scenario parameters and print timestamp; never drops schedule rows silently. |
| **Formula Explorer** | Single-page **Formula Verification Card**: formula text, inputs, evaluated AST tree, format preview, and add-in version. | Ideal for code review or add-in bug reports. |

### What a macro can do to views

VBA macros and Excel add-ins execute under standard Excel security quarantine (blocked by default with the Mast trust banner until explicitly enabled by the user). Once trusted, macros run normal Excel object-model operations (`Range.Value`, `Worksheet.Activate`, `Range.Select`, `Calculate`) seamlessly—with **Classic** following `Activate`/`Select` automatically—and may optionally call a small, non-breaking **DNA Calc View API** (exposed safely as an optional host extension that no-ops gracefully in Excel):

* **Allowed view actions:** `SwitchView(viewId, sheet?)`, `SelectAddress(canonicalAddress)`, `ShowPanel(panelId, open)` (`"formulaExplorer"`, `"inspector"`, `"parameterRail"`, `"plotter"`), `SetFormulaLens(enabled)`, `SetScenario(scenarioName)`, and `ShowNoticeBanner(message, severity)` (non-modal info/warning toast in the Bridge).
* **Forbidden macro actions:** A macro may **never** spoof or suppress security/macro banners, override the WCAG contrast gate or `red`/`signal` safety semantics, bypass the Agent Preview Seam, hide the View Switcher escape hatch, or silently alter calculation semantics outside Excel's own `Application.Calculation` / `Iteration` properties.

---

## 9. Testing the experience

### 9.1 Usability-test scripts (derived directly from Section 2 storyboards)

Each storyboard runs immediately as a moderated 15-minute usability test (desktop + browser, keyboard-only and screen-reader variants included):

| Script | Persona & Workbook | Core Task Prompt | Objective Success Criteria |
| :--- | :--- | :--- | :--- |
| **UT-1** | Analyst · `Inherited.xlsx` | *"You just inherited this file. Without editing it yet, tell me how many independent root problems exist, why it feels slow, and inspect `Dispatch_Hourly`."* | Reaches first paint `< 300 ms` with zero modal blocks; identifies all **4 root causes**, **3 missing links**, and **9 `OFFSET` names** in `≤ 3 min`; understands Model Map's 12-run summary without thinking rows were deleted. |
| **UT-2** | Analyst · `Budget.xlsx` | *"Turn this budget into a 1-page live memo for your director with live Revenue/EBITDA figures and a Stretch toggle, then copy the summary into email."* | Creates Notebook draft, binds `≥ 2` live prose chips, toggles `Base ↔ Stretch` verifying live update, pastes a linked Classic range, and undoes an input edit from Classic cleanly (`≤ 6 min`, `≤ 1` prompt hint). |
| **UT-3** | Auditor · `Sales.xlsx` | *"Q3 EMEA on `Report` doesn't match the PivotTable. Find out why, confirm what else is wrong downstream, and fix it cleanly."* | Pinpoints hardcoded `$20000` ceiling + 493 excluded rows (`$118,420`) and downstream Rebate Tier error (`F8/G8`) in `≤ 4 min`; applies structured-ref fix reducing `D14` to `$0`; verifies `Ctrl+Z` revert. |
| **UT-4** | Systems Eng. · `Stock.xlsx` | *"Build an inventory loop with 3-day reorder delay and a ReorderPoint slider on Model Map, run 10 ticks, then verify Excel parity in Classic."* | Wires boxes without help; resolves circular-ref Preview Seam in 1 click (enabling Iteration); runs ticks seeing topological ripple; locates exact formula & Form Slider in Classic (`≤ 8 min`). |
| **UT-5** | Manager · `Loan.xlsx` | *"Compare paying `$300` extra per month against the Base loan, try typing an invalid `-2%` rate to see what happens, and export a 2-page PDF."* | Zero accidental formula edits; recovers from `-2%` validation refusal in `< 5 s`; creates side-by-side comparison discovering `$142,795` interest savings; prints clean 2-page brief (`≤ 4 min`). |
| **UT-6** | Add-in Dev · Standalone + `Budget` | *"Load `TaxPack.xll` with no workbook open, diagnose why `TAX.EFFRATE(290311,"CA",TRUE)` fails, verify `0.00%` formatting, then test and commit it in `Budget!Summary!B13`."* | Loads unsigned add-in via sandbox prompt; catches `"US-CA"` arg error via Eval Tree in `< 90 s`; verifies preview deltas (`-$20,276` Net Income) in Classic before committing (`≤ 5 min`). |
| **UT-7** | Analyst + Agent · `Budget.xlsx` | *"Ask the agent for +5% Q3 pricing and 15% Marketing. Review impact in both Classic and Notebook, keep pricing, reject or undo Marketing."* | Reviews staged Preview Seam across both views without premature commit; accepts Hunk 1 cleanly; reverts Hunk 2 via Reject or single `Ctrl+Z`; locates both actions in Audit Log (`≤ 4 min`). |
| **UT-8** | Keyboard / SR Audit · All 5 | *"Execute UT-1, UT-3, UT-4 (wire with `W`), and UT-5 using keyboard + screen reader only at 200% zoom and `high-contrast` theme."* | 100% task completion with zero keyboard traps, zero unlabeled icons, accurate `aria-live` recalc announcements, zero WCAG AA contrast failures. |

### 9.2 Mandatory visual-review screenshot matrix

Before any release milestone ships, reviewers inspect automated golden screenshots covering **24 critical states** across the five cast workbooks (tested in `cockpit-light`, `cockpit-dark`, and `high-contrast`):

| Workbook | Classic | Inspector | Notebook | Model Map | App | Formula Explorer |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **`Inherited`** | First paint mid-index + triage banner (`M-01`); `Z4410` `#DIV/0!` | 40-sheet Atlas + 4 root causes (`M-02`) | Sheet-picker refusal notice | `Dispatch_Hourly` 12-run refusal (`M-03`) | Auto-App refusal notice | Debugging broken `VLOOKUP(...,#REF!)` |
| **`Budget`** | Mid-edit range pick (`M-12`); Agent Preview Seam (`M-11`) | Pre-board health & calc order | Vertical memo (`M-04`); Pipeline (`M-14`) | 3-sheet composite flow | Executive scenario app | Embedded drawer on `B13` (`M-10`, `M-17B`) |
| **`Sales`** | `Report!D8` vs Pivot mismatch; `Orders` row 20,001 highlight | Coverage Probe + Formula Lens (`M-05`, `M-13`) | Sampled 200/25k Data Frame notice | `Report` block flow + `Orders` source block | *(N/A for raw Orders)* | `FILTER` + `XLOOKUP` array preview |
| **`Loan`** | 360-row schedule + Excel Chart | Dependency check on `IPMT` | Annotated loan brief | *(Optional compact flow)* | Scenario compare + `-2%` refusal (`M-08`, `M-16`) + Print Preview | `PMT` Number Format & CF tab |
| **`Stock`** | Backing sheet + Form Slider + spill (`M-07`) | Circular iteration telemetry | Policy write-up | Mid-wire circular refusal (`M-06`); Live ripple (`M-15`) | Interactive policy slider app | `HSTACK`/`DROP` dynamic array eval |
| *(No file)* | Empty Welcome Stage | *(N/A)* | *(N/A)* | Empty starter canvas | *(N/A)* | Standalone empty + `TaxPack.xll` (`M-09`, `M-17A`) |

---

## 10. Where I would push back

The sections above faithfully realize every point in the settled direction. If I were advising the owner before locking implementation requirements, here are **five specific parts of the settled direction I would push back on**—and what I would do instead:

1. **Per-sheet automatic view switching causes context whiplash.**
   - *Risk:* If Sheet 1 (`Inputs`) is bound to Classic, Sheet 2 (`Monthly`) to Model Map, and Sheet 3 (`Summary`) to Notebook, clicking sheet tabs across the bottom Strip abruptly rearranges the entire screen geometry every click—breaking spatial memory and alarming Excel power users.
   - *Recommendation:* Keep the **active view pinned across sheets by default** once the user is working (`Classic` stays `Classic` across tabs), and show a sheet's preferred view as a calm, clickable **Bridge pill** (`Recommended view for Summary: Notebook [Switch Ctrl+Alt+3]`)—activating automatic per-sheet view switching *only* when the user explicitly turns on "Guided / Published Mode" (e.g., Jo opening an App).
2. **Bidirectional "edit by dragging wires" on Model Map should be scoped strictly to named-actor blocks, not arbitrary A1 grid ranges.**
   - *Risk:* `LINKED_BOXES_SKIN.md`'s dotted-name wiring works beautifully for clean models built on Model Map (`Stock`). On 95% of real Excel sheets (`Budget`, `Sales`, `Inherited`), formulas reference relative A1 ranges (`B5:M5`, `SUMIFS` with mixed `$` anchors). Attempting to synthesize or rewrite positional range formulas from a wire drag risks silently corrupting anchors or generating ugly, brittle Excel formulas.
   - *Recommendation:* Keep Model Map **read, simulate, inspect, and navigate everywhere**, but enable **drag-to-wire formula editing exclusively on explicit named-actor ports** (boxes backed by defined names). On positional grid blocks, dragging a wire refuses gently (*"Grid block uses positional formulas—double-click to edit in Formula Explorer or Classic"*).
3. **Formula Lens (`LET`/`LAMBDA` synthetic rewriting) should be read-only comprehension, never an editable synthetic AST.**
   - *Risk:* Allowing users to type edits directly inside a synthetically rewritten `LET` view creates a nasty mental split: users assume the `LET` they edited is saved in the workbook, only to open Excel and find either a mangled auto-inlined formula or unexpected structural changes.
   - *Recommendation:* Make **Formula Lens (`Alt+L`) strictly a read/trace comprehension lens** (with live inline value chips and instant raw-Excel hover). When a user enters `EDITING`, edit the real underlying formula with inline name-alias badges—and offer an explicit, one-click Command Palette action **"Refactor formula to real `LET` in workbook"** so the `.xlsx` file itself gets cleaner in Excel too.
4. **Shipping standalone "DNA Calc — Formula Explorer" as an early parallel deliverable splits product focus.**
   - *Risk:* Without a workbook, standalone Formula Explorer serves a narrow niche (mostly add-in developers like Ade and occasional formula learners), yet requires its own installer/window shell, local-input persistence, and add-in host lifecycle right when Classic + Inspector + embedded Formula Explorer need every ounce of polish.
   - *Recommendation:* Design Formula Explorer with **zero hard coupling to a workbook** from Day 1 (so it works identically with local inputs or cell precedents), **ship it embedded inside DNA Calc first**, and package the standalone `"DNA Calc — Formula Explorer"` stripped build as a thin final packaging milestone once embedded Explorer and the add-in host are battle-tested.
5. **Allowing code-backed custom components inside workbook-embedded skins opens a second macro-security hole.**
   - *Risk:* The settled direction says *"Code is used only for new components."* If `.xlsx` files can bundle executable component code inside their skin part, enterprise security teams will distrust and strip DNA Calc skins the exact way they block `.xlsm` VBA macros—undermining the "safe plain `.xlsx` with modern surface" value proposition.
   - *Recommendation:* Mandate that **workbook-embedded skins (`customXml`) are 100% declarative JSON/YAML with zero executable code**—safe to render immediately on untrusted files with zero security prompts. Any code-backed custom widget must ship strictly as a signed, user-installed **Add-in / Extension** governed by the existing add-in trust model, referenced by ID from the declarative skin (falling back cleanly to a standard table/chart if the extension is not installed).

---

## 11. Risks, open questions for the owner, and a milestone order

### 11.1 Key UX & product risks

| Risk | Severity | Concrete Mitigation |
| :--- | :---: | :--- |
| **Classic muscle-memory regressions** (one broken `F4`, `Ctrl+Enter`, or double-click fill gesture loses Excel power users permanently) | High | Treat **Appendix A** as a blocking automated conformance suite (every gesture tested in CI on desktop + browser before Classic exits beta). |
| **Browser keyboard shortcut hijacking** (`Ctrl+W`, `Ctrl+T`, `Ctrl+N`, `Ctrl+1..5`, `Alt` menus intercepted by Chrome/Edge/Safari) | High | Full native capture on desktop + installed PWA (`Keyboard Lock API` where supported); deterministic documented fallback chords (`Alt+Shift+1..5` / `Ctrl+K` palette) surfaced dynamically in browser tooltips. |
| **False-positive pattern inference on Enron-style messes** (`Inherited`) misclassifying interleaved legacy sheets | Medium | Strict confidence gating (`< 0.50` stays raw grid), zero silent mutations, one-click `[Dismiss / Reclassify]` persisted in `customXml`. |
| **Round-trip stripping of `customXml` by third-party tools** (Excel preserves `customXml`, but LibreOffice/Google Sheets/some ETL libraries may drop unknown parts) | Medium | Keep every workbook 100% self-sufficient without `customXml` (auto-inference regenerates sensible Notebook/Inspector/Map views on open); warn cleanly if a reopened file lost its saved skin part. |
| **Visual drift between Strand and custom/agent skins** breaking contrast or semantic trust (`red` / `signal` / `amber`) | Medium | Enforce the automated **Strand Contrast & Safety Gate** at skin load/preview time—never relying on manual review. |

### 11.2 Open questions for the owner (5 decisions, with recommendations)

1. **Browser vs Desktop keyboard parity (and AltGr ergonomics):** We specify `Ctrl+Alt+1..5` for global view switching so Excel's `Ctrl+1` (Format Cells), `Ctrl+Shift+1..6` (Number Formats), `Alt` (Key Tips), and browser tab switching (`Ctrl+1..5`) never collide—plus `Alt+Shift+1..5` as a European `AltGr`-safe alias. Should the browser build also request Chromium's `Keyboard Lock` in PWA/fullscreen mode to capture `Ctrl+W` / `Ctrl+N`, or rely strictly on Command Palette / documented web fallbacks?
   - *Recommendation:* Adopt `Ctrl+Alt+1..5` (with `Alt+Shift+1..5` alias) uniformly across desktop and browser, keep `Ctrl+1` strictly for Classic Format Cells, and use `Keyboard Lock` opportunistically in installed PWA mode while always showing active shortcuts dynamically in tooltips.
2. **Per-sheet vs workbook-pinned views (Pushback #1):** Should clicking a sheet tab automatically switch views when a sheet declares a preferred view, or stay in the user's active view with a one-click Bridge suggestion chip?
   - *Recommendation:* Keep active view pinned during authoring; auto-switch only in published App / Guided mode.
3. **App view input locking vs Excel Worksheet Protection:** When an App author designates five editable inputs (`Loan!C3:C7`), should saving enforce *only* App-view UI read-only guards (leaving formulas freely editable in Classic/Excel) or optionally synchronize Excel's native Locked-cell / Worksheet Protection flags?
   - *Recommendation:* UI-level read-only by default (so Excel users aren't locked out by surprise passwords), plus an opt-in *"Also protect formula cells in Excel"* toggle in App export settings.
4. **Model Map Director playback vs pure Excel manual/automatic recalc:** In `Stock`, clicking `[▶ Run]` triggers periodic `F9` recalculations from the front-end UI timer (or an RTD clock add-in as in `watershed.html`). Should DNA Calc ship a built-in, optional local `DnaCalc.Clock` RTD provider out of the box, or drive `[▶ Run]` purely by front-end UI `F9` ticks?
   - *Recommendation:* Drive Model Map `[▶ Run]` / `[Step]` via front-end `F9` recalculations by default (zero external dependency, identical to pressing `F9` in Excel) and keep `DnaCalc.Clock` as an opt-in add-in for heterogeneous RTD/event models like `watershed.html`.
5. **Workbook-embedded skin safety (Pushback #5):** Confirm whether `.xlsx`-embedded skins (`customXml`) are strictly 100% declarative (zero code), restricting custom coded widgets strictly to installed signed add-ins/extensions.
   - *Recommendation:* Strictly 100% declarative inside `.xlsx` files so opening any shared workbook with a custom App or Notebook skin never triggers a macro-style security scare.

### 11.3 Milestone order (no calendar dates)

1. **M0 — Strand Shell & High-Scale Foundation:** Petrol Mast/Bridge/Strip chrome, Strand v0.2 token & WCAG contrast test suite (`cockpit-light`, `cockpit-dark`, `high-contrast`), Unified Address Grammar, non-blocking off-thread loading + cached first paint (`Inherited` `< 250 ms`), Unified Dual-Tagged Undo stack, Command Palette (`Ctrl+K`).
2. **M1 — Classic Parity & Triage Banner:** Full virtualized Classic grid passing the **Appendix A** keyboard/mouse conformance suite, Provenance Overlay (`Ctrl+Shift+P`), non-modal Triage & Macro/Link Security Banners, Excel-parity Print Preview.
3. **M2 — Inspector & Embedded Formula Explorer:** Phyphox-style Inspector (Workbook Atlas, Root-Cause Error Collapse, Causal Trace, Structural Anomaly Probes like partial-table coverage, Calc Order waves, Volatile/RTD monitor), Formula Lens (`Alt+L`), and embedded **Formula Explorer** drawer (`Eval Tree`, `Number Format`, `Cond. Format`, Preview Seam).
4. **M3 — Pattern Inference Engine & Notebook View:** Non-blocking 18-pattern recognizer with confidence tiers & two-click confirmation persisted in `customXml`; Vertical + Horizontal Pipeline Notebook with Source Serif 4 prose, live value chips, sampled Data Frames, inline charts, smart cross-view copy/paste, and publication PDF printing.
5. **M4 — Model Map & Linked-Boxes Presenter:** Hierarchical composite canvas, AST Formula-Name Presenter/Interpreter (`LINKED_BOXES_SKIN.md`), `L0–L3` Semantic Zoom & R1C1 Formula-Run folding (`Inherited` honest refusal), Directors (`Recalc`, `Iteration`, `Ticks`, `Sweep`, `Events`), `z⁻¹` delay chips, circular-ref Preview Seam, Parameter Rail + Plotter, and Engineering Drawing print.
6. **M5 — App View, Declarative Skins & Agent Preview Seam:** Declarative Skin manifest + Contrast/Safety Gate + Skin Studio; App view with validated inputs, Scenario Comparison, grouped schedules, and Executive Brief print; full AI Agent Staged Proposal Batch, semantic hunk review, selective accept/undo, and session Audit Log across all views.
7. **M6 — Standalone Formula Explorer, Add-in Dev Sandbox & Macro View Hooks:** Stripped zero-workbook `"DNA Calc — Formula Explorer"` desktop/web build, local `.xll`/JS dev sandbox + hot-reload for add-in authors (`Ade`), and optional safe Macro View API (`SwitchView`, `SelectAddress`, `ShowPanel`).

---

## Appendix A: Classic keyboard and mouse conformance

Every item below is a binding front-end conformance requirement for **Classic** view. Where desktop and browser platforms differ (e.g., browser interception of `Ctrl+W` or `Alt`), the desktop build enforces 100% native capture and the browser build provides the documented fallback transparently in tooltips and Command Palette (`Ctrl+K`).

### A.1 Key tips & command access

| Gesture / Key | Expected Behaviour |
| :--- | :--- |
| **`Alt` (tap & release)** | Activates **Key Tips** overlay across the Strand Action Band (`H` Home, `N` Insert, `M` Formulas, `A` Data, `R` Review, `W` View) and Mast quick actions. Badges appear within `≤ 30 ms` using high-contrast petrol/amber pills (never Microsoft ribbon artwork). |
| **Sequential Key Tip chords** (e.g., `Alt`, `H`, `V`, `V` for Paste Values; `Alt`, `H`, `B`, `O` for bottom border; `Alt`, `H`, `O`, `I` for AutoFit column width) | Executes matching command cleanly without stealing focus from the cell selection; partial prefix highlights valid next keys; `Esc` steps back one level and exits Key Tips cleanly. Browser fallback when OS/browser captures lone `Alt`: `F10` activates identical Key Tips. |
| **`Ctrl+K` / `Alt+Q`** | Opens the unified **Command Palette** (fuzzy search over all formatting, view, workbook, skin, and navigation commands, showing live shortcut hints). |
| **`Shift+F10` / Context key** | Opens the Strand context menu anchored to the active cell/selection (never off-screen). |

### A.2 Navigation

| Gesture / Key | Expected Behaviour |
| :--- | :--- |
| **`↑` `↓` `←` `→`** | Moves active cell by 1 cell in `SELECTED` mode; respects hidden rows/columns and frozen panes; scrolls smoothly when approaching viewport edge. |
| **`Ctrl+↑` / `↓` / `←` / `→`** | Jumps to the edge of the current contiguous non-blank data region; if on a blank cell or at a region edge, jumps to the next non-blank cell (or sheet boundary). Exact Excel edge-skip semantics. |
| **`End`, then Arrow** | Enters sticky single-shot **End Mode** (`End Mode` readout in Strip) and behaves identically to `Ctrl+Arrow`, then exits End Mode. |
| **`Home` / `Ctrl+Home` / `Ctrl+End`** | `Home`: Column A of current row (respecting frozen panes). `Ctrl+Home`: `A1` (or top-left unfrozen cell). `Ctrl+End`: intersection of last used row and last used column. |
| **`PgUp` / `PgDn` · `Alt+PgUp` / `Alt+PgDn`** | Vertical / horizontal scroll by one full viewport page while moving the active cell by the same offset. |
| **`Ctrl+PgUp` / `Ctrl+PgDn`** | Switches to previous / next visible sheet tab (skipping hidden sheets). |
| **`Ctrl+Backspace`** | Scrolls viewport immediately back to bring the active cell into view without altering the current selection. |
| **`Tab` / `Shift+Tab` + `Enter` memory** | Tabbing across columns (`B5 → C5 → D5`) and then pressing `Enter` returns the cursor to the **starting column of the Tab run** one row down (`B6`), matching Excel's data-entry rhythm. |
| **`Scroll Lock` + Arrows** | Pans viewport by 1 row/column without moving the active cell (`Scroll Lock` indicator shown in Strip). |
| **`F5` or `Ctrl+G`** | Focuses the **Unified Address Box / Go To** popover (accepts A1, R1C1, defined names, table refs, `block:` paths, recent jump list, plus **`Special…`** for Blanks, Constants, Formulas, Errors, Precedents, Visible Cells Only `Alt+;`). |
| **`Alt+←` / `Alt+→`** | Steps backward/forward through the cross-sheet, cross-view Navigation History Stack. |

### A.3 Selection & multi-area selection

| Gesture / Key | Expected Behaviour |
| :--- | :--- |
| **`Shift+Arrows` / `Ctrl+Shift+Arrows`** | Extends rectangular selection from anchor cell by 1 cell / to data-region edge. |
| **`Ctrl+Space` / `Shift+Space`** | Selects entire column(s) / row(s) of active selection; inside an Excel Table (`Orders`), first press selects the **Table column/row data**, second press includes headers, third press selects the full worksheet column/row! |
| **`Ctrl+A` (staged expansion)** | First press selects current contiguous region (or Table data body); second press includes Table headers/totals or expands to full worksheet; third press selects entire worksheet. |
| **`F8` (Extend Mode)** | Toggles sticky selection extension via arrows/clicks without holding `Shift` (`Extend` readout in Strip; `Esc` exits). |
| **`Shift+F8` (Add-to-Selection) / `Ctrl+Click` & drag** | Adds disjoint cells or ranges to a **multi-area selection**; Strip aggregates (`Sum`, `Avg`, `Count`) compute across all non-overlapping areas. |
| **`Ctrl+.` (period) / `Tab` / `Enter` inside multi-cell selection** | `Ctrl+.` rotates the active cell clockwise across the four corners of the selection; `Enter`/`Shift+Enter`/`Tab`/`Shift+Tab` cycle within the selection without collapsing it. |
| **`Alt+;`** | Selects **Visible Cells Only** inside the current selection (skipping hidden/filtered rows and columns so copies and edits never touch hidden cells). |

### A.4 `F2`, `F4`, `F9`, and formula editing

| Gesture / Key | Expected Behaviour |
| :--- | :--- |
| **`F2` (Selected → Editing)** | Opens in-cell editing (or Formula Bar if active cell is off-screen) with caret at the end of existing text; enters **`EDITING (Edit)`** state and highlights all referenced ranges on the sheet with Strand's numbered 6-tone palette (`① ② ③`). |
| **`F2` (while already Editing)** | Toggles between **`EDITING (Edit)`** (arrows move text caret inside formula) and **`EDITING (Point)`** (arrows pick cells/ranges on the sheet). Strip clearly displays `EDITING (Edit)` vs `EDITING (Point)`. |
| **`F4` (while Editing a formula)** | Cycles the cell/range reference touching or spanned by the text caret through all four Excel anchor states: **`A1 → $A$1 → A$1 → $A1 → A1`** (or for ranges `B5:M5 → $B$5:$M$5 → B$5:M$5 → $B5:$M5 → B5:M5`). Works across multiple highlighted references at once. |
| **`F4` (outside Editing)** | **Repeats the last formatting or structural action** (e.g., apply fill, insert row, delete column, apply border) cleanly on the current selection. |
| **`F9` (outside Editing)** | Recalculates all dirty formulas across all open workbooks (and advances one step under `Ticks` / `Iteration` director). |
| **`Shift+F9` / `Ctrl+Alt+F9` / `Ctrl+Alt+Shift+F9`** | `Shift+F9`: recalculates active worksheet only. `Ctrl+Alt+F9`: forces full recalculation of all formulas regardless of dirty state. `Ctrl+Alt+Shift+F9`: rebuilds full dependency graph and forces full recalculation. |
| **`F9` (while Editing with sub-expression selected)** | **Evaluates only the selected sub-expression inline** inside the Formula Bar/cell (e.g., highlighting `MATCH(A9, Curve_Lookup!$A$2:$A$500, 0)` replaces it live with `#N/A` or `412` so the user inspects intermediate results). Pressing **`Esc`** reverts the entire edit cleanly without baking literals into the formula! |
| **Picking ranges while typing a formula** | After typing `=`, an operator (`+`, `-`, `*`, `/`, `,`, `(`, `:`), or inside a function argument: clicking or dragging cells, pressing arrow keys (with `Shift` / `Ctrl+Shift`), or clicking another sheet tab inserts the live reference at the caret with colored numbered bounding box (`①`). Dragging a reference's border moves the reference; dragging its corner handle resizes the reference! Selecting a full column inside an Excel Table inserts structured syntax (`Orders[Amount]`) automatically. |
| **`Tab` / `Ctrl+Shift+A` / `Alt+Enter` (while Editing)** | `Tab` accepts the highlighted function/defined-name/table autocomplete suggestion (opening `(` for functions); `Ctrl+Shift+A` after a function name inserts parameter placeholders; `Alt+Enter` inserts a literal line break inside the cell/formula. |
| **`Esc` (while Editing)** | Discards uncommitted edit completely and returns to `SELECTED` with original value/formula untouched. |

### A.5 Commit, batch entry (`Ctrl+Enter`), and rapid fill

| Gesture / Key | Expected Behaviour |
| :--- | :--- |
| **`Ctrl+Enter`** | Commits the active entry or formula to **every cell in the current selection (including disjoint multi-area selections)** simultaneously, adjusting relative references per cell exactly as Excel does, and keeps the active cell and selection intact. |
| **`Enter` / `Shift+Enter` / `Tab` / `Shift+Tab`** | Commits and moves down / up / right / left (or wraps within multi-cell selection). |
| **`Ctrl+D` / `Ctrl+R`** | **Fill Down / Fill Right:** Copies top row / leftmost column (formulas + formats) across the selected height/width; if only one row/column is selected, copies from the single cell immediately above / to the left. |
| **`Ctrl+;` / `Ctrl+Shift+;`** | Inserts static current local date / static current local time. |
| **`Ctrl+'` / `Ctrl+"`** | `Ctrl+'` copies the **exact raw formula** from the cell directly above *without shifting relative references* and stays in `EDITING`; `Ctrl+"` copies the **evaluated value** from the cell directly above. |
| **`Ctrl+Shift+Enter`** | Preserves legacy CSE array-formula entry semantics for backward compatibility when authoring legacy array workbooks (dynamic arrays spill automatically on plain `Enter`). |

### A.6 Mouse gestures: Fill Handle, Double-Click Fill, Drag-Move & Drag-Copy

| Mouse Gesture | Expected Behaviour |
| :--- | :--- |
| **Fill Handle drag** (bottom-right square handle of selection) | Extends formulas (shifting relative references), copies constants, or continues detected sequences (numbers `1, 2 -> 3, 4`, dates, months `Jan, Feb -> Mar`, quarters `Q1..Q4`, text+number `Item 1`). Renders a live ghost **Preview Seam** showing the prospective final cell value before mouse-up, and shows a post-drop `[Fill Options]` chip (`Copy Cells | Fill Series | Formats Only | Without Formatting`). |
| **`Ctrl` + Fill Handle drag** | Inverts default fill behavior (turns constant copy into `+1` increment, or turns series increment into literal copy), shown live by a `+` cursor badge and live preview tooltip. |
| **Right-click + Fill Handle drag** | Releases into an explicit Fill menu (`Copy`, `Series…`, `Formats`, `Values`, `Fill Days/Weekdays/Months/Years`, `Linear/Growth Trend`). |
| **Double-click Fill Handle** | Auto-fills downward to match the exact contiguous populated extent of the **immediately adjacent left column** (or right column if left is blank), stopping at the first blank gap or Table total row. Renders a brief Undo-ready toast (`Filled 359 rows -> G374 · [Undo Ctrl+Z]`). |
| **Drag Fill Handle inward (up/left over own selection)** | Shades covered cells in muted hatch; mouse-up clears their contents (preserving formats). |
| **Drag selection border (Drag-Move)** | Moves cells (values, formulas, formatting) to the target location and **updates all dependent formulas across the workbook** to point to the moved cells (identical to Cut+Paste). Live Preview Seam highlights target footprint and warns if overwriting non-empty cells (respecting user preference). |
| **`Ctrl` + Drag border (Drag-Copy)** | Copies cells (shifting relative formula references) to the target footprint without disturbing the source (`+` badge on cursor). |
| **`Shift` + Drag border (Move & Insert)** | Shows a crisp teal horizontal/vertical **Insertion I-Beam** between rows/columns; mouse-up moves and shifts displaced cells down/right without overwriting anything. |
| **`Ctrl+Shift` + Drag border (Copy & Insert)** | Same Insertion I-Beam preview, copying and shifting displaced cells. |
| **`Alt` + Drag border over a Sheet Tab** | Switches smoothly onto that sheet after a 400 ms hover dwell so the user can drop/move cells directly across worksheets. |
| **Column/Row divider double-click & drag** | Drag resizes selected columns/rows uniformly; double-click AutoFits to widest rendered cell content; works across multi-selected columns/rows. |

### A.7 Copy, Cut, Paste, Paste Special & external clipboard interop

| Gesture / Key | Expected Behaviour |
| :--- | :--- |
| **`Ctrl+C` / `Ctrl+X`** | Sets animated Strand dashed border around source range (respects `prefers-reduced-motion` with static double-dashed border). `Ctrl+X` enters **Live Cut**: pasting relocates formulas and rewrites workbook dependents cleanly; `Esc` cancels cut/copy state. |
| **`Ctrl+V` / `Enter` (while cut/copy active)** | `Ctrl+V` pastes full cell contents (preserving clipboard for repeated pastes) and offers a non-blocking `[Paste Options (Ctrl)]` chip; `Enter` pastes once and clears clipboard state. |
| **`Ctrl+Alt+V`** | Opens Strand **Paste Special** popover with keyboard letters (`V` Values, `F` Formulas, `T` Formats, `U` Values & Number Formats, `E` Transpose, `W` Column Widths, `L` Paste Link, `B` Skip Blanks, `D/S/M/I` Add/Subtract/Multiply/Divide). |
| **External clipboard interop** | Copying from Classic writes TSV (plain text), styled HTML table, and internal DNA Calc range metadata so pasting into Excel, Word, Slack, Notion, or a text editor works seamlessly. Pasting TSV/CSV/HTML tables from Excel or browsers parses numeric/date formats accurately without corrupting leading zeros on text cells. |

### A.8 Formatting, auditing, structure, tables & find

| Gesture / Key | Expected Behaviour |
| :--- | :--- |
| **`Ctrl+1`** | Opens **Format Cells** panel/modal (Number, Alignment, Font, Border, Fill, Protection) with live preview on active selection. *(Never stolen by view switching—view switching uses `Ctrl+Alt+1..5`.)* |
| **`Ctrl+Shift+1`..`6` and `~`** | Instant Excel number formats: `~` General, `1` `#,##0.00`, `2` `h:mm AM/PM`, `3` `d-mmm-yy`, `4` Currency, `5` `0%`, `6` `0.00E+00`. |
| **`Ctrl+B` / `I` / `U` / `5`** | Bold, Italic, Underline, Strikethrough. |
| **`Ctrl+Shift++` / `Ctrl+-`** | Insert / Delete cells, rows, or columns (instant when entire row/column selected; compact shift popover when partial range selected). |
| **`Ctrl+9` / `Ctrl+0` (`Ctrl+Shift+9/0`)** | Hide / Unhide selected rows (`9`) and columns (`0`). |
| **`Ctrl+~` (backtick)** | Toggles **Show Formulas** mode across the active Classic sheet (auto-widening columns temporarily for readability, restoring widths on toggle off). |
| **`Ctrl+[` / `Ctrl+]` (`Ctrl+Shift+{` / `}`)** | Selects direct precedents / direct dependents (`Shift`: all transitive precedents / dependents), jumping cleanly even across sheets. |
| **`Alt+=`** | **AutoSum**: inserts `=SUM(...)` targeting the adjacent contiguous numeric block above (or left) and highlights the range in `EDITING` for confirmation. |
| **`Ctrl+T` / `Ctrl+Shift+L`** | `Ctrl+T` converts contiguous region to an Excel Table (`ListObject`) with structured refs; `Ctrl+Shift+L` toggles AutoFilter dropdowns (`Alt+↓` opens active header filter or column autocomplete list). |
| **`Ctrl+F` / `Ctrl+H`** | Opens non-modal **Find / Replace** bar docked in the Bridge (supports Formulas vs Values vs Notes, Match Case, Match Entire Cell, Sheet vs Workbook scope, Find All navigable list). |
| **`Ctrl+Shift+P`** | Toggles Strand's non-destructive **Provenance Overlay** (constant/formula/volatile/external/error micro-ticks & inferred block frames) over author formatting. |
| **`Ctrl+Z` / `Ctrl+Y`** | Unified cross-view Undo / Redo (Section 3.4). |

---

## Appendix B: Front-end requirements (R-1 … R-48)

Every requirement below has a concrete, observable pass/fail verification criterion.

### B.1 Shell, Strand & Cross-View Continuity
* **R-1 (Unified Shell):** Every workbook view (`Classic`, `Inspector`, `Notebook`, `Model Map`, `App`) mounts inside the shared **Mast / Bridge / Strip** shell; switching views changes the Stage and view-contextual Bridge controls without recreating the window or resetting scroll/history.
* **R-2 (Strand Token & Contrast Gate):** All three built-in themes (`cockpit-light`, `cockpit-dark`, `high-contrast`) and any user/agent skin pass automated WCAG AA contrast checks (`≥ 4.5:1` text and small glyphs; `≥ 3:1` focus rings, borders, and meaningful non-text marks) on every sanctioned foreground/background token pair.
* **R-3 (Non-Color Redundancy):** Every semantic color cue (error `red`, volatile/external `signal`, focus `amber`, agent proposal `teal-hatch`, fresh `green`) pairs with a distinct non-color indicator (glyph `⊗/⊘/▸/⇄/↻`, border pattern, or label) legible in grayscale.
* **R-4 (Zero Microsoft Artwork):** The UI contains zero Microsoft icons, Segoe/Calibri hardcoded branding, ribbon artwork, or Office visual styles.
* **R-5 (Unified Address Box):** Typing any valid Cell, Range, Defined Name, Table/Column, Block, Object, or View Item address into the Bridge Address Box (`Ctrl+G` / `F5`) navigates to and selects that entity across all views.
* **R-6 (Selection & Continuity Halo):** Switching views via `Ctrl+Alt+1..5` (or Mast switcher) translates the active selection anchor cleanly (Section 3.2), centers the target in the viewport, and displays the 150 ms amber Continuity Halo (instant outline under `prefers-reduced-motion`).
* **R-7 (Unified Dual-Tagged Undo):** `Ctrl+Z` / `Ctrl+Y` undoes and redoes both `[Model]` and `[View]` transactions chronologically across view switches without unexpectedly changing views, displaying a Strip toast naming the reverted target.
* **R-8 (1-Bit Modeless State & Esc Unwind):** At all times the UI is in either `SELECTED` or `EDITING`; pressing `Esc` unwinds strictly one layer (`Editing → Selected → Preview Seam Dismissed → Open Drawer Closed`) with lossless revert.
* **R-9 (Per-Workbook & Per-Sheet View Preferences):** Saving a workbook persists preferred view, per-sheet view hints, Notebook/Model Map/App layouts, block overrides, and declarative skins inside an Excel-preserved `customXml` part; opening the saved `.xlsx` in Excel succeeds with zero repair warnings and identical calculated values.
* **R-10 (Never Trap in Non-Grid View):** Every view, including `App`, provides an explicit, keyboard-reachable path to inspect the underlying workbook in `Classic` or `Inspector` (subject only to standard Excel Worksheet Protection if set).

### B.2 Classic View Conformance
* **R-11 (Keyboard & Mouse Parity):** Classic passes 100% of the conformance behaviors listed in **Appendix A** (`Alt` Key Tips, navigation, multi-area selection, `F2`/`F4`/`F9` including inline sub-expression `F9`, `Ctrl+Enter`, Fill Handle & double-click auto-fill, drag-move/copy/insert, formula range-picking, `Ctrl+1` Format Cells, Paste Special).
* **R-12 (Numbered Formula Reference Highlighting):** While `EDITING` a formula in Classic, referenced ranges highlight on the grid using the 6-tone Formula Reference Palette (never `#D02A23` error red) paired with circled numeric indices (`① ② ③`) matching superscripts in the Formula Bar.
* **R-13 (Author Formatting Preservation & Provenance Overlay):** Author cell fills, borders, fonts, number formats, and Conditional Formatting render faithfully in Classic; Strand provenance marks (`▸` external/volatile, `⊗/⊘` root/propagated error, inferred block outlines) render on a toggleable overlay (`Ctrl+Shift+P`) without altering author formatting.
* **R-14 (Non-Modal Open Triage):** Opening a workbook with errors, missing external links, unsigned macros, hidden sheets, or volatile `OFFSET` names displays cached values immediately with zero blocking modal popups, presenting a single dismissible Triage/Security banner.

### B.3 Inspector View
* **R-15 (Workbook Atlas):** Inspector displays all worksheets—including `xlSheetHidden` and `xlSheetVeryHidden` clearly tagged—organized into topological dependency tiers at `L0 Atlas`.
* **R-16 (Root-Cause Error Collapse):** All workbook error cells collapse deterministically to their root-origin cells (e.g., `Inherited`: 47 error cells → 4 root causes), showing exact downstream affected counts and one-click navigation to origin cells.
* **R-17 (Structural Anomaly Probes):** Inspector surfaces non-error modeling hazards—specifically partial table coverage (`Sales!Report!D8` stopping at row 20,000 of a 25,000-row table), hardcoded overrides inside uniform formula runs, and volatile `OFFSET`/`INDIRECT` fan-out—with non-destructive verification links and Preview Seam fixes.
* **R-18 (Calculation Order & Live Telemetry):** Inspector displays topological evaluation waves with per-block recalc timings, RTD event rates, external-link resolution status, and iteration convergence deltas (`max iterations`, `max change`, converged vs unconverged).
* **R-19 (Audit Export):** One action copies or exports the active trace or full Workbook Health Summary as clean Markdown/CSV/PDF including coordinates, formulas, values, and calculation state.

### B.4 Notebook View
* **R-20 (First-Run Scaffold):** Opening Notebook on a workbook with no saved Notebook part presents a choice between an auto-scaffolded draft (from inferred workbook structure) and a blank canvas.
* **R-21 (Live Reactive Binding):** Inline prose chips (`«$2,889,851»`), editable assumption cards, Data Frames, and inline charts recalculate live on every input change without manual "Run All" execution.
* **R-22 (Excel-Backed Formula Blocks):** Creating a new formula block in Notebook allocates a real Excel cell and defined name and displays its backing address explicitly; zero floating non-Excel formulas exist.
* **R-23 (Vertical & Horizontal Pipeline Layouts):** Users can switch any Notebook between Vertical reading layout (`68 ch` Source Serif 4 measure) and Horizontal left-to-right Pipeline layout (`Alt+P`) with live cross-stage dependency rails.
* **R-24 (Smart Cross-View & External Copy/Paste):** Pasting a copied Classic range into Notebook defaults to a Linked Live Data Frame (with a post-paste option for Static Snapshot); copying Notebook content to external programs places clean rich HTML + Markdown with resolved formatted values and vector SVG charts.

### B.5 Model Map View
* **R-25 (AST Formula-Name Presenter & Interpreter):** Dotted defined names strip their active group prefix inside composite frames and render self-references with `⁻` notation (`OnHand⁻`); edits resolve through the parsed formula AST back to standard Excel formulas without touching strings, ranges, or function names.
* **R-26 (Hierarchy & Optional Sheet Pages):** Model Map supports nested composites (`Enter`/click to drill in, breadcrumbs to climb out) and optional per-sheet page tabs synchronized with the workbook.
* **R-27 (Excel-Backed Directors & `z⁻¹` Delays):** Model Map exposes Directors mapped strictly to Excel mechanisms (`Recalc`, `Iteration`, `Ticks`, `Sweep` over Data Tables, `Events` over RTD), draws self-references with a `z⁻¹` delay chip, and renders `IFS`/`SWITCH` modal cells as compact state badges/diagrams.
* **R-28 (Circular-Reference Preview Seam):** Dragging a wire that closes a cycle while workbook Iterative Calculation is OFF pauses at the Preview Seam, explaining Excel's iteration setting and offering one-click `[Enable Iteration]` or `[Esc Cancel]`—never failing silently or mutating workbook settings unannounced.
* **R-29 (Topological Recalc Ripple & Reduced Motion):** Stepping or editing animates a 140 ms amber ripple along real topological dependency order; under `prefers-reduced-motion`, ripples replace motion with instant border highlights.
* **R-30 (Keyboard Graph Navigation):** Every box, composite, port, and wire action (`W` to wire) is operable via keyboard alone with descriptive ARIA labels.

### B.6 App View
* **R-31 (Zero-Code Declarative App Binding):** App view renders validated input controls, scenario selectors, KPI tiles, workbook charts, and grouped schedules bound 1:1 to real workbook addresses via declarative `customXml` metadata.
* **R-32 (Inline Excel Validation Refusal):** Entering an out-of-bounds value in App view respects Excel Data Validation rules, refuses the commit inline with the exact rule message and previous value preserved, and never shows a modal popup.
* **R-33 (Side-by-Side Scenario Comparison):** Users can create, name, switch, and compare `≥ 2` scenarios side-by-side with live chart baseline overlays and delta polarity cues, writing active scenario values straight to Excel cells.
* **R-34 (Schedule Auto-Grouping):** Schedules exceeding 50 rows auto-group by natural axis (e.g., Year for `Loan`'s 360 months) with per-group expansion and visible total row count.

### B.7 Formula Explorer & Formula Lens
* **R-35 (Dual-Form Parity):** Embedded Formula Explorer (`Ctrl+E`) and standalone `"DNA Calc — Formula Explorer"` (zero workbook) share the exact editor, `Eval Tree`, `Number Format`, `Cond. Format`, and `Add-in & Types` tabs.
* **R-36 (Workbook Pre-Binding & Impact Preview):** Opening embedded Formula Explorer from any cell pre-populates its precedents and renders uncommitted edits plus downstream deltas across the workbook Preview Seam before commit (`Ctrl+Enter`).
* **R-37 (Standalone Sandbox & Add-in Dev Load):** Standalone Formula Explorer supports scalar/array/error local test inputs, loads local unsigned add-ins (`.xll`/JS) behind an explicit non-modal sandbox consent banner, and surfaces per-argument add-in diagnostics.
* **R-38 (Formula Lens Semantics):** Toggling Formula Lens (`Alt+L`) in any view displays semantically equivalent readable `LET`/named structure with live argument chips and an explicit `[Readable view]` badge without modifying the stored workbook formula.

### B.8 Big Workbooks, Performance & Honesty
* **R-39 (Performance Budgets):** Meets all budgets in Section 6.3 (`Inherited` cached first paint `≤ 250 ms` desktop / `≤ 450 ms` browser; keystroke echo `≤ 12–16 ms`; view switch `≤ 60–100 ms`; smooth 60 fps virtualized scroll).
* **R-40 (Non-Blocking Progress & Abort):** Any load, index, inference, or recalculation exceeding `150 ms` renders live progress in the Strip while keeping scrolling, selection, and `Esc` cancellation responsive.
* **R-41 (Honest Folding, Sampling & Refusal):** Every folded formula run (`12/12 runs · 182,400 fmls`), sampled table (`Showing 200 of 25,000 rows · Aggregates exact over all rows`), or declined view (`App` on `Inherited`) displays explicit counts, surfaces exceptions/errors inside folded runs, and offers a direct way forward.

### B.9 Customisation, Agent Review, Accessibility, Print & Macros
* **R-42 (Declarative Skin Sandbox):** User and agent skins preview live through the Preview Seam, pass the automated Contrast & Safety Gate before activation, and never mutate calculation behavior or semantic safety tokens (`red`, `signal`, `amber` focus).
* **R-43 (Agent Staged Proposal & Hunk Review):** AI agent mutations never write directly to cells; they enter a Staged Proposal Batch grouped into semantic hunks with speculative recalc deltas across all views, Safety Warning blocks on introduced errors/protected cells, selective per-hunk Accept/Reject, atomic named Undo commits, and an exportable session Audit Log.
* **R-44 (Screen Reader & Zoom Accessibility):** Full WAI-ARIA roles (`grid`, `tree`, `region`, `tablist`), polite live announcements for recalculation/error updates, full keyboard operability, and clean layout reflow up to 200% OS/browser zoom.
* **R-45 (View-Specific Honest Printing):** `Ctrl+P` opens a live Print Preview tailored to the active view (Section 8) with explicit disclosure whenever rows are summarized/sampled.
* **R-46 (Macro Quarantine & Safe View Hooks):** Macros default to blocked with a non-modal Mast trust bar; once trusted, optional DNA Calc View API calls (`SwitchView`, `SelectAddress`, `ShowPanel`, `SetFormulaLens`, `ShowNoticeBanner`) cannot spoof security banners, disable the View Switcher, or bypass the Agent Preview Seam.
* **R-47 (Crash Recovery Journal):** Unsaved model and `customXml` view edits journal locally and present a non-modal `[Restore] [Discard]` recovery bar upon relaunch after an abnormal exit.
* **R-48 (Touch & Pen Graceful Degradation Note):** While keyboard and mouse come first, hit targets in Reading density (`App`, `Notebook`) meet `≥ 36 px` touch ergonomics and never rely exclusively on hover to reach primary actions.

---

## Appendix C: System needs (C-1 … C-32)

Plain statements of *what the front end needs from the underlying engine, host, and file layer*—without prescribing internal implementation.

### C.1 File Loading, Caching & Persistence
* **C-1 (Immediate Cached Snapshot):** On workbook open, supply sheet metadata (including hidden/very-hidden state), formatting, and cached cell values for the visible viewport before full dependency graph construction or recalculation finishes.
* **C-2 (Progressive Off-Thread Indexing & Recalc Progress):** Report incremental progress counts (`completed / total formulas`) and support safe cancellation (`Esc`) during long indexing or recalculation passes without blocking UI reads.
* **C-3 (Excel-Preserved Extension Part Read/Write):** Read and write arbitrary XML/JSON view, layout, prose, inference-override, and declarative-skin payloads inside an `.xlsx` extension part (`customXml`) that Microsoft Excel preserves untouched across save cycles, and detect if an external editor stripped that part on reopen.
* **C-4 (Crash Recovery Delta Stream):** Provide lightweight read/write access to incremental local recovery snapshots of uncommitted model and presentation edits.

### C.2 Calculation, Recalculation & Telemetry
* **C-5 (Changed-Cell & Changed-Name Diffs):** After every recalculation, return the exact list of changed cells and defined names along with their previous and new values (and error states) so views can highlight deltas and animate ripples.
* **C-6 (Speculative / Transactional Preview Calculation):** Evaluate a proposed set of cell, formula, name, or calc-setting edits in an isolated speculative overlay (without committing them to the live workbook) and return full downstream recalculated values, error introductions, and affected-cell counts—powering the Preview Seam for Formula Explorer, Inspector fixes, circular-ref previews, and AI Agent Proposal Batches.
* **C-7 (Exact Calc State & Convergence Telemetry):** Expose workbook calculation mode (`Automatic`, `Automatic Except Data Tables`, `Manual`), dirty state, last recalc duration, per-block recalc timings, Excel iterative calculation settings (`Iterative`, `MaxIterations`, `MaxChange`), and whether the latest iterative solve converged or hit the iteration cap (with final residual delta).
* **C-8 (Manual / Scoped Recalculation Triggers):** Support invoking standard Excel recalculation scopes (`F9` dirty all, `Shift+F9` active sheet, `Ctrl+Alt+F9` full, `Ctrl+Alt+Shift+F9` rebuild graph + full).
* **C-9 (Partial & Standalone Expression Evaluation):** Evaluate any arbitrary formula string or highlighted sub-expression against either a live cell's workbook context (`F9` inline / embedded Formula Explorer) or a caller-supplied dictionary of local scalar/array/error inputs with **no workbook loaded** (standalone Formula Explorer), returning a step-by-step evaluated AST tree with exact Excel types and coercions.

### C.3 Formula Syntax, Dependencies & Structural Analysis
* **C-10 (Parsed Formula AST & Rewrite Support):** Provide the parsed formula syntax tree with exact character spans for every token (functions, cell/range refs, structured table refs, defined names, strings, operators) and lossless AST-to-Excel-string serialization—powering numbered range highlighting, `F4` anchor cycling, Formula Lens `LET`/name presentation, and the Linked-Boxes Presenter/Interpreter.
* **C-11 (Cell-, Range-, Block-, and Name-Level Precedents & Dependents):** Query direct and transitive precedents and dependents not only as flattened cell coordinates, but preserving **defined-name edges**, **structured table references**, and **dynamic references** (`OFFSET`, `INDIRECT`, `INDEX`), plus summarized block/sheet-level edges for 300k-formula workbooks.
* **C-12 (Topological Evaluation Waves):** Return a topological ordering (evaluation waves) over active blocks/names/sheets, explicitly identifying strongly connected components (circular cycles and self-loops).
* **C-13 (Root-Cause vs Propagated Error Classification):** Classify every error cell in the workbook as either a Root Origin or Propagated from specific upstream root cell(s), grouped with exact affected downstream counts.
* **C-14 (R1C1 Formula-Run Clustering & Exception Detection):** Group contiguous cells sharing identical R1C1 formula patterns into compact runs (`F2:K8761`, 52,560 cells), while explicitly surfacing any outlier cells (hardcoded overrides, mismatched formulas, errors) within or adjacent to a run.
* **C-15 (Structural Anomaly Queries):** Detect and report Excel-valid structural hazards on demand: hardcoded range references partially covering an Excel Table (`Sales!Report!D8`), volatile functions (`OFFSET`/`INDIRECT`/`NOW`/`RAND`) with their downstream fan-out counts, inconsistent row/column formulas, and lookup misses with nearest-match diagnostics.
* **C-16 (Pattern Inference Metadata Feed):** Expose sheet structure primitives needed for pattern inference: contiguous region bounds, Excel Tables (`ListObject` columns/totals), PivotTables & cache sources, Data Tables (`=TABLE(...)`), Form Controls and their linked cells, Charts and series ranges, Data Validation rules, and dynamic array spill footprints.

### C.4 Formatting, Validation & Clipboard Semantics
* **C-17 (Number Format Engine):** Format any numeric, date, text, or error value against any Excel format code and locale—both for grid rendering and live testing in Formula Explorer's Number Format Lab.
* **C-18 (Conditional Formatting Evaluation & Rule Inspection):** Evaluate which Conditional Formatting rules trigger for any cell (or for an arbitrary override test value in Formula Explorer's CF Simulator) and return resolved styles and `Stop If True` outcomes.
* **C-19 (Data Validation Query & Check):** Inspect Excel Data Validation rules on any cell and validate candidate user inputs before commit, returning structured rule descriptions for inline refusal messages (`App`, `Notebook`, `Classic`).
* **C-20 (Excel-Exact Structural Edit Primitives):** Execute atomic workbook operations with full Excel reference updating: fill series / double-click fill extent calculation, drag-move / cut-paste reference relocation, insert/delete rows/columns/cells with shift, copy-with-relative-shift, `Ctrl+Enter` batch commit, and atomic multi-cell transaction commit/rollback for Unified Undo.

### C.5 Live Data, External Links, Add-ins & Macros
* **C-21 (External Link Inventory & Cache Provenance):** List all external workbook links, their resolution status (resolved vs missing), last-cached values, and every cell depending on them.
* **C-22 (Live RTD / Async Feed Events & Throttling):** Expose active RTD topics and async add-in cells with per-topic update timestamps, event frequencies, and a UI-side visual pause/throttle hook that never alters formula definitions.
* **C-23 (Add-in Registry, Sandbox Loading & Diagnostics):** Load/reload add-ins (`.xll`, JS/WASM) with explicit signature/trust metadata, enumerate registered custom functions with parameter docs, tag dependent cells with add-in provenance, and return rich per-argument error diagnostics from add-in calls when available.
* **C-24 (Macro Quarantine & Read-Only Code Inspection):** Detect VBA/macro modules on open without executing them, allow read-only source inspection, gate execution behind explicit user session trust, and relay standard Excel object-model UI events (`Activate`, `Select`) plus optional safe View API calls to the front end.

### C.6 Printing, Charts, PivotTables & Agent Interface
* **C-25 (Excel Print Metadata):** Expose sheet Print Area, page breaks, margins, scaling, repeat rows/columns, and header/footer definitions for faithful Classic printing.
* **C-26 (Vector Chart & PivotTable Data Bindings):** Provide live series data, axes, titles, and formatting metadata for Excel Charts and PivotTable layout/totals so Notebook, App, Model Map, and Print Preview can render crisp vector visuals synchronized with recalculation.
* **C-27 (Unified Entity Resolution & Address Formatter):** Resolve any canonical address (`Sheet!A1`, `Name`, `Table[Col]`, `block:...`, `obj:...`) to its backing cells/metadata and generate canonical addresses in either A1 or R1C1 notation.
* **C-28 (Workbook-Safe Name Allocation):** When Notebook or Model Map creates a new named calculation, allocate a non-colliding cell location and valid Excel defined name cleanly.
* **C-29 (Read-Only Snapshot Access for Agents):** Provide an AI agent read-only access to the workbook's structure, inferred patterns, formulas, values, and diagnostics via the same Unified Address Grammar without blocking user interaction.
* **C-30 (Agent Proposal Diff & Audit Metadata):** Validate proposed agent edits against Excel syntax, sheet protection, and speculative recalculation (C-6), returning per-hunk diffs and error checks prior to commit.
* **C-31 (Virtualized Window Reads):** Read visible rectangular slices of cells, styles, and overlays (`25,000+` or `8,760 × 104` sheets) in constant time relative to viewport size, never requiring full-sheet materialization in UI memory.
* **C-32 (Fast Filtered Row Slicing Without Mutating Sheet State):** Query matching rows of a Table or range (e.g., `Orders!20001:25001` matching `Region="EMEA"` and `Quarter="Q3"` in Inspector) for read-only inspection without altering the workbook's AutoFilter state.

---

## Appendix D: Spreadsheet model pattern catalogue

Complete catalogue of the **15 baseline spreadsheet patterns** plus **3 essential additions (`#16–#18`)**, covering human recognition, automated detection & confidence, confirmation/correction, and presentation across all five views.

| # & Pattern (Cast Exemplar) | How a Person Recognises It | How DNA Calc Recognises It & Confidence | How User / Agent Confirms or Corrects | Presentation Across the 5 Views (`Classic` · `Inspector` · `Notebook` · `Model Map` · `App`) |
| :--- | :--- | :--- | :--- | :--- |
| **1. Sheet-as-database** (`Sales!Orders`, 25,000 rows) | Flat header row with uniform typed columns, thousands of record rows, few or zero cross-row formulas; read by `SUMIFS`/`XLOOKUP`/Pivot. | Excel `ListObject` Table (**High 0.98**) or contiguous `≥ 50` uniform typed rows with unique header row & fan-out lookups (**High 0.88**). | Click inferred Table/DB chip → `[Confirm DB]`, `[Adjust Header/Range]`, or `[Convert to Excel Table Ctrl+T]`. | **Classic:** Full virtualized grid + sticky headers + AutoFilter. **Inspector:** Single source node `[Table: Orders · 25k]`, checks partial-range refs. **Notebook:** Sampled 200-row Data Frame with visible count notice + full-table stats. **Model Map:** Single compact database source block (never 25k boxes). **App:** Read-only searchable/filtered lookup or hidden backing data. |
| **2. Inputs & assumptions** (`Budget!Inputs`, `Loan!C3:C7`) | Dedicated sheet/top block of labeled constants (often blue text or shaded fill), zero precedents, high fan-out to formula blocks. | Zero-precedent constant cells adjacent to text labels with `≥ 2` downstream formula dependents (**High 0.88** when on dedicated sheet/named; **Medium 0.72** inline). | Click block frame → `[Mark as Inputs]`, exclude/include cells, or bind widget type (`slider`, `percent`) in `customXml`. | **Classic:** `prov-const` (`#2D6A8A`) provenance ticks in overlay; author fills intact. **Inspector:** Tier-0 source cluster in Atlas; sensitivity fan-out count. **Notebook:** Editable assumption card strip (`M-04`). **Model Map:** Left-edge parameter boxes + right Parameter Rail. **App:** Primary validated left-rail form controls. |
| **3. Calculation blocks** (`Budget!Monthly` Rev/Cost, `Inherited!Dispatch_Hourly`) | Rectangular labeled sections with uniform copied formulas down/across and a subtotal/total row at the bottom or right. | Contiguous region of identical R1C1 formula patterns bounded by blank rows/headers and/or `SUM`/`SUBTOTAL` footer (**High 0.90**; **Medium 0.70** if mixed overrides). | Click block border to merge/split sections, rename block, or flag intentional overrides vs anomalies. | **Classic:** Native grid + optional Provenance Overlay flagging formula outliers. **Inspector:** Folded formula-run node showing timing & exceptions. **Notebook:** Linked Data Frame with formula peek on hover. **Model Map:** Single Calc Box or Composite (`L2 Formula Runs` when huge). **App:** Hidden engine or summarized readout table. |
| **4. Time series with periods across** (`Budget!Monthly!B4:N15`) | Columns labeled `Jan–Dec`, `Q1–Q4`, or `2026–2030` across top; metrics down left; horizontal roll-forward (`=B5*(1+g)`) + right-edge `Total`. | Sequential date/period header axis (`≥ 3` columns) with uniform horizontal R1C1 recurrence + optional right-edge `SUM` column (**High 0.92**). | Confirm period axis (`Monthly`/`Quarterly`) or adjust total column inclusion in block popover. | **Classic:** Frozen row/col panes + horizontal scroll. **Inspector:** Single time-series run with recurrence arrow (`t-1 → t`). **Notebook:** Compact Data Frame + auto-offered inline sparkline & FY Total (`M-04`). **Model Map:** Period-folded vector box showing trend sparkline & end-period value. **App:** Line/bar chart + collapsible annual/quarterly summary table. |
| **5. Scenarios & sensitivity tables** (`Budget!Inputs` Base/Stretch, `watershed` `=TABLE`) | Parallel scenario columns (`Base`/`Stretch`) with an active switch (`IF`/`CHOOSE`/`INDEX`), or Excel 1D/2D Data Table (`{=TABLE(r,c)}`). | `{=TABLE(...)}` AST node (**High 0.99**); active selector cell with Data Validation list feeding `IF`/`CHOOSE`/`INDEX` across parallel columns (**High 0.86**; **Medium 0.68** without DV). | Confirm scenario names, selector cell address, or add custom preset snapshots in `customXml`. | **Classic:** Native Data Table / scenario columns; Strip notes manual-table recalc mode if set. **Inspector:** Shows sweep dimensions & recalc cost (e.g., `25 × 52 runs`). **Notebook:** Interactive scenario toggle strip (`● Base | ○ Stretch`) + delta prose chips. **Model Map:** `Sweep` director composite (`watershed.html`) or fan-out selector. **App:** Top Scenario Chips + side-by-side comparison table & baseline chart overlay (`M-08`). |
| **6. Reference / lookup tables** (`Inherited!Curve_Lookup`, `Sales!Report!H5:K8`) | Small-to-medium key→value mapping table (rates, tiers, FX, curves) targeted by `XLOOKUP`, `VLOOKUP`, `INDEX/MATCH`. | Contiguous key+value block targeted as lookup array by `≥ 1` `XLOOKUP`/`VLOOKUP`/`MATCH` formula (**High 0.91**). | Confirm key column and whether approximate (`-1`/`1`) or exact (`0`) matching is expected. | **Classic:** Grid with lookup-key highlight on hover in Provenance Overlay. **Inspector:** Lookup hub node showing miss counts & nearest-key fuzzy hints (`M-02`). **Notebook:** Collapsible appendix reference block. **Model Map:** Compact lookup table actor feeding dependent calc boxes. **App:** Hidden backing table (or read-only rate card modal). |
| **7. Pipelines across sheets** (`Budget`: `Inputs → Monthly → Summary`, `Inherited` 40 sheets) | Data flows left-to-right across tabs (`Raw → Staging → Calc → Report`) with minimal reverse cross-sheet references. | Directed acyclic graph (DAG) of inter-sheet cell/name edges computed from dependency graph (**High 0.96**). | Confirm/reorder logical stage tiers or group sheets into domain clusters in Atlas/Map. | **Classic:** Sheet tabs with subtle flow tooltip on hover; `Ctrl+[`/`]` jumps across sheets. **Inspector:** Tiered **Workbook Atlas** (`L0`) showing inter-sheet edge counts & bottlenecks (`M-02`). **Notebook:** Horizontal **Pipeline Mode** (`Alt+P`, `M-14`) mirroring sheet flow. **Model Map:** Top-level Composite boxes wired left-to-right (`Stock`, `Dam`). **App:** Collapses intermediate sheets; exposes only inputs and final outputs. |
| **8. Running balances & deliberate circular references** (`Loan` balance, `Stock!Inv.OnHand`, `watershed` Finance) | Row-by-row accumulator (`=PrevBal - Principal`), or intentional interest/cash/inventory self-loop with Iterative Calc enabled. | Vertical lag formula (`R[-1]C`) over monotonically indexed axis (**High 0.94**); strongly connected component / self-loop in dependency graph (**High 0.99**). | Confirm intentional circular loop vs accidental bug; adjust `MaxIterations` / `MaxChange` via Preview Seam (`M-06`). | **Classic:** Grid + `↻ Iter` overlay pill + Strip convergence readout (`Converged` vs `Max Iter Reached`). **Inspector:** Highlights cycle strongly-connected component & convergence delta trace. **Notebook:** Summary opening/ending balance + chart + expandable period ledger. **Model Map:** Dashed feedback loop with `z⁻¹` delay chip or `Iteration` director badge (`M-15`). **App:** Trajectory chart + grouped balance summary (`M-16`). |
| **9. Simulations** (`watershed` 25 rainfall futures, Monte Carlo `RAND`/`NORM.INV`) | Stochastic formulas (`RAND`, `NORM.INV`) paired with Data Table sweeps or repeated scenario runs producing percentiles (`P10`/`P50`/`P90`). | Presence of volatile RNG functions (`RAND`/`RANDBETWEEN`) feeding `{=TABLE(...)}` or percentile aggregations (**High 0.90**; **Medium 0.65** for single-cell `RAND`). | Confirm trial count, seed cell, and output distribution metrics; pause auto-recalc on edit if desired. | **Classic:** `▸` volatile markers + Strip manual-recalc / `F9` re-roll shortcut. **Inspector:** Volatile fan-out timing + distribution stats probe. **Notebook:** Fan/histogram chart card + `[Re-run Simulation F9]` button. **Model Map:** `Sweep` director composite with inline percentile fan chart (`watershed.html`). **App:** Outcome probability band chart + risk KPI cards (`P10 / Median / P90`). |
| **10. Control panels** (`Stock!Slider_RP`, `Budget!Inputs!C2`, switches/flags) | Form Controls (sliders, checkboxes, dropdowns, spin buttons) or Data-Validation dropdown cells driving model flags (`Sim.Reset`, `ActiveScenario`). | Native OOXML Form Controls / ActiveX controls with linked cells (**High 0.99**); DV list cells referenced by `IF`/`IFS`/`SWITCH` (**High 0.87**). | Bind/unbind control range (`min`/`max`/`step`) or promote any DV cell to Parameter Rail. | **Classic:** Native interactive Excel Form Controls rendered on-sheet (`M-07`). **Inspector:** Lists all controls + linked cells + downstream impact count. **Notebook:** Inline interactive toggles/dropdowns embedded in narrative (`M-04`). **Model Map:** Docked right-hand **Parameter Rail** synchronized live with on-sheet control (`M-15`). **App:** Primary styled input widgets (`M-16`). |
| **11. Pivots** (`Sales!Report` PivotTable + slicers) | Dynamic cross-tab summary (`Row`/`Column`/`Values` headers, `Grand Total`, filter dropdowns) backed by a Table or data range. | Native OOXML `PivotTableDefinition` + cache source (**High 1.00**). | Inspect source binding, refresh cache (`Alt+F5`), or link reconciliation checks. | **Classic:** Full interactive PivotTable + field-list drawer + slicers. **Inspector:** Shows cache freshness (`Stale vs Fresh`), source row coverage (`25,000/25,000`), and `GETPIVOTDATA` dependents. **Notebook:** Clean read-only live Pivot card with filter chips. **Model Map:** Single aggregation actor `[Pivot: Sales by Region × Qtr]` wired from `Orders`. **App:** Interactive summary card with filter dropdowns (no raw field list). |
| **12. Live data** (`watershed` RTD Clock & Market feeds, `STOCKHISTORY`, async add-ins) | Streaming quotes/ticks (`=RTD(...)`), web/add-in feeds (`=TAX.EFFRATE(...)`), or `NOW()`/`TODAY()` cells updating externally. | `RTD` / `CUBE*` / async UDF / `STOCKHISTORY` / `WEBSERVICE` AST nodes (**High 0.99**). | Set UI visual throttle/pause rate (never alters formula); approve/inspect add-in trust. | **Classic:** `signal` (`#FF8001`) `▸` corner tick + hover freshness timestamp. **Inspector:** `Live & Volatile` monitor showing per-topic update rate, event log, and dirty-set size (`M-13`). **Notebook:** Live readout chip with subtle update pulse + timestamp caption. **Model Map:** `Events` director + `signal`-framed RTD source actors + top Event Queue strip (`watershed.html`). **App:** Live status badge (`Live · updated 1s ago`) + pause toggle. |
| **13. Macro-driven models** (`Inherited!modRollForward`, button-triggered roll-forwards) | `.xlsm`/`.xlsb` files with VBA modules, sheet buttons assigned to macros, or refresh/roll-forward workflows. | Presence of `vbaProject.bin` or macro-assigned shapes/buttons (**High 1.00**). | Explicitly inspect read-only VBA source and click `[Trust This Session]` or keep quarantined. | **Classic:** Non-modal Mast security bar (`Macros blocked · [Inspect Code] [Trust]`); buttons work once trusted. **Inspector:** Macro inventory card (module names, LOC, signature status, read-only code viewer). **Notebook:** Disabled/trusted macro action chip only if explicitly placed. **Model Map:** Shows quarantined/trusted macro node badge without claiming formula edges. **App:** Exposes author-whitelisted macro buttons *only* after explicit session trust; otherwise explains quarantine plainly. |
| **14. Reports & dashboards** (`Budget!Summary`, `Sales!Report`, `Inherited!Exec_Summary`) | Presentation-formatted top sheet with large totals, percentage margins, charts, print areas, and minimal raw data entry. | High fan-in / zero fan-out sink sheet containing formatted summary tables, charts, or defined Print Area (**High 0.85**; **Medium 0.68** without charts/print area). | Confirm as primary Executive Output sheet so Notebook/App auto-scaffolding prioritizes it. | **Classic:** Faithful author styling + Excel Print Preview parity. **Inspector:** Output tier of Workbook Atlas; traces every KPI back to assumptions. **Notebook:** Auto-scaffolded headline narrative + KPI summary section (`M-04`). **Model Map:** Right-most output sink composite + Plotter. **App:** Top KPI strip + primary visual dashboard (`M-16`). |
| **15. Stocks, flows, feedback & delays** (`Stock.xlsx`, `names-in-motion.html`, `watershed.html`) | Accumulator state cells (`Stock⁻ + Inflow - Outflow`), shift-register delay arrays (`HSTACK(DROP(...))`), `IFS` operating modes (`Normal`/`Stockout`), and feedback loops. | Self-referencing state formulas + shift-array delay patterns + `IFS`/`SWITCH` modal cells over state variables (**High 0.93** under iterative calc; **Medium 0.70** when unrolled across time columns). | Confirm state vs flow vs modal role; toggle `Presented` (`Stock⁻`) vs raw Excel view; pick `Ticks`/`Iteration` director. | **Classic:** Backing cells/names + Form Controls + spill frames (`M-07`). **Inspector:** Cycle topology + per-tick state delta log. **Notebook:** Interactive policy brief with sliders + trajectory charts. **Model Map:** **Native signature view**—state boxes, `z⁻¹` delay loops, inline FSM badges, directors, live ripple, and Plotter (`M-06`, `M-15`). **App:** Policy simulator with sliders, mode badge, and live history chart. |
| **16. *(Added)* Reconciliations & check blocks** (`Sales!Report!D14`, balance-sheet checks) | Audit/check rows testing `=A-B`, `ABS(diff)<0.01`, or `"OK"/"ERROR"` banners verifying totals match across views or sheets. | Formulas comparing two independent aggregation paths (`SUM`-vs-`GETPIVOTDATA`, `ABS(...)<ε`, `IF(...,"OK","ERR")`) or labeled `Check`/`Variance`/`Diff` (**High 0.86**; **Medium 0.65**). | Confirm expected zero-tolerance rule or dismiss non-check variance rows. | **Classic:** Highlights failing checks in Provenance Overlay without overwriting CF. **Inspector:** Dedicated **Checks Instrument** (`11/12 Pass · 1 Fail: Report!D14 = -$118,420`) with one-click root trace. **Notebook:** Compact verified badge (`✓ All 12 model checks pass` / `⚠ 1 check failing`). **Model Map:** Check-monitor badge on sink composite. **App:** Subtle footer integrity pill (`Model checks verified`) or honest warning if a check fails! |
| **17. *(Added)* Dynamic array / `SPILL` pipelines & `LAMBDA` libraries** (`Stock!Supply.Pipeline`, `Sales!Report!E8` `FILTER`) | Single anchor cell driving a dynamic output footprint (`FILTER`/`SORT`/`UNIQUE`/`SEQUENCE`/`HSTACK`/`SCAN`) and workbook-scoped `LAMBDA` named functions. | Dynamic-array spill metadata from engine (**High 1.00**); `LAMBDA` AST root in Defined Names (**High 1.00**). | Inspect spill bounds, edit anchor formula in Formula Explorer, or document `LAMBDA` parameter descriptions in `customXml`. | **Classic:** Crisp teal spill frame around active footprint + `#SPILL!` obstruction highlight showing exact blocking cell. **Inspector:** Treats whole spill as one atomic node (never 500 cell nodes); lists `LAMBDA` library + call sites. **Notebook:** Renders spill seamlessly as a live reactive Data Frame. **Model Map:** Single array-valued actor box (`{0, 400, 0}`); `LAMBDA` names appear as reusable actor classes. **App:** Binds directly to dynamic tables/charts that auto-resize cleanly without broken range bounds. |
| **18. *(Added)* External-link consolidation & staging hubs** (`Inherited` 3 missing workbooks, paste-special staging tabs) | Formulas referencing `[OtherBook.xlsx]Sheet!A1` or staging sheets full of pasted external values feeding regional consolidation tabs. | External workbook reference AST nodes (`[n]Sheet!Ref`) (**High 1.00**); high-constant staging sheet with external-file naming/notes (**Medium 0.64**). | Inspect/remap missing link targets or explicitly acknowledge cached-value snapshot mode. | **Classic:** `signal` (`#FF8001`) `▸` external corner ticks + hover showing exact external path & cached date; non-modal Mast/Triage link manager (`M-01`). **Inspector:** Top-tier External Links rail listing every external workbook, cell count, and broken-link blast radius (`M-02`). **Notebook:** Prominent footnote/badge (`Includes cached data from 3 external files`). **Model Map:** Left-edge `signal`-framed external boundary actors. **App:** Clear data-freshness header warning whenever any KPI depends on an unresolved external cache. |
