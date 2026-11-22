# Classic Grid User Guide

The **Grid** skin (`Alt+1`) is DNA Calc's worksheet view: rows and columns up to Excel's `XFD1048576` bounds, an in-cell overlay editor, a shared Formula Bar and Name Box, sheet tabs, and worksheet tool panels below the grid.

All behaviour described here is verified from the application code (`apps/dna-calc/`, `apps/dna-calc-model/`, `apps/dna-calc-worker/`, `apps/dna-formula-ui/`, `crates/calc-host/`) and browser smoke cases (`apps/dna-calc/tests/smoke/cases.rs`).

> **Platform note:** On macOS, `Cmd` (`Meta`) is accepted wherever `Ctrl` is listed below.

---

## 1. Shell and global shortcuts

| Shortcut | Action | Notes |
|---|---|---|
| `F6` / `Shift+F6` | Cycle focus across shell regions | Cycles between masthead (`New`), Formula Bar, active skin workspace, and the selected sheet tab. Moving to the Formula Bar carries any active in-cell edit with it. |
| `Alt+1` … `Alt+6` | Switch skin | `1` Grid, `2` Notebook, `3` Atlas, `4` Tree, `5` Canvas, `6` Ledger. |
| `Ctrl+Z` | Undo | Workbook Undo when the grid has focus; local text Undo while typing inside an editor. |
| `Ctrl+Y` or `Ctrl+Shift+Z` | Redo | Workbook Redo when the grid has focus; local text Redo while typing inside an editor. |
| `F9` | Calculate / Evaluate selection | In select mode, recalculates the workbook. Inside an open editor with a formula sub-expression selected, replaces the selection with its calculated literal (`Escape` restores the pre-`F9` formula). |
| `Ctrl+1` | Open **Format Cells…** | Unavailable while a cell edit is open. |
| `Ctrl+F3` | Open **Name Manager…** | Unavailable while a cell edit is open. |
| `Ctrl+P` | Open **Print preview** | Unavailable while a cell edit is open. |
| `Ctrl+F` / `Ctrl+H` / `Ctrl+G` or `F5` | Open **Find** / **Replace** / **Go To** | Unavailable while a cell edit is open. |
| `Ctrl+E` | Run **Flash Fill** | Unavailable while a cell edit is open. |
| `Ctrl+Alt+V` | Open **Clipboard & fill** (Paste Special) | Expands the panel and focuses the Paste Special selector. |

---

## 2. Formula entry, Point mode, and `F4`

### Starting, transferring, and ending an edit

- **Edit in place (`F2` or double-click a cell):** Opens the in-cell editor seeded with the cell's authored input, caret at the end.
- **Type-to-replace (any printable key without `Ctrl`/`Alt`):** Opens the in-cell editor seeded with the typed character.
- **Formula Bar (`[data-onecalc=formula]`):** Clicking or typing in the Formula Bar starts or transfers the edit session there.
- **Workbook VBA completion:** After you enable the workbook's macros, its functions appear in formula completion. The open draft refreshes in place.
- **Switch editor (`Ctrl+F2` while editing):** Moves an active in-cell edit to the Formula Bar, or moves a Formula Bar edit back to the in-cell editor without losing the draft.
- **Commit and walk (`Enter` / `Shift+Enter`, `Tab` / `Shift+Tab`):** Commits the edit and advances in the active selection (`Enter` down, `Shift+Enter` up, `Tab` right, `Shift+Tab` left). When the function completion popup is open, `Tab` accepts the selected proposal instead of committing the cell.
- **Fill selection (`Ctrl+Enter`):** Enters the edited value or formula into every cell in the current selection as a single undoable workbook edit, relocating relative references per cell and keeping the selection intact.
- **Cancel (`Escape`):** Discards the draft and restores the original selection and worksheet. If `F9` just evaluated a selected formula part, the first `Escape` restores the formula text instead.

### Point mode (inserting cell and range references)

