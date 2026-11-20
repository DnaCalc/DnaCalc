# DNA Calc front-end design

**Recommendation: one workbook, five views, one continuous place to work.**  
Design specification · 20 November 2026 · Main design: 22 pages; storyboards: 11 pages. Appendices are additional.  
Companion: `19-front-end-design-mockups.html`. Screen IDs are identical in both files.

<!-- page: 01 -->

## 1. Design philosophy

“Modern on the surface” means that Maria can continue working at spreadsheet speed, while Sam can follow an explanation, Lee can see a system, and Jo can operate a model without learning its construction. The modernisation is **better access to the same model**: addresses never disappear, edits have visible destinations, and every result can be followed back to its source.

Recommend five views: **Classic, Inspector, Notebook, Model map and App**. Keep the Formula Explorer a tool, available beside any view and as a separate application. A workbook has one active selection and one edit history. Changing view changes the presentation around that selection, not the selected model or the calculation rules. Classic remains one click away, including from App.

### Five principles that decide hard cases

1. **Keep the meaning; improve the reading.** A result, error, reference, spill, circular calculation or external dependency must survive presentation unchanged. Readability never justifies silently rewriting a formula.
2. **Protect practiced gestures.** Classic adopts a documented Windows Excel keyboard and mouse baseline, not Microsoft's appearance. New commands use the visible command search, `Alt+Q`; they do not steal established shortcuts. Browser conflicts are disclosed, with clickable and keyboard alternatives.
3. **Show the source at the moment of consequence.** Every editable value exposes its destination before commitment. Every summary exposes its coverage and omissions. A missing observation is “not observed”, never an invented explanation.
4. **Keep context through movement.** Selection, sheet, formula draft and navigation history survive a view change. Each view remembers its own camera and scroll position; the selected item is revealed without resetting the whole view.
5. **Spend attention deliberately.** Dense controls support hands; quiet paper supports reading. Calculation, freshness, errors and external sources have different words and symbols. Motion carries evidence and stops when the user requests reduced motion.

### Strand, refined rather than replaced

Keep petrol chrome `#193F49`, mist paper, teal structure, amber attention, green freshness, orange external provenance and red errors. Retain squared grid cells, 5 px chips, 9 px cards, two densities and the 120–160 ms settling range from **STRAND.md**. Use Recursive/Recursive Mono and Source Serif 4; package open fonts and use Noto/DejaVu fallbacks, removing the Microsoft font fallbacks in the attachment. Review artifacts use those open fallbacks.

Refine amber into an **inner accent with a dark contrasting outer focus stroke**; amber alone is insufficient on white. Distinguish hover, selection, keyboard focus and editing by outlines, brackets, caret and labels. Fresh means “calculated for this revision”, not “audited”. Text and significant marks meet WCAG 2.2 AA contrast; labels accompany colour and alternatives accompany dragging. Strand's colour rules govern application chrome and added annotations; preserve workbook formatting, including red negatives and conditional formats. [S1]

The shell always exposes the file, sheet, view selector, command search, source address, undo and a calculation/status strip. Inspector panels borrow this shell; they do not introduce another navigation language. A compact first-use hint says **“Same workbook. Choose a view for the task. Alt+Q finds any command.”** Dismiss it once; Help can restore it.

**Scope and evidence.** Workbook capabilities are accepted from the brief, not independently certified here. The five named workbooks are concrete design fixtures because no actual `.xlsx` files were supplied. Numbers, addresses and formulas are defined in Appendix D; they are not claimed to be extracted from an Enron workbook. All timings below are proposed acceptance targets, not measurements. Alternatives and disagreements are collected in §10.

<!-- page: 02 -->

## 2. The experience, walked through

### 2.1 Maria opens Inherited: establish what she is looking at

**Start.** A fresh desktop session shows `Open workbook`, recent files and `Formula Explorer`. Maria presses `Ctrl+O`, chooses `Inherited.xlsm`, then `Enter`. The shell appears before analysis finishes. It retains the file's macro-enabled format and does not execute its module or refresh its missing links merely because the file opened.

#### S01 · Inherited, opening with useful work already visible

```text
DNA Calc  Inherited.xlsm                         Search commands [Alt+Q]  Undo
File  Home  Insert  Data  Formulas  View  Tools         Sheet: Summary  [Classic v]
Address [Summary!F18]  Formula [=SUM(Revenue.Window)]                  Lens [off]
+----------------------------+------------------------------------------------+
| Workbook [1]               |       E                     F                   |
| Summary                    | 16    Revenue subtotal     2,401,300 [saved]    |
| Revenue                    | 17    Operating costs      1,720,000 [saved]    |
| Monthly                    | 18    Reported revenue   [ 2,401,300 ] [2]      |
| Archive                    |                                                |
| 34 visible / 6 hidden      | Saved results; current calculation not run.    |
| [Show hidden sheet names]  | [Inspect F18]  [Open workbook overview]         |
+----------------------------+------------------------------------------------+
| [3] Macros off: LegacyClose  | Missing link: Rates2024.xlsx  [Review sources] |
| Reading sheet 12 of 40. Current sheet usable. Indexing names... [Cancel open] |
+------------------------------------------------------------------------------+
```

**[1]** Navigation lists sheets immediately, including a separate hidden-sheet count. Merely listing hidden names does not unhide sheets. **[2]** The old result is explicitly saved, not current. **[3]** Security and unavailable sources are independent of calculation progress; neither is disguised as a formula error.

**Discover.** Maria clicks `Open workbook overview`, or `Alt+Q`, types `Workbook overview`, then `Enter`. The overview occupies the right panel rather than replacing Summary. It reports **40 sheets; 300,000 formulas; 6 hidden sheets; 1 macro module; 1 missing workbook link**, once those counts are complete. While incomplete it says, for example, `18/40 sheets inspected; 14 errors found so far`, not a final error total. `Archive: dense used region` is a navigation entry, not a screen full of illegible cells.

**Orient.** `Ctrl+G`, `Revenue!A1`, `Enter` reveals Revenue in Classic. `Alt+Q`, `Back`, `Enter` returns to Summary!F18. Hovering `Revenue.Window` previews its name definition without moving selection; `Alt+Q`, `Inspect name`, then `Revenue.Window` offers the same information by keyboard. The status strip announces completion politely once, not once per loaded cell.

**First refusal.** Clicking `Refresh missing link` opens a source panel: `Rates2024.xlsx was not found. Locate file / Keep saved values / Cancel`. `Keep saved values` keeps the missing-source label; it does not claim successful refresh. Maria cancels. She does not have to answer a modal questionnaire to inspect the workbook.

**Feel and access.** Aim for shell feedback within 100 ms of the open command, first useful saved cells within 1 s on the reference fixture, and keyboard navigation within 50 ms p95 once displayed. At 200% UI zoom the overview becomes a drawer; the grid keeps two-dimensional scrolling. Screen readers announce address, formula/value and “saved, not recalculated”. Excel familiarity is preserved; the immediate workbook inventory is the improvement.

<!-- page: 03 -->

### 2.1 Maria continues: diagnose without losing her place

**Trace.** Maria clicks `Inspect F18` or searches `View: Inspector`. The address remains `Summary!F18`; the formula, selected item and back path stay fixed. The canvas changes to a bounded dependency explanation. She selects `Calculate now` in the status strip. Local calculation may proceed while the missing link remains unresolved; the result explicitly records that limitation.

#### S02 · Inherited, an error traced to a broken defined name

```text
DNA Calc  Inherited.xlsm       Summary!F18                     [Inspector v]
Selected: Reported revenue     =SUM(Revenue.Window)             [Classic]
+----------------------------------------------------+-------------------------+
| [1] Summary!F18       #REF!                         | Selection               |
|           ^                                        | Saved: 2,401,300         |
| Revenue.Window        #REF!                        | Current: #REF!           |
| =OFFSET(Revenue!#REF!,0,0,12,1)                      | Cause: invalid reference|
|           ^                                        | [Go to name definition] |
| Broken starting reference                          | [Copy evidence]         |
|                                                    |                         |
| [2] Dependency explanation; execution not recorded. | Missing external link   |
| 2 items shown. [Expand other dependents]             | Rates2024.xlsx          |
+----------------------------------------------------+-------------------------+
| [3] Calculation finished with errors. External values not refreshed.          |
| History: Open > Summary!F18 > Revenue.Window      [Back]     Saved / current   |
+------------------------------------------------------------------------------+
```

**[1]** The real error replaces the current value; the earlier saved value remains separate evidence. **[2]** A graph explains dependence, not an invented execution sequence. **[3]** Completion does not mean error-free or externally fresh. The broken name and the missing external file are separate problems.

**Review, do not guess.** `Tab` reaches `Go to name definition`; `Enter` opens a definition editor with the exact `OFFSET` expression. Maria selects the broken reference but presses `Esc`: the draft is discarded exactly and the source name stays unchanged. The Inspector does not propose replacing missing data with zero. In the overview she selects a normally hidden sheet and chooses `Unhide in workbook`; the action says `Model change: sheet visibility`. `Ctrl+Z` restores its hidden state from either view. A protected/very-hidden item instead gives the applicable restriction and a route to workbook structure or macro controls; it does not bypass them.

**Copy and paste.** `Copy evidence` puts a readable table on the clipboard: address, formula, current error, saved value and source status. Pasting into email gives text, not a screenshot of connectors. Maria copies the reply “Check the original revenue range”, chooses `Add annotation`, and pastes it beside F18; the preview says `Presentation text, no cell change`. Undo removes it. Plain `Ctrl+C` on a model item follows §3; explicit value, formula and evidence commands remain available.

**Go wider, safely.** Maria searches `View: Model map`, then `Whole workbook`. The map shows 40 sheet groups and their counted relationships. `Expand all cells` produces: **“300,000 formula cells will not fit a readable map. Open a sheet, choose a block, or inspect one result.”** The complete workbook remains accessible in Classic. This is a refusal of a drawing request, not of the file.

**Recover.** After a simulated interruption, the opening screen offers `Recovered edits, 2 actions after last save`; Maria previews their addresses before restoring. Read-only investigation and view navigation have not silently changed formulas. Returning to Classic restores F18 and its neighbourhood, with a brief outline rather than a camera fly-through. Keyboard-only users follow the same trace as a nested dependency list; focus never requires finding a thin wire. Reduced motion preserves all status labels.

<!-- page: 04 -->

### 2.2 Maria turns Budget into a notebook: compose an explanation

**Start with the workbook.** Budget opens in Classic on Inputs. B2:B13 holds twelve Base assumptions; C2:C13 holds Stretch assumptions. Maria selects `Monthly!B8:M11` with `Ctrl+G`. She clicks the view selector and chooses `Notebook`, or uses `Alt+Q`, `View: Notebook`, `Enter`. No notebook is authored yet.

#### S03 · Budget, an empty notebook with a real starting point

```text
DNA Calc  Budget.xlsx      Monthly!B8:M11                         [Notebook v]
Notebook: Director review [New]          [Classic]       [Source] [History]
+------------------------------------------------------------------------------+
| This workbook has no notebook layout yet.                                    |
| [1] Start with selected cells: Monthly!B8:M11                                 |
|     Revenue / Variable cost / Fixed cost / Operating profit                   |
| [Insert linked table]  [Start with heading]  [Choose other cells]              |
|                                                                              |
| [2] Workbook selection is unchanged. No formulas will be copied or moved.     |
+------------------------------------------------------------------------------+
| Current revision 8. Calculation complete.                    Presentation draft|
+------------------------------------------------------------------------------+
```

**[1]** The empty state offers the selection she brought, not a gallery of generic templates. **[2]** Creating a linked view item is presentation. A separate `Create formula` command would ask for a backing cell before creating model content.

**Build.** Maria clicks `Insert linked table`, then `Add text above`, types **“Budget: base case”**, and clicks `Done` to commit and leave editing. She adds **“At 1,000 units per month and R120 per unit, annual revenue is R1,440,000.”** All three numbers are live fields inserted through `Insert value`: Inputs!B2, Inputs!B3 and Summary!B4 respectively. Surrounding prose is ordinary text. A small link mark appears on focus with the field's source. Escape would cancel an uncommitted text draft.

She selects `Inputs!A1:C5` through `Insert linked table`, choosing `Assumptions`. She then inserts `Summary!B7` as a result card and the existing Summary revenue/profit chart through `Insert workbook object`. The chart is the same object, not a second calculation or a screenshot. Moving its notebook card changes placement in this notebook only.

#### S04 · Budget, the director's reading order

```text
DNA Calc  Budget.xlsx        Summary!B7                         [Notebook v]
Director review     [Reading] [Formula lens off] [Source] [Print]    [Classic]
+------------------------------------------------------------------------------+
| Budget: base case                                                            |
| At 1,000 units per month and R120 per unit, annual revenue is R1,440,000.       |
| [1] Assumptions                     Base             Stretch                  |
|     Monthly units                   1,000             1,100                  |
|     Unit price                        120               125                  |
|     Unit cost                          70                72                  |
| [2] Annual operating profit        [ R216,000 ]   Summary!B7                  |
|     Jan 18,000 | Feb 18,000 | Mar 18,000 | Apr-Dec 162,000                    |
|     [Open all 12 months]      Chart: Revenue and operating profit             |
+------------------------------------------------------------------------------+
| Live values: revision 8. Text last edited by Maria. No calculation pending.    |
+------------------------------------------------------------------------------+
```

**[1]** Tables retain row/column headings, number formats and source ranges. **[2]** The selected result carries its address into Classic. The compact months are an explicitly labelled summary with all twelve available; they are not silently truncated data.

**Interaction.** Tab visits cards in reading order; `Enter` enters a table, arrows navigate its cells, `F2` edits the selected source cell, and `Esc` leaves editing before leaving the table. `Move item`, `Before Assumptions`, `Enter` is the keyboard and click alternative to dragging. Hover previews a source; focus exposes the same buttons. Undo removes the latest layout/text action without undoing earlier workbook formulas. The gain over Excel is a durable explanation beside live values, with no duplicated model.

<!-- page: 05 -->

### 2.2 Maria continues: change an assumption, read a formula, handle a spill

**Edit through the notebook.** Maria selects the Base unit-price cell in the linked table, presses `F2`, then `Ctrl+A` inside the editor, types `125`, then `Enter`. Before commitment the editor says `Inputs!B3: 120 -> 125`. The result card temporarily says `R216,000 · previous revision; recalculating`. It becomes **R276,000** only when that result belongs to the completed new revision. Clicking `Undo` restores 120 and R216,000; layout stays intact.

**Read.** She selects the January profit item and turns `Formula lens` on. It exposes the actual expression as a readable structure with source labels; `Edit source` always gives the exact Excel expression. Existing LET/LAMBDA bindings are expandable. Display aliases do not create Excel names, alter scope, merge repeated calls or add implicit intersection.

#### S05 · Budget, readable structure and a recoverable blocked array

```text
DNA Calc  Budget.xlsx       Monthly!B11                           [Notebook v]
Director review        Formula lens [on]          [Exact formula] [Classic]
+------------------------------------------------------------------------------+
| [1] January operating profit  18,000                                         |
|       Revenue        Monthly!B8    120,000                                   |
|     - Variable cost  Monthly!B9     70,000                                   |
|     - Fixed cost     Monthly!B10    32,000                                   |
|       Exact source: =B8-B9-B10          [Edit source]                         |
|                                                                              |
| [2] Monthly profit list    #SPILL!    Summary!A20                             |
|     =TRANSPOSE(Monthly!B11:M11)                                               |
|     Blocked by Summary!A21: "Director note"                                 |
|     [Reveal blocker]  [Move note...]  [Edit formula]                          |
+------------------------------------------------------------------------------+
| Error in revision 11. Notebook does not remove blocking content.             |
+------------------------------------------------------------------------------+
```

**[1]** This is a presentation of three existing terms, not an algebraic rewrite. `Copy formula text` yields `=B8-B9-B10`; ordinary copy follows the selected cell/text/frame context. **[2]** The twelve-row spill has a real anchor and a real obstruction; the Notebook never treats a spill child as an independently editable formula. [S2]

**Make and recover the error.** Maria had chosen `Add formula`, assigned the destination `Summary!A20`, and entered `=TRANSPOSE(Monthly!B11:M11)`. The source preview disclosed the proposed spill footprint and occupied A21. She committed the formula; the resulting `#SPILL!` is a valid workbook state. Now `Reveal blocker` opens a source peek at A21 without losing the notebook position. `Move note...`, destination `Summary!D21`, shows a one-cell move; after acceptance the array appears. `Ctrl+Z` restores the note and the error; `Ctrl+Y` reapplies the move. The move uses ordinary spreadsheet move semantics, including reference changes.

**Copy and publish.** Selecting a card by its frame then `Ctrl+C` copies a live presentation item within this workbook; pasting it in the notebook inserts a second view of the same source, visibly labelled `Linked to Summary!B7`. Selecting its value copies spreadsheet content instead. Into Word, `Copy report excerpt` supplies rich text and a table with the current revision/time footer; into Excel, `Copy cells` supplies ordinary values/formulas and formats, without notebook layout. Cross-workbook linked-item paste asks for an existing destination binding or a normal cell copy; it never silently creates an external link.

**Read across.** `Layout`, `Pipeline` arranges the same cards as Assumptions > Monthly model > Summary. At 200% zoom it returns to vertical reading order, retaining item IDs and focus. Maria's text remains annotation, not a function language. Screen readers receive heading levels and table headers. Browser-reserved shortcuts have command-search equivalents. Printing waits for calculation or offers a clearly labelled saved-result snapshot; no stale value is presented as newly calculated.

<!-- page: 06 -->

### 2.3 Sam traces the wrong Sales number

**Start.** Sam opens Sales, selects `Report!B5` (`Ctrl+G`, address, `Enter`) and sees **128,450** beside “Paid sales, West”. The visible formula is `=SUMIFS(Orders[Amount],Orders[Region],B2)`. `Alt+Q`, `Explain this result`, `Enter` opens Inspector with no earlier trace. Its first-use prompt is `Follow references / Compare with another result`; Sam chooses `Follow references`.

#### S06 · Sales, a numeric mistake rather than an Excel error

```text
DNA Calc  Sales.xlsx       Report!B5  128,450                     [Inspector v]
Paid sales, West          [Classic] [Copy evidence] [Compare]     Source [1]
+------------------------------------------------------------------------------+
| =SUMIFS(Orders[Amount],Orders[Region],B2)                                      |
| Criteria: Region = West          Status: no criterion                         |
| [2] All 25,000 rows evaluated. 126 matching orders total 128,450.              |
|     Paid       125 orders       124,250                                       |
|     Pending      1 order          4,200      [Show contributing row]          |
|                                                                              |
| Orders row 10482 -> worksheet row 10483                                       |
| SO10482 | 05-Nov-2026 | West | Lark Hardware | 4,200 | Pending                 |
| [3] Report label says "Paid". That suggests a missing condition; confirm it.  |
+------------------------------------------------------------------------------+
| Calculation complete. Diagnosis is a suggestion, not proof of business intent.|
+------------------------------------------------------------------------------+
```

**[1]** Exact source and labels are juxtaposed. **[2]** A complete query, not the visible row sample, determines contributions. **[3]** The program does not infer an authoritative business rule from prose. Sam confirms it against the Report's documented “Paid only” instruction and the PivotTable with Status=Paid, then chooses `Propose formula edit`.

**Repair.** The proposed source is `=SUMIFS(Orders[Amount],Orders[Region],B2,Orders[Status],"Paid")`. A change drawer shows `Report!B5: 128,450 -> 124,250; difference -4,200; 1 formula`. Sam presses `Tab` to `Apply`, then `Enter`. While evaluating, the old number is labelled pending. A stale PivotTable would instead say `Pivot cache not refreshed` and could not be used as current confirmation. The `XLOOKUP` customer lookup and the `FILTER` pending-order extract remain separate traceable Report items.

**Source and undo.** Clicking the order row highlights `Orders!A10483:G10483`; clicking `Classic` reveals that exact row with the table headings frozen, including the seventh column, Salesperson, omitted from this compact evidence excerpt. `Ctrl+Z` reverts the Report formula; `Ctrl+Y` restores it. Reopening a recovered audit draft checks its source revision before calling its findings current. A protected Report gives `Report!B5 is locked. Copy proposed formula / Request an editable copy`; no hidden unlock is attempted.

