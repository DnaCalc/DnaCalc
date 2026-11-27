# Part III. Shell

Clusters: `CL-03-01`–`CL-03-12` · Governing decisions: `D-08`, `D-12`, `D-32`–`D-37`, `D-86` · Mockups: [Pro 21 D01](http://127.0.0.1:8720/p21/design-system.html#D01), [Pro 21 C01](http://127.0.0.1:8720/p21/classic.html#C01), [Pro 21 C07](http://127.0.0.1:8720/p21/classic.html#C07), [Pro 21 O01](http://127.0.0.1:8720/p21/workbook.html#O01), [Pro 21 O02](http://127.0.0.1:8720/p21/workbook.html#O02), [Pro 21 O03](http://127.0.0.1:8720/p21/workbook.html#O03), [Pro 21 O04](http://127.0.0.1:8720/p21/workbook.html#O04), [Pro 21 W04](http://127.0.0.1:8720/p21/workbook.html#W04), [Pro 21 S01](http://127.0.0.1:8720/p21/storyboards.html#S01)

---

## 3.1 One Invariant Shell Across Views and Platforms (`CL-03-01`, `CL-03-05`, `CL-03-06`)

`apps/dna-shell` wraps every workbook view—and, stripped to `skin-0`, standalone **DNA Calc - Formula Explorer**—in one calm `petrol-800` (`#193F49`) flight-deck frame surrounding daylight paper (`#FFFFFF` / `#EDF1F2` in `cockpit-light`, `#1A262B` in `cockpit-dark`), using 100% self-contained original SVGs and zero external network calls or Microsoft assets `[FRAMING-0005, P19-CARBON-0011, P19-NEON-0004, P21-0011]`. Decoupled cleanly from wire protocol decoding (`Work area 4`), one shared shell serves Windows desktop (`WebView2`), browser WebAssembly (`/app/`), `dnacalc serve`, and `dna-onecalc` `[FRAMING-0035, FRAMING-0113]`.

Four structural guarantees prevent the five views from feeling like five disconnected apps `[P19-FABLE-0230]`:

1. **Coordinate invariance (`R-1`):** Switching views keeps the Mast, Deck, Bridge, Sheet-Tab bar, and Instrument Strip at fixed screen coordinates with zero window reload or layout jump `[P19-CARBON-0172, P19-FABLE-0413, P19-NEON-0191]`.
2. **At most two stage panes:** The Stage mounts **one** primary view and at most **one** optional Companion pane (`Classic` grid by default, toggled via `Ctrl+Alt+0` / `Alt+Shift+0`), sharing the one `Selection`; free multi-pane tiling and isolated per-view document tabs are permanently excluded `[P19-FABLE-0126, P19-FABLE-0230]`.
3. **Never-trapped guarantee (`R-10`, `R-13`):** File actions, commands, view switching, source inspection, Undo, Print, Add-ins, Settings, and Macro Security remain reachable in every view; even an authored `App` page cannot hide the Strip or remove the one-action return to `Classic` (subject only to standard Excel Worksheet Protection) `[P19-NEON-0024, P19-GPT-0243, P19-NEON-0200]`. On first launch, a once-dismissible, Help-restorable orientation line states: *`"Same workbook. Choose a view for the task. Alt+Q finds any command."`* `[P19-GPT-0009]`.
4. **Strand chrome discipline (`R-12`–`R-15`):** Profile identity uses Strand's two-block base-pair badge (`twin` for workbook DNA Calc, `dot-long` for standalone Formula Explorer) strictly in the Mast identity slot, never on data surfaces; neutrals stay petrol-biased `[FRAMING-0095]`. Keyboard focus always shows the >=2 px dual-stroke ring (drawing stage selection hollow when focus moves into chrome or a panel), the five semantic tokens and three line styles (`solid` stated, `dashed` inferred, `dotted` ghost) hold identically in chrome and stage, and motion stays strictly within the three sanctioned transitions `[P19-FABLE-0424, P19-FABLE-0425, P19-FABLE-0426, P19-FABLE-0427]`. Every shell flow explicitly specifies all twelve make-or-break UX moments (first run, discovery, loading/calc, view switch, refusals, undo/recovery, clipboard, hover/focus, speed, accessibility, and Excel-user wins) `[FRAMING-0122]`.

---

## 3.2 Shell Regions, Command Deck, Bridge, Sheet Tabs, and Strip (`CL-03-02`, `CL-03-03`, `D-32`, `D-33`)

```text
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ MAST: [twin] Budget.xlsx · saved   [Classic|Inspector|Notebook|Map|App]  [Alt+Q] [Agent] │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ DECK (32 px / 68 px, Ctrl+F1): File(F) Home(H) Insert(N) Formulas(M) Data(A) View(W) More│
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ BRIDGE (30 px): [ Monthly!B8        ▾] │ fx │ =Inputs!$B$2 * Inputs!B3                   │
├────────────────────────────────────────────────────────┬─────────────────────────────────┤
│ STAGE (14 px top corners, #FFFFFF working paper)       │ DOCKABLE PANEL (right/bottom)   │
│ Primary View  [+ optional Ctrl+Alt+0 Classic Companion]│ Details · Trace · Issues · Live │
├────────────────────────────────────────────────────────┴─────────────────────────────────┤
│ SHEET TABS: [≡ 3 Sheets]  [ Inputs ] [ Monthly ▤ ] [ Summary ¶ ]             [+]  100% -+│
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ STRIP (26 px, never hidden): Ready · ● (quiet steady mark)           Avg: 18k  Sum: 216k │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

1. **Mast:** Displays the base-pair badge, workbook title, host-computed state word (`new`, `unchanged`, `saved`, `edited`, `saved, layout edited`, `read-only`), the five-view segmented switcher, the `Alt+Q` unified command/search/Go-To/agent bar, trust/quarantine pill when relevant, and the AI agent activity chip `[P19-GPT-0009, P21-0009]`.
2. **Compact Strand Command Deck (`D-32`):** A slim single-row `32 px` Strand command band by default—expandable to a `68 px` two-row labeled band and foldable via `Ctrl+F1`—exposing Excel's familiar tabs (`File`, `Home`, `Insert`, `Formulas`, `Data`, `View` directly, with `Page Layout (P)`, `Review (R)`, and `Developer (L)` tucked visually under `More` while firing immediately by exact Excel `Alt` key-tip letters) `[P19-FABLE-0021, P21-0009, D-32]`. Pressing `Alt` (or `F10`) raises key tips; on the first five uses in an installation, a one-line hint beneath the Deck summarises the `Alt+Shift` DNA Calc tool layer (`R-10`) `[P19-FABLE-0422]`. Outside Classic, the Deck condenses to view-contextual actions while keeping `Alt` key tips and `Alt+Q` active `[D-32]`.
3. **Bridge & Shared Formula Bar (`R-3`–`R-7`):** A `30 px` bar (collapsible in `App`) holding the **Unified Address & Target Box** and the shared `dna-formula-ui` **Formula Bar** (`Recursive Mono`, multi-line `LET`/`LAMBDA` expandable) `[P19-CARBON-0013, P19-FABLE-0415, P19-FABLE-0416]`. `SELECTED` vs `EDITING` updates the stage item, Formula Bar, and Strip mode word together (`R-5`), `Esc` restores the pre-edit model hash (`R-6`) and steps back one level along the `Esc` ladder (`edit -> preview -> copy outline -> popover -> look-inside`, `R-7`) `[P19-FABLE-0025, P19-FABLE-0417, P19-FABLE-0418, P19-FABLE-0419]`.
4. **Shared Peek Card & Docked Panels (`R-11`):** Resting on any target for `500 ms` (or focusing and invoking Peek) opens one consistent **Peek Card** across every view without occluding its target; the identical content docks persistently in **Details** (`Alt+Shift+D`, renamed from Strand v0.1's "Inspector" panel because **Inspector** is now a full view) alongside **Trace** (`Alt+Shift+T`), **Changes**, **Issues**, **Outline**, **Live**, and **Watch** `[P19-FABLE-0025, P19-FABLE-0033, P19-FABLE-0423]`.
5. **Dedicated Stage-Bottom Sheet-Tab Bar (`D-33`, `CL-03-09`):** Workbook sheet tabs sit on their own thin bar directly below the Stage and above the 26 px Strip (never crammed inside the Strip), accompanied by an `[≡ N Sheets]` drawer listing visible, `Hidden`, and `VeryHidden` sheets `[P19-CARBON-0014, P21-0009, D-33]`. Sheet tabs remain visible across all workbook views and carry subtle micro-glyph badges showing per-sheet preferred views (e.g., a notebook sheet feeding a grid sheet with a map sheet over both)—never auto-switching views on tab click (`D-86`) `[FRAMING-0015, FRAMING-0064, FRAMING-0155, P19-CARBON-0014, D-33]`.
6. **Standing 26 px Instrument Strip (`R-2`):** **Never hidden under any view or skin.** Houses the input mode word (`Ready`/`Enter`/`Edit`/`Point` + `End`/`Extend`/`Add`/`ScrollLock`), quiet steady mark or abnormal calculation/progress/error-root/external/macro/agent pills, selection aggregates, and worksheet zoom `[P19-CARBON-0015, P19-FABLE-0414]`.

---

## 3.3 Keyboard Architecture: View Switching, `Alt+Q`, and `Alt+Shift` Tools (`CL-03-04`, `D-12`, `D-34`)

Grounded in the legacy shell keyboard atlas `[FRAMING-0071]` and Microsoft 365 Excel collision verification (`D-12`, `D-34`):

| Layer | Chords | Action & Collision Rule |
|---|---|---|
| **Primary View Switch (`D-12`)** | `Ctrl+Alt+1`..`4`, `Ctrl+Alt+0` | `1` Classic, `2` Inspector, `3` Notebook, `4` Model map, `0` toggle Classic Companion pane. **`Ctrl+Alt+5` is intentionally left unbound over Classic** because Excel binds `Ctrl+Alt+5` to floating-shape navigation (`A-111`) `[FRAMING-0177, D-12]`. Whenever an `AltGr` keyboard layout maps `Ctrl+Alt+<digit>` to a printable character (`²`, `³`, `{`), character entry wins. |
| **AltGr-Safe Secondary View Alias (`D-12`)** | `Alt+Shift+1`..`5`, `Alt+Shift+0` | Uniform view switch reaching all five views—including **`Alt+Shift+5` for App**—and `0` Companion on European `AltGr` keyboards without touching `Ctrl+1..5` (Format Cells, Bold, Italic, Underline, Strikethrough) `[D-12]`. |
| **Unified Command Search (`D-34`, `R-9`)** | `Alt+Q` | Opens within `<=50 ms` (`Ctrl+K` remains Hyperlink; `Ctrl+Shift+P` remains Font Size). Searches commands, key-tip paths, `Target` addresses, names, sheets, blocks, views (`App`, `Forms maker`), Help topics, and offers **`Ask the agent: …`** as its final fall-through row `[P19-FABLE-0421, D-34]`. |
| **DNA Calc Tool Layer (`D-34`)** | `Alt+Shift+L / E / T / D / P` | `L` Formula Lens (preserving `Alt,L` for Developer key tip), `E` Docked Formula Explorer (alongside `Shift+F3` and `Alt,M,V`; preserving `Ctrl+E` for Flash Fill), `T` Trace panel, `D` Details panel, `P` Classic Provenance Overlay `[D-34]`. |

---

## 3.4 Start Stage, Staged Open, Startup Trust Bar, and Loss-Safe Save (`CL-03-07`, `CL-03-08`, `D-35`, `D-36`)

See [Pro 21 O01 — File · Open, recent & patterns](http://127.0.0.1:8720/p21/workbook.html#O01), [Pro 21 O02 — Save & view metadata](http://127.0.0.1:8720/p21/workbook.html#O02), and [Pro 21 S01 — Find your footing in Inherited](http://127.0.0.1:8720/p21/storyboards.html#S01).

### 3.4.1 Calm Start Stage and "New from Pattern" (`O01`)

Cold launch opens a zero-popup, zero-account-wall **Start Stage** on mist paper presenting three primary actions—**Open Workbook** (`Ctrl+O` or drag-and-drop anywhere on the window), **New Workbook** (`Ctrl+N`), and **Formula Explorer** (`Alt+Shift+E`)—beside Recent files (badged with their preferred view and Classic fallback), **New from example / pattern** (such as `Budget: Inputs, Monthly model & Summary`, disclosing the exact ordinary sheets, cells, and formulas before creation, with scoped Undo), a one-line Excel compatibility promise (*`"Your file stays an Excel file. Nothing is added that Excel cannot open."`*), and the discovery rule (*`"Everything you know from Excel works in Classic. What is new is on Alt+Shift and Alt+Q."`*) `[FRAMING-0157, P19-CARBON-0026, P19-FABLE-0041, P21-0096]`. Unavailable recent paths report the missing path cleanly without substituting a blank workbook `[P21-0096]`.

### 3.4.2 Staged Non-Blocking Open & Single Startup Trust Bar (`D-36`)

Workbooks open in stages (`C-1`): sheet metadata (including `Hidden`/`VeryHidden`), active-viewport formatting, and `AsSaved` cached cell values paint immediately (`<=1 s` p95 on `Inherited`) before background dependency graph construction or recalculation finishes `[P19-CARBON-0207, P19-FABLE-0525, P19-NEON-0239]`. Untrusted macros (`LegacyClose`), missing external workbooks (`Rates2024.xlsx`), manual calculation mode, `OFFSET` volatility, and error roots never trigger modal popups (`Quarantine Without Hostage-Taking`); instead, one compact non-blocking **Trust & Notice Bar** docks at the top of the Stage with counts (`40 sheets · 6 hidden · 3 missing links · 1 macro quarantined`) and a one-click **`[Open Overview / Inspector]`** button—reachable via `F6` or the Mast trust pill, never stealing `Alt+Q` and never auto-opening a side panel that shrinks the grid `[P19-CARBON-0025, P19-NEON-0025, D-36]`.

### 3.4.3 Loss-Safe Save vs. Explicit Export (`O02`, `D-35`, `R-67`)

Standard **Save (`Ctrl+S`)** preserves the last-good file on any I/O failure, shows a quiet confirmation when view settings travel inside `urn:dnacalc:views:v1`, **never silently downgrades `.xlsm` to macro-free `.xlsx`**, preserves unknown supported objects/parts, and refuses legacy binary `.xls` writes `[FRAMING-0157, P19-GPT-0297, D-35]`. Converting a workbook to a format that drops macros, formulas, or `urn:dnacalc:views:v1` metadata requires an explicit **`Save a Copy / Export as…`** flow with an itemized content-loss confirmation naming every affected VBA module, UDF, or view declaration before writing `[P19-GPT-0297, D-35]`.

---

## 3.5 Frozen Consistent Print Engine Across All Views (`CL-03-10`, `CL-03-11`, `D-37`)

See [Pro 21 O03 — Print · the computing notebook](http://127.0.0.1:8720/p21/workbook.html#O03) and [Pro 21 O04 — Print · selected Model map region](http://127.0.0.1:8720/p21/workbook.html#O04).

1. **Frozen `consistent()` snapshot (`D-37`, `C-29`):** Pressing `Ctrl+P` (or `Ctrl+F2` in Classic) captures a single-generation `runtime.consistent()` snapshotforced to `cockpit-light` daylight vector paper (`R-34`), stamped with `Current` / `Previous` / `Saved` freshness, active scenario, filters, missing sources, and timestamp (with snapshot/revision IDs printed in page footers only, `R-63`, `R-96`) `[P19-CARBON-0093, P19-CARBON-0205, P19-FABLE-0203, P19-FABLE-0513, P19-GPT-0077, P19-GPT-0293, P19-GPT-0328, P21-0097, D-37]`. The preview never mutates mid-inspection if RTD ticks arrive, surfacing a quiet `Model changed — Refresh preview` button instead; real `#Error`s print honestly as errors and blocked/stale states never masquerade as fresh numbers `[P19-GPT-0077, P21-0097, D-37]`.
2. **View-specific honest layouts (`R-45`, `R-64`, `R-95`) + `"Print as Excel would"`:**
   - **Classic:** Calls `calc_host::printing` (`C-21`, `C-25`) to honour `PrintArea`, `PrintTitles`, manual/automatic page breaks, scaling, and headers/footers identically to Excel (`R-95`) `[P19-CARBON-0227, P19-FABLE-0512, P19-GPT-0076, P19-NEON-0263]`.
   - **Notebook (`O03`):** Typesets A4 portrait reading order (paginating horizontal pipelines vertically), marks captured RTD values as non-updating, states exact row counts on windowed DataFrames, and offers an `[ ] Include formula & anchor appendix` checkbox that updates page count live `[P19-GPT-0076, P19-GPT-0294, P19-NEON-0235, P21-0098]`.
   - **Model map (`O04`):** Prints A4 landscape for a readable selected region/hierarchy with boundary-connector index, delay-vs-iteration legend, and dependency-explanation disclaimer, refusing unreadable whole-graph shrink-to-fit `[P19-GPT-0076, P19-GPT-0294, P21-0099]`.
   - **Inspector, App, & Explorer:** Print the active instrument (or 1-click Full Audit Dossier, `D-54`), 1-page Executive Summary (+ optional schedule pages, `D-69`), and 1-page Formula Verification Card, while every non-Classic view also offers **`Print worksheet as Excel would`** `[P19-FABLE-0203, P19-GPT-0076]`. Native OS print dialogs and physical/PDF pagination remain host-verified milestones (`RUN-ON-WINDOWS.txt`) `[P21-0129]`.

---

## 3.6 Shell-to-Host State and Object Contracts (`CL-03-12`)

The shell consumes neutral `calc-skin` queries and operations (`C-2`–`C-7`, `C-24`, `C-26`, `C-28`, `C-34`): `Summary` and `Cells` separate saved-cache metadata (`AsSaved` + writer/date) from `Calculated` values (`C-2`, `C-3`); `Changes` and `CalcEvents` deliver post-recalc value diffs, trigger causes, and optional evaluation order (`C-4`, `C-5`); `CalcState` and `CancelCalculation` provide sliced progress and responsive typing during long runs plus calculation/iteration/date-system settings (`C-6`, `C-7`); `Objects` and `ModelChange` operate native charts (`calc-chart-render` SVG + `ChartData` series, `C-26`), PivotTables, Form Controls, scenarios, validation, and `PrintPlan` (`C-24`); `CopyToClipboard`/`PreparePaste` preserve origin and format-loss disclosure (`C-28`); and `Refusal` keeps unsupported host capabilities strictly distinct from invalid workbook content while preserving the file untouched (`C-34`) `[P19-FABLE-0526, P19-FABLE-0527, P19-FABLE-0528, P19-FABLE-0529, P19-FABLE-0530, P19-FABLE-0531, P19-GPT-0323, P19-GPT-0327, P19-GPT-0333, P19-NEON-0264]`.