- **When Point mode starts:** Starting a formula with `=` in a blank cell (from either the cell editor or the Formula Bar) arms Point mode (`Edit · F2 to point to references` vs `Point · F2 to edit text` shown in the edit bar). Pressing `F2` while editing toggles Point mode on or off.
- **Pointing by pointer:** Clicking a cell while the caret expects a reference (after `=`, `+`, `-`, `*`, `/`, `^`, `&`, `(`, `,`, `;`, `<`, `<=`, `>`, `>=`, `<>`, `:`) or while Point mode is on at a reference inserts or replaces that reference while keeping the edit target and selection on the original cell. Dragging across cells or `Shift+click` extends the pointed reference into a range (`A1:A2`); clicking a cell on another sheet adds a sheet-qualified reference (`'Q2 Data'!C4`).
- **Pointing by keyboard:** While Point mode is active, `ArrowUp`/`ArrowDown`/`ArrowLeft`/`ArrowRight` move the pointed cell reference and `Shift+Arrow` extends it into a range.
- **Returning to text navigation:** Pressing `Home`, `End`, `Ctrl+Arrow`, or an arrow key when Point mode is off returns the editor to ordinary text-caret movement.

### Cycling reference anchors (`F4`)

- Pressing `F4` while editing cycles the A1 cell reference, whole-row/column reference (`A:B`, `1:3`), or selected A1 endpoints at the caret through all four anchor states:
  `A1` → `$A$1` → `A$1` → `$A1` → `A1`
- For a range just picked or selected (such as `=SUM(A1:A2`), `F4` cycles both ends together:
  `A1:A2` → `$A$1:$A$2` → `A$1:A$2` → `$A1:$A2` → `A1:A2`
- Sheet qualifiers and text inside string literals or function names (`LOG10`) are left untouched.

### Differences from Excel today

- **Point-mode edge keys:** `Ctrl+Arrow`, `Home`, and `End` during Point mode exit Point mode for text-caret movement rather than jumping to sheet data edges while pointing (`dc-971p`).
- **Inline syntax diagnostics:** Neither the cell editor nor the Formula Bar renders inline diagnostic error spans (`dc-971p`).
- **Native IME and caret placement:** Native IME composition, and double-click caret placement within proportional text, have not yet been compared with Excel (`dc-971p.62`).

---

## 3. Worksheet navigation and selection

| Shortcut / Gesture | Action |
|---|---|
| `ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight` | Move active cell one visible cell (skipping hidden rows/columns; stepping out of merged cells from their footprint edge). |
| `Shift+Arrow` | Extend or shrink selection from the anchor corner (merge-aware around merged footprints). |
| `Enter` / `Shift+Enter` | Move down / up; inside a multi-cell or multi-area selection, walk column-by-column through visible cells and wrap across areas. |
| `Tab` / `Shift+Tab` | Move right / left; inside a multi-cell or multi-area selection, walk row-by-row through visible cells and wrap across areas. |
| `Home` / `Shift+Home` | Move / extend to column `A` of the current row (or first visible column). |
| `Ctrl+Home` / `Ctrl+Shift+Home` | Move / extend to `A1` (or first visible cell). |
| `Ctrl+End` / `Ctrl+Shift+End` | Move / extend to the last used cell on the worksheet (including formatted cells). |
| `Ctrl+Arrow` / `Ctrl+Shift+Arrow` | Jump / extend to the next occupied data-region edge or worksheet edge (`1048576` / `XFD`). Cells with `=""` formula results count as occupied. |
| `PageUp` / `PageDown` (`Shift+…`) | Move / extend vertically by one viewport height. |
| `Alt+PageUp` / `Alt+PageDown` | Move horizontally by one viewport width (`dc-971p.67`). |
| `Ctrl+Space` / `Shift+Space` | Select whole column(s) / whole row(s) of the current selection. |
| `Ctrl+A` | Toggle between the current contiguous occupied region and the full worksheet. |
| `Delete` | Clear contents of the selected cell(s) (one undoable edit). |
| Mouse click / drag | Select a cell or rectangular range (auto-scrolls when dragging past the viewport edge). |
| `Shift+click` / `Ctrl+click` | Extend range to clicked cell / toggle cell in a disjoint selection. |
| Row / column header click (`Shift+click`) | Select whole row(s) or column(s). |
| Top-left corner click | Select the entire worksheet (`A1:XFD1048576`). |

### Differences from Excel today