**Copy/paste.** `Copy evidence` includes the formula change, all-row coverage, row address and the 4,200 contribution for the audit note. A copied three-column row subset pastes to Excel as cells; pasting replacement source data back into a filtered table previews the exact affected visible/source rows, with overwrite and type warnings. `Esc` cancels without changing the selection.

**Feedback and access.** Hovering a criterion outlines its contributing field; focus on that criterion provides `Show contributing rows`. Arrows and Enter traverse a dependency list, and a table supplies the same content as the graph. A request for all 25,000 rows initially opens a paged/virtual table with an exact total, not 25,000 graph nodes. Small trace interactions target 100 ms; longer queries show progress/cancel within 100 ms. At high zoom the contribution panel stacks below the formula. Sam gains an explanation for a plausible wrong number, while retaining familiar F2, source addresses and undo.

#### S07 · Sales, the exact repair before commitment

```text
Sales.xlsx  Report!B5                 [1] Formula change: not applied
Before 128,450     After preview 124,250     Difference -4,200
=SUMIFS(Orders[Amount],Orders[Region],B2,Orders[Status],"Paid")
Changes: 1 formula. Complete query: 25,000 rows.  [Apply] [Cancel]
```

**[1]** This drawer follows S06's diagnosis. Apply commits the shown source; Cancel preserves the original SUMIFS. The arithmetic preview is separate from the actual current value.

<!-- page: 07 -->

### 2.4 Lee builds Stock on the model map

**Empty start.** Lee creates `Stock.xlsx`, then chooses `View: Model map`. The empty map says **“Add input, calculation or existing cells. New model items need sheet cells.”** He chooses `Add input block`, names the sheet `Stock`, and accepts destination `Stock!A2:B8`. The preview lists opening stock 400, daily demand 100, lead time 3 days, reorder point 260, order quantity 420, reserve target 20, and an initial reserve value of 0. It creates ordinary labels/cells and optional names, not independent actors.

**Build the timeline.** `Add calculation block`, destination `Stock!A12:H43`, opens a compact sheet editor in the map. Lee enters the thirty-day schedule in Appendix D, initially omitting `$B$5` from H15's threshold so he can complete it by wiring. Ports are named, qualified references to cells or blocks. `Stock.ReorderPoint` refers to `$B$5`; names are aliases over the sheet. Existing formula-valued names remain supported, but this flow does not move sheet calculations into hidden name formulas.

#### S08 · Stock, wiring previews an exact formula change

```text
DNA Calc  Stock.xlsx        Stock!H15                             [Model map v]
Stock / Days 0-30       [Source] [Fit selection] [Connections list]  [Classic]
+------------------------------------------------------------------------------+
| [Demand 100/day] ---> [Outflow] ---+                                          |
| [Prior closing] ---> [Opening] --+--> [Closing stock] ---> [Position]         |
| [Inflow] -----------------------+                             |              |
|     ^                                                         v              |
|     +---- delay: 3 day rows <---------------------- [Order preview: 420]     |
| Collapsed period dependencies; expand a day for every reference.             |
|                                                                              |
| [1] Reorder point 260  ---- ghost wire ----> Order threshold argument          |
| [2] Stock!H15 before: =IF(G15<$B$8,$B$6,0)                                    |
|               after: =IF(G15<$B$5+$B$8,$B$6,0)                               |
|     Changes 1 formula. Result 0 -> 420.  [Apply wire] [Cancel]                |
+------------------------------------------------------------------------------+
| Presentation positions saved. Wire is a model edit; not committed.           |
+------------------------------------------------------------------------------+
```

**[1]** A ghost line is a pending edit; an ordinary line is an existing dependency. **[2]** Wiring identifies an actual argument/expression. Here Lee explicitly chooses `Add reorder point to threshold`; dragging to an arbitrary box does not guess which operand to replace. Applying to a repeated formula run is a separate choice with a range preview; this example changes one row before Lee fills the corrected pattern down.

**Keyboard and mouse parity.** Mouse: drag the reorder output to the threshold port, choose the offered operation, inspect and apply. Keyboard or click-only: select `Reorder point`, `Connections`, `Connect to...`, choose `Order > threshold`, choose `Add`, then `Apply`. Escape cancels the ghost. Enter on a group enters it; the breadcrumb returns upward. Arrows move between nodes; Alt+Q `Move selected item` opens numeric position controls. Typing on a selected numeric input begins ordinary cell editing.

**First refusal.** Connecting to an opaque add-in call or to a whole formula with two plausible operands says **“Choose the reference to replace in the source formula.”** The map opens F2 editing with candidate references marked. Delete on a node removes its view item only, with that consequence labelled; `Delete source cells...` is a separate model command. Delete on a wire opens `Replace reference...`; it cannot silently erase operands or substitute zero.

**Copy and recovery.** Copying a node's frame duplicates its presentation when pasted here; `Duplicate source block...` instead previews new cells and Excel reference adjustment. Paste a tab-separated parameter table from a text editor into the input-block source editor to create real cells. Undo cancels the latest accepted wire change in either view; an unsaved map layout recovers as presentation without replaying calculations.

<!-- page: 08 -->

### 2.4 Lee checks Stock in Classic: distinguish delay from iteration

**Iteration is a visible model decision.** Entering `=0.5*(B8+B7)` at Stock!B8 creates the deliberate circular reserve calculation. With iteration off, the UI retains Excel's circular-reference state and offers `Inspect cycle`, `Open calculation settings` or `Undo formula`. Lee enables iteration, sets maximum iterations **100** and maximum change **0.001**, and reviews the actual affected workbook/session scope before applying. The map shows `Reserve: circular group; iteration enabled`, not an invented one-tick memory element. Convergence settings are not a physical clock. [S3]

**Check the sheet.** Selecting the day-2 order port and choosing `Classic` reveals H15, with the same source formula and selected row. The map's delay refers to an explicit schedule dependency: day-2 H15 arrives at day-5 C18. Opening `Connections list` describes that relation in words as well as visually.

#### S09 · Stock, the same model in its backing cells

```text
DNA Calc  Stock.xlsx        H15 =IF(G15<$B$5+$B$8,$B$6,0)           [Classic v]
Formulas  Data  View          [Model map] [Inspector]       Stock.ReorderPoint
+------------------------------------------------------------------------------+
|  5  Reorder point     [260]    Form-control slider: 0 ---|------- 600 [1]      |
|  8  Reserve          20.00    =0.5*(B8+B7)   [Circular; iteration enabled]     |
|     A       B        C        D        E        F         G         H        |
| 12 Day   Opening  Inflow  Outflow   Closing  In transit  Position   Order      |
| 13   0       400       0        0      400          0       400       0       |
| 14   1       400       0      100      300          0       300       0       |
| 15   2       300       0      100      200          0       200    [420]       |
| 16   3       200       0      100      100        420       520       0       |
| 17   4       100       0      100        0        420       420       0       |
| 18   5         0     420      100      320          0       320       0       |
+------------------------------------------------------------------------------+
| [2] Iteration on: max 100 / change 0.001. Latest reserve displayed rounded.   |
| [3] Delay: H15 (day 2) -> C18 (day 5). Values current for this revision.       |
+------------------------------------------------------------------------------+
```

**[1]** This is the workbook's existing form-control slider linked to B5; the map and App may present that same control. **[2]** The displayed 20.00 is formatting, not an assertion of an exact fixed point or an observed iteration count. **[3]** The delay is backed by period rows and lead-time input, not by animation.

**Change and compare.** Lee focuses the slider and presses Right six times, or enters 320 in its numeric companion. The fixture's control increment is 10; the same linked cell changes. Each arrow is an undoable input; a pointer drag is one completed gesture, with any intermediate published values explicitly shown. The order now occurs on day 1 and arrives on day 4. Selecting the map reveals the same control and the newly selected source. `Ctrl+Z` reverses the numeric 260->320 edit as one action; repeated arrow edits undo individually or as an explicitly grouped history action.

**Failure and recovery.** In the source-edit preview Lee tries `=B8+1` as a replacement at B8 in a temporary copy of the workbook state; evaluating it merely against captured inputs would not test a cycle. The preview reports `Iteration limit reached; convergence not confirmed` if that stop reason is available. Otherwise it says `Convergence status unavailable`. Cancel preserves the working model. Neither a failed solve nor a stalled external feed gets a green “verified” label.

**Feel and access.** The slider handle, value field and cell announce the same address and range. Focus/hover exposes actual formulas; the connected-list view works with screen readers. At small map scale, blocks become labelled groups with counts, never microscopic text. Panning targets 60 Hz; calculation is separately labelled. Copying schedule cells into Excel gives the same formulas and values; copying the map as an image includes its scope, iteration settings and calculation timestamp. The benefit is a systems explanation that can be checked directly in the sheet.

<!-- page: 09 -->

### 2.5 Jo runs Loan as an App, tries inputs and prints

**Arrival.** Jo opens Loan. The workbook's preferred view is `App: Loan comparison`; the shell still offers `Classic` and `Source`. While loading, the form skeleton labels its inputs and the result says `Loading saved result`. The authored form binds principal to Loan!B2, annual rate to B3 and months to B4; it does not introduce another loan calculation. If its layout is missing, the empty state offers `Open Classic` and `Restore a saved view`, without inventing an application from guessed inputs.

**Try.** Jo focuses Annual rate, presses `Ctrl+A`, pastes `12%` from an email, tabs to Months, presses `Ctrl+A` and types `0`. The field shows the workbook's validation: `Enter a whole number from 1 to 360`. Focus stays in the field; Apply is unavailable and the existing R9,147.39 result is explicitly the applied 10.5% case. `Esc` restores Months to 360 while the 12% rate remains a draft. She reviews `Loan!B3: 10.5% -> 12%` and clicks `Apply inputs`.

#### S10 · Loan, a pending input and an honest result

```text
DNA Calc  Loan.xlsx       App: Loan comparison                  [Source] [Classic]
Loan repayment                                               [Print] [History]
+------------------------------------------------------------------------------+
| [1] Principal (ZAR) [1,000,000]  Annual rate [12% *]  Months [360]             |
|     Pending: Loan!B3  10.5% -> 12%              [Apply inputs] [Discard]      |
|                                                                              |
| [2] Applied case                     Proposed input not yet applied          |
|     Monthly payment  R9,147.39        Earlier rate: 10.5%                     |
|     Total interest   R2,293,061.46    360 payments, month-end                 |
|                                                                              |
| Payment formula: -PMT(Loan!B3/12,Loan!B4,Loan!B2)     [Explain]               |
| [3] Schedule: months 1-12 of 360    [Open all]    Chart: Outstanding balance  |
+------------------------------------------------------------------------------+
| Inputs have uncommitted edits. Displayed result belongs to the applied case. |
+------------------------------------------------------------------------------+
```

**[1]** Star and text mark a draft; source addresses appear on focus or `Source`, not in every ordinary label. **[2]** A pending edit cannot relabel the earlier result as current. **[3]** The full 360-row schedule remains available; a shortened display states its scope.

**Results.** During recalculation the prior result stays labelled previous. Completion changes payment to **R10,286.13**, total interest to **R2,703,005.35**, and the chart to the same revision. `Compare last applied inputs` shows the earlier 10.5% case as a **recorded result**, never a second live scenario. `Save scenario...` previews an ordinary Excel Scenario Manager record for changing cells Loan!B2:B4; that is a workbook edit. Payments use the fixture's PMT/IPMT/PPMT schedule, with display rounding only. [S14]

**Inspect and recover.** Jo clicks the payment, then `Classic`; selection becomes Loan!B6 and the grid scrolls there. `Undo` restores 10.5% and the earlier result. Back in App the rate field and payment both reflect that undo. Draft inputs recovered after a restart are labelled unapplied and require `Apply inputs`; they are never replayed silently. `Copy result` pastes a labelled value/assumptions table into email; copying the schedule to Excel gives real cells, with `Values only` available.

**Print.** `Ctrl+P` opens a one-page proof: principal R1,000,000; rate 10.5%; months 360; payment R9,147.39; total interest R2,293,061.46; balance chart; revision and calculation status. `Include full schedule` explicitly adds pages. If calculating, `Wait for current result` is primary; `Print labelled previous result` remains possible. Jo presses `Esc` to cancel, retaining the selected payment. Tab order matches reading order, field errors are associated with labels, 200% zoom stacks controls, and the chart has a data-table alternative. The gain is a calm form over the very same cells.

#### S11 · Loan, a one-page print proof after restoring baseline

```text
Loan.xlsx  Print preview                     [Print] [Cancel]  Page 1 of 1
[1] Loan repayment | Principal R1,000,000 | Rate 10.5% | 360 months
Monthly payment R9,147.39     Total interest R2,293,061.46
Balance chart: month 1 R999,602.61 -> month 360 R0.00 (display rounded)
[2] Current completed revision. Full 360-row schedule: not included.
[Include full schedule]   [Print previous result: not selected]
```

**[1]** Print binds the applied inputs and completed result together. **[2]** The proof states its scope; adding the schedule changes the page count before printing.

<!-- page: 10 -->

### 2.6 Ade tests an add-in, then returns to Budget

**Standalone first run.** Ade launches **DNA Calc - Formula Explorer**. It has no file, sheet tabs or hidden workbook. Its empty state offers `Enter a formula`, `Browse functions`, `Load add-in` and `Open a saved test`. He clicks `Load add-in`, chooses his `DiscountFunctions.xll`, reviews its actual provider, location and requested execution host, and chooses `Load for this session`. A loading panel can cancel before activation; a failure preserves the formula draft. The fixture add-in defines `DNA.DISCOUNT(price,rate)` as price times one minus rate, returning `#NUM!` outside rate 0..1.

#### S12 · Formula Explorer, a deliberately failing add-in test

```text
DNA Calc - Formula Explorer                         [Functions] [Add-ins] [Save test]
Host: Windows desktop   DiscountFunctions.xll 1.0 [loaded]       No workbook
+------------------------------------------------------------------------------+
| [1] =DNA.DISCOUNT(120,1.2)                         [Evaluate] [Cancel]        |
| Result  #NUM!                Rate must be between 0 and 1                    |
| Arguments: price [120]  rate [1.2]    Provider: DiscountFunctions.xll         |
|                                                                              |
| [2] Tests                     Expected      Observed                         |
|     DNA.DISCOUNT(120,0.1)      108           108       Match                  |
|     DNA.DISCOUNT(120,1.2)      #NUM!         #NUM!     Match                  |
| [3] Number format [0.00]   Display: #NUM!   [Conditional formatting]          |
| Context: literal arguments only. [Open from a workbook cell]                |
+------------------------------------------------------------------------------+
| Complete. An expected error is a passing test; the error itself stays visible.|
+------------------------------------------------------------------------------+
```

**[1]** Error text comes from the actual test contract/provider, not a swallowed exception. **[2]** “Match” describes the expected test outcome, not the correctness of the business model. **[3]** Formatting exploration does not change the underlying result.

**Explore.** Ade first enters `=DNA.DISCOUNT(120,0.1)` and clicks Evaluate, seeing 108. He applies number format `"R "#,##0.00`, seeing `R 108.00`, then the Excel conditional-format rule `Cell Value < 100`, which is false. Changing rate to 0.2 produces 96 and the rule's fill/pattern. He saves a test case; Ctrl+Z reverts the test edit. A caller-dependent function says **“This function requires a workbook cell. Open Explorer from a cell.”** A native XLL unavailable on this browser host gives `Open in desktop host` or `Choose an available equivalent`, without silently substituting an implementation. [S13]

**Use it in Budget.** Ade opens Budget in DNA Calc, goes to Summary!D14 and types `=DNA.DISCOUNT(Inputs!B3,10%)`. Before execution the shell shows the already loaded provider or asks to activate it for this workbook. At the baseline price 120, the cell shows **108**. `Alt+Q`, `Explore formula`, `Enter` opens the tool alongside Classic with D14 as its caller and Inputs!B3=120 as an input. An experimental rate of 0.2 shows 96 in the tool; D14 stays 108 until `Apply to Summary!D14`. Cancel leaves the workbook unchanged; applying is one ordinary formula edit and Ctrl+Z restores it.

**Copy, focus and recovery.** `Copy formula text` pastes exact Excel syntax, with its origin disclosed, not displayed aliases; `Copy test results` pastes a table into an issue report. A saved test reopens with its provider requirement and no silent provider execution. Hovering an argument shows its type; keyboard focus opens the same help. F6 cycles editor, result, tests and metadata; chart/rule previews have textual equivalents. Format-only changes feel immediate; unknown UDF duration shows cancellable progress instead of a frozen Evaluate button. This is a focused workbench for Ade, with the same formula editor and errors as a cell.

#### S13 · Standalone Explorer, caller context refused plainly

```text
DNA Calc - Formula Explorer                       Context: no workbook
[1] =DNA.CALLERADDRESS()               Not run: requires a caller cell
This function needs workbook context. Open Explorer from a real cell.
[Open Budget in DNA Calc] [Keep this test]     Provider remains loaded.
```

**[1]** The fixture function requests caller identity. Its refusal is a tool status, not a fabricated cell result. Ade's formula and saved tests remain intact.

<!-- page: 11 -->

### 2.7 The agent proposes a Budget change: Maria controls the boundary

**Ask.** In Budget's Notebook, Maria opens `Tools > Agent` and types: **“Show what happens at a unit price of R125. Suggest a short heading for the director's note. Flag other assumptions worth reviewing.”** The empty panel states its scope: `Budget, selected view and referenced cells; propose changes`. Choosing a provider may require an explicit data-sharing decision; a workbook or skin cannot grant that permission. Once authorised, the agent uses the same visible workbook targets and commands as Maria.

**Watch.** Its progress says `Reading Inputs!A1:C13 and Monthly!B8:M11`; expanding it shows scope and the current document revision. Maria can continue reading, select a cell or press `Stop`. Stopping cancels pending work, retaining an incomplete proposal labelled incomplete. The agent does not animate a busy cursor over the grid or seize focus. A first result is `No workbook edits yet`.

#### S14 · Budget, an agent proposal divided into reviewable units

```text
DNA Calc  Budget.xlsx        Inputs!B3  120                       [Notebook v]
Director review                   [Classic]     Agent proposal P17, base rev 8
+------------------------------------------+-----------------------------------+
| Current annual profit                    | [1] Proposed changes              |
| R216,000                                 | [ ] A  Model: Inputs!B3           |
|                                          |       Unit price 120 -> 125        |
| Preview with A only                      |       Profit 216,000 -> 276,000    |
| R276,000  [preview; not applied]          | [ ] B  Presentation: heading      |
|                                          |       "Budget: price review"      |
| [2] Monthly!B8:M11: 24 values change      | [ ] C  Model: Inputs!B4           |
|     Revenue +5,000/month                 |       Unit cost 70 -> 72           |
|     Profit  +5,000/month                 |       Suggested, no source        |
| [Show exact cell differences]            | [Review selected] [Reject all]    |
+------------------------------------------+-----------------------------------+
| [3] Proposal only. Selection and workbook formulas have not changed.          |
+------------------------------------------------------------------------------+
```

**[1]** Formula/input changes and presentation changes are separately named review units. The cost suggestion is unsupported and says so. **[2]** A preview is scoped and labelled; changed-cell counts include exactly the stated range, not all downstream workbook effects. **[3]** Merely selecting a proposal never changes the file.

**Review.** Maria tabs to A's checkbox and presses `Space` to select it; the drawer shows the old/new input, exact target, dependencies and calculation status. For B it shows the heading before/after, with `No formula changes`. For C it shows `Agent suggestion, not an observed cost increase`. Maria selects A and B, leaves C unselected and presses `Review selected`. The final preview evaluates A+B together; output changes cannot be borrowed from the unselected C case. `Apply 2 changes` lists the history order, A then B.

**Choose a valid subset.** If a proposal creates a name and another item uses it, they are visibly dependent; selecting the latter also requires the former or offers an explicit alternative. Such structurally necessary edits form one undo unit, or a disclosed safe reversal order. A checkbox cannot accept half an edit. A formula-spelling or formatting-only claim is checked against the actual source difference. Bulk edits show range counts, representative changes and all exceptions; `Show all` remains available as a table.

**Access and pace.** The diff has a linear reading order and real row headers; red is reserved for errors, not ordinary removed text. `Before`/`After` labels and strike/underline distinguish changes. Hover previews the affected range; focus exposes `Reveal source`. Maria can copy the proposed formula or a diff table to Excel/email; pasting a revised instruction into the agent composer does not submit until she presses `Send`. A failed preview says why and can be retried without applying any edit. The gain is auditable assistance with the same source visibility as manual work.

