# Part XV. Testing the Experience

Clusters: `CL-15-01`–`CL-15-10` · Governing decisions: `D-03`, `D-90`–`D-92` · Mockups: [Pro 21 S01](http://127.0.0.1:8720/p21/storyboards.html#S01)–[Pro 21 S16](http://127.0.0.1:8720/p21/storyboards.html#S16) (plus [S10](http://127.0.0.1:8720/p21/app.html#S10)–[S11](http://127.0.0.1:8720/p21/app.html#S11) in `app.html`), `verification.html` in [21-front-end-round3.zip](../../../architecture/review-2026-11/pro/21-front-end-round3.zip)

---

## 15.1 Two-Phase Human and Scripted Evaluation Protocol (`CL-15-01`–`CL-15-03`, `D-90`)

Static screenshots alone never prove an interactive calculation tool works or tells the truth (`P19-GPT-0080`). DNA Calc evaluates the front end across the shared cast of **five canonical workbooks** (`Budget.xlsx`, `Sales.xlsx`, `Loan.xlsx`, `Stock.xlsx` + `Stock (ticked)`, and `Inherited.xlsm`, plus `DiscountFunctions.xll`) and **six personas** (**Maria**, **Sam**, **Lee**, **Jo**, **Ade**, and **an AI agent**) `[FRAMING-0115, FRAMING-0116, FRAMING-0117, FRAMING-0118, FRAMING-0120, FRAMING-0121]` using a **two-phase evaluation protocol** (`D-90`):

1. **Phase 1 — Goal-Based Moderated Usability + 90% Model-Truth Quiz (`D-90`):**
   - **Cohort:** `8–10` role-matched participants per milestone release cycle (finance analysts, model auditors, systems/control engineers, non-author managers, add-in authors, **plus dedicated keyboard-only and real NVDA/JAWS/VoiceOver screen-reader participants** on `SB2`, `SB3`, `SB5`, `SB7`, and `UT-8`) `[P19-FABLE-0216, P19-GPT-0080, D-90]`.
   - **Zero-jargon prompt:** Each participant receives the starting `.xlsx`/`.xlsm` file and a plain-language goal with **zero prescribed clicks, zero DNA Calc view names, and zero moderator coaching** `[P19-FABLE-0208, P19-GPT-0080]`.
   - **Task-scaled budgets & hard safety bars:** Timed per task (`3–8 min` target, `5–20 min` outer session window) requiring **0** unnoticed model edits, **0** silent stale/cached value misreadings, **0** accidental macro executions (`100%` avoid running `LegacyClose`), **0** lost drafts on view switch, and **0** focus traps `[P19-GPT-0080, D-90]`.
   - **Mandatory 5-statement Model-Truth Comprehension Quiz (`>= 90%` pass mark):** Immediately after each task, the participant answers five true/false statements about what changed in the file, which numbers are calculated vs cached/pinned, and what Microsoft Excel will show on reopen—detecting any view that looks pretty while misleading the user `[P19-FABLE-0216, D-90]`.
2. **Phase 2 — Deterministic Scripted-Key Conformance & Evidence Bundle (`__dnaProbe()`, `D-90`):**
   - Every storyboard step is also encoded as an exact keyboard/pointer script executed via `cargo xtask app smoke` in headless Chromium and Firefox across `cockpit-light`, `cockpit-dark`, `high-contrast`, `prefers-reduced-motion`, and `100%` / `200%` / `400%` zoom `[P19-FABLE-0040, P19-FABLE-0216, P19-GPT-0080, D-90]`.
   - Each run emits an **evidence bundle**: structured `__dnaProbe()` interaction logs, ARIA/screen-reader live-region transcripts, before/after `Stamp` + SHA-256 model comparisons, and round-tripped `.xlsx` archives verified in Excel `[P19-GPT-0080]`.

### 15.1.1 Eleven Make-or-Break UX Moments Mapped Across Storyboards (`CL-15-02`)

Every one of the eleven critical UX categories is exercised end-to-end by named storyboard steps `[P19-CARBON-0046, P19-FABLE-0118, P21-0132]`:

| # | Make-or-Break UX Moment | Primary Storyboard Steps & Pro 21 Scenes |
|---|---|---|
| 1 | **First run & empty / starter states** | `SB4` (`Stock` authoring), `SB6` (standalone Explorer cold start `F06`), File `O01` patterns |
| 2 | **Feature & view discovery without jargon** | `SB1` (`Inherited` triage & Inspector), `SB2` (`Budget` Notebook), `SB5` (`Loan` App) |
| 3 | **Loading, staged `AsSaved` open & calc feedback** | `SB1` (`Inherited` `<= 1 s` first viewport, hatched stale `F18`, `300 ms` Strip pill `S01`) |
| 4 | **Lossless cross-view switching (`Selection` + drafts)** | `SB1` (`S02` Inspector -> Classic `F18`), `SB3` (`S07` Inspector -> Classic undo), `SB4` (`S09` Map -> Classic) |
| 5 | **Honest errors, warnings & typed refusals** | `SB1` (`#REF!` root + `Rates2024.xlsx` + quarantined `LegacyClose`), `SB5` (`A03` Stop validation), `SB6` (`F07` caller-context refusal) |
| 6 | **Unified Undo, `Revert` & conflict recovery** | `SB3` (Classic `Ctrl+Z` of Inspector repair), `SB7` (`S15` single-unit `Ctrl+Z` + `S16` `120/123/125` conflict) |
| 7 | **Semantic copy/paste & export fidelity** | `SB2` (Notebook chip/table copy + `.xlsx` round-trip), `SB5` (`S11` frozen `consistent()` 1-page print) |
| 8 | **Hover, focus, lens & selection legibility** | `SB2` (`NC01`–`NC06` spills), `SB4` (`S08` port wiring), `UT-8` (dual-stroke focus across all 5 views) |
| 9 | **Perceived speed & bounded big-workbook caps** | `SB1` (`Inherited` 300k formulas, `M03`–`M05` 40-sheet groups), `SB3` (`Sales` 25k-row `Contributions` ladder) |
| 10 | **Keyboard-only, screen-reader & 200%/400% zoom** | `UT-8` + keyboard/SR runs of `SB1`–`SB7` (`D02`, `U01`, `U02`) |
| 11 | **Immediate Excel-power-user wins** | `SB1` (root-error folding), `SB3` (`SO10482` `4,200` discrepancy proof), `SB6` (non-locking `.xll` hot-reload + impact preview) |

---

## 15.2 Canonical End-to-End Walkthrough Scripts: `SB1`–`SB7` and `UT-8` (`CL-15-04`, `CL-15-05`, `D-03`, `D-91`)

Per Decider Ruling `D-91`, all scripts share the exact arithmetically verified fixture contracts in [Appendix D](appendix-d-fixtures-and-scenes.md) `[P19-FABLE-0040, P19-GPT-0081, P19-GPT-0082, P19-GPT-0083, P19-GPT-0084, P19-GPT-0085, P19-GPT-0086, P19-GPT-0087, P19-NEON-0101, D-91]`.

### `SB1` — Maria Orients Herself in `Inherited.xlsm` (`<= 3 min`)
- **Mockups:** [Pro 21 S01](http://127.0.0.1:8720/p21/storyboards.html#S01), [Pro 21 S02](http://127.0.0.1:8720/p21/storyboards.html#S02), [Pro 21 W01](http://127.0.0.1:8720/p21/workbook.html#W01), [Pro 21 M03](http://127.0.0.1:8720/p21/model-map.html#M03)–[M05](http://127.0.0.1:8720/p21/model-map.html#M05).
- **Spoken goal:** *"You just inherited `Inherited.xlsm` before a close meeting. Without running any macros, find out why `Summary!F18` is failing, check whether the missing `Rates2024.xlsx` file caused it, and return to `Summary!F18`."* `[P19-GPT-0081, D-91]`
- **Scripted conformance steps:**
  1. Open `Inherited.xlsm` (`40` sheets, `6` hidden/very-hidden, `~300,000` formulas): first cached Classic viewport paints in **`<= 1.0 s` p95** showing saved `Summary!F18` (`2,401,300` badged `◊ As saved` until background recalc reveals `#REF!`), while the Trust Bar quarantines VBA module `LegacyClose` (`Macros: Off`) and notes missing external link `Rates2024.xlsx` (`0` modal dialogs, `0` macros run).
  2. Switch to **Inspector** (`Ctrl+Alt+2`): `Overview` lists `40` sheets (`6` hidden readable without un-hiding) and folds downstream `#REF!` cascade cells under **one root cause**: broken defined name `Revenue.Window = OFFSET(Revenue!#REF!, 0, 0, 12, 1)`.
  3. Inspect `Sources`: verify `Rates2024.xlsx` is `Missing` (holding cached FX values) on a disjoint dependency branch—it does **not** feed `Summary!F18`.
  4. Select `Summary!F18` in the issue tree and press `Ctrl+Alt+1`: Classic lands directly on `Summary!F18` within `<= 150 ms` with selection preserved and `Stamp.model` unchanged (`before.model == after.model`).
- **Comprehension quiz checks:** Distinguishes saved `2,401,300` from current `#REF!`, names `Revenue.Window` (not `Rates2024.xlsx`) as `F18`'s root cause, and confirms `LegacyClose` never executed (`100%` macro avoidance) `[P19-GPT-0081, D-91]`.

### `SB2` — Maria Turns `Budget.xlsx` Into a Live Notebook (`<= 8 min`)
- **Mockups:** [Pro 21 S03](http://127.0.0.1:8720/p21/storyboards.html#S03)–[S05](http://127.0.0.1:8720/p21/storyboards.html#S05), [Pro 21 F03](http://127.0.0.1:8720/p21/notebook.html#F03), [Pro 21 N02](http://127.0.0.1:8720/p21/notebook.html#N02), [Pro 21 NC01](http://127.0.0.1:8720/p21/notebook-compute.html#NC01)–[NC07](http://127.0.0.1:8720/p21/notebook-compute.html#NC07).
- **Spoken goal:** *"Create a short executive write-up on `Budget.xlsx` citing annual operating profit (`Summary!B7 = 216,000`), test raising base unit price `Inputs!B3` from `120` to `125`, and resolve the `#SPILL!` on `Summary!A20` without deleting the note in `A21`."* `[P19-GPT-0082, D-91]`
- **Scripted conformance steps:**
  1. Open `Budget.xlsx`, press `Ctrl+Alt+3` (**Notebook**), enter `Edit` mode (`Enter`), write Source Serif 4 prose binding live chip `{{Summary!B7}}` (`R 216,000`), and press `Shift+Enter` (`PresentationChange` only; `Stamp.model` unchanged).
  2. Edit referenced input `Inputs!B3` (`120 -> 125`): Preview Seam discloses `1` cell write affecting `24` downstream cells in `Monthly!B8:M11` + `Summary!B7`; commit updates `{{Summary!B7}}` live to **`R 276,000`** (`23,000/mo`) across Notebook and Classic.
  3. Navigate (`j`/`k`) to spilling cell `Summary!A20` (`=TRANSPOSE(Monthly!B1:M2)`), currently `#SPILL!` because `Summary!A21` contains a text note; invoke **`Relocate note / move spill to free lane`** via the Preview Seam, verifying `A20` spills cleanly across `12` rows and zero user text is deleted.
  4. Save `.xlsx` and verify round-trip: formulas and `276,000` are plain Excel cells, while prose lives in `urn:dnacalc:views:v1` `[P19-GPT-0082, D-91]`.

### `SB3` — Sam Audits and Repairs `Sales.xlsx` (`<= 4 min`)
- **Mockups:** [Pro 21 S06](http://127.0.0.1:8720/p21/storyboards.html#S06), [Pro 21 S07](http://127.0.0.1:8720/p21/storyboards.html#S07), [Pro 21 F02](http://127.0.0.1:8720/p21/inspector.html#F02), [Pro 21 W03](http://127.0.0.1:8720/p21/workbook.html#W03).
- **Spoken goal:** *"On `Sales.xlsx`, `Report!B5` shows `128,450` for West Paid revenue, which disagrees with the `124,250` finance check. Find the exact row(s) causing the `4,200` difference, fix `Report!B5` only, then undo the fix from Classic."* `[P19-GPT-0083, D-91]`
- **Scripted conformance steps:**
  1. Select `Report!B5` (`=SUMIFS(Orders[Amount], Orders[Region], "West")`, `128,450`) and open **Inspector** (`Ctrl+Alt+2` -> `Precedents` / `Contributions`).
  2. Inspect the `Contributions` criterion ladder over the `25,000`-row `Orders` table: `126` rows match `Region = "West"` (`125` `Paid` totalling `124,250` + **`1` `Pending` order `SO10482` at data row `10,482` / sheet row `10,483` for `4,200`**).
  3. Confirm the `Database + Query Sheet` pattern reading (`Suggested -> Confirmed by Sam`, `PresentationChange::Reading`), then choose **Single-cell repair** (not full-column run rewrite) adding `Orders[Status], "Paid"` through the Preview Seam -> `Report!B5` recalculates to **`124,250`** (while `Pivot` sheet flags `Pivot cache older than Orders` if source changed).
  4. Switch to **Classic** (`Ctrl+Alt+1`) and press `Ctrl+Z`: `Report!B5` reverts cleanly to `128,450` (`Undid formula edit on Report!B5`) while Sam's confirmed pattern reading remains intact `[P19-GPT-0083, D-91]`.

### `SB4` — Lee Wires `Stock.xlsx` on Model Map and Verifies Classic (`<= 8 min`)
- **Mockups:** [Pro 21 S08](http://127.0.0.1:8720/p21/storyboards.html#S08), [Pro 21 S09](http://127.0.0.1:8720/p21/storyboards.html#S09), [Pro 21 F04](http://127.0.0.1:8720/p21/model-map.html#F04), [Pro 21 M01](http://127.0.0.1:8720/p21/model-map.html#M01), [Pro 21 M02](http://127.0.0.1:8720/p21/model-map.html#M02), [Pro 21 I03](http://127.0.0.1:8720/p21/inspector.html#I03).
- **Spoken goal:** *"Open `Stock.xlsx` in Model map, distinguish the 3-day order-arrival lag from the iterative reserve calculation, rewire the reorder trigger port to `Stock.ReorderPoint` (`B5`), test ticked clock steps on `Stock (ticked)`, and confirm the exact Excel formulas in Classic."* `[P19-GPT-0084, D-03, D-91]`
- **Scripted conformance steps:**
  1. Open `Stock.xlsx` in **Model map** (`Ctrl+Alt+4`): verify `A12:H43` renders as one folded 30-day period group where day-2 order `H15` (`420`) arriving at day-5 inflow `C18` via `INDEX` carries a **row-lag badge (`t-3`)**, visually and structurally distinct from same-period iterative cell `B8 = 0.5*(B8+B7)` inside its **dashed `↻ Iterative` enclosure** (`max 100, tol 0.001`, `20.00`) (`D-03`).
  2. Press `W` (or drag port) to wire `Stock.ReorderPoint` (`Stock!B5 = 260`, linked to native `ScrollBar` `0..600` step `10`) into the reorder threshold operand (`ModelChange::ReplaceReference` previewed in the Seam and committed).
  3. On `Stock (ticked)` (`M02`/`I03`), step **`DnaCalc.Clock`** (`Step` -> `Tick 14 -> 15`), verifying synchronized `z⁻¹` clocked delay badges and Inspector `Live` strip traces (`D-03`, `P21-0123`).
  4. Switch to **Classic** (`Ctrl+Alt+1`): inspect `Stock!B5`, `B8`, and `C18`, proving every wire compiled to standard Excel formulas that calculate identically in desktop Excel `[P19-GPT-0084, D-03, D-91]`.

### `SB5` — Jo Operates `Loan.xlsx` as an App, Recovers Validation, and Prints (`<= 3 min`)
- **Mockups:** [Pro 21 S10](http://127.0.0.1:8720/p21/app.html#S10), [Pro 21 S11](http://127.0.0.1:8720/p21/app.html#S11), [Pro 21 F05](http://127.0.0.1:8720/p21/app.html#F05), [Pro 21 A01](http://127.0.0.1:8720/p21/app.html#A01), [Pro 21 A03](http://127.0.0.1:8720/p21/app.html#A03).
- **Spoken goal:** *"Using `Loan.xlsx` in App view, pin the current `10.5%` payment as a baseline, test `12.0%`, see what happens if someone enters `0` months, restore or compare against baseline, and print a 1-page summary."* `[P19-GPT-0085, D-91]`
- **Scripted conformance steps:**
  1. Open `Loan.xlsx` in **App** (`Alt+Q -> App` / `Alt+Shift+5`): baseline shows Principal `R 1,000,000` (`B2`), Rate `10.5%` (`B3`), Term `360` months (`B4`), Monthly Payment **`R 9,147.39`** (`B6 = -PMT(B3/12,B4,B2)`). Click **`Pin Baseline`** (transient comparison badge, clearly marked `Session pin · not saved as Excel Scenario`).
  2. Enter `12%` in Rate: Payment updates live (or on `Apply inputs` when batched mode `A01` is toggled) to **`R 10,286.13`** (`+R 1,138.74` vs pinned baseline).
  3. Enter `0` in Term (`B4`): Excel's `Stop` validation (`1..360` integer months) blocks commit inline (`A03`) preserving `B4 = 360` and `R 10,286.13` with `[Revert to 360]`.
  4. Press `Ctrl+P` (`S11`): `runtime.consistent()` freezes one `CalcGeneration` into a 1-page **Executive Summary** (`Inputs`, `KPIs` with explicit `Pinned session baseline` label, dynamic `INDEX` balance chart, and generation timestamp), then restore `10.5%` `[P19-GPT-0085, D-91]`.

### `SB6` — Ade Tests and Hot-Reloads `DiscountFunctions.xll` in Formula Explorer (`<= 5 min`)
- **Mockups:** [Pro 21 S12](http://127.0.0.1:8720/p21/storyboards.html#S12), [Pro 21 S13](http://127.0.0.1:8720/p21/storyboards.html#S13), [Pro 21 F06](http://127.0.0.1:8720/p21/formula-explorer.html#F06), [Pro 21 F07](http://127.0.0.1:8720/p21/formula-explorer.html#F07), [Pro 21 E01](http://127.0.0.1:8720/p21/formula-explorer.html#E01)–[E03](http://127.0.0.1:8720/p21/formula-explorer.html#E03).
- **Spoken goal:** *"In standalone `DNA Calc - Formula Explorer`, load `DiscountFunctions.xll`, test `DNA.DISCOUNT(120, 0.1)` and `DNA.DISCOUNT(120, 1.2)`, try `DNA.CALLERADDRESS()`, hot-reload a rebuilt `.xll` without restarting, then preview and apply `=DNA.DISCOUNT(Inputs!B3, 0.1)` to `Summary!D14` inside `Budget.xlsx`."* `[P19-GPT-0086, D-91]`
- **Scripted conformance steps:**
  1. Launch standalone **Formula Explorer** (`<= 0.5 s` cold start, zero `calc-engine`), approve `DiscountFunctions.xll 1.0` on its native-code Trust Card (`E03`), evaluate `=DNA.DISCOUNT(120, 0.1)` -> **`108`** and `=DNA.DISCOUNT(120, 1.2)` -> **`#NUM!`** in the Live Tree (`E01`), and verify `=DNA.CALLERADDRESS()` returns an explicit **standalone caller-context refusal** (`Requires worksheet caller cell`).
  2. Rebuild `DiscountFunctions.xll` on disk (verifying **zero file lock**) and click **`Reload add-in`** without restarting Explorer (`D-91`).
  3. Open `Budget.xlsx` in DNA Calc, select `Summary!D14`, press `Alt+Shift+E` (**Docked Explorer**), draft `=DNA.DISCOUNT(Inputs!B3, 0.1)` (`TrialFor { at: Summary!D14 }` -> `108` at `B3 = 120`), inspect **`Preview Impact`**, and click **`Apply to Summary!D14`** (`expects` stale-checked commit) `[P19-GPT-0086, D-91]`.

### `SB7` — Maria Reviews, Partially Accepts, and Conflict-Resolves an AI Proposal (`<= 4 min`)
- **Mockups:** [Pro 21 S14](http://127.0.0.1:8720/p21/storyboards.html#S14)–[S16](http://127.0.0.1:8720/p21/storyboards.html#S16).
- **Spoken goal:** *"Review the agent's three-part proposal on `Budget.xlsx`: accept units A and B while rejecting C, undo only unit B (the heading), then handle a second proposal where you edited `Inputs!B3` to `123` while the agent proposed `125` from `120`, and verify the workbook's change trail."* `[P19-GPT-0087, D-14, D-91]`
- **Scripted conformance steps:**
  1. Agent submits a 3-unit `Proposal` (`S14`): Unit **A** (`Model`: `Inputs!B3` Unit price `120 -> 125`), Unit **B** (`Presentation`: Notebook heading `"Budget: price review"`, `0` formula changes), Unit **C** (`Model`, unsupported suggestion: `Inputs!B4` Unit cost `70 -> 72`). Maria ticks **A + B** and leaves **C** unticked: `WithEffects` subset preview computes **`Summary!B7 = 276,000`** (`24` changed cells in `Monthly!B8:M11`) for exact subset `A + B` (never borrowing `A + C`'s `252,000`).
  2. Click **`Accept 2 of 3`** (`Operation::Decide`): commits `A` and `B` as separate `Origin::Agent` `HistoryEntry` steps in one `GroupId` (`C` rejected). Press `Ctrl+Z` once (`S15`): **only Unit B reverts** (restoring the previous Notebook heading), keeping Unit A (`Inputs!B3 = 125`, `Summary!B7 = 276,000`).
  3. On a fresh `120` baseline (`S16`), agent stages `Inputs!B3: 120 -> 125` while Maria edits `Inputs!B3 -> 123`: clicking `Accept` triggers **`Refusal::Conflict`** showing the three-way diff (**Base `120` · Yours `123` · Agent `125`**) with zero silent overwrite.
  4. Verify `<trail>` in `urn:dnacalc:views:v1` records accepted A, accepted-then-undone B, and rejected C (`D-14`) `[P19-GPT-0087, D-91]`.

### `UT-8` — Cross-Cast Keyboard-Only, Screen-Reader & High-Zoom Audit
- **Scope (`P19-NEON-0101`):** Execute `SB1`, `SB3`, `SB4` (keyboard port-wiring via `W`), and `SB5` using **keyboard + screen reader only** at **`200%` zoom** (`1280×720` viewport) and **`high-contrast`** theme, plus `400%` prose/form reflow (`320 CSS px`) `[P19-NEON-0101]`.
- **Pass criteria:** `100%` task completion, `0` keyboard focus traps, `0` unlabelled icon controls, real Excel row/column coordinates announced under virtualization (`Orders` row `10,483` is never announced as row `1`), polite completion-only `aria-live` recalc announcements, and `0` WCAG 2.2 AA contrast failures across all 61 `SANCTIONED_PAIRS` `[P19-GPT-0284, P19-GPT-0285, P19-GPT-0286, P19-GPT-0287, P19-GPT-0288, P19-GPT-0289, P19-NEON-0101]`.

---

## 15.3 Accessibility, Corpus Model-Hash, and Visual Regression Harnesses (`CL-15-06`–`CL-15-10`, `D-92`)

1. **Nineteen-Fixture Keyboard & Screen-Reader Matrix (`CL-15-06`, `R-54`–`R-59`, `C-30`):** Beyond `SB1`–`SB7`, automated conformance tests verify `selected` vs `focused` vs `editing` announcements, click-only alternatives to every drag gesture (map wiring, block move, Notebook reorder, slider step), `200%`/`400%` reflow (`D02`, `U01`, `U02`), focus-triggered hover disclosures, and `prefers-reduced-motion` across **19 structural fixture families**: data boundaries, blank islands, formatted empty ranges, merged cells, filtered/hidden rows, non-contiguous multi-ranges, `ListObject` tables, dynamic spills, legacy CSE arrays, protected sheets, sheet-scoped names, cross-sheet formula picking, `""` empty-text formulas, clipboard round-trips, border drag/insert, `Esc` calc interrupts, hatched stale results, quarantined macros, and missing add-ins `[P19-GPT-0230, P19-GPT-0284, P19-GPT-0285, P19-GPT-0286, P19-GPT-0287, P19-GPT-0288, P19-GPT-0289, P19-GPT-0329, P21-0012]`.
2. **Automated Corpus, Contrast, and Cross-Module Harnesses (`CL-15-07`):**
   - **Corpus cross-view invariance (`Work area 8`):** `cargo xtask app smoke` opens oracle and Enron-tier (`20.28M` formula cells; `15,871` `.xlsx` files / `19.67M` saved-value formula cells in the repo corpus) corpus workbooks across all 5 views, asserting zero crashes/hangs, latency/memory ceilings, and **`before.model == after.model`** (exact model-hash invariance proving views and `Esc` reverts never mutate workbook semantics) `[FRAMING-0039, P19-FABLE-0412]`.
   - **Mechanical contrast unit tests (`SANCTIONED_PAIRS`):** `apps/dna-strand` fails `cargo test` immediately if any theme or `resolve` change breaks any of the 61 foreground/background pairs (`STRAND.md` updated whenever `theme.rs` evolves) `[FRAMING-0098]`.
   - **Six cross-module integration invariants (`P21-0123`):** (i) synchronized `M02`/`I03` clock tick counters and reset boundaries, (ii) `File` menu re-entry preserving sheet, filename, and accepted skin, (iii) scoped `New from pattern` Undo, (iv) Explorer tree/metadata/result alignment and `Evaluate` Undo, (v) `1440×900` first-viewport fit without clipping primary actions, and (vi) arrow-key stepping suppressed whenever focus is inside an interactive input `[P21-0013, P21-0123]`.
3. **Two-Tier Golden Visual Regression Matrix (`CL-15-08`, `D-92`):**
   - **Tier 1 (every milestone check):** Captures the **25 workbook × view intersections** (`5` views × `Budget`, `Sales`, `Loan`, `Stock`, `Inherited`) + **standalone & docked Formula Explorer** + **Strand v0.2 component sheet** (`D01`–`D03`: 10 cell states, 4 drawer types, ports/wires) at `1440×900` (`cockpit-light`, `cockpit-dark`, `high-contrast`) and `1280×720 @ 200%` zoom using real fixture numbers—never placeholder `lorem ipsum` `[FRAMING-0131, FRAMING-0144, P19-GPT-0299, D-92]`.
   - **Tier 2 (full milestone sweep):** Captures paired **Normal + Exception** and active lifecycle states across all views and storyboards (at rest, selection+peek, mid-edit+Preview Seam, mid-calc/`AsSaved`, error+root fold, Formula Lens on, ghost agent proposal, print preview, `Inherited` scale decline, and visible dual-stroke focus per control; any unproducible cell is tracked as a defect) `[P19-FABLE-0217, P19-GPT-0299, P21-0013, D-92]`.
4. **Independent Arithmetic Verification & Honest Reporting (`CL-15-09`):** Expected fixture aggregates (such as `Sales` `Orders` days 1–14 `Pending` = `2,073` rows / `R 2,334,750`, West `Paid` = `125` rows / `R 124,250`, West `Pending` = `1` row `SO10482` / `R 4,200`) are computed by an independent verification script rather than calling UI projection code `[FRAMING-0163, P21-0120]`. Reports keep heterogeneous counts strictly separate (unit assertions, browser smoke cases, scenario scripts, visual targets, and corpus pass/fail rates—never summed into one inflated number) and state clearly what was measured natively vs in headless browsers vs on Windows Excel `[P19-GPT-0010, P19-GPT-0365, P21-0121, P21-0124]`.
5. **Cross-View Milestone Exit Checks (`CL-15-10`):** Across `M0–M8` (`SKIN_ARCHITECTURE.md §16`, aligned with `D-93`'s capability waves), no view ships as an isolated demo: each milestone exit runs all 5 cast workbooks + zoom + screen-reader + Enron stress checks across every completed view without bypassing prior milestone checks, requiring that a reviewer can name the Excel backing cell/object for every non-prose item, predict every edit and `Ctrl+Z`, tell live calculated values from saved/pinned observations, and return to Classic without losing their place `[P19-GPT-0100, P19-GPT-0102, P19-GPT-0103, P19-GPT-0104, P19-GPT-0105, P19-GPT-0106]`.