- **Interior active-cell extension:** Extending a multi-cell selection with `Shift+Arrow` when the active cell is strictly in the interior of the selection is refused (`"Range extension from an interior active cell awaits Excel measurement"`) (`dc-971p`).
- **Excel Table multi-stage selection:** `Ctrl+A`, `Ctrl+Space`, and `Shift+Space` inside an Excel Table refuse (`"Table selection stages are not modelled"`) (`dc-971p`).
- **Disjoint axis expansion:** `Ctrl+Space` and `Shift+Space` refuse when multiple disjoint areas are selected (`dc-971p`).
- **Horizontal extend-by-page:** `Alt+Shift+PageUp` / `Alt+Shift+PageDown` is not bound (`dc-971p`).
- **Imported range selection parity:** Stored range selections from `.xlsx` paint in the grid, while Excel pixel-comparison checks remain open (`dc-971p.43.2`).
- **Used-range edges:** Excel's retained used-range edges for `Ctrl+Arrow` and `Ctrl+End` remain open against Excel (`dc-971p`).

---

## 4. Clipboard, range drag, and Paste Special

- **Copy / Cut / Paste (`Ctrl+C`, `Ctrl+X`, `Ctrl+V`):** Captures a workbook snapshot (up to `1,000,000` cells) and writes/reads both plain TSV (`text/plain`) and an inert HTML table (`text/html`, up to `8,000,000` bytes) on the system clipboard. A dashed outline (`copy-outline`) marks the copied or cut range; `Escape` clears it.
- **Range drag (border drag):** Dragging the border of a single-rectangle selection moves the range and updates dependent references across the workbook in one undo step; holding `Ctrl` while dragging copies instead of moving. `Escape` cancels the drag and preserves any existing copied snapshot.
- **Paste Special (`Ctrl+Alt+V` or Clipboard & fill panel):**
  - **Paste mode:** `Contents and cell formats` (`all`), `Values` (`values`), `Formulas` (`formulas`), or `Cell formats` (`formats`).
  - **Transpose rows and columns:** Swaps rows and columns when pasting.
  - **Paste copied cells:** Pastes the retained internal workbook snapshot even if system clipboard permission is unavailable.
  - **Paste text… / Copy text…:** Modal TSV fallback dialog when browser clipboard permissions are blocked.
- **Overwrite review:** Pasting, moving, or filling over already-occupied cells opens a confirmation dialog (**Review paste, move or fill**) showing how many existing inputs will be replaced before committing.

### Differences from Excel today

- **External HTML limits:** Pasting external HTML strips all styles/scripts/links to plain table text; `Cell formats` paste from external data is refused, and HTML tables with `rowspan`/`colspan != 1` are refused (`dc-971p`).
- **Paste Special on Cut:** `Values`, `Formulas`, and `Cell formats` modes are refused on a `Cut` (`dc-971p`).
- **Transpose limits:** Transposing a `Cut`, table cells, a fill, or array formulas (unless `Values` or `Cell formats`) is refused (`dc-971p`).
- **Protected sheets:** Pasting into a protected sheet is refused atomically (`dc-971p`).
- **Moved sheet-scoped names:** A sheet-scoped defined name used without its sheet qualifier in a formula moved to another sheet is not automatically sheet-qualified (`dc-971p`).
- **Windows clipboard parity:** Clipboard verification against Windows Excel remains open (`dc-971p.62`).

---

## 5. Fill handle, `Ctrl+D` / `Ctrl+R`, and Flash Fill

- **Fill Down / Fill Right (`Ctrl+D` / `Ctrl+R`, or buttons):** Copies the first row downward or first column rightward across a multi-row / multi-column selection.
- **Fill handle drag:** Dragging the square handle at the bottom-right corner of a single-rectangle selection extends it vertically or horizontally. Using the **Fill mode** selector in **Clipboard & fill**:
  - **Series from seeds / formulas:** Extends evenly spaced numeric seeds, daily dates, numbered text, or relocates formulas.
  - **Copy cells:** Repeats the selected seed block verbatim (`dc-971p.68`).
  - `Escape` cancels an in-progress fill drag.
- **Fill through (keyboard fill):** In **Clipboard & fill**, enter an end cell address in **Fill through** (e.g. `A10`) and press `Enter` or click **Preview fill**.
- **Flash Fill (`Ctrl+E` or button):** Select a single output column containing at least two text examples and blank cells below, with source text immediately to its left (`column - 1`). Infers first/last word, delimiter splits (`@`, `,`, `-`, `_`, `.`), ASCII case, and fixed prefix/suffix, and previews up to 8 sample values before applying.

### Differences from Excel today