<!-- page: 12 -->

### 2.7 Maria accepts part, undoes the rest, and handles a conflict

**Apply.** Maria chooses `Apply 2 changes`. A single commit validates both against revision 8 and applies them together; neither can half-apply on failure. The accepted proposal records two explicit undo units, **A: unit price** then **B: heading**, in that order. This combination provides atomic acceptance and predictable stepwise undo. C is marked `Not accepted`; she clicks `Reject C`, which changes the proposal record, not the workbook.

#### S15 · Budget, undoing the heading while keeping the model change

```text
DNA Calc  Budget.xlsx       Summary!B7  276,000                   [Notebook v]
Budget: base case                                 [Undo] [Redo] [History]
+---------------------------------------------+--------------------------------+
| Unit price 125   Annual profit R276,000      | History [1]                    |
|                                             | Maria accepted Agent P17        |
| Source Inputs!B3  120 -> 125                 | A  Set unit price to 125  kept  |
| Formula Summary!B7 =B4-B5-B6                 | B  Rename heading       undone |
|                                             | C  Unit cost to 72     rejected |
| [2] Heading restored by Undo; price retained.|                                |
| [Inspect acceptance] [Copy change record]    | Next Undo: A, Inputs!B3         |
+---------------------------------------------+--------------------------------+
| [3] Current calculation complete. Agent authorship and Maria's acceptance kept.|
+------------------------------------------------------------------------------+
```

**[1]** History records who proposed, who approved, what changed and which revision resulted. **[2]** Maria pressed `Ctrl+Z` once immediately after applying A+B. It reverted B; the latest source model remains A. **[3]** Undo is not deletion of the audit record. Redo can restore B until a new edit invalidates that redo branch.

**Keep place.** She selects Inputs!B3 in History and chooses `Classic`: the same cell is selected and its price is 125. `Undo` now names `Set unit price to 125`; pressing it restores 120 and R216,000. She redoes A and returns to the Notebook with the original heading and R276,000. View navigation, zoom and reading a history entry do not consume undo steps. Clicking an old revision offers a read-only comparison; it does not move the current workbook backwards.

**Stale proposal.** In a second run the agent proposes the price edit against B3=120, but Maria has already typed 123 in Classic. Review reports **“Inputs!B3 changed since this proposal: expected 120; current 123; proposed 125.”** Apply for A is unavailable. `Recompute using current workbook` creates a fresh preview; B can still be accepted independently if its heading has not changed. There is no last-writer-wins overwrite disguised as acceptance. The same rule applies to moved/deleted source objects and to changed external-source observations used in the preview.

**Undo after more work.** To keep later unrelated edits while reverting an older action, Maria selects it in History and chooses `Create reversal`. She sees a new before/after proposal with conflicts and dependent effects. This is not arbitrary selective undo. If a macro wrote a file or sent a message outside the workbook, history states that document undo cannot reverse the external effect; no “all restored” promise is made.

**Recovery and paste.** A restart after acceptance restores the accepted source edits and their identity once, not by asking the agent to repeat them. A proposal awaiting approval reopens uncommitted and must be revalidated. Maria copies `Copy change record` into the director's email, then pastes a two-line response into a Notebook annotation: it is plain text until she explicitly inserts live value fields. `Ctrl+Z` removes that annotation only. Screen readers announce “A accepted; B accepted then undone; C rejected”, without moving focus into the history drawer. At 200% zoom, the drawer becomes a full-width panel with a Back button returning to the original focused item.

#### S16 · Budget, a proposal made against an older input

```text
Budget.xlsx  Agent proposal P18                 [1] Conflict: Inputs!B3
Expected 120       Current 123       Proposed 125
Preview out of date. Apply unavailable for this change.
[Recompute using current workbook] [Reject change] [Reveal source]
Heading change is independent and may still be reviewed.
```

**[1]** No value is overwritten by opening this drawer. Refresh creates a new preview; rejecting the price change does not reject unrelated presentation work.

<!-- page: 13 -->

## 3. The conceptual model

### What the user points at

| Thing | Meaning and editing consequence |
|---|---|
| Cell / range | Original sheet coordinates and Excel selection rules. A multi-area selection retains its active cell. A cell edit changes workbook content. |
| Block | A view grouping of an explicit member set, perhaps discontiguous. Expanding lists members and exceptions. Grouping alone creates no formula, name or new scope. |
| Name | A real workbook- or sheet-scoped identifier referring to cells, a constant or a formula. Scope and evaluation context stay visible; dotted text is not an extra namespace. [S9] |
| Table / array | A real Table with headers/rows, or an array with an anchor and shape. Presentation sorting/filtering never silently changes source order or formulas. |
| Object | The actual workbook chart, shape, form control or other supported object. A view item may reference it; copying the view is distinct from duplicating the object. |
| View item | A card, paragraph, group, port, binding or panel arrangement. It may have one source, several sources, or none for prose. Its identity is not a cell address. |

### Selection, editing and movement

Every selection carries a **source**, **active member**, and **presentation item if any**. The address field shows these plainly: `Summary!B7`, `Orders[Amount]`, `Workbook::Stock.ReorderPoint` or `Notebook paragraph: Budget: base case`. A command accepting an Excel address still accepts Excel syntax; scope qualification in a UI label is not silently inserted into a formula.

View preference is sheet-local unless the user opens a workbook-wide map or overview; the scope control names which is active. Switching sheets restores that sheet's preferred/last-used view and source position. A file's supported/preferred-view list supplies defaults; an explicit user choice takes precedence, and Classic always remains available.

On switching view, reveal that source if represented. Otherwise show `Selected source is not in this layout` with `Reveal in Classic` and `Add to this view`; do not manufacture a different selection. A source-less paragraph retains its view selection while the last model cell remains separately identified. Formula drafts remain in an anchored editor over the destination view; if that view cannot host it, defer the switch and keep the draft intact with Commit/Cancel choices. Nothing is implicitly committed by switching.

Every non-grid editor distinguishes **Model change** from **Presentation change** before commitment. Editing a bound number changes its cell; moving a card changes layout; creating a formula asks for its destination; editing notebook prose creates annotation. An App may limit controls but respects protection, saved validation rules/alert styles and macro security, without silently changing the validation behaviour of the chosen write operation. Grouped-sheet edits disclose every affected sheet. Classic is available even when a file prefers another view.

### Copy, history and calculation state

Selection determines ordinary copy: source cells copy Excel-compatible cells with their origin for relative-reference adjustment; selected text copies text; a view-item frame copies its presentation inside this workbook. `Copy formula text` copies exact source text, with origin disclosed; pasting text elsewhere does not promise unchanged reference meaning. Paste preview names the target type and refuses ambiguous external bindings. Context commands also provide values, evidence and images. Lens formatting never silently becomes executable clipboard syntax.

There is one ordered undo history per workbook for reversible model and authored-presentation changes. Navigation and transient panel/camera changes use Back/Forward, not Undo. Multi-part agent acceptance declares its undo units before commit. Local draft undo operates inside the active editor; once committed, Ctrl+Z acts on workbook history. Recovery retains committed edits and unapplied drafts distinctly.

States are orthogonal: **loading / ready; clean / dirty / calculating / cancelled; current / saved / previous; source connected / stale / missing; error / no error**. A formula error remains its actual Excel value, not a replacement app status. Live panels show source timestamp when supplied, receipt time, last calculation revision and retained-history coverage. Pause observation does not stop a feed. Inspector offers observed execution only when supplied, and dependency explanation otherwise. These distinctions are design requirements informed by Excel's recalculation model and phyphox's separation of observations from instrument status. [S6, S7]

<!-- page: 14 -->

## 4. The views and the Formula Explorer

### Classic: the precision workspace

#### F01 · Budget, full-screen Classic during reference editing

```text
DNA Calc  Budget.xlsx  [File]     Search [Alt+Q]                      Undo  Redo
Home  Insert  Data  Formulas  View  Tools            [Classic v] [Inspector]
Address [Monthly!B8]   fx [=Inputs!$B$2*Inputs!B3|]                    [1] Edit
+------------------------------------------------------------------------------+
|      A                   B Jan         C Feb        D Mar                    |
|  8   Revenue           [120,000]       120,000      120,000                   |
|  9   Variable cost       70,000         70,000       70,000                   |
| 10   Fixed cost          32,000         32,000       32,000                   |
| 11   Operating profit    18,000         18,000       18,000                   |
| [2] Editing Inputs!B3; F4 anchors it; F2 switches to Point.                 |
+------------------------------------------------------------------------------+
| [Inputs] [Monthly*] [Summary]   [3] Pending edit; Enter commit / Esc revert   |
```

**[1]** The caret identifies editing, not a second application mode. **[2]** F4 acts on the active reference token; F2 changes between caret editing and reference picking. **[3]** Sheet navigation while picking does not move the edited cell. The draft shown is intentionally not yet the canonical final fixture formula.

Enter from any view's `Classic` button; leave via the same view selector, with source and draft rules from §3. Keep A1/R1C1, name box, formula bar, sheet tabs, freeze panes, range selection, fill/drag and familiar command groupings. Use compact text-led command bands and original icons; Page Layout, Review and Developer remain reachable through More, key tips and command search. Appendix A is the acceptance baseline, including context-sensitive F2/F4/F9. All five workbooks fit Classic; it is less effective for a long explanation or a whole dependency system. It never declines a workbook merely because a different view would read it better.

### Inspector: evidence around a selected result

#### F02 · Sales, full-screen Inspector with a bounded trace

```text
DNA Calc  Sales.xlsx   Report!B5  124,250           [Inspector v] [Classic]
[Precedents] [Dependents] [Calculation] [Live]       [Source] [Copy evidence]
+------------------------------------------+-----------------------------------+
| [1] Report!B5 SUMIFS -> 124,250           | Criteria                          |
|     Orders[Amount]                       | Region West; Status Paid          |
|     Region criterion: B2 = West        | 125 matching rows / 25,000        |
|     Status criterion: "Paid"             | [Open complete matches]           |
| [Expand next level]                      | [2] Execution trace not recorded. |
+------------------------------------------+-----------------------------------+
| Last calculation: complete. [3] Pivot: refresh state known separately.        |
| Selected Report!B5   Back: Orders!A10483:G10483             Analysis complete |
```

**[1]** Dependencies have a list and a graph presentation of the same scope. **[2]** The Calculation tab separates an observed execution trace from a proposed dependency order. **[3]** A pivot's cache state does not borrow freshness from ordinary formulas.

Enter with `Explain this result` or the view selector. Arrows select trace entries; Right expands, Left collapses, Enter reveals the source, and the Back command restores the previous trace. Ctrl+C copies the selected source; `Copy evidence` includes explanation/coverage. An empty selection says `Choose a cell, range, name or object`; a whole workbook starts with counts, not a graph explosion. Best for Sales and Inherited; useful for Stock's cycle and Ade's UDF. For Jo's everyday Loan input, App is simpler. The live instrument approach follows phyphox's distinction between readings and state, not its measurement execution semantics. [S6]

<!-- page: 15 -->

### Notebook: a live explanation in a deliberate reading order

#### F03 · Budget, full-screen Notebook after an input change

```text
DNA Calc  Budget.xlsx    Inputs!B3 125              [Notebook v] [Classic]
Director review      [Reading] [Vertical v] [Lens on] [Source] [Print]
+------------------------------------------------------------------------------+
| Budget: base case                                                            |
| At 1,000 units per month and R125 per unit, annual revenue is R1,500,000.       |
| [1] Assumptions: Unit price [125]  Unit cost 70  Fixed cost 32,000            |
| [2] January profit: Revenue 125,000 - Cost 70,000 - Fixed 32,000 = 23,000     |
| Annual operating profit: R276,000              Source Summary!B7             |
| Chart: Revenue and operating profit       [Open chart data]                  |
| [3] Add text / Linked cells / Workbook object / Formula at a chosen cell      |
+------------------------------------------------------------------------------+
| Revision 12 current. View order does not change calculation order.            |
```

**[1]** The input writes its actual cell. **[2]** The lens shows existing terms. **[3]** Content creation distinguishes annotations from model edits.

Enter from a selection; a saved notebook reopens near that source. Tab follows reading order, Enter enters a table/control, F2 edits its source, Escape cancels editing then leaves the inner item. Drag or `Move before/after` changes layout. Pipeline layout changes arrangement only. Table/array cards support labelled view filters, row windows and export; changing the actual Excel Table filter is a separately named action. Borrow Pluto's reactive reading experience while retaining Excel's manual-calculation, reference and scope behaviour. [S5] Best for Budget and Loan explanations; Sales fits as a report with a bounded table, not 25,000 prose cells. Inherited starts with selected excerpts. A blank notebook is an invitation to choose content, not evidence of an empty workbook.

### Model map: inspectable blocks and reference edits

#### F04 · Stock, full-screen map of a selected delivery

```text
DNA Calc  Stock.xlsx    Stock!C18 420                 [Model map v] [Classic]
Stock / Replenishment / Day 5      [Fit selection] [Connections] [Source]
+------------------------------------------------------------------------------+
| [1] Day 2: Order H15 420 ---- 3 days ----> Day 5: Inflow C18 420             |
|                                          |                                   |
| Day 5: Opening B18 0 -------------------> Closing E18 320                     |
| Day 5: Outflow D18 100 -----------------> Closing E18 320                     |
| [2] Reserve B8 20.00   (circular group; iteration on)                         |
|     Source: =0.5*(B8+B7)   Target B7 20     [Open calculation settings]       |
| [3] Exact C18: =IF(A18<$B$4,0,INDEX($H$13:$H$43,A18-$B$4+1))                 |
+------------------------------------------------------------------------------+
| Focus: Inflow C18. 3-day lag is a row reference; reserve loop is iteration.   |
```

**[1]** Lag labels have source and target periods. **[2]** A circular calculation has its own enclosure and no invented delay. **[3]** Dynamic INDEX resolution is inspectable; the full possible range and current selected precedent are different disclosures.

Enter from a block/name or view selector. Click/arrow selects; Enter opens a group; breadcrumb/Back exits. Wheel scrolls the canvas; explicit Zoom and Fit commands avoid stealing ordinary scroll. Ctrl+wheel uses the app's zoom where the host permits it, with a visible fallback. Connect/delete operations follow S08's source preview. Every thin port has a larger hit area and connections-list alternative. Future touch chiefly needs larger port/slider targets; pen chiefly benefits annotations. Neither changes model editing now. Best for Stock and Budget's cross-sheet stages; Sales becomes Table > Report/Pivot groups. Inherited stays at sheet/block level until a target is chosen. A 360-node Loan schedule is a recurrence block, expandable by period.

<!-- page: 16 -->

### App and the Formula Explorer: focused surfaces, shared sources

#### F05 · Loan, full-screen App with valid applied inputs

```text
DNA Calc  Loan.xlsx          [App v] [Source] [Classic] [Print] [Undo]
+------------------------------------------------------------------------------+
| Loan repayment     Principal [1,000,000] Rate [10.5%] Months [360]            |
| [1] Monthly payment R9,147.39       Total interest R2,293,061.46              |
| Outstanding balance chart [Open data]   Schedule [Open all 360 months]        |
| [Compare last applied inputs]          [Apply inputs: no pending changes]   |
+------------------------------------------------------------------------------+
| Current inputs and results, revision 4. Payment source Loan!B6.               |
```

**[1]** A result has an accessible source action; ordinary operation stays uncluttered. Tab follows authored form order; Enter activates the focused action. Input drafts commit together through `Apply inputs`; Escape cancels the active field draft, and Discard clears the complete pending set. Enter through file preference or view selector; leave through Classic/Source. Best for Jo's Loan and confirmed Budget/Stock control panels. Sales can expose approved filters and outputs. Inherited declines automatic publication until an author chooses its contract.

#### F06 · Budget, Formula Explorer docked with a real caller

```text
DNA Calc  Budget.xlsx     Summary!D14 108                        [Classic v]
+------------------------------------+-----------------------------------------+
| D14 =DNA.DISCOUNT(Inputs!B3,10%)    | Formula Explorer [1] Caller Summary!D14 |
| Inputs!B3 120                      | =DNA.DISCOUNT(Inputs!B3,20%)             |
| Source result 108                  | Trial result 96    [Preview]             |
|                                    | [Apply to Summary!D14] [Discard]         |
+------------------------------------+-----------------------------------------+
| Trial only. Budget source unchanged. Add-in DiscountFunctions.xll loaded.    |
```

**[1]** Opening from a cell captures its source revision, inputs, formatting and real caller context. Trial evaluation cannot silently run a side-effecting macro/UDF; such a provider requires an explicit supported execution action or refuses preview. A stale caller requires refresh before apply. Enter via `Explore formula`; close with its Close button or Escape after cancelling a draft. F6 cycles tool regions. Useful in every workbook for a selected formula; not a second whole-workbook editor.

#### F07 · Standalone Formula Explorer, no workbook context

```text
DNA Calc - Formula Explorer                      [Add-ins] [Open test] [Save test]
+------------------------------------------------------------------------------+
| [1] =DNA.DISCOUNT(120,0.1)                    [Evaluate]                     |
| Result 108    Number format ["R "#,##0.00]      Display R 108.00               |
| Conditional rule [Cell Value < 100]            False                          |
| Tests: price120/rate0.1 ->108; price120/rate1.2 ->#NUM!  [Run selected]       |
| [2] Caller context: none. Workbook-dependent functions need a real cell.     |
+------------------------------------------------------------------------------+
| DiscountFunctions.xll 1.0 / Windows desktop. Last test completed.              |
```

**[1]** Enter/Tab edits arguments; `Evaluate` is the explicit command for unknown or external providers. Safe built-ins may preview while typing, with a draft label. **[2]** Loading a test never invents a workbook. Copy emits exact formula/test data; save retains test cases and provider requirements, not a disguised `.xlsx`. Conditional-format rules requiring relative references request an explicit anchor in the workbook tool; standalone supports only context-free tests. The tool covers built-ins, add-ins, errors, arrays and formatting; font/colour previews never substitute for the result value.

<!-- page: 17 -->

## 5. Real spreadsheet models

The program should recognise **workbook structure before business meaning**. A Table's declared range, a chart's source, an existing form control and a formula's actual references are strong evidence. A rectangle of constants is not necessarily an input form; a repeated formula is not necessarily correct; an OFFSET name may change its target. Appendix D gives the full recognition/correction and five-view mapping for 23 recurring patterns.

Use four visible interpretation states: **From workbook**, **Suggested**, **Confirmed by Maria**, and **Needs review after change**. Do not invent confidence percentages. Confirmation records what was confirmed and by whom; it cannot convert an inference about intent into a calculation fact. A changed header, moved range, new exception or unresolved dependency can invalidate the interpretation without invalidating the underlying workbook.

| Family | Recognition and correction | Recommended first presentation |
|---|---|---|
| Database and references | Sales' Orders Table is explicit; key uniqueness and Paid-only intent require evidence. Confirm row/header ranges, keys and lookup modes. | Classic for editing; Inspector for contributions; Notebook/App for bounded tables; one map node per table. |
| Assumptions and scenarios | Budget's labelled Base/Stretch columns suggest scenarios. Confirm cells, units and comparison outputs; preserve validation/protection. | Notebook for explanation, App for operation, Classic for bulk edits; map inputs remain cell-bound. |
| Repeated calculation and time | Monthly has twelve repeated columns; Loan has 360 period rows. Show seeds, relative patterns and exceptions. | Classic keeps geometry; Inspector compares formulas; map uses a run/block; Notebook/App show labelled excerpts and full data access. |
| Systems and feedback | Stock's arrivals refer to earlier order rows; reserve is a genuine circular calculation. Confirm units and intended loop. | Model map distinguishes lag, recurrence and iteration; Inspector exposes each exact source. |
| Execution and external state | Macros, RTD, external links, random functions and data tables are actual mechanisms, but do not prove their business purpose. | Inspector shows observed state; other views display relevant source/permission/freshness notices. |
| Reporting and controls | Existing print areas, charts, pivot objects and linked controls provide candidates. Author confirms reading order and input/output contract. | Notebook for a report; App for a constrained interaction; Classic remains the complete source. |

**Correction flow.** Maria opens Budget's proposed `Monthly time series`, clicks `Why suggested`, sees dates plus repeated relative formulas, and chooses `Confirm periods B:M; headings row 1`. If row 11 includes one hard-coded override, it is listed as an exception, not removed. `Split block`, `Change heading row`, `Include cells` and `This is not a time series` change presentation only. An agent uses the same proposal flow and never converts a range to an Excel Table merely to make a cleaner view.

