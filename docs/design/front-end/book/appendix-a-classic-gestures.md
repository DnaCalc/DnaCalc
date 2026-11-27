# Appendix A. Classic Keyboard, Key-Tip, and Mouse Conformance

Cross-reference: [Part IV. Classic](04-classic.md) · Governing decisions: `D-11`, `D-12`, `D-35`–`D-43` · Architectural contract: [SKIN_ARCHITECTURE.md §§7.1, 8.3, 17.3](../SKIN_ARCHITECTURE.md) · Mockups: [Pro 21 F01](http://127.0.0.1:8720/p21/classic.html#F01), [Pro 21 C01](http://127.0.0.1:8720/p21/classic.html#C01)–[Pro 21 C07](http://127.0.0.1:8720/p21/classic.html#C07)

---

## A.1 Conformance Rules and Tier Definitions

1. **Single data-driven profile (`excel-365-win-en-us`):** Every row below is an entry in Classic's machine-readable conformance table (`apps/dna-skin-runtime` + `apps/dna-grid`), driving both keymap dispatch and automated browser/native conformance suites (`SKIN_ARCHITECTURE.md §8.3`). Where Microsoft's public documentation is ambiguous, observed behaviour on pinned Windows Desktop Excel 365 (`en-US`) wins (`RUN-ON-WINDOWS.txt`).
2. **Tier 1 (`T1`) vs Tier 2 (`T2`):** **`T1`** gestures are mandatory for the Milestone 3 switch commit (`>= 832` smoke cases); **`T2`** gestures complete full desktop power-user parity across Classic's 13 dialog and object families. Unimplemented commands reached by key tip or chord surface a typed `Refusal::NotModelled` naming the owning work package—never failing silently or firing a different command (`D-11`).

---

## A.2 Modes, Entry, Commit, and Formula Keys (`Parts 1–3`)

| ID | Tier | Gesture / Chord | Mode / Scope | Exact Classic Behaviour |
|---|---|---|---|---|
| `A-001` | `T1` | Printable character | `Ready` | Clears cell presentationally in draft buffer, enters **`Enter`** mode (`=`, `+`, `-`, `@` start formula entry; `' ` starts literal text prefix hidden on grid). |
| `A-002` | `T1` | `F2` | `Ready` / `Enter` / `Edit` / `Point` | `Ready -> Edit` (caret at end of formula/value); `Enter <-> Edit` (toggles whether arrows move cells or move the text caret); `Point -> Edit`. |
| `A-003` | `T1` | Double-click cell | `Ready` | Enters `Edit` mode placing the amber caret at the clicked character offset (`Edit directly in cells` on; focuses Bridge formula bar if off). |
| `A-004` | `T1` | `Backspace` / `Delete` | `Ready` vs `Edit` | `Ready`: `Delete` clears contents of entire selection (preserving formats; safe on spills per `02-conceptual-model.md §2.5`); `Backspace` clears active cell and enters `Enter` mode. `Edit`: deletes 1 char before/at caret. |
| `A-005` | `T2` | `Ctrl+Delete` | `Edit` | Deletes from the caret to the end of the current line in the cell/formula editor `[P19-FABLE-0255]`. |
| `A-006` | `T2` | `Insert` | `Edit` | Toggles character overtype mode inside the active `EditSession` (block caret indicator) `[P19-FABLE-0257]`. |
| `A-007` | `T1` | `Alt+Enter` | `Enter` / `Edit` | Inserts a literal newline (`LF`, `U+000A`) at the caret and enables `Wrap Text` on the cell upon commit. |
| `A-008` | `T2` | Column AutoComplete & `Alt+Down` | `Enter` / `Edit` / `Ready` | Typing a unique prefix of existing text entries in the same contiguous column proposes the remainder inline (`Enter` accepts; typing further declines); `Alt+Down` opens the pick-from-drop-down list of column text values (or Validation/AutoFilter list when present) `[P19-FABLE-0268]`. |
| `A-009` | `T1` | `Enter` / `Shift+Enter` | `Enter` / `Edit` / `Point` | Commits draft and moves active cell down (`Enter`) or up (`Shift+Enter`), wrapping within a multi-cell selection or returning to the start column of a `Tab`-traversed row entry run. |
| `A-010` | `T1` | `Tab` / `Shift+Tab` | `Ready` / `Enter` / `Edit` | Commits any active draft and moves right (`Tab`) or left (`Shift+Tab`), wrapping inside a multi-cell selection or Excel `ListObject` Table (inserting a new Table row at the bottom-right cell). |
| `A-011` | `T1` | `Ctrl+Enter` | `Enter` / `Edit` / `Point` | Commits the active value or R1C1-adjusted relative formula across **every selected cell/area** simultaneously without moving the active cell (`C05`). |
| `A-012` | `T1` | `Ctrl+Shift+Enter` | `Enter` / `Edit` / `Point` | Commits a legacy CSE `{=…}` array formula over the selected range where explicitly requested (dynamic arrays spill natively on plain `Enter`). |
| `A-013` | `T1` | `Esc` | Any draft / ants / mode | Cancels active cell/formula draft restoring exact prior formula and `Stamp.model`; in `Ready` cancels marching-ants clipboard mode or `Extend`/`Add` mode (`D-36`). |
| `A-014` | `T1` | `Ctrl+;` / `Ctrl+Shift+:` | `Ready` / `Enter` / `Edit` | Inserts current local date (`Ctrl+;`) or time (`Ctrl+Shift+:`) as a static Excel serial number with matching date/time format. |
| `A-015` | `T1` | `F4` | `Enter` / `Edit` / `Point` vs `Ready` | While editing a formula (`C03`): cycles the reference token(s) under/touching the caret or selection through `A1 -> $A$1 -> A$1 -> $A1 -> A1`. In `Ready`: repeats the last repeatable formatting/structural command (`Ctrl+Y` equivalent). |
| `A-016` | `T1` | `F3` / `Ctrl+Shift+F3` | `Edit`/`Point` vs `Ready` | `F3` while editing inserts a Defined Name (`Paste Name`); in `Ready` pastes list of names. `Ctrl+Shift+F3` creates names from selection headers (via Preview Seam). |
| `A-017` | `T1` | `F9` / `Shift+F9` / `Ctrl+Alt+F9` | `Edit` vs `Ready` | In `Edit`: evaluates selected sub-expression (or whole formula) to literal(s), reversible via **single-step `Ctrl+Z` un-bake** while staying in `Edit` (`D-37`). In `Ready`: recalculates workbook (`F9`), active sheet (`Shift+F9`), or full rebuild (`Ctrl+Alt+F9`). |
| `A-018` | `T1` | `Ctrl+~` (`Ctrl+\``) | `Ready` | Toggles **Show Formulas** mode on the active sheet (widening columns 2x temporarily without mutating stored column widths). |
| `A-019` | `T2` | `Ctrl+Shift+U` | `Ready` / `Edit` | Expands or collapses the multi-line Bridge formula bar; its bottom splitter edge also drags smoothly by pointer `[P19-FABLE-0284]`. |
| `A-020` | `T2` | `Ctrl+'` / `Ctrl+Shift+"` | `Ready` / `Enter` / `Edit` | Copies the exact formula text (`Ctrl+'`) or evaluated value (`Ctrl+Shift+"`) from the cell immediately above into the active cell and remains in `Edit` mode `[P19-FABLE-0286]`. |
| `A-021` | `T1` | `Shift+F3` / `Alt+Shift+E` | `Ready` / `Edit` | Opens **Docked Formula Explorer** (`TrialFor` on active cell; `Shift+F3` opens Function Browser on empty cell). |
| `A-022` | `T2` | `Alt,M,P` · `Alt,M,D` · `Alt,M,A,A` | `Ready` | Draws precedent (`P`) or dependent (`D`) tracer arrows on the Classic canvas (solid blue same-sheet, dashed black with miniature sheet icon cross-sheet, red on error); double-clicking an arrow head/tail jumps to the other end (or opens Go To list); `Alt,M,A,A` removes all arrows `[P19-FABLE-0289]`. |
| `A-023` | `T2` | Background Error Flags & `Alt+Shift+F10` | `Ready` | Flags Excel's standard background rules (number stored as text, inconsistent formula in run, formula omitting adjacent numbers, unlocked formula cell, two-digit text year) with top-left corner indicator; `Alt+Shift+F10` opens the action menu including `Convert to Number`, `Update Formula`, `Trace Error`, and `Ignore Error` `[P19-FABLE-0291]`. |

---

## A.3 Pointing and Range Picking During Formula Entry (`Part 4`)

| ID | Tier | Gesture / Chord | Scope | Exact Classic Behaviour |
|---|---|---|---|---|
| `A-024` | `T1` | Arrow / `Shift+Arrow` / `Ctrl+Arrow` after operator (`=`, `+`, `(`, `,`, `:`) | `Enter` / `Point` | Enters **`Point`** mode (`C02`), moving the dashed marching-ants range cursor and rewriting the active reference token at the amber caret using the 5-step cool Reference Palette + numbered corner notches `①..⑤` (`D-36`). |
| `A-025` | `T1` | Click / drag cells or headers while in `Enter`/`Point`/`Edit` after operator | Any sheet | Inserts or replaces the active reference (`B3`, `B3:B14`, `B:B`, `3:3`); clicking another sheet tab (`Ctrl+PgUp`/`PgDn` or mouse) prefixes `Sheet!` (`'My Sheet'!B3`) and returns the viewport cleanly to the formula's home sheet on commit or `Esc`. |
| `A-026` | `T2` | Click cell in another open workbook | `Point` | Inserts an absolute external workbook reference matching Excel syntax: `[Book.xlsx]Sheet!$A$1` `[P19-FABLE-0298]`. |
| `A-027` | `T1` | Select inside an Excel `ListObject` Table | `Point` | Inserts structured references (`Orders[Amount]`, `[@Amount]`, `Orders[[#Headers],[Amount]]`) when `Use table names in formulas` is enabled (`D-42`). |
| `A-028` | `T2` | Select the exact extent of a Defined Name | `Point` | Substitutes the workbook or sheet-scoped Defined Name (`Revenue.Base`) in place of raw A1 coordinates, matching Excel `[P19-FABLE-0301]`. |
| `A-029` | `T2` | Click a data cell inside a `PivotTable` | `Point` | Inserts `GETPIVOTDATA(...)` when Excel's `Generate GetPivotData` option is enabled (plain cell address when toggled off) `[P19-FABLE-0302]`. |

---

## A.4 Navigation and Selection (`Parts 5–6`)

| ID | Tier | Gesture / Chord | Exact Classic Behaviour |
|---|---|---|---|
| `A-030` | `T1` | `Arrow` / `Home` / `Ctrl+Home` / `Ctrl+End` | Single-cell step (skipping hidden rows/columns; traversing merged cells as units); `Home` -> column `A` (or first unfrozen column when `Scroll Lock` off); `Ctrl+Home` -> `A1` (or top-left unfrozen cell); `Ctrl+End` -> bottom-right used cell. |
| `A-031` | `T1` | `Ctrl+Arrow` / `Ctrl+Shift+Arrow` | Data-edge jump (or selection extension): inside contiguous non-blank run jumps to last non-blank cell before blank; on blank or run edge jumps across blank island to next non-blank cell (or sheet boundary `1048576` / `XFD`). |
| `A-032` | `T1` | `End`, then `Arrow` / `Home` / `Enter` | Arms `END` Strip indicator; next arrow performs data-edge jump (`End, Home` -> last used cell; `End, Enter` -> last non-blank in row). |
| `A-033` | `T1` | `PgUp` / `PgDn` · `Alt+PgUp` / `Alt+PgDn` | Scrolls one full viewport height up/down (`PgUp`/`PgDn`) or one full viewport width left/right (`Alt+PgUp`/`Alt+PgDn`). |
| `A-034` | `T1` | `Ctrl+PgUp` / `Ctrl+PgDn` | Switches to previous / next visible worksheet tab (keeping active view pinned per `D-86`; `Alt+[` / `Alt+]` alternate in browsers per §A.8). |
| `A-035` | `T1` | `Ctrl+G` / `F5` & Name Box | `Ctrl+G`/`F5` opens **Go To** (with back-history + `Special…` for Formulas, Constants, Blanks, Dependents, Precedents, Conditional Formats, Data Validation, Differences, Visible cells); Name Box accepts any `SourceRef` / A1 / Name (`Enter` jumps/selects; typing an unused identifier defines a Name). |
| `A-036` | `T1` | `Ctrl+[` / `Ctrl+]` · `Ctrl+Shift+{` / `Ctrl+Shift+}` | Selects direct precedents (`[`), direct dependents (`]`), all precedents (`{`), or all dependents (`}`) of selection on active/first sheet. |
| `A-037` | `T2` | `Ctrl+Backspace` | Scrolls the viewport immediately to bring the active cell back into view without altering the selection `[P19-FABLE-0311]`. |
| `A-038` | `T2` | `Scroll Lock` on | Arrow and page keys scroll the viewport window while keeping the active cell and selection stationary (`SCRL` Strip badge) `[P19-FABLE-0312]`. |
| `A-039` | `T2` | Cell Hyperlinks (`Click` / long-press / `Ctrl+K`) | Short click follows the internal `#Sheet!A1` or external URI hyperlink; press-and-hold (`>= 350 ms`) selects the cell without following; `Ctrl+K` opens Insert/Edit Hyperlink `[P19-FABLE-0319]`. |
| `A-040` | `T2` | Scrollbar thumb drag | Displays a live row-number / column-letter tooltip (`Row 10483` / `Column AA`) tracking the thumb target during drag `[P19-FABLE-0321]`. |
| `A-041` | `T2` | `Ctrl+Alt+Right` / `Ctrl+Alt+Left` | Moves active focus forward/backward among disjoint rectangular areas of a multi-area selection `[P19-FABLE-0324]`. |
| `A-042` | `T2` | Double-click cell selection border (`+ Shift`) | Jumps active cell to the data edge in the clicked border direction (top/bottom/left/right); holding `Shift` while double-clicking extends selection to that data edge `[P19-FABLE-0325]`. |
| `A-043` | `T1` | `Ctrl+Space` / `Shift+Space` / `Ctrl+A` | `Ctrl+Space`: column (in Table: Table data column -> with headers -> sheet column); `Shift+Space`: row; `Ctrl+A`: progressive `Current region / Table body -> Table with headers/totals -> Entire worksheet`. |
| `A-044` | `T2` | `F8` (`Extend`) & `Shift+F8` (`Add`) | `F8` latches Extend Selection mode (`EXT` badge) so unmodified arrows/clicks grow the range until `F8` or `Esc`; `Shift+F8` latches Add/Remove Selection mode (`ADD` badge) for keyboard multi-area selection `[P19-FABLE-0331]`. |
| `A-045` | `T1` | `Ctrl+Click` / `Ctrl+Drag` | Adds disjoint range area, or symmetrically deselects/splits already-selected cells (`Excel 365` deselection parity). |
| `A-046` | `T2` | `Shift+Backspace` | Collapses a multi-cell or multi-area selection down to the single active cell `[P19-FABLE-0334]`. |
| `A-047` | `T1` | `Alt+;` | Restricts the current selection strictly to **visible cells** (excluding hidden/filtered/collapsed rows and columns). |
| `A-048` | `T2` | Floating Objects (`Click` / `Ctrl+Click` / `Tab` / `Esc`) | Clicking a chart or shape selects it; `Ctrl+Click` multi-selects objects (or selects a macro/form control without firing it); `Tab` / `Shift+Tab` cycles through sheet objects; `Esc` returns focus to grid cells `[P19-FABLE-0337]`. |

---

## A.5 Fill Handle, Border Dragging, and Clipboard (`Parts 7–9`)

| ID | Tier | Gesture / Chord | Exact Classic Behaviour |
|---|---|---|---|
| `A-049` | `T1` | Drag Fill Handle (`+ Ctrl`) | Extends formulas (adjusting relative refs) or linear/date/text-number series (`C04`) with live tooltip preview; holding `Ctrl` toggles Copy <-> Series. |
| `A-050` | `T1` | Double-click Fill Handle | Auto-fills downward to match the contiguous extent of the adjacent populated column (left preferred, then right; stops cleanly before blank breaks or totals). |
| `A-051` | `T2` | Post-Fill Auto Fill Options button | Appears after fill commit offering `Copy Cells`, `Fill Series`, `Fill Formatting Only`, `Fill Without Formatting`, `Fill Days/Weekdays/Months/Years`, and `Flash Fill` `[P19-FABLE-0342]`. |
| `A-052` | `T2` | Right-button drag Fill Handle | Opens explicit context menu on release: `Copy Cells`, `Fill Series`, `Fill Formatting Only`, `Fill Without Formatting`, date-unit fills, `Linear Trend`, `Growth Trend`, and `Series…` dialog `[P19-FABLE-0343, P19-NEON-0166]`. |
| `A-053` | `T2` | Drag Fill Handle inward over selection | Shades passed-over cells in muted grey hatch during drag and clears their values/formulas on release (preserving cell formatting) `[P19-FABLE-0345, P19-NEON-0168]`. |
| `A-054` | `T2` | `Shift` + drag Fill Handle | Outward drag inserts blank cells shifting downstream rows/columns; inward drag deletes cells shifting upstream `[P19-FABLE-0346]`. |
| `A-055` | `T1` | `Ctrl+D` / `Ctrl+R` / `Ctrl+E` (Flash Fill) | `Ctrl+D` fills down; `Ctrl+R` fills right; `Ctrl+E` synthesizes Flash Fill examples and routes large/ambiguous writes through the Preview Seam (`D-39`). |
| `A-056` | `T2` | `Alt,H,F,I` Fill Menu | Exposes `Down`, `Right`, `Up`, `Left`, `Across Worksheets`, `Series…` (Linear/Growth/Date/AutoFill with step & stop value), `Justify`, and `Flash Fill` `[P19-FABLE-0348]`. |
| `A-057` | `T1` | Selection border drag (`Move` / `+Ctrl` Copy / `+Shift` Insert) | Plain border drag cuts-and-moves (`C06`, updating dependent refs like Excel `Cut`); `Ctrl`+drag copies (`+` cursor badge); `Shift`+drag shows an I-beam between rows/columns to insert-and-shift without overwriting. |
| `A-058` | `T2` | `Alt` + drag selection border onto Sheet Tab | Hovering over another worksheet tab for `400 ms` while holding `Alt` switches to that sheet so the dragged range can be dropped directly across sheets `[P19-FABLE-0354, P19-NEON-0173]`. |
| `A-059` | `T2` | Right-button drag selection border | Releases into Excel's 10-choice drop menu: `Move Here`, `Copy Here`, `Copy Here as Values Only`, `Copy Here as Formats Only`, `Link Here`, `Create Hyperlink Here`, and the four `Shift Down/Right and Move/Copy` actions `[P19-FABLE-0355]`. |
| `A-060` | `T2` | `"Enable fill handle and cell drag-and-drop"` setting | Workbook/host preference disabling fill handle and border drag-and-drop is honoured cleanly when toggled off `[P19-FABLE-0357]`. |
| `A-061` | `T1` | `Ctrl+C` / `Ctrl+X` / `Ctrl+V` / `Enter` | Starts marching-ants source mode: `Enter` pastes once and clears ants; `Ctrl+V` pastes and keeps ants (`Ctrl+C`) for repeated pastes; `Ctrl+X` moves on paste rewriting references across the workbook (`D-29`). |
| `A-062` | `T1` | `Ctrl+Alt+V` & `Alt,H,V,*` Paste Special | Full Paste Special dialog and single-key accelerators (`V` Values, `F` Formulas, `T` Formats, `W` Column Widths, `E` Transpose, `S` Skip Blanks, `D`/`S`/`M`/`I` Add/Subtract/Multiply/Divide, Paste Link). |
| `A-063` | `T2` | Multi-area & `Alt+;` Visible-Only Copy | Multi-area `Ctrl+C` succeeds whenever selected areas align on shared rows or shared columns (and returns Excel's exact refusal message otherwise) `[P19-FABLE-0363]`; `Alt+;` followed by `Ctrl+C` copies strictly visible cells, collapsing hidden rows/columns on paste `[P19-FABLE-0366]`. |
| `A-064` | `T2` | Format Painter (`Single-click` vs `Double-click`) | Single click applies copied formatting once; double-clicking locks Format Painter across multiple targets until `Esc` or button click `[P19-FABLE-0369]`. |
| `A-065` | `T2` | Object Clipboard & `Ctrl+Drag` Duplicate | Charts, shapes, and form controls copy/paste as live objects; `Ctrl+Drag` on an object duplicates it in place `[P19-FABLE-0370]`. |

---

## A.6 Structure, Formatting, Outlines, Objects, and Sheet Views (`Part 10`)

| ID | Tier | Gesture / Chord | Exact Classic Behaviour |
|---|---|---|---|
| `A-066` | `T1` | `Ctrl++` (`Ctrl+Shift+=`) / `Ctrl+-` | Inserts or deletes rows/columns/shifted cells (immediately when whole row/column selected; Shift prompt on cell range). |
| `A-067` | `T1` | `Ctrl+9` / `Ctrl+0` / `Ctrl+Shift+(` / `Ctrl+Shift+)` | Hides rows (`Ctrl+9`), hides columns (`Ctrl+0` where not intercepted by browser zoom reset; `Alt,H,O,U,C` always works), unhides rows (`(`), unhides columns (`)`), plus header drag/double-click autofit across multi-selected columns/rows. |
| `A-068` | `T2` | `Alt+Shift+Right` / `Alt+Shift+Left` / `Ctrl+8` | Groups (`Right`) or ungroups (`Left`) selected rows/columns into Excel outline levels; `Ctrl+8` toggles outline gutter visibility; gutter `1`/`2`/`3` level buttons and `+`/`-` expand/collapse by click `[P19-FABLE-0374]`. |
| `A-069` | `T1` | `Ctrl+1` & Number/Font Chords | `Ctrl+1` opens Format Cells (`Number`, `Alignment`, `Font`, `Border`, `Fill`, `Protection`); `Ctrl+B`/`I`/`U`/`5` toggle Bold/Italic/Underline/Strikethrough; `Ctrl+Shift+~` General, `$` Currency, `%` Percent, `^` Scientific, `#` Date, `@` Time, `!` Number (`#,##0.00`). |
| `A-070` | `T1` | `Ctrl+T` / `Ctrl+Shift+L` / `Alt+Down` | `Ctrl+T` (`Ctrl+L`) creates a `ListObject` Table; `Ctrl+Shift+L` toggles AutoFilter dropdowns; `Alt+Down` on a header opens Sort/Filter criteria. |
| `A-071` | `T1` | `Ctrl+F` / `Ctrl+H` | Opens **Bridge-docked Find / Replace bar** (never covering grid cells; `Replace All` stages through the Preview Seam, `D-40`). |
| `A-072` | `T1` | `Shift+F2` / `Ctrl+Alt+M` | Edits/inserts legacy Note (`Shift+F2`) or threaded Comment (`Ctrl+Alt+M`). |
| `A-073` | `T2` | Object Manipulation & `Ctrl+6` | Drag moves, 8 handles resize (`Shift` locks aspect ratio, `Alt` snaps to cell grid edges), arrows nudge `1 px`, `Delete` removes, `Ctrl+6` cycles object visibility, right-click opens `Assign Macro…` and `Format Control…` `[P19-FABLE-0379]`. |
| `A-074` | `T2` | Chart Element Selection & `Alt+F1` / `F11` | First click selects chart frame, second click selects sub-element (series, axis, legend, title; arrows cycle elements, `Delete` removes removable elements); `Alt+F1` embeds default chart on sheet; `F11` creates chart sheet (desktop; `Alt,N,C` in browser) `[P19-FABLE-0381]`. |
| `A-075` | `T2` | Sheet Views (`Alt,W,L` · `Alt,W,P` · `Alt,W,I`) | Switches sheet view between **Normal** (`L`), **Page Layout** (`P`), and **Page Break Preview** (`I`, where dashed automatic and solid manual page-break lines can be dragged) `[P19-FABLE-0382]`. |

---

## A.7 Exact Excel Key-Tip Letter Trees (`Part 11`, `D-11`)

Pressing and releasing bare `Alt` in Classic (`C01`) activates modal Deck key-tip badges matching **Windows Excel 365 (`en-US`)** letter-for-letter (`D-11`):

- **Top-level tab letters:** `F` File · `H` Home · `N` Insert · `P` Page Layout · `M` Formulas · `A` Data · `R` Review · `W` View · `L` Developer (`1`..`9` Quick Access slots).
- **High-frequency verified sequences:** `Alt,H,V,V` (Paste Values), `Alt,H,V,F` (Paste Formulas), `Alt,H,V,T` (Paste Transpose), `Alt,H,O,I` (AutoFit Column Width), `Alt,H,O,A` (AutoFit Row Height), `Alt,H,B,*` (Borders), `Alt,H,L` (Conditional Formatting), `Alt,H,F,I` (Fill), `Alt,A,V,V` (Data Validation), `Alt,A,S,S` (Sort), `Alt,A,T` (Filter), `Alt,A,W,G` (Goal Seek), `Alt,A,W,S` (Scenario Manager), `Alt,M,N` (Name Manager), `Alt,M,P` / `D` / `A,A` (Trace Precedents / Dependents / Remove Arrows), `Alt,W,F,F` (Freeze Panes), `Alt,W,V,G` (Gridlines), `Alt,R,P,S` (Protect Sheet).

---

## A.8 Browser Host Shortcut Differences (`N-01`–`N-06`) and Deliberate Strand Differences (`SD-01`–`SD-10`)

### A.8.1 Browser-Captured Chords (`wasm32` Browser Host Only; Desktop `WebView2` Captures Natively)

| ID | Excel Chord | Browser Interception | DNA Calc Browser Alternate & Surface |
|---|---|---|---|
| `N-01` | `Ctrl+W` / `Ctrl+F4` | Closes browser tab | `Alt,F,C` or Command Palette (`Alt+Q -> Close workbook`); `beforeunload` guards unsaved changes. |
| `N-02` | `Ctrl+N` | Opens new browser window | `Alt,F,N` or `Alt+Q -> New workbook`. |
| `N-03` | `Ctrl+PgUp` / `Ctrl+PgDn` | Switches browser tabs in Chrome/Firefox | `Alt+[` / `Alt+]` (or `Alt+Shift+PgUp` / `PgDn`) + Sheet Tab bar click. |
| `N-04` | `Ctrl+0` | Resets browser page zoom | `Alt,H,O,U,C` or column header context menu -> `Hide`. |
| `N-05` | `F11` | Toggles browser fullscreen | `Alt+F1` (embedded chart) or `Alt,N,C` -> `Move to Chart Sheet`. |
| `N-06` | Async Clipboard read (`Ctrl+V` external) | First-use browser permission prompt | Falls back seamlessly to native `paste` DOM event payload (`text/html` + `text/plain`). |

### A.8.2 Deliberate Strand & Safety Improvements Over Excel (`SD-01`–`SD-10`)

1. **`SD-01` — Cool 5-step Reference Palette + Amber Caret (`D-36`):** Replaces Excel's warm red/orange reference borders with 5 cool hues + distinct border dash styles + corner numbers `①..⑤`, reserving red strictly for `#Error` and amber strictly for the active caret/focus.
2. **`SD-02` — Single-step `F9` Sub-Expression Un-Bake (`D-37`):** Pressing `Ctrl+Z` immediately after `F9` inside `Edit` restores the sub-expression without dropping the entire edit session.
3. **`SD-03` — Non-Destructive `#SPILL!` Diagnostic Card (`D-38`):** Outlines the intended dashed spill footprint, highlights blocking cells (`Summary!A21`), and offers previewed `Select blocker` / `Move blocker` repairs—never auto-clearing user data.
4. **`SD-04` — Preview Seam on Bulk/Inferred Writes (`D-24`, `D-39`, `D-40`):** `Flash Fill`, `Replace All`, and multi-cell overwrites stage exact cell diffs in the Seam before mutating the model.
5. **`SD-05` — Non-Occluding Bridge Find/Replace (`D-40`):** Docks under the Bridge rather than floating a modal box over the cells being searched.
6. **`SD-06` — `< 40%` Semantic Structural Zoom (`D-41`):** Below `40%` zoom, swaps unreadable 3 px text for clean structural block regions and heat tints while keeping coordinates selectable.
7. **`SD-07` — Explicit Provenance Badges (`D-30`, `D-31`):** `◊ As saved`, hatched `Stale`, and external/add-in tags never masquerade as freshly calculated numbers.
8. **`SD-08` — View-Switching Chords (`D-12`):** `Ctrl+Alt+1..4` (`Alt+Shift+1..5` when `AltGr` collides, `Alt+Q` for App/Forms) leave `Ctrl+1` (`Format Cells`), `Alt+1..9` (QAT), and `Ctrl+Alt+5` (floating objects) 100% intact.
9. **`SD-09` — Pinned Active View Across Sheet Tabs (`D-86`):** Clicking a sheet tab never unexpectedly swaps Classic out from under the user.
10. **`SD-10` — Author-Owned Worksheet Paper vs Strand Chrome (`D-08`):** Chrome uses Recursive/Strand tokens; worksheet cells honour author fonts, borders, fills, and explicit colors even in `cockpit-dark` (`U04`).