- **Fill handle preview & double-click:** Dragging the fill handle always opens the preview confirmation dialog (even over blank cells), and double-clicking the fill handle to auto-fill down an adjacent column is not wired (`dc-971p`).
- **Series limits:** Series fill requires a single date, numbered text, or evenly spaced numeric seeds in each row/column; custom lists and post-fill smart-tag options are absent (`dc-971p`).
- **Flash Fill limits:** Requires at least 2 examples (not 1), single adjacent left-column text sources only, up to `10,000` rows / `1 MB` source text / `5,000,000` pattern steps; no multi-column synthesis, learned number/date transforms, or locale case conversion (`dc-971p`).

---

## 6. Status-bar aggregates (Selection summary)

- The **Selection** disclosure bar (`[data-dnacalc=selection-summary]`) below the grid shows its enabled aggregates in the heading when more than one cell is selected. By default these are **Average**, **Count** and **Sum**. Its **Show in status bar** list turns on any of six aggregates: **Average**, **Count**, **Numerical count**, **Minimum**, **Maximum** and **Sum**. It expands to display:
  - `Selected cells`, `Count of current data cells`, `Numeric count`, `Sum`, `Average`, `Minimum`, `Maximum`
  - Provenance breakdown (`literals`, `calculated`, `blanks`, `empty text values`, `errors`) and explicit exclusions (`file caches`, `stale`, `refused`).

### Differences from Excel today

- **10,000-cell bound:** Selections larger than `10,000` cells (such as whole-column selections) refuse summary computation (`"Selection summary is limited to 10000 cells"`) (`dc-971p`).
- **Provenance exclusions:** `FileCached`, `Stale`, and refused cells never enter current totals (`dc-971p`).
- **UI presentation:** The aggregates sit in an expandable Selection panel, and the **Show in status bar** list chooses the heading items. Excel's right-click status-bar menu is not reproduced (`dc-971p`).
- **Excel comparison:** Excel's display of each aggregate for mixed, empty and single-cell selections has not yet been compared (`dc-971p.62`).

---

## 7. Context menu

- **Open:** Right-click a cell, row/column header, or sheet corner, or press `Shift+F10` in the grid.
- **Selection rule:** Right-clicking inside an existing selection keeps the selection; right-clicking outside selects the clicked cell (or header band). `Shift+Right-click` opens the browser's own context menu instead.
- **Commands:** `Copy`, `Cut`, `Paste table text…`, `Clear contents`, `Insert cells…`, `Delete cells…`, `Find…`, `Go to…`, and `Rules & review`. When a worksheet is protected or uneditable, write items are disabled and the reason is shown below the menu.
- **Keyboard:** `ArrowUp`/`ArrowDown`/`Home`/`End` move focus among enabled items; `Escape` or `Tab` closes the menu and returns focus to the grid.

### Differences from Excel today

- Contains the 9 commands above only; Paste Special choices, Format Cells, Hide/Unhide, Freeze, Filter/Sort, Notes/Comments, and Merge commands are not in the context menu (`dc-971p`, `dc-971p.73`).

---

## 8. Sheet tabs and worksheet lifecycle

- **Switch sheets:** Click a sheet tab, press `Ctrl+PageUp` / `Ctrl+PageDown` in the grid, or use `ArrowLeft`/`ArrowRight`/`Home`/`End` when focus is on the tab strip (`F6` reaches it). Each sheet retains its own active selection, scroll position, and zoom across switches.
- **Rename by double-click:** Double-clicking a sheet tab opens the **Sheets** disclosure (`data-dnacalc="sheet-tools"`) and focuses/selects the **Current sheet name** field (`dc-971p.45`).
- **Tab menu:** Right-clicking a sheet tab opens **Worksheet tab commands**: **Rename**, **Hide**, **Unhide…** and **Delete…**.
- **Tab reorder:** Drag a sheet tab to move it. Undo, Redo and XLSX reopening keep the new order.
- **Sheets panel:** Go to sheet selector, Add worksheet (automatic `SheetN` naming checks hidden sheets case-insensitively), Rename worksheet, Move worksheet (1-based position), Hide worksheet, Delete worksheet (confirmation dialog, undoable), and **Unhide…** for hidden sheets (`VeryHidden` sheets are listed as disabled).

### Differences from Excel today

- **No multi-sheet grouping:** `Ctrl`- or `Shift`-clicking sheet tabs to select a group of sheets is not supported (`dc-971p.74`).
- **No inline tab input:** Double-click opens the **Sheets** panel input rather than editing inline on the tab button (`dc-971p`).