**Unknown is useful.** Inherited may contain unclassified islands and merged forms. They appear as labelled source ranges with a Classic preview. Discovery can propose a report or control panel, but no unconfirmed assumption is promoted to Jo's App. A hidden sheet is included in the inventory and respects its visibility/protection state; inference does not make it public.

**Sampling is an explicit tool.** Inspection can say `Sample: 200 of 25,000 rows, selected evenly by row position`; that is not the basis for an exact total or for claiming every formula is sound. A full contribution calculation and a virtualised display are distinct. Every condensed formula run exposes its member count, bounds and exception count. The Enron corpus motivates irregular workbooks: its report describes 16,189 unique files, 15,770 analyzable files and 20,277,835 formula cells. It is not evidence that our invented Inherited fixture was inspected, or that legacy corpus coverage tests modern features. [S15]

<!-- page: 18 -->

## 6. Big and busy workbooks

For Inherited, “everything is available” must not mean “everything is drawn at once”. Its 40 sheets, approximately 300,000 formulas, hidden sheets, dense region, missing file and macros are separate sources of work. Opening, navigating, analysing and calculating have separate progress and cancellation.

| View | Everything / simplification / honest refusal |
|---|---|
| Classic | Every cell/object remains addressable; draw the viewport. Preserve hidden/filter/freeze meanings. An unloaded region says Loading, not blank. Dense formatting alone does not imply dense data. A failure identifies the unavailable region and keeps the original file intact. |
| Inspector | Begin at the selected result, one dependency frontier. Group repeated formulas and cycles; show known/unknown edges and exact member/exception counts. Above 200 visible nodes or 400 edges, offer block/list form. Refuse “draw every dependency” with scope-preserving alternatives. |
| Notebook | Render the saved narrative, or an empty selected-content page. Virtualise large tables; show window/total counts. Refuse “make a paragraph for every cell”; offer blocks, a selected range or a report. Unplaced workbook content remains in the inventory. |
| Model map | Start with sheet groups, then blocks. Auto-layout at most 200 expanded nodes/400 edges per viewport; budgets are defaults, not data deletion. Beyond this show boundary ports and counted collapsed groups. Expansion opens a focused submap with breadcrumb. |
| App | Render the authored input/output contract. A missing layout says `No App is defined. Open Classic / Choose inputs and outputs`. A broken binding shows Source missing; it never silently substitutes a nearby cell. Unknown calculation/runtime dependencies remain visible. |

**Proposed reference environment:** Windows 11, four modern CPU cores, 16 GB RAM, local SSD, 1440×900 display; current supported browser on that machine. Test a fixed, checksummed Inherited fixture with saved values. Start the local-open clock after bytes are available; measure network transfer separately. Also run a constrained 8 GB machine and 200%/400% zoom, reporting its own results rather than claiming the same baseline.

| Experience budget | Acceptance target on reference environment |
|---|---|
| Open acknowledgement / shell first paint | ≤100 ms / ≤300 ms p95 after Open is accepted. |
| First useful viewport | ≤1 s p95, saved values labelled; do not wait for full dependency analysis. |
| Interactive sheet inventory / coarse block overview | ≤2 s / ≤5 s p95; incomplete counts and scope visible while processing. |
| Keystroke to visible caret/selection | ≤50 ms p95, ≤100 ms p99 during background load/recalc. |
| Local view switch with known source | ≤150 ms p95 for continuity feedback; bounded content ≤500 ms or an explicit loading state. |
| Cancellation | Acknowledge ≤100 ms; return control ≤500 ms where cancellable. Identify an uninterruptible provider instead of claiming it stopped. |
| Panning / visible live-value delivery | Aim 60 Hz; apply a received calculated revision within 100 ms p95 without forcing every intermediate event to render. |

Use ten cold opens and at least 1,000 input actions with recorded p50/p95/p99, hardware, file bytes, cache state and provider state. There is no universal completion deadline for unknown VBA/UDF/network work. Crossing a budget is a performance failure to fix, not permission to omit data. Save/export progress remains cancellable before final replacement; failure must not corrupt the last good file. Screen readers receive total coordinates and scope even when cells are virtualised; announcements are coalesced, never a stream per recalculated cell. Full calculation and source refresh are measured separately from responsiveness.

<!-- page: 19 -->

## 7. Customisation

A skin is a **declaration over a fixed interaction contract**: theme tokens, density, view layouts, source bindings, reading/tab order, panels, command placements, keymaps and print layouts. It may declare a theme within Strand's contrast rules, but cannot recolour errors into success or suppress a stale-source notice. A keymap is named and previewable; selecting a custom map explicitly leaves the Classic-conformance profile. An inaccessible command always has a named shell route.

The immutable core is address/source identity, formula meaning, calculation state, workbook permission/security, protected commands, truthful error/status presentation, clipboard semantics and review/undo rules. A custom component may render or request supported workbook actions; it cannot supply a hidden calculation engine or inject secret input transformations. Its requirements and execution host are reviewed like an add-in. Merely opening a workbook does not authorise new component code.

**Author, try, share.** `View > Edit this layout` opens the current layout declaration as an inspectable draft with a live preview. The author can choose a role, rearrange blocks, bind source cells, change tab order and run the same keyboard/contrast checks as built-ins. `Try in this window` affects only the preview; `Use for this workbook` saves a presentation transaction; `Export skin` produces a versioned package without workbook values by default. Import lists declared commands, component requirements and unavailable bindings. Undo restores the previous declaration. A broken binding remains visibly broken until rebound; it never falls back to an approximate address.

### Worked example: the agent restyles Classic

Maria asks: **“Keep the compact grid. Use larger formula text and put the Inspector on the right.”** The agent proposes `Working density unchanged; formula text 15.5 px; right panel 320 px; cockpit-light; Classic keymap unchanged`. A side-by-side preview shows Inputs!B3 in Budget selected and a formula draft open. `Changes: presentation only; 0 cells, formulas or names` is computed from the actual diff. The draft is rejected if larger text clips the address or if a contrast pair fails. Maria accepts; F2/F4, Ctrl+Enter and the fill handle behave exactly as before. Ctrl+Z reverts the restyle as one authored-presentation edit.

### Worked example: the agent builds Loan's App from three lines

> Let Jo change the principal, annual rate and term.  
> Show the monthly payment, total interest and balance chart.  
> Print a one-page result and keep the full schedule available.

The agent first proposes **bindings**, not a new mortgage calculator: B2 principal, B3 annual rate, B4 months, B6 payment, B7 interest, existing balance chart and A12:F371 schedule. It reads the workbook's units/validation and asks for correction only if ambiguous. The preview lists three writable inputs, two output cells, one chart and one schedule; it never derives a second PMT result in the component. Maria checks a 10.5% to 12% edit, an invalid term, keyboard order, 200% zoom, a source jump and print proof. Accepting stores the App layout; creating missing Excel validation or scenario records would appear as separate model changes.

**Persistence.** Use the file's supported custom-XML presentation part, versioned and optional, with stable binding identities plus human-readable source addresses. Normal supported Excel save cycles must preserve it and be tested. External structural edits require identity validation: an A1 address still existing does not prove the same target survived. Uncertain bindings require review/rebinding. If a cleaning tool removes it, the workbook still opens in Classic, and previously exported presentation can be restored. A view preference is a preference, not a way to hide Classic or force code execution. [S10, S11]

<!-- page: 20 -->

## 8. Printing and macros

Print a fixed, labelled calculation revision. Classic uses workbook print areas, breaks, titles, scaling and chart placement. Notebook prints authored reading order; pipeline layouts can paginate vertically. Inspector prints selected evidence, source formulas, scope and omitted branches. Model map prints the selected hierarchy/region with boundary connectors and an index; it never shrinks an entire dense graph into illegibility. App prints its authored report and explicitly selected schedule pages. Formula Explorer prints expression, inputs, caller context if any, provider/version, expected/observed results and formatting rules.

Print preview states **Current / Previous / Saved results**, timestamp, filters and any missing sources. An error stays an error. Export to Excel preserves actual model content and supported presentation metadata; PDF is a frozen report. Converting a macro workbook to a macro-free format requires the ordinary content-loss choice.

Macros may select cells, request an existing view and show a panel through optional, capability-checked presentation commands. Classic selection commands reveal the same source in the active view. A calculation macro must not depend on DNA-only view calls to work in Excel. Unsupported presentation calls have a documented no-op/fallback; unavailable execution is reported before a user starts the action. Do not silently change calculation mode to support a view. Macro trust applies in every view, including an attractive App button. Preserve code even when a particular host cannot run it; Excel's own web client, for example, preserves VBA without executing it. [S12]

Document changes from a macro are grouped where the host can capture them. Before a non-undoable action, name that specific limitation and offer a recoverable workbook checkpoint; external effects remain outside document undo. The history says what was observed, not what code might have done. Macro-driven focus changes are announced and recorded in navigation history; a view must not hide the destination.

## 9. Testing the experience

Run the seven storyboards as tasks, giving participants the goal and workbook, not the prescribed clicks. Then run the exact scripted keys to verify deterministic conformance. Recruit the corresponding roles, including keyboard-only and screen-reader users; test desktop and browser, normal and high contrast, reduced motion, and 100%/200%/400% UI zoom. Treat targets as prototype acceptance hypotheses.

| Script | Observable success target |
|---|---|
| Maria / Inherited | Within 3 min, finds F18's broken name, distinguishes it from the missing file, and returns to F18 without enabling macros. |
| Maria / Budget | Within 8 min, authors a live note; price edit updates every bound number; spill is recovered with no deleted content. |
| Sam / Sales | Within 4 min, identifies the 4,200 Pending contribution, confirms the rule, repairs one formula and undoes it from Classic. |
| Lee / Stock | Within 8 min, wires a precise operand, identifies H15->C18 and explains why B8 iteration is different. |
| Jo / Loan | Within 3 min, applies 12%, recovers from invalid term, restores baseline and prints a correctly labelled result. |
| Ade / Explorer | Within 5 min, tests 108 and #NUM!, sees caller-context refusal, then previews and applies from Summary!D14 in Budget. |
| Maria + agent | Within 4 min, accepts A+B, rejects C, undoes B only and correctly handles the 120/123/125 conflict. |

Require zero unnoticed model edits, silent stale-result claims, lost drafts, focus traps or hidden content loss. Record backtracking and participants' explanation of the displayed state, not just completion time. Review the 25 workbook/view screenshot cases and both Explorer forms listed in Appendix B, plus each storyboard's error, pending and recovery state. A successful static screenshot cannot establish keyboard, accessibility, performance or calculation parity; those need interaction and workbook round-trip evidence.

<!-- page: 21 -->

## 10. Where I would push back

These are explicit qualifications to the brief or attachments. They should be agreed rather than hidden inside implementation choices.

**1. A circular reference is not automatically a delay.** The later linked-boxes direction says cycles are drawn as feedback through a delay. I would draw feedback, but reserve a time-delay label for actual prior-period references or an established workbook mechanism. In Names in Motion, the JavaScript snapshots previous values and imposes an execution sequence. Watershed additionally gives finance its own settling loop while other state advances once. Those algorithms do not establish that the displayed Excel formulas have the same semantics. Use Stock's day rows for the real lag and a separate iteration enclosure for B8. Keep the sketches' hierarchy, port values, source toggle, parameter rail and click-to-inspect; omit ungrounded per-group “directors”.

**2. A dependency picture is not an execution recording.** The previews infer a firing order and compare before/after values. That reveals neither every formula evaluated nor a unique actual order. Excel can revise its calculation chain and calculate independent sections concurrently. A graph animation should therefore say `Dependency explanation`; `Observed execution` is offered only when the host reports it. Preserve partial order/concurrent bands instead of inventing a serial story. A static or simultaneous change highlight is the safe default when only changed values are known. [S7]

**3. Formula lens must not become automatic formula refactoring.** Existing names and LET/LAMBDA structure are excellent reading aids. Introducing shared bindings can change evaluation: `RAND()+RAND()` is not `LET(r,RAND(),r+r)`. Relative names, local scopes, `@`, spill `#`, coercion and error behaviour also matter. Default to faithful structure and source-linked aliases. Offer `Rewrite formula` as a separate proposed model change only when justified, with parity evidence appropriate to that change. Do not call a successful test on today's inputs proof of universal equivalence. [S8, S9, S16]

**4. “Excel ignores and keeps it” needs a tested boundary.** Custom XML is a documented persistence mechanism, but Document Inspector can remove it. Promise retention for an explicit supported Excel/save-format matrix, plus graceful loss/recovery of presentation. Do not suggest that every format conversion, privacy cleaner or third-party editor preserves custom views. This retains the settled in-file direction; it makes the promise honest. [S10, S11]

**5. One file does not imply every runtime everywhere.** The existing macro/add-in capabilities in the brief should be used. The UI still needs to distinguish file preservation from availability of a native provider, caller context, permission or execution host. Ade's standalone Explorer must not secretly create a workbook to satisfy caller-dependent functions. Jo's App must not replace unavailable UDF outputs with a different implementation. A known unavailable capability has a direct route to an appropriate host. [S12, S13]

### Other alternatives, decided

**Keep six experimental views as top-level modes:** no; ledger/database work belongs in Classic/Notebook tables, atlas in Inspector/map overview, tree in hierarchy navigation, and canvas in map/App composition. **A separate visual-programming runtime:** no; it would break the stated workbook contract. **Notebook cells as run-once scripts:** no; preserve the workbook's dependency calculation and explicit manual mode. **Automatic App publication from inference:** no; author-confirmed bindings are necessary for an operator-facing contract. **Per-view undo stacks:** no; they make a source edit's reversal depend on where it originated. **Hide Classic for managers:** no; keep it accessible while making App the preferred entry. **A full Excel ribbon reskin:** no; retain interaction structure in Strand's compact command bands, without Microsoft assets.

<!-- page: 22 -->

## 11. Risks, open questions for the owner, and a milestone order

The largest risk is a convincing interface that overstates what it knows. The second is disrupting practiced Excel editing. The third is coupling five presentations so tightly that one expensive analysis stalls all work. Source/state honesty, Classic conformance and bounded presentation must therefore be release gates, not polish after the views are built.

### Five owner decisions, each with a recommendation

| Question / risk | Recommendation |
|---|---|
| Which Excel interaction/version profile is the acceptance oracle? | Use a pinned current Microsoft 365 Windows desktop build for Classic gestures, with a separately named legacy profile only where needed. Record locale, keyboard and settings. Calculation compatibility and file targets remain separate contracts. |
| Can circular calculations be drawn without implying time delays? | Approve the explicit §10 qualification: feedback enclosure for iteration; period-labelled delay only when backed by model evidence. Keep the actual Excel mechanisms. |
| What execution evidence and host capabilities can be exposed reliably? | Make changed values, runtime reads, true calculation trace, convergence and provider availability separately discoverable. Require honest `Unavailable / Not observed` states; do not block basic views on optional diagnostics. |
| Which file/save routes retain presentation and which histories travel? | First certify ordinary `.xlsx` and macro-preserving `.xlsm` routes on named desktop/web Excel versions. Save authored layouts and accepted-change provenance; keep transient undo/recovery local unless explicitly exported. Permit a privacy choice for provenance. |
| How much independent work may an agent perform? | Default to proposal-and-review for model writes, allow explicitly delegated scopes, and always show attributed source changes. Presentation drafts can be previewed freely; accepting them cannot grant provider, macro or external data access. |

### Milestone order, with visible exit evidence

1. **One dependable editing shell.** Classic source editing, selection, clipboard, addresses, undo/recovery, calculation states and file preservation. Exit: Budget/Loan scripted keys, source snapshots and save/reopen comparisons; Inherited usable during load.
2. **One source in two presentations.** Inspector plus the shared source peek/Formula Explorer. Exit: Sam's complete diagnosis, Ade's standalone and workbook flows, and graceful unavailable-provider/caller cases. This tests the source contract before multiplying layouts.
3. **A useful explanation surface.** Notebook, bound values, arrays/tables, source-safe text and print. Exit: Maria's Budget write-up, stale/blocked-spill recovery and exact cross-view undo/copy.
4. **A faithful map.** Read-only grouping first, then explicit operand wiring, hierarchy and iteration/delay distinctions. Exit: Lee's Stock flow with the same saved workbook checked in Classic and Excel; Inherited remains bounded and truthful.
5. **An authored application.** App declarations, input transaction/validation, original controls, complete source access and print proof. Exit: Jo completes Loan with keyboard only and understands the source/result state.
6. **Repeatable customisation and assistance.** Skin preview/import, agent diffs, partial acceptance, conflict handling and provenance. Exit: the two §7 examples and S14/S15 work end to end, without bypassing the prior gates.

Throughout, run the same cast fixtures across every completed view, rather than building an isolated demonstration for each. Include screen-reader and zoom checks with each milestone. Use the Enron-derived workload as a stress family alongside modern array/table/add-in fixtures; no single corpus stands in for the whole product.

**Design completion condition:** a reviewer can identify the backing cell or object for every non-prose item, predict each edit and undo, distinguish a current result from a stored observation, and move to Classic without losing their place. The clickable prototype should demonstrate those properties before visual embellishment expands.

<!-- appendices -->

## Appendix A: Classic keyboard and mouse conformance list

These are **proposed acceptance requirements**, not a claim that a prototype has passed them. The compatibility reference is Excel for Microsoft 365 on Windows, English UI and US keyboard, with the exact observed build recorded in every conformance run. Different locales get displayed native key names and their own conformance fixture. Do not silently import a Mac, Excel for the web, Excel 2010, or OS-reserved behavior into this profile. All commands also have discoverable menu entries. No Microsoft icons, fonts, ribbon artwork or styling are implied.

**Context rule.** The focused editor, open menu or dialog receives input first; then the focused view; then the shell. A grid selection remains visible when a panel has focus, but that does not give the grid the keystroke. Every row below is checked in its stated context, and again with an unrelated panel focused to detect accidental global interception. A disabled command states why; it never performs a different operation.

**Evidence boundary.** Microsoft documents the core keyboard vocabulary [KA1], while the specialized sources below establish the important data behaviors. The acceptance cases and DNA Calc feedback requirements here are our design. Ambiguous or edition-dependent details require an observed side-by-side fixture, rather than an invented universal rule. Particularly important: current Microsoft documentation gives **Ctrl+F2 as formula-bar editing**, whereas older Excel documentation gives Print Preview; this design chooses the current behavior. Ctrl+P always provides print preview. [KA1]

### A.1 Command discovery, focus and selection

