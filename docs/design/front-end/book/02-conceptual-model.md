# Part II. Conceptual Model

Clusters: `CL-02-01`–`CL-02-21` · Governing decisions: `D-04`, `D-12`, `D-14`, `D-18`, `D-22`–`D-31` · Mockups: [Pro 21 D03](http://127.0.0.1:8720/p21/design-system.html#D03), [Pro 21 S05](http://127.0.0.1:8720/p21/storyboards.html#S05), [Pro 21 S14](http://127.0.0.1:8720/p21/agent.html#S14), [Pro 21 S15](http://127.0.0.1:8720/p21/agent.html#S15), [Pro 21 S16](http://127.0.0.1:8720/p21/agent.html#S16)

---

## 2.1 One Workbook, Five Views, Two Tools, One Seam (`CL-02-01`)

Every open workbook in DNA Calc has **one authoritative model** (`ModelRoot` + `CalcEngine` inside `calc-host`) and **one optional presentation part** (`ViewPart` in `urn:dnacalc:views:v1`) `[P19-NEON-0041, P21-0010]`. Five built-in persona views project that workbook—**Classic**, **Inspector**, **Notebook**, **Model map**, and **App**—alongside any declarative skin (`dna.skin/1`) authored by a user or AI agent `[FRAMING-0006, FRAMING-0047, FRAMING-0114]`. Two focused tools accompany them: **Formula Explorer** (`skin-0` on host Level 0, docked or standalone) and **Forms maker** (`"forms"` authoring view over `PageRenderer`) `[FRAMING-0114]`.

Earlier experimental prototype skins collapse cleanly into this architecture—`Grid` becomes **Classic**, `Notebook` becomes the Jupyter-chain **Notebook**, `Canvas` becomes **Model map**, `Atlas` splits between **Inspector Overview** and **Model map** sheet zoom, `Tree` becomes the **Outline / Live Tree** panel, and `Ledger` splits between **Notebook DataFrames** and **Inspector Calculation** tables `[P19-FABLE-0160]`.

Three invariants hold across every view `[FRAMING-0061, FRAMING-0123, FRAMING-0134, P21-0010]`:

1. **Switching views changes presentation only.** Cycling through all five views never copies values, creates a second model, advances an RTD clock, or alters any cell, formula, name, or calculation setting (`Stamp.model` before equals `Stamp.model` after) `[FRAMING-0061, P19-GPT-0231, P21-0010]`.
2. **A view can do to the model only what a user can do in Excel, plus arrange its own presentation.** Every model mutation translates to an Excel-native `ModelChange`; pure layout, prose, view-sorting, and annotations are `PresentationChange`s in `urn:dnacalc:views:v1` `[FRAMING-0004, FRAMING-0114, P19-NEON-0041]`.
3. **One selection, one active clock, one undo history.** Focus, addresses, clipboard semantics, and Undo belong to the workbook session rather than to individual views `[FRAMING-0061, FRAMING-0134, P21-0010]`.

---

## 2.2 Targets and the Canonical Address Grammar (`CL-02-02`, `CL-02-03`, `D-22`)

Humans typing into the Bridge Name Box / Go To (`Ctrl+G` / `F5`), formulas referencing workbook structures, declarative `dna.page/1` component bindings, VBA view verbs, URL `#fragment` links, and AI agent tools all speak **one collision-free canonical address grammar** (`Strand Tenet 7`: *what a human types is what an agent sends*) `[FRAMING-0089, P19-CARBON-0047, P19-FABLE-0119, P19-NEON-0006]`:

| Target Kind (`calc-skin`) | Canonical Address Syntax (`D-22`) | Semantics & Display Rules |
|---|---|---|
| **Cell / Range** (`SourceRef::Cells`) | `Monthly!H13`, `Inputs!C5:D16`, `B2:B4,D2:D4` | Exact A1 (or R1C1) sheet coordinates; multi-area selections retain their single `ActiveCell`; edits mutate workbook content `[P19-FABLE-0120, P19-GPT-0027]`. |
| **Dynamic Spill** (`SourceRef::Spill`) | `Analysis!A10#`, `Monthly!C21#` |Anchored at the top-left formula cell and dynamically tracks its live spilled extent `[P19-FABLE-0120, P19-GPT-0030]`. |
| **Defined Name** (`SourceRef::Name`) | `ServicesShare`, `Store.Stock`, `Monthly!LocalRate` | Real workbook- or sheet-scoped Excel name; UI badges scope (`Workbook` vs `Monthly`) without polluting formula text; dots are valid Excel name characters, never a custom engine namespace `[P19-FABLE-0120, P19-GPT-0029, P19-GPT-0033]`. |
| **Excel Table** (`SourceRef::Table`) | `Orders`, `Orders[Amount]`, `Orders[#Headers]`, `Orders[@Date]` | Native `ListObject` structured reference; view-only sorting/filtering never mutates source row order `[P19-FABLE-0120, P19-GPT-0030]`. |
| **Sheet Object** (`SourceRef::Object`) | `obj:Loan/Balance` (or `Loan!"Balance"` in Excel contexts) | Actual embedded chart, shape, picture, Form Control (`Model!"Slider 1"`), or PivotTable (`Report!"PivotTable1"`); copying a view reference is distinct from duplicating the workbook object `[P19-FABLE-0120, P19-GPT-0031, D-22]`. |
| **Inferred Block** (`StructureItem` over `SourceRef::Cells`) | `Monthly!B5:O9` (displayed as `‹Revenue›`) | A contiguous R1C1 formula run or block reading; canonically addressed by its real Excel range, drawn dashed with guillemets `‹…›` until explicitly pinned as a Defined Name, and never creating hidden scope `[P19-FABLE-0121, P19-GPT-0028, D-22]`. |
| **View Item** (`Target::Item`) | `view:notebook/Summary/text-1`, `view:map/Store` | Pure presentation item (Markdown prose cell, layout section, visual group); displayed in muted ink with a `[View]` badge and dotted focus ring, leaving the Formula Bar empty `[P19-FABLE-0121, P19-FABLE-0122, P19-GPT-0032, D-22]`. |

Every bound value, DataFrame, map port, and App tile reveals its exact `SourceRef` in one action, while source-less prose is explicitly marked as presentation annotation (`R-2`) `[P19-GPT-0232]`. Typing any valid address into the Bridge Address Box (`Ctrl+G` / `F5`) resolves and selects that entity in the active view (`R-5`), backed by deterministic host entity resolution and existence tracking across edits and reopens (`C-1`, `C-27`) `[P19-NEON-0195, P19-GPT-0300, P19-NEON-0265]`.

---

## 2.3 Cross-View Selection, Camera Stability, and Global Navigation (`CL-02-05`, `CL-02-06`, `D-24`)

`HostSession` owns one authoritative `Selection { seq, target: SourceRef, active, grouped, item }` `[P19-FABLE-0022, P19-FABLE-0029]`. Switching views projects that same selection into the destination view (`150 ms` amber continuity halo) `[FRAMING-0072, P19-CARBON-0024]`:

1. **Exact or coarser containment (`Located::Exact` / `Located::Within`):** When the destination view shows an item containing the selected cells (such as a folded formula run `Monthly!B8:M8` containing `Monthly!C8`), that item is outlined within `<=150 ms` (`<=100 ms` warm target) and the Strip states the coarser mapping (`Selected Monthly!C8 · inside run B8:M8`); returning to Classic restores `Monthly!C8` `[P19-FABLE-0123, P19-FABLE-0429, D-24]`.
2. **Absent from curated layout (`Located::Absent` — zero silent substitution):** If a selected cell is not placed on a curated Notebook, Model map, or App page, DNA Calc **never manufactures a fake substitute selection** on a nearby cell; it preserves `Selection.target` intact and shows a Strip notice (`Selected Inputs!D9 is not in this layout`) with one-click **`Reveal in Classic`** and **`Add to this view`** actions `[P19-GPT-0035, P19-GPT-0233, D-24]`. Switching away from a source-less prose cell (`Target::Item`) activates the last remembered model cell with a Strip note `[P19-GPT-0035, D-24]`.
3. **Minimal-scroll camera preservation:** Each view and sheet remembers its own scroll offset and zoom independently; switching views never yanks the viewport if the target is already visible, scrolling only when needed to bring an off-screen target into view `[P19-FABLE-0124, D-24]`.
4. **Surviving formula drafts:** Requesting a view switch while editing a formula keeps the uncommitted `EditSession` draft alive in the shared Bridge editor (or prompts explicit `Commit` / `Discard` before mounting a surface without a formula bar), never committing half-typed syntax implicitly (`R-4`) `[P19-GPT-0035, P19-GPT-0234]`.
5. **Universal navigation chords:** Across every view, `F5` / `Ctrl+G` / Name Box / `Alt+Q` jump to any `Target`; `Ctrl+[` and `Ctrl+]` step upstream/downstream; `Alt+Left` and `Alt+Right` walk the cross-sheet, cross-view `Places` history; `F6` cycles Stage, Companion, Panel, Deck, Mast, and Strip; a workbook's `preferred` view applies once on initial open while per-sheet preferences stay non-switching tab badges; and from every view (including App), the backing cells in Classic are one action away (`Ctrl+Alt+1` or `Ctrl+Alt+0` companion pane) `[P19-FABLE-0125, P19-FABLE-0431, P19-FABLE-0432]`.

---

## 2.4 Read-Only Formula Lens vs. Scoped-Name Interpreter (`CL-02-04`, `D-23`)

DNA Calc cleanly separates two AST-driven mechanisms in `calc-formula-kernel`—both honouring the OxFml `W074` call/value and scope-resolution rules `[FRAMING-0080]`:

1. **Read-Only Formula Lens (`Present`, toggled via `Alt+Shift+L`):** Displays formulas in human-readable form—showing defined names plainly, sheet-inferred row/column headers in dashed guillemet chips `‹Revenue›`, and clean structural indentation—without ever mutating the stored formula or clipboard copy syntax `[FRAMING-0012, FRAMING-0063, D-23]`. An optional sub-toggle **`Show as steps`** factors repeated pure sub-expressions visually with an explicit badge (`Shown as steps · stored as one formula`), while **`Rewrite as LET…`** is strictly an explicit, previewed `ModelChange::RewriteFormula` batch `[FRAMING-0063, D-23]`. In Classic, pressing `F2` always edits the exact underlying Excel formula text.
2. **Scoped-Name Input Interpreter (`Interpret`):** Inside named Model map group frames (`Store.*`) and Notebook cells, typing unambiguous short names (`Stock * 2` inside `Store`) expands deterministically over `calc-fml`'s AST (leaving string literals, function names, and A1 ranges untouched) into exact Excel syntax (`=Store.Stock * 2`), which is shown live in the Formula Bar and Preview Seam before commit—refusing ambiguous dual-scope names instead of guessing `[FRAMING-0107, D-23]`.

---

## 2.5 Modeless 1-Bit Editing, the Preview Seam, and Non-Grid Edit Rules (`CL-02-07`–`CL-02-09`, `D-25`, `D-26`)

See [Pro 21 D03 — Review surfaces & drawers](http://127.0.0.1:8720/p21/design-system.html#D03) and [Pro 21 S05 — Notebook preview & blocked spill](http://127.0.0.1:8720/p21/storyboards.html#S05).

### 2.5.1 Modeless Truth and the `Esc` Unwind Ladder (`CL-02-07`)

One bit separates `SELECTED` from `EDITING`, reflected simultaneously in three places: the active item on stage, the Bridge Formula Bar, and the Strip's mode word `[FRAMING-0087, P19-FABLE-0023]`. Pressing `Esc` unwinds strictly one layer at a time (`Active Edit -> Open Completion/Popover -> Pending Preview Seam -> Copy Outline -> Drill-In Group`) and always restores the exact pre-action workbook and presentation state (`R-8`, `R-9`) `[P19-CARBON-0180, P19-NEON-0198]`.

### 2.5.2 Trigger Scope and Two Forms of the Preview Seam (`D-25`)

Every mutation batch passes through `HostSession::prepare` (`Preview`), which computes `ChangeKind` (`Model`, `Presentation`, or `Both`) from the actual prepared writes—never from caller assertions `[P19-FABLE-0447, P21-0008]`:

- **Form 1 — Inline destination label (zero extra clicks):** Single-cell entry in Classic, already-placed single-formula Notebook cells, and live scalar App inputs commit directly on `Enter` / `Tab` with zero modal ceremony `[P19-CARBON-0023, P19-NEON-0005, D-25]`. Outside Classic, focusing the control displays its inline destination label (`Model change · Loan!B3` or `Presentation only · 0 cells changed`) `[FRAMING-0062, P21-0008]`. In Classic, if an entry would overwrite a formula inside a uniform run, paste into hidden rows, or write across grouped sheets, a non-blocking inline seam warning appears before commit (`Replaces a formula in a run of 16 alike · Esc cancels`; `Grouped: 3 sheets`) `[P19-FABLE-0069, P19-FABLE-0400, P19-FABLE-0437, D-25]`.
- **Form 2 — Review Card (explicit `Apply` / `Cancel`):** Shown before committing any action that allocates new backing cells (`Placement`), edits a multi-cell formula run (`EnterRun`), rewires a Model map reference (`ReplaceReference`), renames a Defined Name (`RenameName`), applies a docked Formula Explorer trial, mutates an Excel Table filter/sort from Notebook, or reviews an AI agent `Proposal` `[P19-CARBON-0023, P19-NEON-0005, P21-0008, D-25]`.

### 2.5.3 Backing-Sheet Primacy, Placement, Run Exceptions, and Safe Delete (`CL-02-08`, `CL-02-09`, `D-26`)

- **Four legal edit outcomes:** Every gesture in every view is either (1) an exact Excel `ModelChange` reproducible in desktop Excel (`R-36`), (2) a pure `PresentationChange` leaving the model hash byte-identical (`R-37`), (3) an atomic `Both` batch (such as creating a placed Notebook cell), or (4) **refused** with a plain reason and the nearest valid Excel alternative (for example, typing `=A1+B1` inside prose offers to quote an existing cell or allocate a workbook cell, `R-42`, and view-only sorts/filters never alter Excel's row order, `R-41`) `[P19-CARBON-0021, P19-FABLE-0030, P19-FABLE-0130, P19-FABLE-0131, P19-FABLE-0448, P19-FABLE-0449, P19-FABLE-0453, P19-FABLE-0454, P19-GPT-0036, P19-NEON-0044]`.
- **Placement rule (`R-40`):** Authoring new model content from Model map or Notebook allocates a label cell, formula/value cell, and visible Defined Name in the next non-colliding slot on the target sheet (or column lane for dynamic arrays), shown and editable in the Preview Seam before commit `[P19-FABLE-0128, P19-FABLE-0452]`.
- **Exception-preserving run edits (`R-39`, `D-26`):** A formula run with interior overrides is always badged `"n alike, m differ"` (`11 alike, 1 differs`); editing the run updates only the uniform members by default unless the user explicitly ticks `Also overwrite m exception(s)` in the Preview Seam (`Shift+Enter` edits a single cell) `[P19-CARBON-0050, P19-FABLE-0451, D-26]`.
- **Safe `Delete` across views (`D-26`):** `Delete` clears cell contents in Classic, removes the presentation card only (`[View]`) on referenced Notebook/App items, clears an owned Notebook cell (with dependent warning if referenced), and on Model map opens a two-choice inline seam defaulting to **`Remove from Map (Presentation)`** beside **`Delete backing cell & name (Model — lists #NAME? dependents)`** (`Shift+Delete` pre-selects the Model option through the same seam) `[P19-CARBON-0050, D-26]`.

---

## 2.6 Unified History, Two-Revision Stamps, Recovery, and Semantic Clipboard (`CL-02-10`, `CL-02-11`, `D-27`, `D-28`)

### 2.6.1 Two Revisions and View-Aware Unified Undo (`D-27`)

`Stamp { workbook, model, view, calc }` advances `model` strictly on `ModelChange`s and `view` strictly on `PresentationChange`s (`SKIN_ARCHITECTURE.md §4.3`), driving the Mast's exact six-state word: `new`, `unchanged`, `saved`, `edited`, `saved, layout edited`, or `read-only` (`R-34`—undoing all edits back to opening restores `unchanged`) `[P19-FABLE-0049, P19-FABLE-0446]`.

All committed batches live in `HostSession`'s single linear forward-committing `History` (`WorkbookOp::EditView` alongside cell ops), surfaced chronologically in the **History** panel with `[Model]` and `[View]` filters (`SKIN_ARCHITECTURE.md §4.5, §18`) `[P19-FABLE-0082, P19-FABLE-0501, D-27]`. When the user presses `Ctrl+Z` (outside an active text draft), DNA Calc undoes the top `HistoryEntry` of the workbook regardless of which view created it—never switching views unexpectedly, and naming what was undone and where in the Strip (`Undid: moved box Stock (Model map · presentation) · [Show]`); out-of-order reversal of an earlier step uses conflict-checked `Operation::Revert(EntryId)` `[P19-FABLE-0082, P19-FABLE-0501, D-27]`. Local crash-recovery journals (`CommandJournal` tokens in OPFS / desktop profile storage) separate committed operations from unapplied drafts and pending proposals so reopening never double-applies a committed batch or silently commits a draft (`R-22`) `[P19-GPT-0252]`.

### 2.6.2 Cross-View Semantic Clipboard Matrix (`CL-02-11`, `D-28`)

Copying carries semantic identity alongside system TSV, sanitized HTML, and vector/bitmap representations (`R-90`, `C-46`), keeping exact formula text distinct from Formula Lens chips (`R-19`) `[P19-FABLE-0143, P19-FABLE-0502, P19-FABLE-0570, P19-GPT-0037, P19-GPT-0249]`:

| Copied Source | Paste -> Classic | Paste -> Notebook | Paste -> Model Map | Paste -> External App |
|---|---|---|---|---|
| **Cells / Range** | Full Excel relative/cut/Paste Special rules `[P19-FABLE-0503]` | **Linked Live DataFrame** (same workbook) with 5 s `Linked \| Static snapshot` chip (`D-28`) `[P19-FABLE-0503]` | Preview Seam: allocate named boxes for cells `[P19-FABLE-0503]` | TSV (displayed), styled HTML table, image; formulas to spreadsheets `[P19-FABLE-0503]` |
| **View Frame / Card** | Underlying backing cells `[P19-FABLE-0504]` | Second presentation view `[View]` of same cells `[P19-FABLE-0504]` | Second view ref `[View]` `[P19-FABLE-0504]` | Resolved values table + exact formula text `[P19-FABLE-0504]` |
| **Model Map Box** | Backing cells & names `[P19-FABLE-0505]` | Live inline value chip `[P19-FABLE-0505]` | Inline seam: **`Duplicate backing cells & names`** (resolving clashes) vs **`Place second view reference`** (`D-28`) `[P19-FABLE-0505]` | Vector image + `name = formula` text lines `[P19-FABLE-0505]` |
| **Inspector Trace / Issues** | Addresses & values `[P19-FABLE-0506]` | Prose with live `{=Ref}` chips `[P19-FABLE-0506]` | Not offered (plain notice) `[P19-FABLE-0506]` | Structured Markdown outline & TSV `[P19-FABLE-0506]` |
| **External Text / Table / Cross-Book** | Excel text/table import (leading-zero safe) `[P19-FABLE-0507]` | Prose text, or prompt `Text vs Backing cells` (never silent external link) `[P19-FABLE-0507, D-28]` | `name = formula` lines previewed as boxes `[P19-FABLE-0507]` | — |

---

## 2.7 Provenance, Orthogonal States, and First-Open Honesty (`CL-02-12`, `CL-02-13`, `D-18`)

No query or UI component carries a bare `CellValue`; every value travels inside `Shown<T> { value, provenance }` (`SKIN_ARCHITECTURE.md §4.3`) `[P19-GPT-0039, P19-GPT-0238]`:

1. **Orthogonal freshness vs. error values:** `Provenance` distinguishes `Empty`, `Literal`, `Calculated { calc }`, `AsSaved` (file-cached value not yet calculated by DNA Calc—honesty rule 1), `External { source, observed_at }`, `Stale { cause }` (`ManualMode`, `Calculating`, `Cancelled`, `SourceUnavailable`, `UnrefreshedPivotCache`), and `Refused { owner, reason }` `[P19-CARBON-0052, P19-GPT-0039, P19-GPT-0238]`. Actual Excel errors (`#REF!`, `#SPILL!`, `#NUM!`, `#NAME?`, `#CALC!`) remain real `CellValue`s and are grouped by root cause (`R-9`, `R-32`); missing add-ins, loading tiles, or iteration limits are reported via `Provenance`, never as fabricated cell error codes `[P19-FABLE-0444, P19-GPT-0239]`.
2. **First-open behaviour without modal hostage-taking (`R-26`–`R-33`):** Opening any file—even with manual calculation, missing external links, circular loops, or quarantined VBA—never raises a startup modal dialog `[P19-FABLE-0043, P19-FABLE-0442]`. File-cached values appear within budget under an explicit `As saved <date> · not recalculated here · F9` Strip mark (`R-26`); manual dirty cells hatch (`R-27`); recalculations exceeding `300 ms` show Strip progress with `Esc` interrupt while keeping typing responsive (`R-28`); completion reports how many values differed from the saved cache (`R-29`); missing links, macro quarantine, manual mode, and hidden/VeryHidden sheets summarise in one non-blocking notice line with read-only peeking of hidden sheets that leaves visibility untouched (`R-31`, `R-33`), calculation mode/scope stays visible (`R-11`), and gallery/print snapshot reads never advance live RTD streams or ticks `[P19-FABLE-0438, P19-FABLE-0439, P19-FABLE-0440, P19-FABLE-0441, P19-FABLE-0443, P19-FABLE-0445, P19-GPT-0241, P21-0028]`.

---

## 2.8 AI Agents as First-Class Participants (`CL-02-14`–`CL-02-17`, `D-14`, `D-29`–`D-31`)

See [Pro 21 S14 — Review the agent's three-unit proposal](http://127.0.0.1:8720/p21/agent.html#S14), [Pro 21 S15 — Accept A+B, then undo B only](http://127.0.0.1:8720/p21/agent.html#S15), and [Pro 21 S16 — Stale-base conflict](http://127.0.0.1:8720/p21/agent.html#S16).

1. **Zero backdoors & Excel-safe invocation (`D-30`):** Agents connect through `SkinHost` using the identical `Query`, `Target`, and `Batch` schemas as human views (`Strand Tenet 7`), invoked from **`Alt+Q` fall-through** (`Ask the agent: …`), **`Tools > Agent`**, or the **Mast agent chip** (never stealing Excel's `Ctrl+I` Italic or `Ctrl+J` newline) `[P19-FABLE-0108, P19-FABLE-0137, P19-FABLE-0138, D-30]`. Per-workbook write policy defaults to **`Propose`** (`R-81`), with user-selectable **`Look`** (read-only) and **`Apply, then review`** (commits marked `unreviewed` in Changes for one-key Keep/Revert), while unattended CLI agent runs write to a `.agent.xlsx` copy by default `[P19-FABLE-0139, P19-FABLE-0493, P19-GPT-0099, D-30]`. Agents can never grant themselves external data-sharing, enable macros, load add-ins, install extensions, alter calculation settings silently, or overwrite files (`R-50`, `R-87`) `[P19-FABLE-0115, P19-FABLE-0142, P19-FABLE-0499, P19-GPT-0280]`.
2. **Dotted teal proposal grammar & Looks vs. Model split (`D-29`):** Visual/layout requests (`"make Classic calmer"`, `"build a Loan App"`) enter via `Operation::TryOn` as pure `Presentation` previews (`0` model writes) `[P19-FABLE-0115, P19-FABLE-0142]`. Model proposals render on stage with a **dotted `teal-600` border**, 8% petrol-teal hairline hatch, casual-axis ghost values, and `[Agent]` badge—never amber (`R-86`)—settling with a 150 ms green pulse on accept `[P19-FABLE-0039, P19-FABLE-0498, D-29]`.
3. **Inert checkbox selection, exact subset preview, and safety checks (`D-31`):** Proposals arrive split into discrete `ProposalUnit`s (grouping R1C1 formula runs, separating `Model`, `Presentation`, and flagged unsupported suggestions, and locking structurally coupled dependencies via `needs: Vec<UnitId>`, `R-45`), with value-changing units left **unticked by default** to prevent review fatigue `[P19-CARBON-0053, P19-CARBON-0204, P19-FABLE-0140, P19-FABLE-0227, P19-GPT-0025, P19-GPT-0273, P19-GPT-0275, P19-NEON-0047, P19-NEON-0233, P21-0110]`. Each unit shows its before/after diff and three automated checks (`Opens in Excel`, `Adds no #Errors`, `Breaks no run`, `R-82`) `[P19-FABLE-0140, P19-FABLE-0494]`. Ticking checkboxes is 100% inert; **`Review selected` (`P`)** computes the exact combined value effects for **that chosen subset** (for example, on `Budget`, `A+B` previews `276,000` profit and `24` changed cells, never the `A+C` `252,000` total, `R-44`) `[P19-GPT-0025, P19-GPT-0274, P21-0110, P21-0111, D-31]`.
4. **Atomic conflict checks, per-unit Undo, `Create reversal`, and file `<trail>` (`D-14`, `D-31`):** **`Apply selected` (`Ctrl+Enter`)** validates all selected units atomically against `base` + `expects`—if an intervening edit changed `Inputs!B3` (`expected 120, current 123, proposed 125`), conflicting units block atomically showing all three values (`R-47`) with `Recompute` while independent units stay reviewable (`Stop` leaves an unapplied incomplete proposal, `R-49`) `[P19-GPT-0277, P19-GPT-0279, P21-0114]`. Accepted units commit in order as separate `Origin::Agent` `HistoryEntry` steps (`A` then `B`) under one `GroupId`, so one `Ctrl+Z` reverts `B` (heading) while keeping `A` (`price = 125`, `profit = 276,000`, `R-46`), recording `Undone` in immutable History (`Ctrl+Y` redoes) and allowing re-selection in Changes (`R-48`) `[P19-FABLE-0110, P19-FABLE-0117, P19-GPT-0026, P19-GPT-0276, P19-GPT-0278, P21-0112, P21-0113, D-31]`. Rejecting (`R`) records an optional reason (marking a formula-run override as a noted exception if desired), the Changes list is keyboard/ARIA/clipboard accessible, out-of-order rollback uses **`Create reversal`** (a checked forward Preview Seam batch highlighting any overlapping later edits), and accepted/rejected/undone metadata persists by default (with an opt-out/strip toggle) in `<trail>` inside `urn:dnacalc:views:v1` (`R-85`, `D-14`) `[P19-FABLE-0112, P19-FABLE-0116, P19-FABLE-0497, P19-GPT-0026, P21-0113]`.

---

## 2.9 Underlying System Capabilities Required by the Conceptual Model (`CL-02-18`–`CL-02-21`)

The conceptual model maps directly onto `calc-skin` and `calc-host` capabilities (consolidated in [Appendix C](appendix-c-system-needs.md)): exact cell/style/formula/setting reads and `Shown<T>` provenance (`C-2`, `C-3`), dirty/stale visibility (`C-11`), scoped change deltas including spill-shape shifts (`C-4`), editability classification across spills/arrays/tables/pivots/protection (`C-10`), non-committing `prepare` and budgeted snapshot `WithEffects` previews (`C-11`, `C-12`, `C-16`), Excel-exact structural reference rewrites (`C-20`), non-blocking read-only agent snapshots and proposal validation (`C-29`, `C-30`), attributed undo/redo, checked forward `Revert`, model-digest equality, and deduped recovery journals (`C-13`, `C-14`, `C-33`, `C-40`–`C-42`), and `ViewPart` anchor transforms, fingerprints, and pre-save Excel compatibility checks (`C-43`–`C-45`) `[P19-CARBON-0217, P19-CARBON-0222, P19-FABLE-0564, P19-FABLE-0565, P19-FABLE-0566, P19-FABLE-0567, P19-FABLE-0568, P19-FABLE-0569, P19-GPT-0301, P19-GPT-0302, P19-GPT-0303, P19-GPT-0309, P19-GPT-0310, P19-GPT-0311, P19-GPT-0312, P19-GPT-0313, P19-GPT-0332, P19-NEON-0244, P19-NEON-0258, P19-NEON-0267, P19-NEON-0268]`.