---

## 9. Find, Replace, Go To, and Go To Special

- **Find (`Ctrl+F`):** Searches `Worksheet`, `Original selection`, or `Workbook, including hidden sheets`, looking in `Formulas & entries` or `Displayed values`, with `Match case`, `Whole cell`, and `Wildcards (* ? ~)` toggles. `Enter` runs Find then steps to the next match; `Shift+Enter` steps to the previous match (wrapping and skipping hidden-sheet hits, which stay visible but disabled).
- **Replace (`Ctrl+H`):** Replaces one selected match or all matches in one undoable workbook command. Requires `Formulas & entries` with `Wildcards` off and a complete (non-partial) search; any validation failure aborts the whole Replace command.
- **Go To (`Ctrl+G` or `F5`, or Name Box):** Navigates to an address, range, or defined name (including names scoped to hidden sheets).
- **Go To Special (in the Go To dialog, `dc-971p.68`):** Selects `Formulas` (authored formulas, legacy arrays, data tables), `Constants` (non-formula occupied cells), or `Blanks` (leaving `=""` formula results and spilled child cells unselected). Operates on the current multi-cell selection if larger than one cell, or the whole sheet (`used_area` for `Blanks`) from a single active cell.

### Differences from Excel today

- **Search bounds:** Stops at `100,000` cells inspected, `5,000,000` characters, or `1,000` hits (`partial` results disable Replace until the scope is narrowed) (`dc-971p`).
- **Replace modes:** No format replacement or wildcard replacement (`dc-971p`).
- **Go To Special scope:** Only `Formulas`, `Constants`, and `Blanks` are offered (no Number/Text/Logical/Error sub-filters, Comments, Conditional formats, Data validation, Current region, or Visible cells only) (`dc-971p.74`).
- **Excel and WebView2 selection edge cases:** Remain open for comparison (`dc-971p.62`).

---

## 10. Freeze and split panes

- **View & panes** panel (`data-dnacalc="pane-tools"`):
  - **Freeze:** `Freeze above & left of active cell`, `Freeze first row`, `Freeze first column`, `Unfreeze` (undoable and saved to XLSX).
  - **Split:** Numeric `Split across (pt)` and `Split down (pt)` (`0` to `10,000` pt), `Split`, and `Remove split`. Renders up to 4 independent scrollable pane viewports (`data-pane="0..3"`), synchronizing row/column scroll across adjacent panes.
  - Imported frozen and split panes (including `top_left` / `pane_top_left` origins) are rendered and restored.

### Differences from Excel today

- **Freeze preconditions:** `Freeze above & left of active cell` requires the active pane to be scrolled to `(0, 0)`, and an imported offset-origin freeze must be unfrozen before choosing new frozen bands (`dc-971p.16`).
- **Numeric split entry:** Splits are set in points rather than by dragging split boxes on scrollbar corners (`dc-971p`).
- **Unmeasured Excel pane cases:** Scrolled-origin freeze, boundary deletion around frozen panes, and spill borders across frozen panes remain open for Windows/Excel measurement (`dc-971p.62`, `dc-971p.16`).

---

## 11. Row/column resize, AutoFit, hide/unhide, insert/delete, and outline

- **Header drag-resize:** Dragging a visible trailing row or column header boundary resizes that band (or every band in a single whole-row/whole-column selection) in Excel character units (`1..=255 ch`, or `0` to hide) or points (`<= 409 pt`, or `0` to hide). `Escape` cancels an active drag.
- **Header double-click AutoFit:** Double-clicking a visible trailing header boundary fits column width or row height to selected displayed contents (or the whole column/row from the header).
- **Rows & columns** panel (`data-dnacalc="axis-tools"`): Set explicit width (`ch`) or height (`pt`), `AutoFit width` / `AutoFit height`, `Hide` / `Show` rows or columns (`Show` restores the retained nonzero size).
- **Insert / Delete (`Ctrl++` / `Ctrl+-`, or buttons):** Inserts or deletes whole rows/columns immediately when whole rows/columns are selected; otherwise opens the **Insert cells / Delete cells** dialog (`Shift cells down/up`, `Shift cells right/left`, `Entire rows`, `Entire columns`).
- **Outline:** Group/Ungroup selection, Hide/Show detail, Summary after detail toggle, Show levels `1..=8`, and gutter `+`/`−` controls.

### Differences from Excel today