| ID | Input and context | Required observable result |
|---|---|---|
| A-01 | Alt or F10, no editor active | Show key tips on the compact command strip; no model or selection change. Esc removes tips and restores prior focus. |
| A-02 | Key-tip letters | Follow the displayed path; show the remaining valid letters. Preserve familiar group initials F/File, H/Home, N/Insert, P/Page Layout, M/Formulas, A/Data, R/Review, W/View, L/Developer. Layout stays Strand. |
| A-03 | Common key-tip sequences | Support Alt,H,V,V (paste values), Alt,H,O,I (autofit columns), Alt,H,O,R (rename sheet), Alt,A,T (toggle filter), Alt,W,F,F (freeze panes); test each against the named baseline. Sequence is not a simultaneous chord. |
| A-04 | Legacy Alt,E,S,V,Enter | Paste values through Paste Special. Legacy paths are aliases, shown in command search; unknown paths do not guess a command. |
| A-05 | F6 / Shift+F6 | Cycle view, command strip, open panels and status/zoom landmarks forward/backward; preserve the last focus inside each. Split panes are reachable. |
| A-06 | Tab / Shift+Tab in controls | Traverse controls in reading order; do not move the underlying active cell. In a composite list, arrow keys move its items. |
| A-07 | Shift+F10 or Menu key | Open the same contextual actions as right-click for the focused cell, block, object or sheet tab. Esc returns to that exact target. |
| A-08 | F1; Ctrl+F1 | Open contextual help; show/hide the command strip, respectively. Neither obscures the current edit without retaining it. |
| A-09 | Click a cell; Shift+click another | Select one cell; extend a rectangle from the selection anchor. Expose address, dimensions, active cell and error state to assistive technology. |
| A-10 | Drag through cells | Grow a rectangular selection; scroll at viewport edges; show final dimensions before release. Dragging cell interior is distinct from dragging its border. |
| A-11 | Ctrl+click/drag | Add a noncontiguous area; an already-selected area can be removed. Do not collapse existing areas. Unrepresentable downstream operations explain their limitation. [KA2] |
| A-12 | Arrow; Shift+Arrow | Move active cell; extend from the fixed anchor. A selected table or spill is still navigable by real cell addresses. |
| A-13 | Ctrl+Arrow; Ctrl+Shift+Arrow | Move/select to Excel's next data boundary. Fixtures include blank islands, formulas returning empty text, hidden rows and the sheet edge. No semantic block detector may redefine this gesture. |
| A-14 | Home; Ctrl+Home; Ctrl+End | Row start; worksheet start; last used cell, respectively. Used-range formatting can put the last used cell beyond visible data; report that honestly. |
| A-15 | End then Arrow | Enter the visible End-navigation state and move to its next boundary; a completed movement clears that state. |
| A-16 | Page Up/Down; Alt+Page Up/Down | Move one viewport vertically/horizontally, preserving approximate relative position; frozen areas remain frozen. |
| A-17 | Ctrl+Page Up/Down | Previous/next worksheet in workbook order. Skip hidden sheets as Excel does; restore each sheet's own selection and scroll position. |
| A-18 | Ctrl+Shift+Page Up/Down | Extend grouped-sheet selection. Show a persistent Grouped sheets label and the affected sheet count before edits. |
| A-19 | Ctrl+Space; Shift+Space | Select column/row in the current context. In a table, the first Ctrl+Space selects column data; the second includes the column header. Test subsequent expansion and Shift+Space against the recorded baseline. [KA3] |
| A-20 | Ctrl+A | In a table: data, then the entire table including headers. Outside a table: current region, then its summary rows, then worksheet where applicable. In an editor select its text, not the worksheet. [KA3] |
| A-21 | Ctrl+Shift+* | Select the current region. Blanks determine its boundary; a guessed model block does not. |
| A-22 | F8; Shift+F8 | Toggle Extend selection; enter Add to selection. State is explicitly named in the Strip, announced once, and canceled without mutation. |
| A-23 | Enter/Shift+Enter; Tab/Shift+Tab inside a selection | Move active position down/up or across/back within the selected area; retain the area. Single-cell Enter movement follows the user setting. |
| A-24 | Ctrl+.; Ctrl+Backspace | Cycle corners of a rectangular selection; reveal active cell without collapsing selection. |
| A-25 | F5 / Ctrl+G / address field | Go to `Monthly!B4`, a defined name, multi-area address or `Report!H4#`. Invalid, missing and protected targets get distinct explanations. [KA4] |
| A-26 | Go To Special; Alt+; | Select cells by a stated category; select visible cells only. Expose whether filtered/hidden cells are included. [KA4] |
| A-27 | Mouse wheel; scrollbar; Scroll Lock | Scroll without changing selection. Scroll Lock changes arrow behavior and has a visible/announced status, rather than looking like broken navigation. [KA5] |
| A-28 | Ctrl+wheel; zoom field | Zoom the worksheet about the focus while preserving the active cell; shell/text zoom stays separately available. At large zoom there is no lost command or trapped focus. |

### A.2 Entry, formulas and calculation

| ID | Input and context | Required observable result |
|---|---|---|
| A-29 | Printable text while a cell is selected | Start replacement entry; old content remains recoverable with Esc until commit. Leading `=` starts a formula. Apostrophe permits explicit text. |
| A-30 | F2 / double-click | Edit the active cell with the caret at the end. If in-cell editing is disabled, focus its formula-bar editor. Selection outline and caret are visibly different. |
| A-31 | F2 during formula editing | Toggle Point versus Edit behavior: arrows select references in Point state and move the caret in Edit state. The Strip names the state. [KA1] |
| A-32 | Ctrl+F2 | Focus/edit the formula bar; while editing, transfer the same draft between formula bar and in-cell editor. It must not print. [KA1] |
| A-33 | Enter; Tab; Shift variants during entry | Commit valid entry once, then navigate. A syntax error retains the draft and selection; no partial formula is silently saved. |
| A-34 | Esc during entry | Restore the exact pre-edit formula/value, selection and cell formatting. Once an entry has committed, use Undo; Esc is not historical Undo. |
| A-35 | Ctrl+Enter with a selected range | Apply the entry to the whole selection as one edit, with relative references adjusted from the active-cell origin. Example: entering `=A1*2` into C1:C3 with C1 active produces references A1, A2, A3. [KA6] |
| A-36 | Alt+Enter | Insert an in-cell line break; do not commit or create another worksheet cell. |
| A-37 | Backspace / Delete in an editor | Remove text around the caret. Delete in Select state clears contents, preserving formatting; Backspace starts replacement of the active cell. |
| A-38 | Ctrl+Arrow; Home/End with an editor focused | Move within text. Shift extends text selection; Ctrl+A selects editor text. Grid navigation must not leak through. |
| A-39 | Formula autocomplete | Offer Excel functions, names, table references and loaded add-in functions; arrows choose, Tab inserts, Esc dismisses. Completion is distinct from committing the cell. [KA7] |
| A-40 | Shift+F3; F3 | Open function argument help; insert a defined name. Empty names list explains that no names exist and offers Define name. |
| A-41 | Type `=SUM(`, then mouse-select B4:M4 | Insert `B4:M4` at the reference insertion point; numbered outlines link text to ranges without relying on rainbow colors. Type `)` and Enter to commit. [KA8] |
| A-42 | Pick a reference on another sheet or open workbook | Preserve the unfinished formula and its origin; generate the correct quoted qualifier. Returning focus does not commit accidentally. External links are marked. [KA8] |
| A-43 | Drag reference outline/corner during editing | Change the selected reference range in the draft; preserve its absolute/relative flags; update formula text and preview together. Esc cancels the entire draft. [KA8] |
| A-44 | F4 with a reference active | Cycle `A1` -> `$A$1` -> `A$1` -> `$A1` -> `A1`; selected ranges retain corresponding endpoint behavior. Structured-reference details follow observed fixtures, never a blind string rewrite. [KA6] |
| A-45 | F4 outside editing | Repeat the last repeatable command; if it cannot be repeated, label that state. Never enter formula edit merely because a formula cell is selected. [KA9] |
| A-46 | F9 in a formula editor | Replace the selected complete expression, or whole formula when appropriate, with its evaluated result in the draft. A visible `Draft contains evaluated values` notice offers Esc; Enter commits that replacement. [KA10] |
| A-47 | F9 outside editing | Recalculate dirty formulas across open workbooks in the current session; keep last completed results explicitly stale while working. Do not refresh external sources merely because F9 was pressed. |
| A-48 | Shift+F9; Ctrl+Alt+F9; Ctrl+Alt+Shift+F9 | Calculate active sheet; full calculation; dependency recheck plus full calculation. Show which scope was requested and cancellation state. [KA11] |
| A-49 | Ctrl+`; Alt+= | Toggle formulas/values display; propose AutoSum's range before commit. Neither is the richer Formula lens, and their labels must not imply that it is. |
| A-50 | Ctrl+Shift+Enter | Commit a legacy array formula over the chosen range; ordinary Enter uses dynamic-array semantics where applicable. Never auto-convert one kind into the other. [KA12] |
| A-51 | Edit/delete an interior legacy-array cell | Refuse partial mutation with the whole array address and `Select array` action. One undo reverses an allowed full-array edit. [KA12] |
| A-52 | Select/edit a spill child | Show anchor, spill extent and read-only formula projection. `Edit source` selects the anchor; do not silently redirect a Delete or paste to the entire array. [KA13] |
| A-53 | Spill is obstructed | Keep `#SPILL!` at the anchor; reveal the obstruction cells. Clearing blockers is an explicit worksheet edit with Undo, not automatic repair. [KA13] |
| A-54 | Ctrl+Shift+"; Ctrl+'; Ctrl+; / Ctrl+Shift+; | Insert the above cell's value; insert its formula text; enter current date/time. Values and formulas are distinguished before commit. Verify locale and reference behavior in the baseline. |

### A.3 Clipboard, fill and structural editing

| ID | Input and context | Required observable result |
|---|---|---|
| A-55 | Ctrl+C / Ctrl+X | Copy/cut current cell selection with explicit extent. Keep the source marked until the operation ends. Copying editor text copies characters instead. |
| A-56 | Ctrl+V; Enter with a copied range | Paste at the active destination with Excel-compatible relative-reference adjustment for copies and identity-preserving move semantics for cuts; show a clipboard-format limitation before any lossy paste. [KA14] |
| A-57 | Esc after copy/cut, before paste | Cancel pending clipboard operation and source marching outline; do not erase source values. After paste use Undo. |
| A-58 | Ctrl+Alt+V | Open Paste Special. Support All, Formulas, Values, Formats, Comments/Notes, Validation, widths, number-format combinations, Transpose, Skip blanks and arithmetic operations with explicit choices. [KA15] |
| A-59 | Ctrl+Shift+V under current profile | Paste values; show exact destination and retained number-format behavior. Older-profile or host conflicts are disclosed in shortcut settings. |
| A-60 | Paste from text/CSV/another program | Expose delimiter, locale and type interpretation when ambiguous; retain leading-zero identifiers via Paste as text. An app's HTML table must not import executable content. [KA16] |
| A-61 | Copy to another program | Supply an ordinary readable table/text representation; Excel-compatible cell formats where the destination supports them. If formulas cannot travel, say that the exported representation contains values. |
| A-62 | Copy between Classic and another DNA Calc view | Source-cell selection copies cells; a view-item frame copies presentation inside this workbook. Notebook paste offers `Link existing cells` (default) or an explicit value snapshot. Grid paste writes cells. External clipboard text never promises an internal bound target. |
| A-63 | Drag fill handle | Preview destination and fill meaning: copy/series/date pattern. Release performs one edit; subsequent Fill Options can choose the other supported interpretation. Relative, absolute and mixed references behave correctly. [KA17] |
| A-64 | Double-click fill handle | Fill down to the reference application's inferred neighboring-data extent; show the affected range in the completed operation. Fixture: formula beside Loan's continuous periods 1-360 fills to period 360. Blank gaps, unequal neighbors, existing targets and filtered rows each require observed baseline fixtures. Do not invent a single universal stopping rule. [KA18] |
| A-65 | Ctrl+D / Ctrl+R | Fill down from the selection's top row / right from its leftmost column, including relative-reference adjustment and relevant formatting, as one operation. [KA17] |
| A-66 | Drag a selected range's border | Move cells, not a view item; a ghost shows target and overwrite consequences. Formula dependencies follow the same moved cells. [KA19] |
| A-67 | Ctrl+border-drag | Copy instead of move. A plus marker and `Copy cells` label track the modifier state through pointer release. [KA19] |
| A-68 | Shift+drag; Ctrl+Shift+drag whole rows/columns | Insert moved/copied rows or columns at the shown boundary. Modifier changes update the preview immediately. Noncontiguous row drag is refused rather than rearranged approximately. [KA20] |
| A-69 | Border drag over occupied cells | Follow the recorded overwrite-warning setting for conformance; never mislabel copy as move. The preview must reveal occupied destination cells even when no blocking confirmation is required. [KA21] |
| A-70 | Ctrl++ / Ctrl+- | Insert/delete selected whole rows/columns directly; for ordinary cells ask direction of shift. Multi-area, table, protected and array conflicts explain why they cannot commit. |
| A-71 | Row/column boundary drag; double-click | Set explicit size; AutoFit to contents. Different gestures cannot accidentally move worksheet data. |
| A-72 | Ctrl+9 / Ctrl+0; corresponding Shift chords | Hide/unhide rows/columns. Hidden indicators and count remain discoverable. If Windows reserves the unhide-columns chord, offer the displayed command instead of promising interception. |
| A-73 | Right-click sheet tab; drag / Ctrl+drag tab | Rename, move, copy, hide, unhide or delete using the shown action; Ctrl+drag creates a copy. Grouped-sheet state remains prominent. |
| A-74 | Shift+F11 | Insert a worksheet at the baseline position and select it. Undo removes that insertion without affecting its prior neighbor. |
| A-75 | Sheet protection or locked target | Preserve the selection and explain which target prevents the edit. Do not partially paste a rectangle around protected cells without a separate explicit operation. |

### A.4 Tables, names, formatting, analysis and workbook actions

| ID | Input and context | Required observable result |
|---|---|---|
| A-76 | Ctrl+T / Ctrl+L in grid | Create an Excel table from a confirmed extent and header choice. Existing table selection opens relevant table actions, not a nested table. |
| A-77 | Tab from table's last data cell | Follow Excel's table extension behavior and inherited calculated-column rules; one Undo restores the prior table extent. |
| A-78 | Edit a calculated table column | Show whether the formula will fill the column; preserve structured references and reveal any exception rows. Never treat it as a display-only map change. [KA22] |
| A-79 | Ctrl+Shift+L; Alt+Down on filter header | Toggle AutoFilter; open that column's filter menu. Filtering changes visibility, not data order. Visible row count is exact. [KA23] |
| A-80 | Filter menu keyboard / sort command | Search and toggle items by keyboard, preserve focus on return, and identify active filter conditions. A sort moves the full record, or requires an explicit partial-range choice. [KA23] |
| A-81 | Ctrl+Alt+L | Reapply applicable sort/filter after data changes. Keep the current row identifiable by its source address and report if it is now hidden. [KA23] |
| A-82 | Ctrl+F3; Ctrl+Shift+F3 | Manage names; create names from selected labels. Show scope, exact definition, value/error and duplicate/invalid-name refusal. No hidden change of scope to resolve a collision. [KA24] |
| A-83 | Ctrl+F / Ctrl+H | Find/Replace with explicit Selection/Sheet/Workbook scope, formulas/values, case, whole-cell and wildcard choices. Replace All gives a count and one Undo group. [KA25] |
| A-84 | Shift+F4 / Ctrl+Shift+F4 | Next/previous result using the retained search options; an unavailable result says so. Returning to results does not clear selection context. |
| A-85 | Ctrl+1 | Format Cells for the current model selection; show number-format preview using actual selected data. Layout-only appearance settings use a different labeled command. [KA26] |
| A-86 | Ctrl+B/I/U/5 | Toggle bold, italic, underline or strikethrough on cells, or selected text when editing. Red remains reserved for error chrome; imported cell formatting is preserved as workbook data. |
| A-87 | Ctrl+Shift+~, !, $, %, ^, #, @ | Apply General, Number, Currency, Percentage, Scientific, Date, Time formats using the active locale and reference-profile precision. The value stays unchanged. [KA26] |
| A-88 | Ctrl+Shift+& / Ctrl+Shift+_ | Apply outline border / remove outline border as defined by the reference profile. Distinguish workbook borders from temporary selection and dependency outlines. |
| A-89 | Freeze Panes command | Freeze rows above and columns left of the active cell; keep selection/navigation meaningful across frozen and scrolled areas. Split creates independent scroll panes. [KA27] |
| A-90 | Insert PivotTable | Confirm source, destination and aggregation. Existing PivotTable layout uses field controls; it is not presented as an ordinary editable result range. [KA28] |
| A-91 | Alt+F5; Ctrl+Alt+F5 | Refresh selected PivotTable/connection; refresh all. Show which source is requested and last refresh time; missing credentials or files produce explicit retained-cache state. F9 remains calculation, not refresh. [KA29] |
| A-92 | Pivot drill-through / expand-collapse | Match supported Excel pivot actions, with the created detail sheet treated as a model change. Warn/refuse unavailable external details without inventing rows. |
| A-93 | Alt+F1 / F11 | Create an embedded chart / chart sheet from selected data. Empty selection and unsuitable source produce a concrete range-picking state. |
| A-94 | Select shape/chart; Tab; arrows; Ctrl+drag | Select the object separately from cells; cycle objects, nudge, or copy it. Shape text editing captures text keys; Esc returns to object selection. [KA30] |
| A-95 | Form-control slider by keyboard | Change the linked cell using the control's min, max, increment and page increment; show the new linked value and recalc state. A macro-enabled control discloses its executable action. |
| A-96 | Shift+F2 / Ctrl+Shift+F2 | Edit a note / threaded comment, respectively, where supported. Plain notes are not automatically converted into conversations. |
| A-97 | Ctrl+K | Insert/edit a hyperlink, preserving cell content; external navigation does not run a workbook macro. |
| A-98 | Ctrl+Z | Undo the last committed model or presentation transaction across all views. Uncommitted text uses editor undo first; navigation and recalc notifications are not edit-history entries. |
| A-99 | Ctrl+Y / F4 | Ctrl+Y redoes when a redo branch exists, otherwise repeats a repeatable action according to context. F4 outside editing is Repeat, not Redo; in formula editing it anchors references. Toolbar labels name the action. [KA9] |
| A-100 | Save after Undo; switch view | Save does not clear DNA Calc history; switching view neither commits an invalid edit nor changes the undo target. This cross-view contract is deliberate. |
| A-101 | Alt+F8 / Alt+F11 | Open macros / macro editor where available. Explain blocked execution and unavailable host capability; preserve the workbook module. No claim that arbitrary external side effects are undoable. [KA31] |
| A-102 | Ctrl+N/O/S; F12 | New/Open/Save/Save As with standard dirty-document recovery. Recovered and read-only copies are explicitly named; unavailable add-ins do not disappear on save. |
| A-103 | Ctrl+P | Open print preview for the current view; `Worksheet print` remains a direct choice. Include calculation freshness, scenario and scope where material. |
| A-104 | Ctrl+W / Ctrl+F4; Alt+F4 | Close workbook; close application. Offer save/recovery for dirty content; never treat a browser-tab close as a guaranteed cancelable app action. |
| A-105 | Ctrl+F6 / Ctrl+Tab, desktop | Switch open workbook windows/documents in the chosen desktop convention. Browser tab-switching remains browser-owned unless a tested profile explicitly supports otherwise. |
| A-106 | Shortcut customization | Show conflicts, affected contexts, baseline command and reset. Imported skins cannot override core recovery/escape or reserve a hidden action behind a familiar command. |
| A-107 | Alt+Down on a validation cell | Open its permitted-choice list; arrows navigate, Enter chooses, Esc keeps the old value. The same chord in the formula editor toggles completion, never a cell-validation menu. |
| A-108 | Ctrl+E | Run Flash Fill from the supplied examples when supported; disclose that outputs are literal values, not a new formula. If unavailable, state that directly. One Undo restores every affected cell. [KA32] |
| A-109 | Trace Precedents / Dependents; double-click a trace | Add one real dependency level; navigate the chosen dependency. Missing external workbooks and untraceable relationships have visible endpoints, not fabricated edges. [KA33] |
| A-110 | Ctrl+[ / Ctrl+]; Shift variants | Select direct precedents / dependents; extended variants select their transitive equivalents according to the reference profile. Cross-sheet and closed-file cases require explicit observed fixtures. |
| A-111 | Ctrl+Alt+5, then Tab | Enter floating-object navigation and cycle shapes; Esc returns to the grid selection. This supplements pointer selection rather than taking over ordinary Tab. |
| A-112 | F7; formatting toolbar by keyboard | Check spelling in the selection/sheet; traverse formatting commands without losing the cell target. All changes appear in the same workbook history. |
| A-113 | Navigation through merged cells | Treat the merged area as the visible selectable unit; expose its extent and top-left storage cell. Arrow, Tab, Shift selection, paste and unmerge match recorded fixtures. Never create independent hidden values in covered cells. |
| A-114 | Edit while sheets are grouped | Keep `Grouped sheets: Inputs, Monthly` prominent and show all affected source addresses in edit feedback. Ordinary entry retains its familiar commit; bulk preview states the cross-sheet consequence. Ungroup through a tab/context command; Undo restores the entire grouped edit. |
| A-115 | Operate versus select a form control | A normal click operates the control; right-click or the Selection panel selects it for editing without firing its action. Show control-selection handles distinct from a focused slider thumb. Move/resize and linked-value changes have separately named undo actions. |
| A-116 | Status detail / date-system setting | Expose selection aggregates, edit/calc state and the workbook's 1900/1904 date system in accessible status details. Changing date system is a previewed workbook setting edit, never an inference made by a skin or paste operation. |
| A-117 | Conditional Formatting > Manage Rules | Show current scope, rule order, formula anchor, Applies to range, format and Stop If True. Preview edits on real selected values; Apply commits one reversible rule change. Workbook rule colours survive theme changes. |
| A-118 | Invalid direct entry under data validation | Honor the saved alert style: Stop offers retry/cancel; Warning permits explicit acceptance, revision or cancel; Information permits acceptance or cancel. Retain the draft and message. Paste/fill cases follow recorded Excel validation behavior, not a newly invented universal rejection. |
| A-119 | Worksheet zoom versus UI/page zoom | Label worksheet zoom separately from shell/UI zoom. The former changes sheet geometry; the latter keeps text and commands accessible. The browser's page zoom remains usable. Test both together at 200% and 400%, with frozen panes, panels and active editing. |

