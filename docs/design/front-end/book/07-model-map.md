# Part VII. Model Map

Clusters: `CL-07-01`–`CL-07-12` · Governing decisions: `D-03`, `D-59`–`D-64` · Mockups: [Pro 21 F04](http://127.0.0.1:8720/p21/model-map.html#F04), [Pro 21 M01](http://127.0.0.1:8720/p21/model-map.html#M01)–[Pro 21 M05](http://127.0.0.1:8720/p21/model-map.html#M05), [Pro 21 S08](http://127.0.0.1:8720/p21/storyboards.html#S08), [Pro 21 S09](http://127.0.0.1:8720/p21/storyboards.html#S09), [Pro 21 O04](http://127.0.0.1:8720/p21/workbook.html#O04)

---

## 7.1 Sheet-Truth Schematic and Three-Stage Rollout (`CL-07-01`, `CL-07-05`, `D-62`)

**Model map** (`dna-view-map`, `Ctrl+Alt+4` / `Alt+Shift+4`) gives Lee (modellers, engineers, systems thinkers) a hierarchical block-and-wire schematic over the exact same Excel workbook—leveraging first-class Defined Names in `calc-doc`/`calc-engine` (including Excel's native support for dotted names like `Store.Stock`) so every box, port, and wire reflects real sheet formulas (`R-21`) `[FRAMING-0010, FRAMING-0106, FRAMING-0109, P19-CARBON-0192, P19-FABLE-0085]`. Pressing `Ctrl+Alt+0` / `Alt+Shift+0` opens the **Classic Companion** grid beside the map sharing selection and the Formula Bar so edits in either pane update the other live `[P19-FABLE-0081]`.

To prevent accidental corruption of positional relative/mixed `$` references, Model map ships in **three disciplined stages** (`D-62`) `[FRAMING-0065, P19-FABLE-0220, P19-NEON-0104]`:

1. **Stage 1 — Architectural Reader:** Hierarchical levels (`Sheet Groups -> Sheets/Districts -> Blocks/Runs -> Named Ports`), semantic zoom, live port values, exact `"x of y references accounted for"` footer (`R-48`), keyboard-accessible **Connections list**, and click-to-inspect `[FRAMING-0065, P19-FABLE-0220, D-62]`.
2. **Stage 2 — Named-Port & Explicit Single-Operand Rewiring (`S08`, `R-30`):** Dragging an existing wire endpoint (or pressing `W`) rewrites unambiguous named ports (`MyCell.Input1 = Other.Out`) or a targeted operand on a single cell (`Stock!H15 =IF(G15<$B$8,$B$6,0) -> =IF(G15<$B$5+$B$8,$B$6,0)`) via `ModelChange::ReplaceReference` through the before/after Preview Seam; multi-row positional A1 blocks refuse raw drag-wiring gently, ambiguous multi-operand formulas open `F2` operand selection (`R-54`: clicking a box while editing inserts its address), and **unwiring inside a compound formula NEVER silently substitutes `0`**—requiring an explicit user replacement edit `[P19-FABLE-0078, P19-FABLE-0466, P19-GPT-0260, P19-NEON-0104, P21-0067, D-62]`.
3. **Stage 3 — Blank-Paper `name = formula` Authoring (`CL-07-02`, `D-59`, `R-22`, `R-51`, `C-28`):** Typing `Store.Initial = 400` (or using `I`/`B`/`G`/`S` starter actions) allocates a two-column (`label` + `value`) row on the active visible sheet bound to a workbook-scoped dotted Defined Name (with a Preview Seam option to pick another sheet/cell or sheet-scoped template names, and offering to create empty boxes for undefined referenced names)—never hidden names (`D-13`) and never blank-sheet Name-Manager ghost workbooks (`Carbon Push-back`) `[P19-CARBON-0036, P19-CARBON-0112, P19-CARBON-0118, P19-CARBON-0193, P19-FABLE-0074, P19-FABLE-0075, P19-FABLE-0234, P19-FABLE-0463, P19-NEON-0033, P19-NEON-0266, D-59]`.

---

## 7.2 Ports, Hierarchy, Navigation, and Bounded Sheet Excerpts (`CL-07-03`, `CL-07-04`, `CL-07-10`, `D-60`, `D-61`, `D-64`)

See [Pro 21 F04 — Stock period model map](http://127.0.0.1:8720/p21/model-map.html#F04), [Pro 21 M01 — Hierarchy drill-down](http://127.0.0.1:8720/p21/model-map.html#M01), [Pro 21 M03](http://127.0.0.1:8720/p21/model-map.html#M03)–[Pro 21 M05](http://127.0.0.1:8720/p21/model-map.html#M05).

- **Dotted-name ports, line styles, & typeable qualifiers (`D-60`, `R-48`, `R-49`):** Real sheets, tables, and Defined Names (`MyCell.Input1`, `MyCell.Result`) draw with **solid** borders, whereas inferred blocks and dotted-prefix groups draw with **dashed** teal frames and guillemet labels (`‹Revenue›`) `[FRAMING-0104, P19-FABLE-0147, P19-FABLE-0460, P19-FABLE-0461]`. Nodes expose live port values, a **Source** toggle showing exact Excel syntax, and typeable, screen-reader-speakable qualifiers (`Stock[prev]` or `Prev.Stock`—never non-typeable Unicode superscripts like `OnHand⁻`) `[P21-0065, D-60]`.
- **Hierarchy, wheel pan vs. `Ctrl+Wheel` zoom, & `M05` sheet excerpts (`D-61`, `D-64`, `R-33`):** Plain mouse wheel pans/scrolls the canvas while **`Ctrl+Wheel`**, `+`/`-`, and toolbar `Zoom`/`Fit` execute semantic zoom (`D-61`); `Enter` drills into composite groups (`Stock › Replenishment › Day 5`, preserving selection and parameter rail) and `Esc`/breadcrumbs step out without inventing Excel scope (`R-33`) `[P19-GPT-0043, P19-GPT-0263, P21-0066, D-61]`. At scale (`Inherited`, `M03`–`M05`), Model map opens at `<=60` calm sheet/group boxes (hard auto-layout cap `200 nodes / 400 edges`), stacks identical monthly sheets (`Jan…Dec · 12 alike`), folds dense sheets into exact L2 R1C1 formula runs with 100% coverage accounting, and supports Pro 21's opt-in **bounded geometry-preserving sheet excerpt (`M05`)** (`7 of 1,000 rows × 12 cols` with boundary ports and fingerprint check) `[P19-FABLE-0046, D-64]`.

---

## 7.3 Period Lags vs. Iterative Cycles vs. Ticked `z⁻¹` Clocks (`CL-07-06`–`CL-07-09`, `D-03`, `D-63`)

See [Pro 21 M02 — Stock (ticked) RTD clock](http://127.0.0.1:8720/p21/model-map.html#M02) and [Pro 21 S09 — Check Stock in Classic](http://127.0.0.1:8720/p21/storyboards.html#S09).

Grounded in Owner Ruling #3 (`D-03`), Pro 19 Push-back #1, and the OxCalc `W048` circular-reference observation ledger `[FRAMING-0053, FRAMING-0079, P19-GPT-0088, P19-GPT-0096]`, Model map **strictly separates three structures visually and by type** (`R-32`, `R-50`) `[P19-GPT-0262, P21-0064]`:

| Structure | Backing Excel Mechanism | Map Notation & Honesty Rule (`D-03`, `D-63`) |
|---|---|---|
| **1. Period-Row / Column Lag** | Acyclic prior-row/column ref (`Stock!H15` day 2 -> `C18` day 5 via `INDEX`, or `Balance[prev]`) | Solid wire carrying period-lag pill **`[t-1]`** / **`3-day lag`** `[FRAMING-0140, P19-FABLE-0147, P19-GPT-0022]`. |
| **2. Within-Calculation Iterative Cycle** | Circular SCC solved within one recalc (`Stock!B8 =0.5*(B8+B7)`, `Iterate=ON`, `Max 100`, `0.001`, `C-10`) | Double-dashed **Iteration Enclosure** (`Iterates · Max 100 · tol 0.001`)—**never labelled a time delay** (`z⁻¹` drawn on a pass-read only where `C-21` confirms last-pass semantics); closing a loop with Iteration OFF opens a non-modal 3-way choice at the box (`R-52`) `[P19-CARBON-0216, P19-FABLE-0076, P19-FABLE-0077, P19-FABLE-0462, P19-FABLE-0464, P19-GPT-0022]`. |
| **3. Clocked Per-Tick Delay (`z⁻¹`)** | Confirmed RTD clock (`=RTD("DnaCalc.Clock",,"Stock","tick")`, `MaxIterations=1`, `Prev.*` state) **or** valid macro-ticked runner (`Calculate` + state step) | Source clock block (`Run`, `Pause`, `Step`, `Rate`, `Reset`) + **`[z⁻¹ · per tick]`** delay mark (`M02`, `A02`, `I03`) `[FRAMING-0111, FRAMING-0141, FRAMING-0142, FRAMING-0168, P21-0068]`. |
| **4. Unconfirmed / Ambiguous Cycle** | Self/mutual reference lacking confirmed clock or host guard profile | Labelled **`Unknown`**, refusing to assume once-only tick delivery `[P21-0064, P21-0071]`. |

**Excel runtime honesty (`P21-0069`, `P21-0070`) & plain chips (`D-63`):** Because Excel's `RTD.ThrottleInterval` defaults to `2,000 ms`, edits trigger recalculation, and `Application.MaxIterations` is Application-scoped (making 100-iteration period `Stock` and 1-iteration `Stock (ticked)` distinct calculation profiles), file formulas alone are treated as *candidate clocked* until paired with a confirmed host/provider profile `[P21-0069, P21-0070]`. Harmonising the `names-in-motion` and `watershed` sketches into Strand v0.2 retires academic `"Director"` labels in favour of **plain Excel calculation chips** (`Auto`, `Manual`, `Iterates · Max N`, `RTD clock`, `Macro-ticked`, `Data table`) and routes live tick plotting to the shared dockable **Inspector `Live` / `Watch`** panel (`D-63`) `[P19-CARBON-0017, P19-FABLE-0159, P19-NEON-0019, D-63]`.

---

## 7.4 Controls, Ripple, Keyboard Accessibility, and Schematic Printing (`CL-07-11`, `CL-07-12`)

Adding a Slider on a map box previews creating a standard linked Excel Form Control (`Commit on release` so iterative models tick once) `[P19-FABLE-0079]`. Topological dependency-explanation ripples (`<=140–400 ms`) collapse to instant highlights under `prefers-reduced-motion` (`R-29`) `[P19-NEON-0219]`. Every box, group, port, and wire action (`Arrows`, `Ctrl+[`/`]`, `Enter`/`Esc`, `W`, `Tab`) is keyboard- and screen-reader-accessible (`R-53`, `R-30`) via `dna-view-map`'s semantic node/port ARIA tree `[P19-FABLE-0084, P19-FABLE-0465, P19-NEON-0220]`. Pressing `Ctrl+P` (`O04`) renders a crisp A4 landscape **Engineering Schematic Sheet** with Strand title block, exact `"x of y accounted for"` coverage stamp, cross-page connector tags when tiled, and an optional Defined-Name Formula Schedule `[P19-CARBON-0097, P19-FABLE-0200, P19-NEON-0089]`.