- **AutoFit limits:** Column AutoFit refuses widths under `1 ch` (except `0`), refuses wrapped text (unless stacked `rotation = 255`), refuses unavailable fonts, refuses `shrinkToFit`/`fill`/`justify`/`distributed` alignment and combined wrap+rotation, and refuses merged content (`dc-971p.16`, `dc-ysl.9`).
- **Wrapped rows on load & headers:** Wrapped rows without a stored height in `.xlsx` are not auto-fitted on load (`dc-971p.43.1`), and row/column header sizes are fixed (`64 px` × `22 px` at 100%) rather than sized to visible row digits and the Normal font (`dc-971p.43.3`).
- **Unhide by dragging:** Hidden bands cannot be unhidden by dragging between header boundaries; use **Show columns** / **Show rows** (`dc-971p`).
---

## 12. Number formats, alignment, fonts and borders

- **Number formats:** Percentages, dates, custom format sections and imported formats display as their format asks. **Format Cells…** (`Ctrl+1`) previews, authors and round-trips them through XLSX. `General` narrows to fit the available width.
- **Alignment:** Stored horizontal and vertical alignment, indentation, wrap, justify and overflow display as stored. Alignment applied in **Format Cells…** persists to XLSX.
- **Fonts, fills and borders:** Ordinary font, fill and border rendering works, including merged perimeters, patterns and gradients.

### Differences from Excel today

- **Pixel-level number display:** Pixel-level number display, and the green number-stored-as-text indicator, remain open (`dc-971p.43`, `dc-971p.43.4`).
- **Wrapped row metrics:** Wrapped row metrics and baseline comparison remain open (`dc-971p.43`). Wrapped rows without a stored height are not auto-fitted on load (`dc-971p.43.1`).
- **Excel pixel comparisons:** Pixel comparisons of fonts, fills and borders at extreme zoom remain open (`dc-971p.43`).

---

## 13. Zoom

- **Bounds:** `10%` to `400%`.
- **Controls:** `Ctrl+Wheel` over the grid (`dc-971p.69`), `-` / `+` buttons (`1.25×` step), direct percentage input (`zoom-percent`), and `Reset` (`100%`).
- Scales cell text, headers, borders, charts, sparklines, shapes, and cell dimensions; remembered per sheet and saved in XLSX.

### Differences from Excel today

- **Wheel step and anchoring:** `Ctrl+Wheel` uses the `1.25×` worksheet zoom step rather than Excel's fixed percentage notch steps and scroll anchoring (`dc-971p.62`).
- **Extreme-zoom pixel parity:** Text weight at `50%`, 1-px baseline offset at `200%`, narrow-number `###` width thresholds, and the green number-stored-as-text indicator remain open (`dc-971p.43.4`).

---

## 14. Notes, comments, and hyperlinks

- **Markers and hover preview:** Cells with notes, threaded comments, or hyperlinks display a corner marker (`data-dnacalc="review-marker"`, up to `5,000` per sheet) with a native `title` tooltip including a bounded **240-character preview** (`dc-971p.70`). Clicking the marker selects the cell and opens **Manage Rules & review**.
- **Notes and threaded comments:** In **Manage Rules & review**, create a new Note or Comment (with editable Author), reply to a thread, resolve/reopen a thread, edit text (up to `32,767` characters), and format Notes (`Bold`, `Italic`, `Underline`, `Text colour`, `Clear formatting`). All edits are undoable and persisted in XLSX.
- **Hyperlinks:** In **Manage Rules & review**, `http://`, `https://`, and `mailto:` links render as deliberate `<a target="_blank" rel="noopener noreferrer">` links with their tooltip; internal workbook locations render as a navigation button; other schemes are disclosed as unavailable.

### Differences from Excel today

- **Clicking a hyperlink cell:** Clicking a cell with a hyperlink selects the cell rather than opening the link directly; follow links via the review marker or **Manage Rules & review** (`dc-971p`).
- **No hyperlink authoring UI:** Hyperlinks cannot be created or edited from the UI (`dc-971p`).
- **Comment/note limits:** Threaded comments with `@mentions` or unsupported metadata are read-only, a cell with an existing note/comment disables creating another root note/comment, and hover only shows the marker's `title` preview rather than a floating rich note box (`dc-971p`).

---

## 15. Dynamic array spills and `#SPILL!`