| ID | Input and context | Required observable result |
|---|---|---|
| A-120 | Alt+Q | Focus command search; typing filters commands, arrows choose, Enter invokes, and Esc restores prior focus and selection. A browser-owned conflict has an explicit reachable alternative. |
| A-121 | A1 / R1C1 reference style | Change display/input style without changing selected source or formula meaning. Verify reference picking, F4, copy and fill in both styles; R1C1 text is not treated as A1 syntax. |

### A.5 Browser, accessibility and conformance gates

**Browser choice:** keep the same command names and nonconflicting grid gestures, but publish a visible `Browser shortcut differences` sheet. Keyboard capture is limited to keys actually delivered to the focused app. Do not claim that a setting can seize every OS/browser shortcut. `Commands`, `Go to`, `Recalculate`, `Open`, `Save` and `Help` remain keyboard-reachable controls when their familiar chords are unavailable. Browser tab, address-bar and page-zoom controls must retain a reliable route. Microsoft itself documents browser shortcut differences and configurable overrides; this design adopts disclosure without promising universal interception. [KA1]

For each supported browser/OS, test F1, F5, F6, F11, F12, Alt, Ctrl+O, Ctrl+W, Ctrl+L, Ctrl+Tab, Ctrl+Page Up/Down, Ctrl+Shift+V, Ctrl+0 and Ctrl+wheel. Publish the tested result as **same**, **explicit alternate**, **browser-owned** or **unavailable capability**. Do not silently let Ctrl+L sometimes mean Table and sometimes mean address bar. Start with familiar behavior when the user has expressly enabled a tested spreadsheet shortcut profile; list its exact exceptions.

For keyboard-only and screen-reader runs, test selected versus focused versus editing states; announce address, row/column labels, number format, value/formula, selection size, spill or table membership, protection and staleness. Announce calculation completion, not every cell update. At 200% and 400% zoom, keep full command access and a route to the source cell. Hover help is also exposed on focus. With high contrast or color removed, range identity uses labels/line patterns, errors use text, and the active caret is distinct from an inactive selection.

**Required fixture families:** data boundaries; adjacent blank islands; formatted empty used range; merged cells; filtered and manually hidden rows; noncontiguous selection; table totals and exception formulas; dynamic spills; legacy arrays; protected sheets; scoped names; cross-sheet/workbook reference picking; formulas returning empty text; clipboard round trips with Excel/text/HTML; drag overwrite, insert and modifier release; interrupted edits; stale results; macros and unavailable add-ins. Each records target selection, final formula/value, formatting, workbook structure and Undo outcome. Compare real behavior, not screenshots alone.

## Appendix B: Front-end requirements

These are acceptance requirements for the recommended design, not claims about the current implementation. A “source comparison” checks formulas, values, names, objects, workbook settings and relevant formatting, not only what the screen looks like. The cast fixture definitions are in Appendix D. Appendix A supplies the individual Classic interaction cases.

### B.1 Meaning, identity and calculation state

| ID | Requirement and pass condition |
|---|---|
| R-1 | Switching through all five views of Budget leaves its calculation content unchanged. Compare source content before/after; view metadata may differ. |
| R-2 | Every bound value, table, map port and App output reveals its exact source in one action; source-less prose is explicitly annotation. Test every item in S01–F07. |
| R-3 | A cross-view switch preserves selected source and active member. A source absent from the destination layout produces Reveal/Add choices, not a substitute selection. |
| R-4 | A formula draft survives a requested view switch in an anchored editor or an explicit deferred-switch state. Commit applies once; Escape restores exact pre-edit content. |
| R-5 | Named/relative/scoped references, structured references, implicit `@`, spill `#`, errors and literal strings retain meaning under Formula lens. Turning it on/off never edits the source formula. |
| R-6 | Readable aliases expose their scope/binding. An ambiguous shortened input is refused or opens the exact source editor; it cannot resolve to an arbitrary same-spelled name. |
| R-7 | Existing LET/LAMBDA structure can be expanded without changing evaluation multiplicity. The lens must not collapse the two calls in `RAND()+RAND()`. |
| R-8 | Saved, previous, pending and current results are distinguishable by words and accessible state. Edit Budget price under slow calculation and verify no mixed-revision report is labelled current. |
| R-9 | `#REF!`, `#SPILL!`, `#NUM!` and other actual formula errors remain intact. Missing provider, iteration-limit and loading statuses are not invented Excel cell error codes. |
| R-10 | The Inspector separates value changes, observed evaluation, dependency explanation and observed event sources. If only before/after values exist, no execution order or unchanged-cell evaluation is asserted. |
| R-11 | Manual/automatic/iterative calculation settings and effective host scope are visible and unchanged by view switches. A settings edit identifies all affected documents/session state. |
| R-12 | Live data displays source time if known, receipt time, status, calculation revision and retained-history scope. Pausing history observation leaves feed behaviour unchanged unless a separate real feed-control action is chosen. |

### B.2 Shared shell, Classic and editing

| ID | Requirement and pass condition |
|---|---|
| R-13 | File, commands, views, source, undo, printing, add-ins, settings and macro security remain reachable in each view. App cannot remove Classic or suppress a required status. |
| R-14 | Each sheet retains its preferred/last-used view and source position; a workbook-wide view names its scope. A user's explicit selection overrides file preference. |
| R-15 | Pass every Appendix A case for the pinned reference profile, including edit/selection/Point states, F2/F4/F9, Ctrl+Enter, fill, drag, range picking and clipboard semantics. Record any deliberate extension separately. |
| R-16 | Input goes to the focused editor/control, then view, then shell. Ctrl+Enter on a grid selection never accepts an agent proposal merely because its panel is visible. |
| R-17 | Existing workbook formats, conditional formats and colours survive rendering/save, including red negative numbers. Strand overlays remain distinguishable from cell formatting. Retain original font metadata; disclose open-font display substitutions affecting layout, and never save them back as silent format changes. |
| R-18 | Multi-area, filtered, grouped-sheet, merged-cell and protected selections keep their actual extent. A refused paste explains the exact incompatible target and leaves all cells unchanged. |
| R-19 | Ordinary cell copying retains source-origin semantics for relative references. Copy formula text copies exact syntax with its origin disclosed; copying lens text never silently substitutes that syntax. |
| R-20 | View-frame copying within a workbook duplicates presentation; source duplication is separately named. Cross-workbook paste previews bindings and any new external links before committing. |
| R-21 | One ordered workbook history covers model and authored-presentation changes. View/camera navigation uses Back/Forward. Run every storyboard's undo from a different view. |
| R-22 | Recovery distinguishes committed changes, unapplied drafts and unaccepted proposals. Reopening cannot replay an accepted action twice or automatically apply a recovered draft. |

### B.3 View-specific behaviour

| ID | Requirement and pass condition |
|---|---|
| R-23 | Inherited opens with a usable Classic viewport before full analysis. Hidden sheets, missing links and macro presence are inventoried without being enabled or exposed by guesswork. |
| R-24 | Sales' contribution explanation covers all 25,000 rows or explicitly states incompleteness. The complete fixture trace identifies 125 Paid orders totalling 124,250 and one Pending order of 4,200. |
| R-25 | A stale/unrefreshed PivotTable has its own state, independent of formula-calculation completion. A cache cannot be labelled proof of a fresh source total. |
| R-26 | Notebook reading order and pipeline arrangement have no effect on calculation order. Moving a source-less paragraph creates no cell or name. |
| R-27 | Every number in Budget's authored live sentence is bound. Changing Inputs!B3 to 125 updates the price text, revenue field and dependent result coherently. |
| R-28 | Notebook tables preserve source ownership and disclose display filters, windows and counts. A display sort does not silently reorder source rows; editing a displayed row addresses the correct source row. |
| R-29 | Creating a notebook formula previews a real backing destination and spill consequences. A blocked Summary!A20 retains `#SPILL!` and the A21 note until an explicit move/delete action. |
| R-30 | A map wire previews the exact source formula, targeted occurrence, new expression and affected range. Ambiguous aggregated edges refuse automatic rewiring and open operand selection. |
| R-31 | Removing a map node removes presentation by default, with that consequence labelled. Removing a dependency requires a valid explicit replacement/edit; it never silently substitutes zero. |
| R-32 | Stock's map distinguishes H15->C18 three-day delivery from B8's iterative circle. Clicking either reveals the exact cells and applicable settings. |
| R-33 | Map hierarchy uses workbook/group membership without inventing Excel scope. Duplicate source appearances share state; each group exposes full membership and exceptions. |
| R-34 | Loan App binds inputs to B2:B4 and outputs to B6:B7 plus the existing chart. Applying 12% produces the stated fixture outputs; invalid term 0 never writes B4. |
| R-35 | App drafts are not applied inputs; recorded comparisons are not live scenarios. Saving a reusable scenario identifies the actual Excel scenario changing cells and is undoable. |
| R-36 | App/source/protection restrictions match Classic. A locked input, unavailable macro or broken binding has a visible explanation and route forward, not a hidden substitute calculation. |

### B.4 Formula Explorer

| ID | Requirement and pass condition |
|---|---|
| R-37 | Opening from Summary!D14 in Budget records its real caller, source revision, captured inputs, exact formula and formatting. Trial results remain separate until explicit Apply. |
| R-38 | Applying an Explorer trial checks the current source revision; a stale target requires refresh/review. Cancel leaves formula, format and provider state unchanged except explicitly authorised provider actions. |
| R-39 | Standalone Explorer has no workbook or fabricated caller. Literal-argument built-ins/UDFs can be tested; a caller-dependent function is refused with a route to a real workbook cell. |
| R-40 | Add-in loading exposes the actual provider/version/host and permission state. Unavailable native providers never silently become another implementation or zero-valued results. |
| R-41 | Formula, number-format and conditional-format tests record expected and observed outputs separately. `108` and `R 108.00` share the same underlying numeric result; expected `#NUM!` can pass while retaining the error. |
| R-42 | Unknown or side-effecting providers do not execute on every editor keystroke. The UI offers explicit supported execution, a safe preview capability or a plain refusal. Cancellation cannot claim to undo external effects. |

### B.5 Agent review and provenance

| ID | Requirement and pass condition |
|---|---|
| R-43 | A proposal shows author, base revision, source scope, before/after changes, model versus presentation classification and calculation status before any accepted edit. |
| R-44 | Accepting a subset evaluates that subset. Selecting A+B in P17 shows profit 276,000; it cannot display the A+C result 252,000. |
| R-45 | Dependency-required edits are accepted/undone in structurally valid units. Creating a name and using it cannot leave a dangling accepted half-unit through the ordinary review UI. |
| R-46 | A+B acceptance is atomic but declares two independent undo units. Immediate Ctrl+Z restores only the heading; price remains 125 and profit 276,000. |
| R-47 | The 120/123/125 price conflict prevents stale overwrite and shows all three values. Independent unconflicted proposal units remain reviewable. |
| R-48 | Undo, rejection and later reversal retain attribution in history. An old selective reversal is a new checked change, not invisible history rewriting. |
| R-49 | Stop leaves a visibly incomplete, unapplied proposal. Continuing or reopening revalidates its base revision; a recovered accepted proposal cannot execute twice. |
| R-50 | Agent customisation cannot grant data-sharing, macro or component execution permissions. Scope already authorised by the user remains usable without repeated unnecessary prompts. |

### B.6 Scale, accessibility and responsiveness

| ID | Requirement and pass condition |
|---|---|
| R-51 | Meet §6's measured p95/p99 opening, input, switching and cancellation budgets on the reference fixture; report hardware/cache/provider state. Unmet targets are reported failures, not claims of success. |
| R-52 | Every aggregate/map/table reports coverage, member count, hidden exceptions and any sampling. A sample never supports an exact whole-data conclusion without a separate complete calculation. |
| R-53 | Above drawing budgets, offer counted groups, a focused submap, a list or Classic. Never hang, silently drop content or reduce text below legibility. |
| R-54 | All storyboard operations work with keyboard alone and with click-only alternatives to dragging. Test map wiring, node movement, notebook reorder, slider changes and source selection. [S20] |
| R-55 | Screen readers expose real row/column coordinates, headers, selection, source, editability, error and freshness. Virtualisation does not renumber row 10483 as row 1. [S21] |
| R-56 | UI zoom at 200% and 400% keeps all controls available. Prose/forms reflow at 320 CSS pixels; genuinely two-dimensional grids/maps retain scrolling plus a linear/list alternative. [S17] |
| R-57 | Meaningful text and non-text states meet the chosen contrast thresholds in light/dark/high-contrast themes. Focus remains visible and unobscured; colour removal preserves meaning. [S1, S18] |
| R-58 | Hover information is equally accessible through focus/explicit details, and dismissible. Reduced motion removes dependency ripples without removing event/state information. |
| R-59 | Status completion/progress is announced without stealing focus or reading every recalculated cell. A stalled provider produces a stable labelled state with a valid next action. [S19] |

### B.7 Customisation, printing, macros and preservation

| ID | Requirement and pass condition |
|---|---|
| R-60 | A skin preview reports its actual changed model/presentation scope. The Classic-restyle example changes zero formulas, values or names and preserves the Classic keymap. |
| R-61 | Imported skins cannot override source identity, Escape/recovery, truthful status or security. Invalid contrast/key bindings/component requirements give actionable validation before activation. |
| R-62 | A missing presentation part opens the workbook in Classic; a broken source binding remains visibly unresolved. Normal supported Excel save/reopen routes preserve layout and verified binding behaviour. |
| R-63 | Print previews label revision, result freshness, filters, scope and missing dependencies. Loan prints one summary page unless its full schedule is explicitly selected. |
| R-64 | Inspector/map prints disclose hidden branches/boundary connectors; Notebook prints reading order; Explorer prints caller/provider/test context. No output silently treats an excerpt as complete. |
| R-65 | Macro security applies uniformly in every view. Optional presentation requests may switch/select/show a panel, but calculation does not require a DNA-only presentation call to work in Excel. |
| R-66 | A non-undoable macro/provider action is identified before execution and offers a document checkpoint where possible. History distinguishes document reversal from external side effects. |
| R-67 | Save failures preserve the last good workbook; content-loss conversions identify affected macros, formulas or metadata before committing. Stored code and unknown supported objects are not silently discarded. |
| R-68 | Inherited has no auto-published App. An author must confirm writable inputs, outputs and source/runtime requirements before a manager-facing layout becomes preferred. |

### B.8 Required screenshot review set

Capture these **25 workbook/view cases** at the same 1440×900 working viewport, then capture selected states at high contrast and 200% zoom. These are acceptance screenshots to obtain from the future prototype, not screenshots claimed to exist in this design deliverable. The companion HTML contains the design's numbered wireframes.

| View | Budget | Sales | Loan | Stock | Inherited |
|---|---|---|---|---|---|
| Classic | Twelve assumptions/two scenarios; mid-F4 reference edit | Orders row 10483 under filters; corrected Report formula | Payment and first schedule rows; full extent visible | H15 selected; B8 circle; linked slider | Loading/saved values; dense region; hidden-sheet inventory |
| Inspector | Summary!B7 dependents; exact current revision | Wrong contribution and one-formula correction | PMT inputs; running-balance seed | H15->C18 lag and B8 loop separately | Broken OFFSET name and missing external file separately |
| Notebook | Empty insertion; live write-up; blocked spill | Bound Report/Pivot with refresh state | Narrative with 12-of-360 schedule | Flow explanation with source-linked lag | Chosen excerpt plus unplaced-content count |
| Model map | Inputs > Monthly > Summary with exceptions | Orders > SUMIFS/FILTER/XLOOKUP/Pivot, grouped | One recurrence block; expanded chosen month | Ghost wire; confirmed delay; Classic continuity | Forty-sheet overview; bounded expansion/refusal |
| App | Confirmed twelve-input contract; scenario columns | Authored filters and true Report results | Pending/invalid/current/print states | Existing reorder slider and warning state | No App defined; broken-binding recovery if a layout exists |

Also capture **Formula Explorer inside Budget** (real caller, trial/apply/conflict) and **standalone** (empty, provider-loading, success, expected error, context refusal). For each storyboard retain before, consequential edit, pending calculation, failure/refusal, cross-view return and undo/recovery captures. Screenshots include source and status; they do not replace interaction logs, screen-reader transcripts or file comparisons.

## Appendix C: What the design needs from the system underneath

These are plain needs, independent of implementation language, object model, threading strategy or wire format. “Be told” does not prescribe an event bus; “temporary state” does not prescribe a cloned engine. Optional diagnostic evidence may be absent, but its absence must be explicit.

| ID | Needed capability | Why it is needed |
|---|---|---|
| C-1 | Identify a workbook, sheet, cell/range, name, table and object, and tell whether a formerly identified source still exists after edits or reopen. | Preserve selection and bindings without guessing nearby replacements. |
| C-2 | Read exact formulas, stored values, formatted text, number formats, styles, errors, annotations and relevant workbook settings. | Show source truth and preserve imported presentation. |
| C-3 | Know whether a value is saved, current, dirty, calculating, cancelled or unavailable, and which completed document revision it belongs to. | Prevent stale or mixed-revision results from appearing current. |
| C-4 | Be told which cells/objects/settings changed after an edit or calculation, including formatting and changed spill shape. | Update all bound views without re-reading the entire workbook. |
| C-5 | Read available cells incrementally, with trustworthy progress and the distinction between empty, not yet loaded and failed to load. | Keep Inherited usable during opening. |
| C-6 | Read sheet/table/object inventories with hidden/protection state, used extents and complete-versus-partial counts. | Honest navigation and large-workbook overview. |
| C-7 | Know formula precedents/dependents, summarised by range/block, with unresolved, conditional and runtime-selected references distinguished. | Explain Sales and Stock without fabricating connections. |
| C-8 | Obtain exact dependency membership and exceptions behind a grouped formula run or edge. | Expand summaries and target a precise map edit. |
| C-9 | Inspect a formula's syntactic structure, reference identity, local bindings, evaluation location and exact source spelling. | Render a faithful lens and safely resolve a displayed alias. |
| C-10 | Distinguish cells, spill anchors/children, legacy array regions, tables, pivot outputs, protected cells and objects when deciding editability. | Make every view obey the same edit rules. |
| C-11 | Preview and apply normal Excel-compatible edits with exact old/new targets, reference adjustments, validation and affected scope. | Share one editing contract across grid, map, notebook and agent. |
| C-12 | Apply a related set of changes atomically, reject stale expectations, and report conflicts at their actual source. | Prevent partial agent acceptance and stale Explorer apply. |
| C-13 | Record reversible edits and meaningful undo units across both model and authored presentation, while identifying non-reversible effects. | Predictable global undo and honest macro history. |
| C-14 | Retain/recover committed work and unapplied drafts distinctly, with identities sufficient to avoid repeating accepted actions. | Safe restart without automatic re-execution. |
| C-15 | Evaluate a formula trial in an explicit caller/input context, and distinguish expression testing from replacing a formula in temporary workbook state. | Ade's UDF tests and Lee's genuine cycle preview. |
| C-16 | Say whether a provider can be safely previewed, requires explicit execution, needs workbook context, or has external side effects. | Avoid invisible execution while typing or reviewing. |
| C-17 | Enumerate add-in functions and their available metadata, provider/version, permission state and supported execution host. | Honest autocomplete, loading and missing-provider paths. |
| C-18 | Expose formula-result values separately from number-format and conditional-format evaluation, including reference anchors when needed. | Test formatting without confusing display with calculation. |
| C-19 | Expose actual calculation mode, effective scope, iterative settings, cycle members and any genuinely observed convergence/stop reason. | Distinguish Stock's circular reserve from its period lag. |
| C-20 | Supply observed calculation trace/order/concurrency only when available, separately from dependency relations and value differences. | Avoid a fictional recalc animation. |
| C-21 | Report external link/provider identity, source availability, last supplied value/time and refresh/connection state without silently refreshing. | Missing sources and live instruments remain honest. |
| C-22 | Report observed live updates and their timestamps/coverage, and tell whether intermediate events were coalesced or not observed. | Label retained histories and avoid false event causality. |
| C-23 | Perform exact filtering/contribution queries on the requested scope and disclose incomplete processing; retrieve bounded row windows with stable source targets. | Sales' exact audit and large table interaction. |
| C-24 | Read and operate actual PivotTables, chart objects, form controls, scenario records, validation rules and print settings. | Views reuse existing workbook mechanisms. |
| C-25 | Know macro/module presence, trust, execution availability, run boundaries and observed workbook effects; support optional presentation requests. | Keep macro-driven models usable and attributed. |
| C-26 | Save/load versioned optional view declarations and source bindings in supported files, preserving unrelated workbook parts. | Make preferred views travel without changing calculation content. |
| C-27 | Detect missing/unsupported presentation versions and provide preserved declarations for recovery rather than silently throwing them away. | Graceful fallback to Classic. |
| C-28 | Preserve/transfer ordinary clipboard content with source-origin, formula/value/text/format distinctions and declare loss at an unsupported destination. | Reliable cross-view and Excel/program copy-paste. |
| C-29 | Provide a consistent completed-result snapshot and all relevant filters/source statuses for printing or report export. | Avoid mixed calculation revisions on paper. |
| C-30 | Expose all commands, edit states, coordinates, labels, selections and statuses through accessible interaction surfaces. | Keyboard, screen reader and high-zoom parity. |
| C-31 | Cancel or bound long work, report uninterruptible operations honestly, and keep ordinary navigation/edit feedback responsive. | Meet §6 without inventing computation completion. |
| C-32 | Supply resource/coverage estimates and exact totals where available before expanding large graphs, tables or histories. | Useful simplification and plain refusal instead of hangs. |
| C-33 | Attribute proposed/accepted/manual/macro edits, compare revisions and compute a proposed reversal against current state. | Auditable agent collaboration and conflict-safe recovery. |
| C-34 | Distinguish unsupported capability from invalid workbook content, and preserve the original content even when this host cannot execute it. | Respect the brief's file compatibility and cross-host reality. |

## Appendix D: Pattern catalogue and cast fixtures

Confidence vocabulary: **E** = explicit structure or directly observed formula fact; **S** = structural suggestion; **W** = weak semantic guess. E is not proof of the author's business intent. Every inference has an evidence disclosure and a range/address. An agent can propose a confirmation; the same visible review applies. Tables D1 and D2 share IDs so the printed appendix need not squeeze nine text-heavy columns onto one page.

### D1. Recognition, uncertainty and correction

| ID / pattern | What a person recognizes | Program evidence and limit | Confirm or correct |
|---|---|---|---|
| P-01 Sheet as database | Sales: repeated order rows beneath one header; filtering, IDs, quantities, amounts | E for `Orders` Table and its declared range; S for rectangular ordinary ranges with consistent column types; neither proves key uniqueness or absence of subtotals | Accept row/header range; mark `Order ID` as a display key; show duplicate counts; exclude totals; do not convert to a Table without an explicit model edit |
| P-02 Inputs and assumptions | Budget: twelve labelled assumptions with Base/Stretch columns; Loan: principal, rate and term | E for validation and named ranges; S for constants feeding formulas; W from colour, border or words such as `Input` | Confirm editable cells, labels, units and scenario columns; exclude constants that are historical observations; show protection/validation independently |
| P-03 Calculation blocks | Monthly revenue and cost rows repeat across Jan-Dec | E for repeated relative formula structure; S for borders, labels and adjacency; a shared pattern is not proof of a shared business role | Show exact member ranges and exceptions; rename/split/merge visual grouping; retain hard-coded overrides as explicit exceptions |
| P-04 Time series across columns | Budget dates progress left to right; repeated monthly calculations | E for values and references; S for date cadence and repeated formulas; text `Jan` alone does not prove year, cadence or chronology | Confirm period row, order, date system context and units; distinguish actual dates from captions; reverse or skip periods only in presentation with a notice |
| P-05 Scenarios and sensitivity | Budget's Base/Stretch columns; Loan's alternative assumptions | E for existing What-If data tables or stored scenario structures; S for parallel columns; W that differing inputs mean scenarios | Name input/output sets; distinguish viewing an existing scenario from writing selected input cells; review new scenario tables as workbook edits |
| P-06 Reference tables and lookups | Sales product/customer mappings, XLOOKUP keys and return columns | E for lookup arguments/table structure; S for key-like columns; duplicate keys and approximate matches may be intentional | Confirm key and descriptive fields; reveal duplicate/missing keys and actual match mode; never silently deduplicate or sort |
| P-07 Pipelines across sheets | Inputs -> Monthly -> Summary; Orders -> Report | E for resolved dependencies; S for dominant direction; dynamic references and external links can make the relation incomplete | Approve stages and grouping; label cross-links; disclose unresolved or runtime-only dependencies; allow one cell to appear in more than one presentation |
| P-08 Running balances | Loan balance carries from preceding row and subtracts principal | E for previous-row recurrence; S for a balance interpretation or its units | Confirm opening/closing terms and period axis; expose the first-row seed; do not treat a row recurrence as an iterative circular calculation |
| P-09 Deliberate circular calculation | Stock's circular estimate, iteration enabled, convergence readout | E for a resolved cycle and workbook iteration settings; W for intent/convergence without evidence; unknown dependencies remain unknown | Show participating cells and settings; ask whether intended; approval annotates intent only; enabling iteration is a separate workbook-setting edit |
| P-10 Simulation / repeated trials | A trial table, random functions, or an existing macro/data-table sweep | E for RAND/RANDBETWEEN and data-table/macro presence; S that repeated rows are independent trials; distributions and independence are not inferred facts | Confirm trial axis, outputs and refresh trigger; show random/volatile markers; no view-side random generator, reseeding, or invented execution clock |
| P-11 Control panels | Loan labelled controls; Stock reorder slider; buttons beside outputs | E for controls and linked cells; S for inferred input labels; a control can invoke a macro rather than write a value | Inspect each binding/action; approve grouping and tab order; preserve the control's actual min/max/step and linked-cell meaning |
| P-12 Pivot summaries | Sales PivotTable, dimensions, totals and filters | E for PivotTable and cache/refresh information available in workbook; source freshness may be unknown | Choose shown dimensions; disclose refresh time/status when known; filter or refresh through the real pivot action, not a new view-only aggregate |
| P-13 Live data | RTD/external feed values change while the workbook is open | E for supported provider and observed updates; unavailable provider or stored result does not imply live connection | Confirm displayed source/units; show connected/stale/paused/unavailable; configure history retention as presentation; keep pause-observation distinct from stopping the feed |
| P-14 Macro-driven models | Inherited buttons/module and values changed by a macro | E for macro/assigned-action presence; W for effects inferred from code text; runtime observations may be partial | Show permission and platform state; record actual changed regions after a run; do not label an unexecuted macro's effects as known |
| P-15 Reports and dashboards | Summary KPIs, captions, charts and print areas | E for chart objects/print areas; S for report intent; merged cells and bold numbers alone are weak | Confirm reading order and source bindings; pin chart objects and outputs; keep explanatory prose separate from workbook values |
| P-16 Stocks and flows | Stock opening stock + arrivals - shipments = closing stock | E for an arithmetic relationship; S for quantity accumulation; W that every additive relationship is a physical flow | Confirm units and stock/flow roles; expose sign conventions and bounds; the diagram follows the actual formula and can be corrected independently |
| P-17 Feedback and delays | Stock reorder decisions affect later arrivals; arrows return to inventory | E for dependencies; S for feedback at a grouped level; a circular reference is not itself a time delay | Confirm lag and period axis from explicit formulas/rows; label `3 days` only with that evidence; use `iteration` for convergence loops |
| P-18 Reconciliation and control totals | Inputs equal outputs; check is zero; Grand Total comparisons | E for subtraction/equality expressions; S for their role as checks; tolerance and materiality cannot be invented | Confirm check cells and what zero/TRUE means; surface existing tolerance; adding new checks or IFERROR is a separate model change |
| P-19 Exceptions and overrides | One hard-coded value interrupts a formula run; errors or unusual formats | E for difference from neighbouring formula structure; W that the exception is a mistake | Keep `exception` distinct from `error`; mark accepted override with an annotation; propose repair only with original/new formula and downstream preview |
| P-20 Repeated-sheet families | Similar regional/monthly sheets feed a consolidation sheet | E for matching structure/reference patterns; S for family membership; hidden sheets and local names may differ materially | Approve group, order and representative sheet; list all deviations and all members, including hidden status; never fold exceptions out of the summary |
| P-21 External-file consolidation | Inherited references missing supplier or rates files | E for external-link records and resolved formulas; last stored value is not current external evidence | Map/repair a link through a workbook edit; keep `missing file`, `update not requested` and `cached result` distinct; allow read-only investigation |
| P-22 Spill / array result | Sales FILTER result occupies changing rows; outline has one anchor | E for array/spill metadata and occupied cells; obstruction or unavailable calculation may leave shape unknown | Confirm presentation columns/labels; preserve anchor ownership; editing a spill child refuses with a jump to the anchor or obstruction |
| P-23 Irregular forms and sparse layouts | Inherited contains forms, blank spacing, merged labels and separate small islands | E for cell/object geometry; S for label-to-value pairing; proximity alone is weak | Correct groups and reading order; preserve a Classic fallback; show `Unclassified content` rather than assigning a false model type |

### D2. What each view does with the pattern

Here `bound` means the displayed item resolves to an existing workbook cell/range/object. Any new formula, name, scenario storage or chart is an explicit workbook edit.

| ID | Classic | Inspector | Notebook | Model map | App |
|---|---|---|---|---|---|
| P-01 | Full Orders table; headers, filters, pinned columns | Selected row/value provenance; type/duplicate/error summaries with exact scope | Bound table with visible total/filtered count and row window; underlying data remains accessible | One Orders node with column/row-count ports; expand chosen fields | Approved search/filter/table panel; edit only allowed input columns |
| P-02 | Existing assumption cells and validation | Input -> dependents; constants/protected state | Labelled bound controls beside explanation | Input node or ports, exact cells on focus | Main input form with units, validation and source details |
| P-03 | Original rows/columns; exceptional formula visible | One formula family plus exception list | Formula block with result table and source range | Calculation block with grouped edges and member count | Selected outputs; advanced detail can reveal block |
| P-04 | Original twelve columns, freeze labels | Period-relative dependencies and exceptions | Compact time table/chart, horizontal pipeline if chosen | Period-labelled block; expand a chosen period, do not duplicate an unbounded timeline | Bound chart and period selector, if selection already has a workbook meaning |
| P-05 | Existing scenario cells/data table | Compared inputs/results with calculation state | Side-by-side bound scenario results | Parallel input sets feeding the same calculation block | Explicit `Try`/`Apply` behavior with baseline and changed-input list |
| P-06 | Lookup table/filter and source formulas | Match mode, actual matched rows, duplicate/missing-key warnings | Lookup explanation and bounded reference table | Reference-table node, actual lookup dependency | Validated chooser with key/label and missing-key errors |
| P-07 | Sheet tabs, formula references | Cross-sheet path, omitted branch count and dynamic edges | Reading sequence can differ from calculation order | Nested sheet/stage groups with actual edges | Published process steps only; formulas still evaluate by workbook rules |
| P-08 | 360-row Loan schedule | Seed, previous-row relation, final balance and exceptions | Schedule excerpt labelled `rows 1-12 of 360`; full table action | One recurrence block with initial state and period axis | Summary/graph plus expandable full schedule |
| P-09 | Actual circular formulas and workbook settings | Cycle members, iteration count/residual if supplied, convergence/limit state | Explanation plus latest actual result; no fake run button | Explicit iteration enclosure, separate from delay notation | Show current result and convergence warning; do not hide failure |
| P-10 | Existing trial grid/data table | Volatility, trigger and observed run provenance | Bound distribution chart with sample/trial counts | Trial/sweep block only when workbook mechanism is known | Approved controls and real trigger, visible result freshness |
| P-11 | Original form controls and bindings | Control -> linked cell/macro effects | Embedded original control or faithful alternative | Parameter port with bound control | Primary form/button layout; permission and disabled reason visible |
| P-12 | Native pivot layout and field/filter operations | Cache/source lineage and refresh state | Bound pivot with exact filters and totals | Pivot node between source and report | Approved slicers/filter controls and existing pivot results |
| P-13 | Live cells with freshness/status cue | Timestamped values, source state and labelled retained history | Live result/chart, still respecting calculation mode | External-source node and actual observed updates | Source state next to results; no claim of currentness when stale |
| P-14 | Workbook and real macro controls | Permission, run boundaries and observed changes | Bound result with last-run status; explicit macro action | Macro as action/observed-effects node, never invented dependency truth | Authorized macro button; plain refusal and desktop route if unavailable |
| P-15 | Original report and chart objects | KPI -> source/formatting/data lineage | Main reading/publishing surface | Output group, select a KPI to trace | Main presentation surface, authored reading order |
| P-16 | Actual balance formulas | Inflow/outflow/stock relationships and units | Short explanation, bound table/chart | Main systems surface; flow role is confirmed annotation | Stock outputs and input controls relevant to operator |
| P-17 | Rows or columns implementing actual lag | Separate dependency cycle from period lag | Explicit lag explanation and result sequence | Labelled delayed dependency or iteration enclosure, according to evidence | Timing/lag explanation near outputs when it affects interpretation |
| P-18 | Check cells and original tolerance formulas | Failed checks grouped by affected output; no invented repairs | Visible check summary with exact predicate | Check attached to relevant block | Compact checks; failure stays visible and affects readiness label |
| P-19 | Exact override/error cell | Explain deviation, original neighbours and dependents | Exception footnote linked to cell | Badge on containing block; expand precise member | Relevant warning, original error text and a source link |
| P-20 | All original sheets and hidden status | Family differences and consolidation lineage | Representative template plus all-member list | Family group with member/exception counts | Approved region/period chooser backed by existing workbook selection |
| P-21 | External-link formula and stored value | File status, known/unknown link targets and last update | Cached-result notice and source citation | External boundary node with unresolved-edge marker | Visible stale/missing-source state; no optimistic `Ready` |
| P-22 | Anchor/spill boundary/obstruction | Shape, anchor, dependent ranges and error origin | Reactive data-frame display with anchor label | Array port with dimensions or `shape unavailable` | Bound result table; edit requests lead to permitted inputs |
| P-23 | Complete faithful escape hatch | Unclassified islands, objects and formula/error inventory | User-selected excerpts with address captions | Sheet/block overview, no forced semantic labels | Decline automatic publication; show `Choose inputs and outputs first` |

### D3. Authoritative fixture content for the storyboards

These are designed example workbooks, not inspected uploads or Excel execution evidence. The source brief supplies their overall shapes; this section supplies exact addresses, formulas and displayed numbers so a prototype can be built consistently. Do not infer that every historical Enron file uses these names or structures. Each storyboard starts from its stated baseline; changes in one storyboard do not silently alter another's starting fixture.

Numbers use an English formula dialect with commas as argument separators, `.` decimals and ZAR display in the finance examples. Regional input/display is a user setting; formulas must retain their underlying meaning through locale changes. Dates below are actual dates, not ambiguous strings. No example uses precision-as-displayed; round only the visible text. Currency examples are demonstration arithmetic, not an additional product calculation layer.

#### Budget

Sheets are **Inputs**, **Monthly**, **Summary**. Inputs!A1:C13 is an ordinary assumption range with row 1 `Assumption | Base | Stretch`. Colours or table-like appearance do not automatically convert it into a formal Excel Table.

| Row | Assumption (column A) | Base (B) | Stretch (C) |
|---|---|---:|---:|
| 2 | Monthly units | 1,000 | 1,100 |
| 3 | Unit price (ZAR) | 120 | 125 |
| 4 | Unit cost (ZAR) | 70 | 72 |
| 5 | Fixed operating cost / month (ZAR) | 32,000 | 34,000 |
| 6 | Monthly unit growth | 0% | 0% |
| 7 | Monthly price growth | 0% | 0% |
| 8 | Monthly cost inflation | 0% | 0% |
| 9 | Annual borrowing rate | 10.5% | 11% |
| 10 | Tax rate | 27% | 27% |
| 11 | Starting cash (ZAR) | 200,000 | 200,000 |
| 12 | Debtor days | 30 | 30 |
| 13 | Minimum cash (ZAR) | 50,000 | 50,000 |

The Monthly worksheet shows the Base case in the storyboards. Stretch is an alternate assumption column, not a secretly active second model. An authored action to use it must expose the ordinary workbook inputs/scenario record it changes. Borrowing, tax, cash and debtor assumptions are present for the broader model; the operating-profit excerpt below does not use them. No preview claims an effect from an assumption outside its actual dependencies.

| Range/cell | Label / formula |
|---|---|
| B1:M1 | Monthly dates January–December 2027; B1 `=DATE(2027,1,1)` and C1 `=EDATE(B1,1)` filled right. |
| B2 / C2:M2 | Units: `=Inputs!$B$2`; then `=B2*(1+Inputs!$B$6)` filled right. |
| B3 / C3:M3 | Price: `=Inputs!$B$3`; then `=B3*(1+Inputs!$B$7)` filled right. |
| B4 / C4:M4 | Unit cost: `=Inputs!$B$4`; then `=B4*(1+Inputs!$B$8)` filled right. |
| B5 / C5:M5 | Fixed cost: `=Inputs!$B$5`; then `=B5*(1+Inputs!$B$8)` filled right. |
| B8:M8 | Revenue: `=B2*B3`, filled right. |
| B9:M9 | Variable cost: `=B2*B4`, filled right. |
| B10:M10 | Fixed cost: `=B5`, filled right. |
| B11:M11 | Operating profit: `=B8-B9-B10`, filled right. |
| Summary!B4 | Annual revenue: `=SUM(Monthly!B8:M8)` = 1,440,000. |
| Summary!B5 | Annual variable cost: `=SUM(Monthly!B9:M9)` = 840,000. |
| Summary!B6 | Annual fixed cost: `=SUM(Monthly!B10:M10)` = 384,000. |
| Summary!B7 | Annual operating profit: `=B4-B5-B6` = 216,000. |
| Summary chart | Original chart of Monthly!B1:M1 with Revenue B8:M8 and Operating profit B11:M11 series. |
| Summary!A20 | Created during the Notebook story: `=TRANSPOSE(Monthly!B11:M11)`; intended extent A20:A31. |
| Summary!A21 | Existing text `Director note`, blocking that spill until moved to D21. |
| Summary!D14 | Created during Ade's story: `=DNA.DISCOUNT(Inputs!B3,10%)` = 108 at the baseline price. |

Optional workbook names `Budget.Units`, `Budget.Price`, `Budget.UnitCost`, `Budget.FixedCost` refer to absolute Inputs!B2:B5 respectively. They are real names only if explicitly created; the Formula lens's labels do not create them.

At B3=125, monthly revenue is 125,000 and profit is 23,000; annual revenue is 1,500,000 and profit is **276,000**. Within **Monthly!B8:M11**, exactly **24 values** change: twelve revenue cells and twelve profit cells. Input changes and other dependent cells outside that scope are separately counted. At B3=125 and B4=72, annual profit is **252,000**, and 36 values change in that same range. Agent P17 units are A: price 120->125; B: heading `Budget: base case` -> `Budget: price review`; C: unsupported suggestion cost 70->72. A and B are independent undo units; C is not accepted.

#### Sales

Sheets are **Orders**, **Report**, and **Pivot**. Orders!A1:G25001 is a real Table named `Orders` with **25,000 data rows** and columns **OrderID, Date, Region, Customer, Amount, Status, Salesperson**. Data row numbering excludes the header. The fixture's West subset contains 125 Paid orders totalling 124,250 and one Pending order of 4,200; the other 24,874 rows have other regions. These complete totals, not a sample, drive the audit.