- **Spill outlines (`dc-971p.70`):** Selecting a filled spill anchor or any spilled child draws a solid outline (`data-dnacalc="spill-outline"`, `data-state="filled"`) around the spill range; selecting a `#SPILL!` anchor draws a dashed red outline (`data-state="blocked"`) over the attempted extent.
- **Read-only child cells:** Selecting a spilled child cell makes the Formula Bar read-only, displays `Edit the formula in <anchor>` in the result summary, and refuses `F2`/type-to-replace edits.
- **Anchor and blocker jumps:** The result disclosure (`data-dnacalc="result-details"`) opens automatically for refused cells and errors. It provides **Open array anchor** (from a child cell) and **Open blocker `<address>`** buttons (`data-dnacalc="array-blocker"`) that jump straight to the blocker.

### Differences from Excel today

- **Formula Bar on spilled children:** The Formula Bar stays empty/read-only rather than displaying Excel's ghosted anchor formula (`dc-971p.62`).
- **Frozen panes and blocker selection:** Spill borders across frozen panes and live Excel blocker comparison remain open (`dc-971p.62`).

---

## 16. Merged cells

- Imported merged cells render as one spanned cell (clipped into pane fragments across frozen boundaries).
- Clicking, jumping via Name Box, or navigating to any covered address selects the merge anchor (`merged.first()`), highlights the whole merge footprint, and routes edits/Undo to the anchor (`dc-971p.70`).
- `Shift+Arrow` selection extension from a merged cell expands around the whole merged footprint.

### Differences from Excel today

- **No Merge / Unmerge authoring yet:** **Merge Cells**, **Merge & Center**, **Unmerge Cells** commands and XLSX merge authoring persistence are not yet implemented (`dc-971p.73`, in progress).

---

## 17. Conditional formatting rule manager

- In **Manage Rules & review** (`Rules & review` button or context menu):
  - View supported rules on the worksheet in priority order (`priority · kind`, `Applies to`, state).
  - Reorder (`Move up` / `Move down`), `Remove rule`, edit `Applies to` (`1`–`128` space-separated ranges), edit `Condition formulas` (`cellIs` and `expression` rules, one per line), toggle `Stop if true` (non-visual rules), and add new rules (`cellIs`, `expression`, `top10`, `bottom10`, `aboveAverage`, `belowAverage`, `duplicateValues`, `uniqueValues`, `dataBar`, `colorScale`, `iconSet` Three arrows) with an RGB fill colour. Undo/Redo work inside the dialog.

### Differences from Excel today

- **Solid fill authoring only:** New conditional rules author only a solid background fill colour (no font, border, number-format, custom icon-set threshold, or custom colour-scale stop authoring) (`dc-971p`).
- **Extension-backed rules:** Imported Excel 2010+ extension rules are preserved on save, but cannot be edited or removed (`dc-971p`).
- **In-session row/column edits:** Inserting or deleting rows/columns updates rule `Applies to` ranges immediately, while rule formula text is rewritten on XLSX save rather than in memory (`dc-971p`).

---

## 18. Data validation

- **Input prompt:** Displays beside/above the active cell (`data-dnacalc="validation-prompt"`), connected via `aria-describedby` and keyboard-scrollable (`ArrowUp`/`ArrowDown`/`PageUp`/`PageDown`/`Home`/`End`/`Space`/`Shift+Space` or mouse wheel).
- **Drop-down list:** `Alt+Down` or clicking `▼` opens the listbox (up to `32,767` items); `ArrowUp`/`ArrowDown`/`Home`/`End` move selection; `Enter` or click enters the value; `Escape` or `Tab` closes without entering.
- **Entry alerts (`Stop`, `Warning`, `Information`):**
  - **Stop:** Rejects invalid single-cell or `Ctrl+Enter` range edits (up to `10,000` cells) with `Retry` (keeps and focuses draft) and `Cancel entry`.
  - **Warning / Information:** Pauses before committing and presents `Keep entry` (undoable commit), `Edit entry` (Warning only), and `Cancel entry` / `Escape`.
- **Validation audit:** **Circle Invalid Data** / **Clear circles** in **Manage Rules & review** checks up to `10,000` cells on the active sheet, draws red circles (`data-dnacalc="validation-invalid"`) that update automatically as cells are edited/undone (view-only, not saved to XLSX), and lists up to 50 clickable invalid addresses.

### Differences from Excel today

- **No rule authoring UI:** Existing rules can be viewed in **Manage Rules & review** and are preserved/relocated on edits and save, but cannot be created or edited in the UI (`dc-971p`).
- **Audit bounds:** Scans at most `10,000` cells per sheet, lists up to `50` jump buttons, and reports custom/formula rules on covered merged cells as `Unavailable` (`dc-971p`).

---

## 19. Summary of differences from Excel and owning beads

| Area | Difference from Excel today | Owning bead |
|---|---|---|
| Formula entry & pointing | Point-mode `Ctrl+Arrow`/`Home`/`End` revert to text caret; no inline diagnostic squiggles | `dc-971p` |
| Edit gestures | Native IME and double-click caret placement not yet compared with Excel | `dc-971p.62` |
| Selection & navigation | Interior-anchor `Shift+Arrow` refuses; Table multi-stage `Ctrl+A`/`Space` refuses; disjoint axis expansion refuses; `Alt+Shift+PageUp`/`Down` unmapped; used-range edges remain open | `dc-971p` |
| Imported range selection | Stored `.xlsx` range selection pixel parity unmeasured on Windows | `dc-971p.43.2` |
| Clipboard & Paste Special | No external format paste or merged HTML paste; no Paste Special on Cut; transpose refuses Cut/Table/Fill/Array; protected paste refuses; Windows Excel clipboard verification open | `dc-971p`, `dc-971p.62` |
| Fill & Flash Fill | Fill handle drags always preview. Double-click fill and custom lists remain open. Flash Fill needs at least two examples, one source column to the left and at most 10,000 rows. | `dc-971p` |
| Status aggregates | 10,000-cell limit; excludes cached/stale/refused values; aggregates in a collapsible Selection panel, not Excel's right-click status-bar menu; per-aggregate Excel comparison open | `dc-971p`, `dc-971p.62` |
| Context menu | Fixed 9-item menu; missing Paste Special, Format Cells, Hide/Unhide, Freeze, Filter/Sort, Notes, Merge | `dc-971p`, `dc-971p.73` |
| Sheet tabs | Multi-sheet grouping (`Ctrl`/`Shift`-click) and inline tab rename input missing; tab drag-reorder and the tab menu are available | `dc-971p.74`, `dc-971p` |
| Find / Replace / Go To | Find capped at 100k cells / 5M chars / 1k hits; Replace literal-only without wildcards; Go To Special limited to Formulas/Constants/Blanks; Excel and WebView2 selection edge cases open | `dc-971p`, `dc-971p.74`, `dc-971p.62` |
| Freeze & split panes | Freeze-at-active requires `(0,0)` scroll; offset freeze must unfreeze first; numeric split input only; scrolled-origin & boundary deletion unmeasured | `dc-971p.16`, `dc-971p.62`, `dc-971p` |
| Resize, AutoFit & headers | Wrapped-column/unavailable-font/merged AutoFit refusals; no bundled metric fonts; wrapped rows without stored height not auto-fitted on load; fixed `64×22` headers; no drag-unhide | `dc-971p.16`, `dc-ysl.9`, `dc-971p.43.1`, `dc-971p.43.3`, `dc-971p` |
| Zoom & pixel parity | `Ctrl+Wheel` uses `1.25×` step without Excel notch/anchor rules; 50%/200% text weight/baseline and number-as-text green corner indicator open | `dc-971p.62`, `dc-971p.43.4` |
| Number formats, alignment, fonts & borders | Ordinary display works; pixel-level number display, wrapped row metrics, baseline and extreme-zoom font/fill/border comparisons open | `dc-971p.43`, `dc-971p.43.1` |
| Notes, comments & links | Hyperlinks open from Review dialog only; no hyperlink authoring; `@mention` comments read-only; no floating hover note box | `dc-971p` |
| Spill & `#SPILL!` | No ghosted anchor formula in Formula Bar for spilled children; frozen-pane spill border & blocker comparison open | `dc-971p.62` |
| Merged cells | Merge / Merge & Center / Unmerge authoring and XLSX persistence not yet implemented | `dc-971p.73` |
| CF rule manager | Solid-fill-only new rule authoring; extension-backed rules read-only; rule formulas rewrite on save after insert/delete | `dc-971p` |
| Data validation | No DV rule authoring UI; 10,000-cell / 50-link audit bounds; merged covered custom/formula validation unavailable in audit | `dc-971p` |