| Source | Exact content |
|---|---|
| Orders data row 10482 / sheet row 10483 | `SO10482`; `DATE(2026,11,5)`; `West`; `Lark Hardware`; `4200`; `Pending`; `Maria`. The compact S06 excerpt shows its first six fields. |
| Report!B2 | Region criterion `West`. |
| Report!A5 / B5 before | Label `Paid sales, West`; formula `=SUMIFS(Orders[Amount],Orders[Region],B2)` = 128,450. |
| Report!B5 after | `=SUMIFS(Orders[Amount],Orders[Region],B2,Orders[Status],"Paid")` = 124,250. |
| Report!B6 / B7 | B6 is `SO10482`; B7 is `=XLOOKUP(B6,Orders[OrderID],Orders[Customer],"Not found")`, returning `Lark Hardware`. |
| Report!A12 | `=FILTER(Orders,(Orders[Region]=B2)*(Orders[Status]="Pending"),"No matching orders")`; the fixture returns the one pending record. |
| Pivot | Actual PivotTable sourced from Orders: Region rows, Sum of Amount values, Status filter. West with Status=Paid = 124,250; West with all statuses = 128,450. Cache freshness is independently stated. |

The prototype may generate the other rows, but its generator must enforce the counts, totals, order identity and exact row address above. The interface must not claim those rows were extracted from a real sales dataset. A separate stale-cache variant preserves the old Pivot total after a source edit until an actual refresh.

#### Loan

One sheet **Loan**. Inputs are B2 principal **1,000,000**, B3 annual rate **10.5%**, B4 months **360**. Existing data validation uses Stop alerts and requires principal >0, rate between 0 and 100%, and integer months 1..360. Payments are at period end, with future value zero. App inputs reuse those constraints; a changed validation rule is a separate model edit.

| Source | Formula / content |
|---|---|
| B6, Monthly payment | `=-PMT(B3/12,B4,B2)` |
| B7, Total interest | `=B6*B4-B2` |
| A11:F11 | Month, Opening balance, Payment, Interest, Principal, Closing balance. |
| A12:A371 | Integer months 1..360. |
| B12 | `=IF(A12>$B$4,"",$B$2)` |
| B13, filled down | `=IF(A13>$B$4,"",F12)` |
| C12, filled down | `=IF(A12>$B$4,"",$B$6)` |
| D12, filled down | `=IF(A12>$B$4,"",-IPMT($B$3/12,A12,$B$4,$B$2))` |
| E12, filled down | `=IF(A12>$B$4,"",-PPMT($B$3/12,A12,$B$4,$B$2))` |
| F12, filled down | `=IF(A12>$B$4,"",B12-E12)` |
| Balance chart | Category range `=Loan!$A$12:INDEX(Loan!$A$12:$A$371,Loan!$B$4)` and values `=Loan!$F$12:INDEX(Loan!$F$12:$F$371,Loan!$B$4)`, stored through ordinary chart references/names. |

Rows past a shorter term are explicitly inactive/blank through the formulas, and the chart range ends at the term. The view cannot merely hide invalid IPMT results. The workbook's actual floating-point ending residual is shown in full precision on request; ordinary currency display may show zero. No silent rounding is inserted into the schedule.

| Case | Monthly payment | First interest | First principal | First closing balance | Total interest |
|---|---:|---:|---:|---:|---:|
| 10.5%, 360 months | 9,147.39 | 8,750.00 | 397.39 | 999,602.61 | 2,293,061.46 |
| 12%, 360 months | 10,286.13 | 10,000.00 | 286.13 | 999,713.87 | 2,703,005.35 |

Full-precision monthly payments are approximately 9147.392944930707 and 10286.125969255043. Jo's earlier-case comparison is a labelled recorded observation until she explicitly saves a standard Excel Scenario Manager record for B2:B4. Its existence does not make the recorded result a live recalculating model. The PMT/IPMT/PPMT definitions follow their normal per-period rate and payment conventions. [S14]

#### Stock

One sheet **Stock**, seven inputs/result labels in A2:A8. B2 opening inventory=400; B3 daily demand=100; B4 lead time=3 days, validated as integer 1..30; B5 reorder point=260; B6 order quantity=420; B7 reserve target=20; B8 is the iterative reserve. `Stock.ReorderPoint` is the optional workbook-scoped name for `$B$5`. A native form-control slider links to B5, min 0, max 600, small change 10; the numeric companion edits that same cell.

Lee starts B8 at zero, then explicitly enters **`=0.5*(B8+B7)`** with iterative calculation enabled, maximum iterations 100 and maximum change 0.001. The default display is two decimals, **20.00**. This is a convergent scalar example that participates in the reorder threshold; its purpose is to test circular-reference UX, not to claim iteration is needed for this elementary algebra. The exact last iterate and execution count depend on actual calculation and must not be invented by the view.

Headers A12:H12 are **Day, Opening, Inflow, Outflow, Closing, In transit, Position, Order**. Row 13 is day 0: A13=0; B13=`=$B$2`; C13=0; D13=0; E13=`=B13`; F13=0; G13=`=E13+F13`; H13=0. Enter row 14 and fill to row 43:

| Cell | Exact formula |
|---|---|
| A14 | `=A13+1` |
| B14 | `=E13` |
| C14 | `=IF(A14<$B$4,0,INDEX($H$13:$H$43,A14-$B$4+1))` |
| D14 | `=MIN($B$3,B14+C14)` |
| E14 | `=B14+C14-D14` |
| F14 | `=SUM($H$13:H13)-SUM($C$13:C14)` |
| G14 | `=E14+F14` |
| H14 | `=IF(G14<$B$5+$B$8,$B$6,0)` |

The positive lead-time validation ensures the realised arrival reference is to an earlier row. The INDEX source range and the actually selected precedent are separately inspectable. In S08 only, Lee deliberately starts H15 with the incomplete `=IF(G15<$B$8,$B$6,0)`, connects reorder point into its threshold and applies `=IF(G15<$B$5+$B$8,$B$6,0)`. The completed H pattern is then filled over the intended period rows. The same actual reference-copy rules used in Classic apply.

| Day | Opening | Inflow | Outflow | Closing | In transit | Position | Order |
|---|---:|---:|---:|---:|---:|---:|---:|
| 0 | 400 | 0 | 0 | 400 | 0 | 400 | 0 |
| 1 | 400 | 0 | 100 | 300 | 0 | 300 | 0 |
| 2 | 300 | 0 | 100 | 200 | 0 | 200 | 420 |
| 3 | 200 | 0 | 100 | 100 | 420 | 520 | 0 |
| 4 | 100 | 0 | 100 | 0 | 420 | 420 | 0 |
| 5 | 0 | 420 | 100 | 320 | 0 | 320 | 0 |
| 6 | 320 | 0 | 100 | 220 | 0 | 220 | 420 |

**H15, day 2's order of 420, is received at C18, day 5's inflow of 420.** With B5=320, day 1 orders 420; day 4 receives it and closes at 420. The collapsed system diagram can show feedback across periods, but it must not label this as the same-period circular calculation at B8. Testing B8=`=B8+1` as an actual temporary replacement cannot converge; report the real stop reason/latest result or unavailable evidence, never an invented `#CYCLE!` cell error.

#### Inherited

This is a composite stress fixture inspired by the brief and the corpus, not an identified Enron file. Give it 40 sheets, of which 6 are hidden, approximately 300,000 formula cells, and one dense used region called Archive. Include Summary, Revenue and Monthly as visible sheets; another 31 visible and 6 hidden sheets complete the count. The fixture construction must record their actual membership, formula counts and bounds so summaries can be verified. The source brief does not define file byte size or row/column extent; the benchmark fixture must pin both before performance results are compared.

| Item | Fixture definition |
|---|---|
| File type | `Inherited.xlsm`, with original VBA module `LegacyClose` preserved and initially disabled. This is a modern design fixture, not a claim about the original Enron files' format. |
| Missing source | External workbook `Rates2024.xlsx`; its path is deliberately unavailable. Last saved values and failed/unrequested refresh are distinct states. |
| Summary!F18 | `=SUM(Revenue.Window)`. Initial stored result 2,401,300 is a deliberately stale cache; local recalculation produces `#REF!`. |
| Revenue.Window | Workbook-scoped name `=OFFSET(Revenue!#REF!,0,0,12,1)`, a broken starting reference. The source error is separate from the missing external workbook. |
| Other saved labels | Summary row 16 Revenue subtotal 2,401,300; row 17 Operating costs 1,720,000. These are displayed saved readings, not asserted fresh totals. |
| Errors / dynamic names | Include valid OFFSET names with changing extents plus broken-reference/error cells; partial scans report only errors found so far and their scanned scope. |
| Recovery variant | Two recorded reversible edits after the last save, plus an unapplied editor draft. Restore committed actions once; keep the draft explicitly unapplied. |

It is legitimate for the stored value and current formula to disagree in this deliberately dirty fixture. The UI's job is to explain that disagreement, not to present the old number as fresh. No macro runs automatically to “fix” it.

#### Ade's add-in test contract

`DiscountFunctions.xll` version 1.0 is an illustrative provider, not an existing attached binary. It exports `DNA.DISCOUNT(price,rate)`: numeric price times `(1-rate)` for `0<=rate<=1`, and `#NUM!` for numeric out-of-range rate. The reviewed test set uses numeric scalar inputs only; other argument/coercion behaviour belongs to the provider's declared contract.

| Expression | Expected underlying result | Display / rule |
|---|---:|---|
| `=DNA.DISCOUNT(120,0.1)` | 108 | Format `"R "#,##0.00` displays R 108.00; Cell Value < 100 is false. |
| `=DNA.DISCOUNT(120,0.2)` | 96 | Same format displays R 96.00; Cell Value < 100 is true. |
| `=DNA.DISCOUNT(120,1.2)` | #NUM! | Error remains visible; an expected-error test can match. |

The separate fixture function `DNA.CALLERADDRESS()` requiring caller identity must be refused in standalone mode and run only in an explicitly supported real workbook context. The design does not require native XLL binaries to run inside a browser. It does require the provider/runtime requirement, any equivalent implementation and the user's selected execution route to be explicit.

### D4. Verification boundary for this design package

The fixture arithmetic was checked independently for Budget totals and changed-cell counts, Loan payment/interest/schedule identities, Stock's 30-day conservation and lead-time examples, and Sales' stated contribution sum. These checks establish internal numerical consistency of the examples. They do **not** establish Excel runtime parity, native add-in loading, accessibility conformance, actual opening speed or custom-XML round-trip preservation. Those are the prototype and system acceptance gates in Appendices A–C.

## Sources and design provenance

Research checked on **20 November 2026**. The numbered requirements, layouts, example fixtures, timing targets and recommendations are original design proposals. External sources support the specific platform behaviour or precedent stated beside their references; they do not establish that DNA Calc has passed an acceptance test. Where current documentation leaves interaction details ambiguous, Appendix A requires comparison with the pinned Excel build.

### Supplied design inputs

- **19-front-end-design-prompt.md:** product direction, cast, required flows, output structure and compatibility constraints.
- **STRAND.md, v0.1:** colour roles, typography, geometry, density, motion and source-address philosophy. The underlying Strand crate was not supplied; no claim of checking its actual tokens or tests is made.
- **LINKED_BOXES_SKIN.md:** early name-based idea and the later sheet-backed owner direction. The latter governs new map editing in this recommendation.
- **names-in-motion.html:** source/formula toggle, port values, parameter rail and model-map composition; source inspection also identifies its explicit previous-state snapshot and animation policy.
- **watershed.html:** nested groups, breadcrumb, source view and distinct history/outlook surfaces; its JavaScript execution policy is a sketch, not an Excel-parity witness.

### Design evidence

| Ref | Primary source | Used for |
|---|---|---|
| S1 | W3C, [Web Content Accessibility Guidelines 2.2](https://www.w3.org/TR/WCAG22/) | Accessibility acceptance baseline, not a claim of completed conformance. |
| S2 | Microsoft, [Dynamic array formulas and spilled array behaviour](https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior) | Anchor ownership, spill extent and obstruction behaviour. |
| S3 | Microsoft, [Remove or allow a circular reference](https://support.microsoft.com/en-us/excel/remove-or-allow-a-circular-reference-in-excel) | Circular calculations and iteration settings; no asserted time-delay guarantee. |
| S4 | Microsoft, [Keyboard shortcuts in Excel](https://support.microsoft.com/en-us/accessibility/excel/keyboard-shortcuts-in-excel) | Current Windows vocabulary and context-sensitive shortcuts; expanded sources below. |
| S5 | Pluto, [Reactivity: how notebooks update](https://plutojl.org/en/docs/reactivity/) | Reactive reading precedent; Excel behaviour still governs DNA Calc. |
| S6 | phyphox, [Remote-interface communication](https://phyphox.org/docs/remote-interface/) | Separation of observations, history coverage and instrument state. |
| S7 | Microsoft, [Excel recalculation](https://learn.microsoft.com/en-us/office/client-developer/excel/excel-recalculation) | Dependencies versus evolving calculation chain and possible concurrent evaluation. |
| S8 | Microsoft, [LET function](https://support.microsoft.com/en-gb/excel/functions/let-function) | Local bindings and once-per-binding evaluation, supporting the lens/refactoring distinction. |
| S9 | Microsoft, [Names in formulas](https://support.microsoft.com/en-us/excel/names-in-formulas) | Real name definitions and scopes; dotted names do not establish a new calculation namespace. |
| S10 | Microsoft, [Persist add-in state and settings](https://learn.microsoft.com/en-us/office/dev/add-ins/develop/persisting-add-in-state-and-settings) | Custom XML persistence within supported Office documents. |
| S11 | Microsoft, [Remove hidden data with Document Inspector](https://support.microsoft.com/en-us/office/collab-files/remove-hidden-data-and-personal-information-by-inspecting-documents-presentations-or-workbooks) | Custom XML can be deliberately removed; preservation needs a tested boundary. |
| S12 | Microsoft, [Work with VBA macros in Excel for the web](https://support.microsoft.com/en-us/excel/work-with-vba-macros-in-excel-for-the-web) | Preserving macro code is distinct from executing it in a particular host. |
| S13 | Microsoft, [Extend custom functions with XLL add-ins](https://learn.microsoft.com/en-us/office/dev/add-ins/excel/make-custom-functions-compatible-with-xll-udf) | Native-host availability and explicitly equivalent implementations. |
| S14 | Microsoft, [PMT](https://support.microsoft.com/en-us/Excel/functions/pmt-function), [IPMT](https://support.microsoft.com/en-us/excel/functions/ipmt-function), [PPMT](https://support.microsoft.com/en-gb/excel/functions/ppmt-function?nochrome=true) | Loan fixture's standard per-period payment and component conventions. Arithmetic independently checked here. |
| S15 | Hermans and Murphy-Hill, [Enron's Spreadsheets and Related Emails: A Dataset and Analysis](https://repository.tudelft.nl/file/File_6407791c-a000-4062-acaf-63e04502dfdc?preview=1), TUD-SERG-2014-021 | Corpus counts and why irregular business workbooks belong in the test family. Inherited is not an identified corpus file. |
| S16 | Microsoft, [Implicit intersection operator: @](https://support.microsoft.com/en-us/excel/implicit-intersection-operator) | Removing a visible operator can change scalar/array behaviour. |
| S17 | W3C, [Understanding Reflow](https://www.w3.org/WAI/WCAG22/Understanding/reflow) | 320 CSS-pixel reflow and the limited exception for genuinely two-dimensional content. |
| S18 | W3C, [Understanding Non-text Contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast) | Meaningful non-text states need sufficient adjacent contrast. |
| S19 | W3C, [Understanding Status Messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages) | Accessible status announcements without moving focus. |
| S20 | W3C, [Understanding Dragging Movements](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements) | Click-only alternatives are distinct from keyboard accessibility. |
| S21 | W3C APG, [Grid interaction pattern](https://www.w3.org/WAI/ARIA/apg/patterns/grid/) | Focus/selection/accessibility structure; generic APG key behaviour does not override Classic's Excel contract. |

### Classic interaction references

The following references support specific interaction semantics. The Appendix A rows are normative product acceptance cases; they also add DNA Calc-specific feedback, source continuity and recovery requirements. They are not a reproduced Excel manual. When sources disagree on a detail, use the specialised source and an observed baseline fixture; for example, formula copy/move behaviour must be verified rather than inferred from a broad clipboard description.

- **KA1:** [Keyboard shortcuts in Excel](https://support.microsoft.com/en-us/accessibility/excel/keyboard-shortcuts-in-excel).
- **KA2:** [Deselect a selection](https://support.microsoft.com/en-au/excel/deselect-a-selection).
- **KA3:** [Select rows and columns in an Excel table](https://support.microsoft.com/en-us/excel/select-rows-and-columns-in-an-excel-table).
- **KA4:** [Select specific cells or ranges](https://support.microsoft.com/en-us/excel/select-specific-cells-or-ranges-in-excel).
- **KA5:** [Excel status bar options](https://support.microsoft.com/en-us/excel/excel-status-bar-options).
- **KA6:** [Formula tips and tricks](https://support.microsoft.com/en-us/excel/formula-tips-and-tricks).
- **KA7:** [Use Formula AutoComplete](https://support.microsoft.com/en-us/excel/use-formula-autocomplete).
- **KA8:** [Create or change a cell reference](https://support.microsoft.com/en-us/excel/create-or-change-a-cell-reference).
- **KA9:** [Undo, redo, or repeat an action](https://support.microsoft.com/en-us/office/foundations-experiences/undo-redo-or-repeat-an-action).
- **KA10:** [Replace a formula with its result](https://support.microsoft.com/en-us/excel/replace-a-formula-with-its-result-in-excel).
- **KA11:** [Change formula recalculation, iteration, or precision](https://support.microsoft.com/en-us/excel/change-formula-recalculation-iteration-or-precision-in-excel).
- **KA12:** [Rules for changing array formulas](https://support.microsoft.com/en-us/excel/rules-for-changing-array-formulas).
- **KA13:** [Dynamic array formulas and spilled array behavior](https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior).
- **KA14:** [Move or copy a formula](https://support.microsoft.com/en-us/excel/move-or-copy-a-formula-in-excel).
- **KA15:** [Paste options](https://support.microsoft.com/en-us/excel/paste-options).
- **KA16:** [Paste Special when copying from programs other than Excel](https://support.microsoft.com/en-us/excel/paste-special-when-copying-from-programs-other-than-excel).
- **KA17:** [Fill a formula down into adjacent cells](https://support.microsoft.com/en-us/excel/fill-a-formula-down-into-adjacent-cells).
- **KA18:** [Enter a formula](https://support.microsoft.com/en-us/excel/enter-a-formula).
- **KA19:** [Display or hide the fill handle](https://support.microsoft.com/en-us/excel/display-or-hide-the-fill-handle).
- **KA20:** [Move or copy cells, rows, and columns](https://support.microsoft.com/en-us/excel/move-or-copy-cells-rows-and-columns).
- **KA21:** [Advanced options](https://support.microsoft.com/en-us/excel/advanced-options).
- **KA22:** [Using structured references with Excel tables](https://support.microsoft.com/en-us/excel/using-structured-references-with-excel-tables).
- **KA23:** [Use a screen reader to sort or filter a table](https://support.microsoft.com/en-us/accessibility/excel/use-a-screen-reader-to-sort-or-filter-a-table-in-excel).
- **KA24:** [Use the Name Manager](https://support.microsoft.com/en-us/excel/use-the-name-manager-in-excel).
- **KA25:** [Find or replace text and numbers](https://support.microsoft.com/en-us/excel/get-started/find-or-replace-text-and-numbers-on-a-worksheet).
- **KA26:** [Format numbers as currency](https://support.microsoft.com/en-us/excel/format-numbers-as-currency-in-excel).
- **KA27:** [Freeze panes to lock rows and columns](https://support.microsoft.com/en-us/excel/get-started/freeze-panes-to-lock-rows-and-columns).
- **KA28:** [Create a PivotTable to analyze worksheet data](https://support.microsoft.com/en-us/excel/get-started/create-a-pivottable-to-analyze-worksheet-data).
- **KA29:** [Refresh PivotTable data](https://support.microsoft.com/en-us/excel/refresh-pivottable-data).
- **KA30:** [Keyboard shortcuts for working with shapes, text boxes, and WordArt](https://support.microsoft.com/en-us/office/keyboard-shortcuts-for-working-with-shapes-text-boxes-and-wordart).
- **KA31:** [Run a macro in Excel](https://support.microsoft.com/en-us/excel/run-a-macro-in-excel).
- **KA32:** [Using Flash Fill in Excel](https://support.microsoft.com/en-au/excel/using-flash-fill-in-excel).
- **KA33:** [Display the relationships between formulas and cells](https://support.microsoft.com/en-us/excel/display-the-relationships-between-formulas-and-cells).
