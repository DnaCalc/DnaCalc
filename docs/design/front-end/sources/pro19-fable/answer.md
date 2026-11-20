# DNA Calc front end: one recommended design

Answer to Pro 19 (brief of 2026-11-20). One design, decided. Real alternatives get one line each.

**Reading aids.** `S3.4` is step 4 of storyboard 3. `M3.2` is a mockup. `R-12` is a requirement (Appendix B), `C-7` a need from the system underneath (Appendix C), `A-31` a Classic conformance item (Appendix A), `P1`–`P5` the principles of section 1. The same mockups, in Strand's colours, are in `mockups.html`.

**Size.** Sections 1 to 11 hold about 13,600 words and 590 lines of wireframe: some 32 pages at 600 words or 66 lines to the page, against the 20 asked for. The storyboards account for 15 of them, against a floor of 10: I kept every step a test script needs and a mockup at each key moment. Section 4 runs to four and a half pages because it carries seven full-screen mockups. The other sections are within their budgets or a fraction over. The appendices are extra.

**Wireframe conventions.** They are also the design's non-colour cues.

| In a wireframe | Means | In colour |
|---|---|---|
| `▐ … ▌` | the selection; "now" | amber outline |
| a solid line, a plain name | stated in the file | ink, teal |
| `‹label›`, `- - -`, `¦` | **inferred** by the program | dashed teal |
| `· · ·`, `:` | a **ghost**: proposed or previewed, not in the file | dotted, casual face |
| `▌` before an error value | an error | red tick on the cell's left edge |
| `◊` | fed from outside, or volatile | signal orange ink |
| `•` after a value | fresh: changed in the last recalculation | green underline that fades |
| `[!]` | a warning; warnings have no colour | bold ink, hollow triangle |
| `(1)` | a callout, explained underneath | not on screen |

**Cast contents**, so that numbers agree between mockups. *Budget* 2027, Base: revenue 3,940,602, costs 3,017,387, operating profit 923,215; Downside: 3,255,671, 2,689,858, 565,813. *Sales:* `Report!D9` shows 1,204,318.50 where the PivotTable shows 1,251,907.25. *Loan:* 250,000 at 6.50% over 30 years, payment 1,580.17. *Stock:* names such as `Store.Stock` and `Supply.ReorderPoint`, iterating one pass per recalculation. *Inherited* (`Inherited.xls`): 40 sheets, 301,482 formulas, `Curves` packed `A1:IV900`, 2 of 5 linked files missing, `Module1`, 4,226 errors from 3 roots, manual calculation.

---

## 1. Design philosophy

**What "modern on the surface" means here.** Not new paint on a grid, fewer features, or animation. It means four things Excel's surface does not do:

1. **Structure is visible.** Blocks, runs of like formulas, names, tables and flows are read from the file and shown as things to point at.
2. **State is visible.** Calculation, errors and their roots, values not yet recalculated, outside data and who changed what are standing instruments, not dialogs.
3. **Consequences come before commitment.** Every edit shows what it will write to the file before Enter (the *preview seam*). Esc is an exact revert.
4. **The reader picks the surface.** Five views show one workbook to five kinds of reader; the file stays plain Excel and the grid is one key away.

**How the four qualities are weighed.** Honesty is not weighed; it is a constraint. No view shows a value, formula, dependency or state that the file, calculated as Excel calculates it, does not support. A view that simplifies says so in place, with counts. A view that cannot be truthful declines and offers a way forward. The other three trade off by view:

| | Classic | Inspector | Notebook | Model map | App |
|---|---|---|---|---|---|
| First | familiarity | clarity | clarity | clarity | clarity |
| Second | density | density | familiarity | density | familiarity with the task |
| Third | clarity | familiarity | density | familiarity | density |

Outside Classic, familiarity means Excel's formula language, editing keys and names for things, not its layout.

**What every view shares.**

- **One shell** (M0.1): my reading of Strand's Mast, Bridge and Strip, plus a one-row command Deck.
- **One selection, one address, one formula bar.** The address field states the selected thing in Excel terms in every view. Selecting a map box or a notebook item shows its formula in the same bar, where F2 edits it with every Classic key.
- **One edit grammar.** Type to replace, F2 to edit, Enter to commit, Esc to revert, Tab to commit and move. SELECTED or EDITING shows in three places at once: the item, the bar, the Strip.
- **One set of signals.** Amber is now. Red is an error. Signal orange is outside or volatile data. Green is fresh. Teal is structure. Each has a shape as well. Line style is a second channel: solid for stated, dashed for inferred, dotted for a ghost.
- **One peek card, one set of panels** (Details, Trace, Changes, Issues, Outline, Watch), **one undo, one palette** (Alt+Q), **one Esc ladder** that steps back exactly one level.
- **One keyboard rule.** Excel's keys are Excel's. DNA Calc's own live on **Alt+Shift**, ten in all: `1`–`5` the views, `0` the grid alongside, `L` lens, `E` Formula Explorer, `T` Trace, `D` Details.

**M0.1 · The shell every view shares (Classic shown; Budget, `Monthly!H13` selected)**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Budget.xlsx · edited      ▐Classic▌  Inspector  Notebook  Map  App      Ask or run…  Alt+Q (1)│
│ File  Home  Insert  Layout  Formulas  Data  Review  View │ Paste▼  Fill▼  B I U  %  ,   More▼ (2)│
│ Monthly!H13     ▼ │ fx │ =H12*Inputs!$E$14+9000                                     lens off  (3)│
├──────────────────────────────────────────────────────────────────────┬───────────────────────────┤
│    │ A                │       G │         H │       I │       J │ (4)│ DETAILS                (5)│
│  4 │                  │     May │       Jun │     Jul │     Aug │    │ Monthly!H13               │
│ 11 │ Cost of sales    │  68,063 │    69,977 │  74,730 │  76,650 │    │ People · Jun              │
│ 12 │ Headcount        │      20 │        20 │      22 │      22 │    │ 137,000              #,##0│
│ 13 │ People           │ 128,000 │▐  137,000▌│ 140,800 │ 140,800 │    │ =H12*Inputs!$E$14+9000    │
│ 14 │ Marketing        │  27,844 │    28,627 │  30,572 │  31,357 │    │ reads 2 · read by 3 cells │
│ 15 │ Rent and other   │  14,500 │    14,500 │  14,500 │  14,500 │    │ in a run of 12: 11 alike, │
│ 16 │ Total costs      │ 238,406 │   250,103 │ 260,602 │ 263,307 │    │ [!] this one differs      │
│ Inputs │ Monthly │ Summary │ +                                    (6)│ Trace  Explore  Show in▼  │
├──────────────────────────────────────────────────────────────────────┴───────────────────────────┤
│  Ready · Calculated · No errors · 1 exception noted · Agent idle          Sum 137,000   100%  (7)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) **Mast.** The file and its state, the five views, and one field for commands, addresses and requests to the agent.  
(2) **Deck.** One row. Tab names and key-tip letters are Excel's; the look is ours.  
(3) **Bridge.** Address field and formula bar, in every view; folded away by default only in App.  
(4) **Stage.** The view, on paper, with the workbook's one selection.  
(5) **Panel.** Docks right or below; the same panels in every view. Details is the peek card, pinned.  
(6) **Sheet tabs** in Classic; sections, pages or levels elsewhere.  
(7) **Strip.** Mode, calculation, errors, outside data, macros, agent, statistics. Never hidden.

**Five principles that decide hard cases.** When two collide, the earlier wins.

- **P1. Truth before polish.** Simplify only visibly; decline rather than fake. *Decided:* a delay is drawn on a loop only where the system reports a last-pass read (S4.4). An App tile in error shows the error and its cause, not a blank (S5.5). A run of twelve with one odd cell reads "11 alike, 1 differs" (S2.4). Saved values are labelled "as saved" (S1.2).
- **P2. One workbook, one pointer.** Selection, address, clipboard and undo belong to the workbook, not to a view. *Decided:* switching view never loses the place (S3.6). Ctrl+[ and Ctrl+] mean upstream and downstream everywhere.
- **P3. Every edit is an Excel edit, previewed.** A gesture that changes the model is shown first as the cells, names or settings it will write, is something a person could do in Excel, and undoes as one step. Anything else is presentation, and free, or is refused. *Decided:* sorting a notebook frame is a labelled view sort (S2.6). Prose may quote a value, not compute one (S2.3). A slider commits on release, because under iteration each recalculation is a tick (S4.6). App scenarios are Excel scenarios (S5.4).
- **P4. Read the file first, ask second, always allow correction.** The program infers blocks, labels, inputs and patterns, shows its evidence and its confidence, and keeps corrections with the file. Nothing must be declared before a view works. *Decided:* the notebook drafts itself (S2.1). The App finds its inputs from unlocked and validated cells (S5.1).
- **P5. Old hands lose nothing.** In Classic every gesture of Appendix A does what it does in Excel; additions sit on one key layer and in visible controls. *Decided:* Ctrl+\` keeps its Excel meaning and the lens gets Alt+Shift+L. Alt+Q, Excel's search key, opens the palette. Key-tip paths follow Excel's letters. No skin may lock the grid away.

**Where I refine Strand v0.1.**

1. **Rename Strand's "Inspector" panel to Details.** Inspector is now a view.
2. **The paper belongs to the author.** In Classic a cell's own formats are drawn as Excel draws them, red included. Strand's signals live in gutters, edges, overlays and chrome. Provenance tints are an overlay in Classic and the default dress elsewhere.
3. **Line style is a truth channel** (solid, dashed, dotted), added to tenet 2's single border treatment.
4. **Warnings have no hue:** bold ink and a hollow triangle. Saved or stale values get a hatch, not a tint.
5. **Themes inside skins.** A skin may carry a theme, but a theme is only token values, must pass the contrast tests, and cannot re-bind a signal (section 7).
6. **The ripple is capped** at 400 ms in all, stepping by block beyond 200 items. **The copy outline does not march.**
7. **The agent is not a hue.** Its marks are teal, dotted and badged; never amber.

---

## 2. The experience, walked through

Seven storyboards. Steps are numbered so that they can be run as test scripts (section 9) and cited by requirements (Appendix B). Keys are exact. Times are the budgets of section 6, not measurements. A table after the seventh shows where each make-or-break moment is covered.

### Storyboard 1. Maria opens Inherited for the first time and finds her way around

*Start: DNA Calc has never run on this machine. `Inherited.xls` came by email.*

**S1.1 First run.** The start page offers three verbs and one promise (M1.1). **Ctrl+O**, `Inherited.xls`, **Enter**.

**M1.1 · First run: the start page**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ DNA Calc                                                                    Ask or run…  Alt+Q│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│     Open a workbook         Ctrl+O         or drop a file anywhere on this window          (1)   │
│     New workbook            Ctrl+N                                                               │
│     Formula Explorer        Alt+Shift+E    try one formula, no workbook needed                   │
│                                                                                                  │
│     Recent                  nothing yet. Workbooks you open are listed here.                     │
│                                                                                                  │
│     Your file stays an Excel file. Nothing is added that Excel cannot open.                 (2)  │
│     Everything you know from Excel works in Classic. What is new is on Alt+Shift and Alt+Q.  (3) │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Ready · no workbook open                                                                         │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) Three verbs with their keys. A dropped file opens as Ctrl+O would.  
(2) The promise, once, where a cautious Excel user looks first.  
(3) The rule for finding what is new. It is repeated when Alt is pressed (M1.3).

**S1.2 Loading.** The file name and a sheet count are in the Strip within 100 ms. Within two seconds, the budget for a file this size, the sheet that was active when the file was saved is painted from the values stored in it, and she can scroll and select (M1.2). No dialog asks about links or macros: one line states three facts. The file is set to manual calculation, so nothing recalculates, as in Excel; here a hatched band says so. A screen reader hears "Inherited.xls opened. 40 sheets. Calculation manual. 3 notices. Overview, F6."

**M1.2 · Inherited at first paint: values as saved, still loading, nothing blocking**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Inherited.xls · unchanged      ▐Classic▌  Inspector  Notebook  Map  App     Ask or run…  Alt+Q│
│ File  Home  Insert  Layout  Formulas  Data  Review  View │ Paste▼  Fill▼  B I U  %  ,       More▼│
│ Summary!A1      ▼ │ fx │ Desk P&L                                                        lens off│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  [!] 3 things to know: 2 linked files missing · macros off · calculation manual   Overview ►  (1)│
├──────────────────────────────────────────────────────────────┬───────────────────────────────────┤
│ ░░ as saved 14 Nov 2001 · not recalculated here · F9 ░░ (3)  │ OVERVIEW                       (2)│
│    │ A          │        B │         C │        D │       E  │ Inherited.xls · Excel 97-2003     │
│  1 │▐Desk P&L  ▌│          │ 14-Nov-01 │          │          │ 9.4 MB · saved 14 Nov 2001        │
│  3 │ Desk       │ Position │       MTM │  P&L day │ P&L MTD  │ ────────────────────────────────  │
│  4 │ Gas - East │   12,450 │ 1,204,880 │   ▌#REF! │  ▌#REF!  │ Sheets   40: 31 shown, 8 hidden,  │
│  5 │ Gas - West │    8,300 │   911,402 │   ▌#REF! │  ▌#REF!  │          1 very hidden           ►│
│  6 │ Power      │   -3,150 │  -402,117 │   ▌#REF! │  ▌#REF!  │ Formulas 88,210 so far…           │
│  7 │ Crude      │    1,900 │   233,640 │    ▌#N/A │   ▌#N/A  │ Errors   ▌1,926 so far…           │
│  8 │ Total      │   19,500 │ 1,947,805 │   ▌#REF! │  ▌#REF!  │ Links    5 files · ◊2 missing    ►│
│  9 │ Margin %   │          │           │ ▌#DIV/0! │     (4)  │ Names    61 · 4 use OFFSET       ►│
│ 10 │            │          │           │          │          │ Macros   Module1, 2 procedures,   │
│ 11 │  [ Refresh ]   [ Print reports ]   (5)                  │          off for this file       ►│
│ 12 │            │          │           │          │          │ Calc     manual · as saved       ►│
│ Summary │ Positions │ Curves │ Deals │ ◄ 27 more ►           │ ↑↓ move · Enter go · Alt+← back   │
├──────────────────────────────────────────────────────────────┴───────────────────────────────────┤
│  Ready · Reading sheets 9 of 40 ▓▓░░░░░░ · Manual: as saved · ◊2 links missing · Macros off   (6)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) One notice line where Excel raises three dialogs. It blocks nothing; Enter opens the Overview.  
(2) The Overview opens by itself for a file with notices. Every line is a link; counts fill in and say "so far".  
(3) The hatched band: values saved in the file, not recalculated here.  
(4) Errors keep Excel's text and gain a red tick. The cell's own format is untouched.  
(5) The file's buttons are drawn; with macros off they explain themselves (S1.6).  
(6) Progress is in the Strip. The grid is already usable.

**S1.3 Discovery by habit.** **Alt**, to look for Excel's menus. Excel's key-tip letters appear on a Deck that looks nothing like a ribbon, with one line that names the new layer (M1.3). **Esc**.

**M1.3 · Pressing Alt: Excel's key tips, and the one new layer taught in place**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Inherited.xls       Classic[1] Inspector[2] Notebook[3] Map[4] App[5]      Ask or run [Q]  (2)│
│ [F]File [H]Home [N]Insert [P]Layout [M]Formulas [A]Data [R]Review [W]View                     (1)│
│ Excel's letters after Alt.  Alt+Shift: 1-5 views · 0 grid · L lens · E explore · T trace      (3)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) Excel's letters. A path typed from memory runs without waiting for the tips.  
(2) The digits on the view names go with Alt+Shift. Q is Excel's search key and opens the palette.  
(3) One line teaches the new layer, on the first five uses of Alt.

**S1.4 Orientation.** **F6** to the Overview, **↓** to *Errors*, **Enter**: three roots, not 4,226 cells. `'P&L by Desk'!C6:R6` names a deleted sheet (1,912 `#REF!`). `Deals!H2:H24` looks up keys missing from `Lookup` (2,300 `#N/A`). `Summary!D9` divides by an empty cell (14 `#DIV/0!`). **Enter** on the first goes to it: `=#REF!$C$4*Positions!D7`. **Alt+←** returns her; in the browser, so does Back. **Ctrl+C** on the roots pastes into an email as three lines with addresses.

**S1.5 The big picture.** **Alt+Shift+4** shows the map within 150 ms, her selection outlined in amber (M1.4). **Ctrl+[** lights the wires upstream of Summary, one hop a press. Resting on `Curves → Positions` shows the peek card: "8,412 references, for example `Positions!F7 =VLOOKUP($B7,Curves!$A$4:$IV$900,F$2,FALSE)`". **Enter** on *Curves* looks inside: 229,245 formulas as 6 blocks, never as cells. **Esc** steps out. **Space** on a hidden sheet peeks at it under a band, "Hidden in the file. You are looking, not unhiding." **Alt+Shift+1** returns to the cell she left.

**M1.4 · Model map at sheet zoom (Alt+Shift+4): how forty sheets hang together**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Inherited.xls · unchanged      Classic  Inspector  Notebook  ▐Map▌  App     Ask or run…  Alt+Q│
│ Summary (sheet) ▼ │ fx │ 212 formulas · reads P&L by Desk, Jan…Dec · read by nothing     lens off│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  Inherited.xls ► all sheets                   zoom:  sheets · blocks · cells                  (1)│
│  ┌──────────────────────────────┐                                                                │
│  │ ◊ Forward Curves 2001.xls    │   ┌────────────────────────────┐                               │
│  │ ◊ Rates.xls                  │   │ ◊ Q3 Positions.xls MISSING │  (2)                          │
│  │ ◊ GasDaily Index.xls MISSING │   └────────────────────────────┘                               │
│  └──────────────────────────────┘         ¦                                                      │
│                    │ 2 found · 1 missing  ¦ 310 · values as saved                                │
│                    ▼                      ▼                                                      │
│             ┌─────────────┐ 8,412  ┌─────────────┐ 1,204  ┌───────────────┐ 96   ┌─────────────┐ │
│             │ Curves      │───────►│ Positions   │───────►│ P&L by Desk   │─────►│▐ Summary   ▌│ │
│             │ 229,245 fx  │        │ 31,880 fx   │        │ 9,604 fx      │      │ 212 fx      │ │
│             │ packed   (3)│        │             │        │ ▌1,912 #REF!  │      │ ▌14 #DIV/0! │ │
│             └─────────────┘        └─────────────┘        └───────────────┘      └─────────────┘ │
│                                           ▲                                             ▲        │
│                                           │ 4,180                                       │ 144    │
│             ┌─────────────┐  23    ┌─────────────┐                              ┌────────────┐┐┐ │
│             │ Lookup      │───────►│ Deals       │                              │ Jan … Dec  │││ │
│             │ 40 fx       │        │ 18,912 fx   │                              │ 12 alike   │││ │
│             └─────────────┘        │ ▌2,300 #N/A │                              └────────────┘┘┘ │
│                    ▲               └─────────────┘                                 (4)           │
│            ◊ Desk Map.xls                                                                        │
│                                                                                                  │
│  ┌- - - - - - - - - - - -┐  ┌- - - - - - - - - - -┐  ┌─────────────────────────────┐             │
│  ¦ 8 hidden sheets      ►¦  ¦ 1 very hidden     ► ¦  │ 12 sheets nothing reads    ►│  (5)        │
│  └- - - - - - - - - - - -┘  └- - - - - - - - - - -┘  └─────────────────────────────┘             │
│  40 of 40 sheets accounted for · 62 links in 9 bundles · cells are not drawn at this zoom     (6)│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Ready · Manual: as saved · ▌4,226 errors, 3 roots · ◊2 links missing · Macros off        Map 100%│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) Breadcrumb and semantic zoom. Enter looks inside a box; Esc steps out. Text never shrinks.  
(2) A missing file: signal orange, a dashed wire, and the fact that saved values are in use.  
(3) Standing facts on each box: formulas, "packed", errors.  
(4) Twelve sheets whose formulas match cell for cell are one stack.  
(5) Hidden sheets and sheets nothing reads are counted, never left out.  
(6) The count that keeps the picture honest.

**S1.6 A refusal.** She clicks **Refresh**. Nothing runs. A card at the button says: "Macros are off for this file. This button runs `Module1.RefreshAll` (41 lines): it opens 2 workbooks, copies 6 ranges and saves. **Read the code** · **Allow macros this session**". **Esc**.

**S1.7 A slip, and recovery.** **Delete** on `Summary!A1` by mistake; the Mast says "edited". **Ctrl+Z**: the text is back and the Mast says "unchanged", because the workbook is again identical to the file.

**S1.8 Calculating.** **F9**. The Strip turns amber: "Calculating 38% · Esc stops". The grid stays usable; cells still waiting are hatched. Then: "Calculated in 2.3 s · 38 values differ from those saved · ◊2 links use saved values". The middle part is a link to the list.

**S1.9 Zoom and contrast.** **Ctrl+wheel** zooms the stage. On `Curves`, below 40% cell text gives way to block outlines with labels. In the high-contrast theme the tick, the `◊` and the hatch remain, because each is a shape.

*What an Excel user notices, and why it is a win.* No dialogs at opening. "As saved" is stated, so she knows these numbers are from 2001. Three things to fix, not 4,226. Hidden sheets are counted and can be seen without changing the file. Undo back to the start really is "unchanged".

### Storyboard 2. Maria turns Budget into a notebook write-up for her director

*Start: `Budget.xlsx` in Classic, `Monthly!C9` selected. Her director wants "the story, not the spreadsheet".*

**S2.1 Enter the notebook.** **Alt+Shift+3**. Budget has no notebook yet, so the stage offers a draft (M2.1). **Enter**. Within 400 ms there are three sections and nine items. Her selection came with her: *Total revenue* is outlined, *Jan* underlined, and the Strip says where it landed. The Mast still says "saved".

**M2.1 · Notebook, first visit: the empty state is an offer drawn from the file**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Budget.xlsx · saved         Classic  Inspector  ▐Notebook▌  Map  App        Ask or run…  Alt+Q│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│     Budget has no notebook yet.  Draft one from the workbook?                            (1)     │
│                                                                                                  │
│     ▐ ● By sheet, in order   ▌   Inputs → Monthly → Summary         3 sections · 9 items         │
│        ○ Conclusions first       Summary, then what feeds it        3 sections · 9 items         │
│        ○ Blank page              start with text; add items with Alt, N                          │
│                                                                                                  │
│     Read from the file: 1 inputs block (12 assumptions × 2 scenarios) · 2 calculation blocks,    │
│     12 periods across · 1 summary · 1 chart · no tables.       Why these? ►               (2)    │
│                                                                                                  │
│     Drafting changes nothing in the workbook.     Enter drafts · ↑↓ choose · Esc stays blank (3) │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Ready · Calculated · No errors          Monthly!C9 will be shown in Revenue › Total revenue › Jan│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) A first visit is an offer built from what the program read, never a blank page (P4).  
(2) The reading in plain words, with its evidence one step away.  
(3) The seam, stated: drafting is presentation. The Strip already says where the selection will land.

**S2.2 Reorder.** **F6** to the Outline, **↓** to *Summary*, **Alt+↑** twice; with the mouse she would drag its handle. The Mast reads "saved, layout edited": the model is untouched, and the state says so.

**S2.3 Write.** **Alt, N, T** inserts a text item, in edit. She types "We plan operating profit of ", then **=**, and clicks the figure in the Summary frame: the sentence carries a live value. For the margin she tries `=Summary!C7/Summary!C5`. It is refused in place: "Text can quote a cell, not calculate. The margin is in `Summary!C8`. **Tab** quotes it · **Add this formula to the workbook…**". **Tab**.

**S2.4 Read the formulas.** **Alt+Shift+L** turns the lens on. `=C7*Inputs!$E$10` reads `Subscriptions × ‹Services as share of subs›`; the guillemets say the label was read from the sheet and is not a name. Right-click, **Make it a name…**; the seam: "Defines `ServicesShare` and applies it to 12 formulas (Excel: Define Name, Apply Names)". **Enter**, and the chip turns plain. The folded *Costs* section reports "People: 11 alike, 1 differs (Jun)": her own one-off 9,000, typed into `H13`.

**S2.5 Edit the model from here** (M2.2). **↓** to *Services*, **F2**; she rewrites it as `=MAX(Subscriptions × ServicesShare, 2500)`. A slip, `Subscritions`, gets a dotted underline and "Tab: Subscriptions". The seam shows 12 cells, the Excel text and "no value changes". **Enter** writes; **Ctrl+Z** would take back all twelve. Then, on the input *Price rise, Base*, she types `5%`. Within 100 ms the fresh mark runs through Price, Subscriptions, the totals and her sentence, where 923,215 becomes 937,585. **Ctrl+Z**.

**M2.2 · Notebook mid-edit: one formula for a run of twelve, lens on, the seam open**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Budget.xlsx · saved, layout edited    Classic Inspector ▐Notebook▌ Map App   Ask or run… Alt+Q│
│ File Home Insert Layout … View │ Text  Value  Frame  Chart  Control │ Page · Across │ Reading    │
│ Monthly!C8:N8   ▼ │ fx │ =MAX(C7*ServicesShare,2500                                       lens on│
├────────────────────┬─────────────────────────────────────────────────────────────────────────────┤
│  OUTLINE        (6)│  Budget 2027: where the plan stands                                         │
│  Summary           │                                                                             │
│    Result          │  We plan operating profit of 923,215 on revenue of 3,940,602, a margin      │
│    Chart           │  of 23.4%. In the Downside case profit is 565,813.  (1)                     │
│  Monthly           │                                                                             │
│  ▐ Revenue        ▌│  Revenue                                   Monthly!B5:O9 · 12 periods across│
│    Costs           │   Customers      = Customers[prev] × (1 − ‹Monthly churn›) + ‹New customers›│
│    Operating profit│   Price          = ‹Price› × IF(MONTH(‹month›) >= 7, 1 + ‹Price rise›, 1)   │
│  Inputs            │   Subscriptions  = Customers × Price                                        │
│    Assumptions     │  ▐Services       = MAX(Subscriptions × ServicesShare, 2500|            ▌ (2)│
│                    │    · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·    │
│                    │    : Writes 12 cells, Monthly!C8:N8: one formula, filled across.  (3)     : │
│                    │    : Stores as  =MAX(C7*ServicesShare,2500)  in C8                        : │
│                    │    : Result     Jan 29,245 … Dec 40,904 · no value changes                : │
│                    │    : Enter writes · Esc reverts exactly                                   : │
│                    │    · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·    │
│                    │   Total revenue  = Subscriptions + Services                                 │
│                    │                                                                             │
│                    │                │     Jan │     Feb │ ◄ Mar–Nov ► │     Dec │        FY │ (4)│
│                    │  Subscriptions │ 243,706 │ 252,060 │             │ 340,864 │ 3,518,394 │    │
│                    │  Services      │  29,245 │  30,247 │             │  40,904 │   422,207 │    │
│                    │  Total revenue │ 272,951 │ 282,307 │             │ 381,767 │ 3,940,602 │    │
│                    │                                                                             │
│                    │  Costs ► 5 rows, folded           [!] People: 11 alike, 1 differs (Jun)  (5)│
├────────────────────┴─────────────────────────────────────────────────────────────────────────────┤
│  Edit · Calculated · No errors                                  Notebook · Working density · 100%│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) Prose in the reading face. The teal figures quote cells and recalculate with them.  
(2) One item for the run `C8:N8`, in edit. `ServicesShare` is plain, a defined name; `‹Monthly churn›` is a label read from the sheet.  
(3) The seam: what Enter will write, in Excel's text, and its effect on the results.  
(4) Folded columns are named by a control, not dropped.  
(5) A folded section still reports its exception (P1).  
(6) The Outline, successor of the experimental tree. Alt+↑ and Alt+↓ move an item.

**S2.6 Frames and the clipboard.** A click on the *Downside* header of the Assumptions frame sorts the view: "view sort ▲ · the sheet is not reordered". **Ctrl+C** on the Revenue frame pastes into an email as a formatted table and into Excel as cells. Paragraphs pasted from Word become text items. A pasted table asks: "As text" (Enter) or "As cells: adds 6 cells at `Summary!E12:F14` ►".

**S2.7 Reading and printing.** **Reading** on the Deck folds formulas to their names and sets prose in the serif. **Ctrl+P** previews the write-up as a document of three pages. Each page is footed with the file, the scenario and the time of the values; folded sections print as their one line.

**S2.8 Save.** **Ctrl+S**. Once: "Saved as an ordinary Excel workbook. The notebook travels inside it; Excel keeps it and does not show it." The file now prefers Notebook, so her director lands there, in Reading density. In Excel it is the workbook it always was.

**S2.9 Without sight, and at 200%.** Headings are headings. A formula item reads "Services, formula: maximum of Subscriptions times ServicesShare and 2,500. Twelve values, 29,245 to 40,904." At 200% prose reflows and frames scroll inside themselves.

*What an Excel user notices, and why it is a win.* A row of twelve formulas is one formula, so an edit cannot leave a straggler, and an existing straggler is flagged. The numbers in the prose are cells, so the write-up cannot drift from the model.

### Storyboard 3. Sam traces why one number on Sales' Report is wrong

*Start: Sam has `Sales.xlsx`, a ledger figure of 1,251,907.25 for West in the third quarter, and a doubt.*

**S3.1 Open and compare.** **Ctrl+O**. Report is painted in under a second, 25,000 rows notwithstanding. `D9` reads 1,204,318.50; the PivotTable's `D17` reads 1,251,907.25. He clicks `D17` and **Ctrl+clicks** `D9`. With exactly two numbers selected, the Strip adds "Difference 47,588.75".

**S3.2 The Excel habit.** **Ctrl+[** jumps, as in Excel, to three columns of 25,000 rows on another sheet. No help. **Alt+←** returns him, selection intact. Once a session the Strip offers: "Alt+Shift+T traces without leaving the cell."

**S3.3 Trace.** **Alt+Shift+T**. The panel docks under the grid within 200 ms, counts included (M3.1). A conditional sum is laid out as a ladder of tests. **↓** to test 3: "41 fail only this".

**M3.1 · Classic with the Trace panel below: a conditional sum explained as a ladder of rows**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Sales.xlsx · saved         ▐Classic▌  Inspector  Notebook  Map  App         Ask or run…  Alt+Q│
│ Report!D9       ▼ │ fx │ =SUMIFS(Orders[Amount],Orders[Region],$A9,                              │
│                           Orders[Date],">="&D$3,Orders[Date],"<="&D$4)                   lens off│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│    │ A           │             B │             C │             D │             E │               │
│  5 │ Region      │            Q1 │            Q2 │            Q3 │    Q4 to date │               │
│  8 │ East        │  1,301,774.35 │  1,288,460.00 │  1,342,918.65 │    701,336.20 │               │
│  9 │ West        │  1,187,092.40 │  1,229,645.15 │▐ 1,204,318.50▌│    648,907.85 │  (1)          │
│ 10 │ Total       │  4,449,384.45 │  4,596,375.85 │  4,516,998.85 │  2,418,249.10 │               │
│  ══ split ══ PivotTable1: Sum of Amount by Region and Quarter ════════════════════════           │
│ 17 │ West        │  1,201,355.90 │  1,243,880.65 │▐ 1,251,907.25▌│    648,907.85 │  (2)          │
│ 18 │ Grand Total │  4,600,624.75 │  4,756,999.10 │  4,684,065.45 │  2,418,249.10 │               │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  TRACE  Report!D9 = 1,204,318.50     beside D17 (PivotTable1) 1,251,907.25 · difference 47,588.75│
│  ────────────────────────────────────────────────────────────────────────────────────────────────│
│    sum     Orders[Amount]                           25,000 rows · all numbers                    │
│    test 1  Orders[Region] = "West"        ($A9)      6,212 pass           0 fail only this    (3)│
│    test 2  Orders[Date] >= 1-Jul-26       (D$3)      2,155 left           0 fail only this       │
│  ▐ test 3  Orders[Date] <= 30-Sep-26      (D$4)      2,114 left          41 fail only this  ▌ (4)│
│    result  sum of 2,114 amounts                     1,204,318.50                                 │
│  ────────────────────────────────────────────────────────────────────────────────────────────────│
│    [!] Hint, likely: the 41 rows carry a time of day; "<=30-Sep-26" stops at 00:00.   (5)        │
│    Same formula in 16 cells, B6:E9: 412 rows fail only the end-date test, worth 478,930.15.   (6)│
│    ↑↓ move · Enter show the rows · Alt+Shift+E explore · Alt+Shift+2 full Inspector · Esc close  │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  Ready · Calculated · No errors           Sum 2,456,225.75 · Count 2 · Difference 47,588.75   (7)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) The active cell. `D17`, the pivot's figure for the same region and quarter, is selected with it.  
(2) The PivotTable's figure, which the ledger agrees with.  
(3) The ladder: rows left after each test, and rows that fail only that test. Wrong totals hide in the second number.  
(4) The selected node. Enter opens its 41 rows (M3.2).  
(5) A hint is the program's inference, graded, never stated as fact.  
(6) The same formula runs over 16 cells, so the flaw is counted across all.  
(7) Two numbers selected: the Strip adds their difference.

**S3.4 The rows.** **Enter** shows `Orders` under a view filter: 41 rows (M3.2). Resting on a date, the peek card shows a time of day that the format was hiding. All 41 fall on 30 September. The *Amount* column sums to 47,588.75.

**M3.2 · The 41 rows, as a view filter that Excel's own filter never sees**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Sales.xlsx · saved         ▐Classic▌  Inspector  Notebook  Map  App         Ask or run…  Alt+Q│
│ Orders!B18842   ▼ │ fx │ 46295.3416666667                                                lens off│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  View filter from Trace · 41 of 25,000 rows · not Excel's AutoFilter · Esc clears             (1)│
├────────────────────────────────────────────────────────────────────┬─────────────────────────────┤
│        │ A OrderID │ B Date    │ C Region │ D Rep     │  J Amount ││ DETAILS                     │
│ 18,842 │ SO-118841 │▐30-Sep-26▌│ West     │ Okafor    │  1,240.00 ││ Orders!B18842               │
│ 18,847 │ SO-118846 │ 30-Sep-26 │ West     │ Lindqvist │    310.50 ││ Orders[@Date]               │
│ 18,851 │ SO-118850 │ 30-Sep-26 │ West     │ Okafor    │  2,975.00 ││ shown   30-Sep-26 d-mmm-yy  │
│ 18,853 │ SO-118852 │ 30-Sep-26 │ West     │ Haddad    │     88.25 ││ value   46295.341667  (2)   │
│ 18,860 │ SO-118859 │ 30-Sep-26 │ West     │ Marais    │  4,410.00 ││         = 30-Sep-26 08:12   │
│ 18,866 │ SO-118865 │ 30-Sep-26 │ West     │ Haddad    │    695.75 ││ a typed constant            │
│      … │ 35 more   │ (3)       │          │           │           ││ read by 16 formulas         │
├────────────────────────────────────────────────────────────────────┴─────────────────────────────┤
│  Ready · Calculated · View filter on                     41 shown · Sum 47,588.75 · Count 41  (4)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) The band says what is shown and what it is not. Excel's AutoFilter, and so every SUBTOTAL, is untouched (P3).  
(2) Shown against stored. The time of day was always there; the number format hid it.  
(3) Row numbers under a view filter are teal and gapped, and the rows not shown are counted.  
(4) The 41 amounts sum to the difference Sam started with.

**S3.5 Confirm without touching the file.** **Alt+←**, then **Alt+Shift+E**: the Formula Explorer opens on `D9` with its inputs filled in (M8.6). On the trial line he changes test 3 to `"<"&E$3`. The result reads 1,251,907.25 under "Trying. Workbook unchanged." **Esc**.

**S3.6 More room.** **Alt+Shift+2**: the Inspector takes the stage with the same trace open at the same node. **Alt+Shift+1**: Classic again, `D9` selected, nothing scrolled.

**S3.7 A slip, caught before it costs.** On `D9` he starts typing a note to himself: `che`. Under the cell the seam warns before any commit: "Replaces a formula with text. Breaks a run of 16 alike." **Esc** reverts exactly.

**S3.8 Record and report.** **Ctrl+Shift+F2**, Excel's comment key: the finding goes into the file as an ordinary Excel comment. **Ctrl+C** in the Trace panel pastes into Word as an outline, with the 41 rows as a table.

**S3.9 Is the pivot sound?** He selects it. Details reads "refreshed 19 Nov 2026 16:02 · source unchanged since".

**S3.10 Without sight.** The ladder is a tree: "Test 3 of 3. Orders Date less than or equal to 30 September 2026. 2,114 rows left. 41 fail only this test."

*What an Excel user notices, and why it is a win.* Excel's Evaluate Formula reduces this SUMIFS to its total in one step. Here the function explains itself in rows, the format's disguise is lifted, and the fix is tried without editing a reviewer's copy.

### Storyboard 4. Lee builds Stock on the model map, then checks it in Classic

*Start: Lee has a whiteboard sketch: a stock, a demand, a reorder rule with a delay. No workbook yet.*

**S4.1 An empty map.** **Ctrl+N**, **Alt+Shift+4**. The empty map shows one example and one fact (M4.1).

**S4.2 First boxes.** He types `Store.Initial = 400`. The seam lists a label cell, a value cell and a name. **Enter**. Then `Demand = 100`, `Supply.OrderQty = 420` and `Supply.ReorderPoint = 260`. Two dashed frames, *Store* and *Supply*, draw themselves from the prefixes; dashed, because grouping is the program's reading.

**M4.1 · The empty map: the first box is typed, and the seam says where it will live**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Book1 · new             Classic  Inspector  Notebook  ▐Map▌  App            Ask or run…  Alt+Q│
│ (new box)       ▼ │ fx │ Store.Initial = 400                                              lens on│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  Book1 ► Sheet1                                                    zoom:  sheets · blocks · cells│
│       Nothing to map yet. Type a name and a formula, for example  Demand = 100           (1)     │
│       A box is a named cell on a sheet. Excel will see every one of them.                        │
│                                                                                                  │
│            ▐ Store.Initial = 400|                                      ▌              (2)        │
│            · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·                     │
│            : Writes   Sheet1!A2   Initial        (the label)            :          (3)           │
│            :          Sheet1!B2   400                                   :                        │
│            : Names    Store.Initial   =Sheet1!$B$2                      :                        │
│            : Group    Store, from the name's prefix                     :                        │
│            : Enter writes · Esc reverts · place it elsewhere…           :                        │
│            · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·                     │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Enter · Calculated · No errors                                                 Map · cells · 100%│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) One gesture and one fact: boxes are named cells, not a private format.  
(2) Typing on empty paper starts a box, as typing on a cell starts an entry. `name = formula` is the whole grammar.  
(3) The placement rule at work: a label cell, a value cell and a name, on a sheet he can see, changeable before commit.

**S4.3 Sketch first, define later.** `Store.Sales = MIN(Demand, Stock + Supply.Arrivals)`. Inside *Store*, `Stock` means `Store.Stock`. Neither it nor `Supply.Arrivals` exists, and both get a dotted underline. **Enter** offers "Create empty boxes for both" or "Commit as typed: Excel shows `#NAME?`". He takes the first.

**S4.4 Wiring by pointing, and the loop** (M4.2). On the empty *Stock* box he types `= Stock + `, clicks *Arrivals*, types ` - `, clicks *Sales*. Each click inserts a reference and draws a ghost wire. **Enter** raises a choice at the box, not a dialog, and he turns iteration on. The frame gains the chip "iterates · 1 pass per recalculation". Stock's own loop gets a delay mark, `[z-1]`: a cell that reads itself reads its previous value. Between *Sales* and *Stock*, a delay is marked only on the wire the system reports as a last-pass read (C-21). If it cannot say, the loop is drawn and no delay is claimed.

**M4.2 · Pointing closes a loop: ghost wires, and the choice Excel hides in a dialog**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Book1 · edited           Classic  Inspector  Notebook  ▐Map▌  App           Ask or run…  Alt+Q│
│ Store.Stock     ▼ │ fx │ = Stock + Supply.Arrivals - Sales                                lens on│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  Book1 ► Sheet1                                                    zoom:  sheets · blocks · cells│
│                                      ┌- - - - - - - - - - - - - - - - - - - - - - - - - - - - -┐ │
│   ┌────────────────┐                 ¦ Store                   ┌─────────────────────────────┐ ¦ │
│   │ Demand         │──────────────────────────────────────────►│ Sales                     0 │ ¦ │
│   │ 100            │                 ¦                         │ = MIN(Demand, Stock +       │ ¦ │
│   └────────────────┘               ┌──────────────────────────►│ Supply.Arrivals)            │ ¦ │
│                                    │ ¦                         └─────────────────────────────┘ ¦ │
│                                    │ ¦                             :             ▲             ¦ │
│  ┌- - - - - - - - - - - - - - - -┐ │ ¦    · · [z-1] · ·  (3)       : (2)     (1) │             ¦ │
│  ¦ Supply                        ¦ │ ¦    ▼           :            ▼             │             ¦ │
│  ¦ ┌───────────────────────────┐ ¦ │ ¦  ┌──────────────────────────────────────────┐           ¦ │
│  ¦ │ ReorderPoint        260   │ ¦ │ ¦  │▐Stock = Stock + Supply.Arrivals - Sales|▌│           ¦ │
│  ¦ │ 0 ├──────■────────┤ 600   │ ¦ │ ¦  └──────────────────────────────────────────┘           ¦ │
│  ¦ └───────────────────────────┘ ¦ │ ¦           ▲                                             ¦ │
│  ¦ ┌────────────┐ ┌────────────┐ ¦ │ ¦           :             ┌────────────────┐              ¦ │
│  ¦ │ OrderQty   │ │ Arrivals   │· ·│· · · · · · ·              │ Initial        │              ¦ │
│  ¦ │ 420        │ │ (empty)    │───┘ ¦                         │ 400            │              ¦ │
│  ¦ └────────────┘ └────────────┘ ¦   ¦                         └────────────────┘              ¦ │
│  ¦                               ¦   ¦                                                         ¦ │
│  └- - - - - - - - - - - - - - - -┘   └- - - - - - - - - - - - - - - - - - - - - - - - - - - - -┘ │
│     ┌──────────────────────────────────────────────────────────────────────────────────────┐     │
│     │ [!] This makes a loop: Stock reads itself; Stock and Sales read each other.  (4)     │     │
│     │ Excel calculates a loop only with iterative calculation on.                          │     │
│     │ ▐ Turn on iteration, 1 pass per recalculation ▌ Enter   Iterative on · Max 1 · 0.001 │     │
│     │   Keep without iteration: 0 and a circular warning       (Excel: Options › Formulas) │     │
│     │   Back to editing   Esc                                                              │     │
│     └──────────────────────────────────────────────────────────────────────────────────────┘     │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Point · Calculated · [!] would be circular                                     Map · cells · 100%│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) A solid wire: `Sales` already reads `Stock`. Wires are drawn from formulas and from nothing else.  
(2) Ghost wires show the formula being typed. Both references were inserted by clicking boxes.  
(3) A cell that reads itself reads its previous value, so this delay mark is certain.  
(4) The loop is named before commit. All three answers are things Excel can do, and the first states the settings it writes.

**S4.5 The rest, and a rewire.** He adds `Supply.Order`, the delay steps `Supply.InTransit` and `Supply.Arrivals`, a `Shop.Tick` and a `Shop.Reset`. Dragging the source end of *Demand → Sales* onto *Order* shows "In `Store.Sales`: `Demand` becomes `Supply.Order`", with both formulas. **Esc**; nothing changed.

**S4.6 The slider.** On *ReorderPoint*: **Alt, N**, *Control › Slider*. The seam: "Adds an Excel scroll bar at `Model!C3`, linked to `$B$3`, 0 to 600, step 10." While dragged it shows a ghost value. On release the cell is written once and the workbook recalculates once, which under iteration is one tick, as in Excel.

**S4.7 Run.** **F9** is one tick: amber runs through the boxes in evaluation order and the values settle with the fresh mark. The chip offers "Repeat F9, 4 per second". He pins *Stock* to Watch and sees the sawtooth (M8.2).

**S4.8 Check in Classic** (M4.3). **Alt+Shift+0** puts the grid alongside: `Model!B11` is selected and the formula bar shows what Excel will see. He wraps the balance in `MAX(0, …)` there; the box follows. **Alt+Shift+1** gives the grid the whole stage, and **Ctrl+\`** shows every formula, as in Excel.

**M4.3 · Alt+Shift+0: the grid alongside the map, one selection in both**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Stock.xlsx · saved      Classic  Inspector  Notebook  ▐Map▌ + grid  App     Ask or run…  Alt+Q│
│ Model!B11       ▼ │ fx │ =IF(Shop.Reset,Store.Initial,Store.Stock+Supply.Arrivals-Store.Sales)   │
├─────────────────────────────────────────┬────────────────────────────────────────────────────────┤
│  Stock.xlsx ► Model ► Store             │    │ A              │       B │ C                      │
│  ┌─────────────────┐                    │  2 │ Demand         │     100 │                        │
│  │ Sales        100│◄── Demand          │  3 │ Reorder point  │     260 │ 0 ├───■────┤ 600 (3)   │
│  └─────────────────┘                    │  4 │ Order quantity │     420 │                        │
│        ▲    │                           │  5 │ Opening stock  │     400 │                        │
│        │    ▼                           │  6 │ Reset          │   FALSE │                        │
│  ┌─────────────────┐◄─[z-1]─┐  (1)      │  7 │ Tick           │      25 │                        │
│  │▐Stock       420▌│────────┘           │  8 │ Arrivals       │     420 │                        │
│  └─────────────────┘◄── Arrivals 420    │  9 │ In transit     │       0 │                        │
│          │                              │ 10 │ Sales          │     100 │                        │
│          ▼                              │ 11 │ Stock          │▐    420▌│ (4)                    │
│  ┌─────────────────┐                    │ 12 │ Order          │       0 │                        │
│  │ Order          0│──► In transit      │    │                │         │                        │
│  └─────────────────┘                    │ Model │ +                                              │
│  iterates · 1 pass · tick 25  (2)       │                                                        │
├─────────────────────────────────────────┴────────────────────────────────────────────────────────┤
│  Ready · Iterates, 1 pass · tick 25 · F9 steps · No errors                 Map + grid · 100%  (5)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) The delay mark is solid now: the formula is in the file and iteration is on.  
(2) The frame's chip states how this part calculates, in Excel's terms.  
(3) The slider is an Excel form control on the sheet, linked to `B3`.  
(4) One selection: the box and the cell are the same thing, and typing in either edits the same formula.  
(5) The Strip states the calculation mode all the time. Excel shows only "Calculate".

**S4.9 Undo, both kinds.** In Classic, **Ctrl+Z** takes back the `MAX`. On the map, after dragging boxes to tidy, **Ctrl+Z** takes back the last drag: "Undid: moved Store.Stock (layout)". In Classic the same key would have passed over it, because layout cannot be seen from there.

**S4.10 Save, and the Excel test.** **Ctrl+S**: "Names, formulas, the iteration setting and the slider are standard Excel. The map's layout travels in the file; Excel ignores it." In Excel, F9 steps the same model to the same numbers.

**S4.11 Keyboard, screen reader, clipboard.** Arrow keys move by position; **Ctrl+]** and **Ctrl+[** follow wires; **Enter** looks inside a frame and **Esc** steps out. A box reads "Store.Stock. State. 420. Reads itself one pass back, Supply.Arrivals, Store.Sales. Read by Store.Sales, Supply.Order." **Ctrl+C** on a frame pastes into a slide as a picture and into a text editor as `name = formula` lines.

*What an Excel user notices, and why it is a win.* A circular reference is a decision with its settings on the picture, not an accident behind a dialog. What he drew is a sheet that anyone with Excel can read.

### Storyboard 5. Jo runs Loan as an App, tries scenarios and prints the result

*Start: Jo manages a branch and does not use Excel. Maria sent her `Loan.xlsx`, set to open as an App (section 7 shows how that App was made).*

**S5.1 Open.** The file prefers App, so it opens there, painted from saved values within a second. The Strip says "Checking the numbers" for 300 ms, then "Up to date". There is no grid and no formula bar. Once: "White fields are yours to change. Everything else is worked out. Nothing is saved until you save." The inputs are the workbook's unlocked cells with their labels, and their limits are its validation rules.

**S5.2 Change a number.** **Tab** visits the white fields only. `300000`, **Enter**. Within 100 ms the outputs, the chart and the schedule change and carry the fresh mark; "was 1,580.17" appears under the payment.

**S5.3 A refusal** (M5.1). In *Annual rate* she types `65`, **Enter**. The workbook's rule refuses it, in place and with a way out. **Esc**: 6.50% again, exactly.

**M5.1 · Loan as an App: one input changed, one refused in place**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Loan.xlsx · edited          ▐App▌   View as ▼         Print  Ctrl+P    Ask or run…  Alt+Q  (1)│
│ Scenario: Base, edited  15 years  Rate +1 pt │ Compare with: none ▼ │ Reset  Save as scenario…   │
├─────────────────────────────────────┬────────────────────────────────────────────────────────────┤
│  YOUR LOAN                       (2)│  MONTHLY PAYMENT       TOTAL INTEREST        PAID OFF      │
│                                     │  1,896.20•             382,633.47•           Dec 2056   (4)│
│  Loan amount   [    300,000.00 ]•   │  was 1,580.17          was 318,861.22                      │
│  Annual rate  ▐[ 65|           ]▌   │                                                            │
│   ┌──────────────────────────────┐  │  Balance over time       chart "Balance", from the workbook│
│   │ The rate must be between     │  │  300k ┤ █                                                  │
│   │ 0% and 25%. You typed 65.    │  │  240k ┤ █    ▄                                             │
│   │ Tab  use 6.5%                │  │  180k ┤ █    █    █    ▄                                   │
│   │ Esc  keep 6.50%          (3) │  │  120k ┤ █    █    █    █    █                              │
│   └──────────────────────────────┘  │   60k ┤ █    █    █    █    █    ▄                         │
│  Term (years)  [            30 ]    │     0 ┼─█────█────█────█────█────█────_──                  │
│  Start date    [      1-Jan-27 ]    │         2027 2032 2037 2042 2047 2052 2057                 │
│                                     │                                                            │
│  White fields are yours to          │  Schedule             360 payments · showing 3 · all ►  (5)│
│  change. Everything else is         │   #   Date        Payment   Interest Principal     Balance │
│  worked out. Nothing is saved       │   1   1-Jan-27   1,896.20   1,625.00    271.20  299,728.80•│
│  until you save.                    │   2   1-Feb-27   1,896.20   1,623.53    272.67  299,456.12•│
│                                     │   3   1-Mar-27   1,896.20   1,622.05    274.15  299,181.97•│
├─────────────────────────────────────┴────────────────────────────────────────────────────────────┤
│  Edit · Up to date · No errors · 1 input changed since opening         App · Reading density  (6)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) App's Mast: file, view, "View as" to every other view, Print, Ask. F6 or Alt brings back the Deck and Bridge.  
(2) Inputs are found, not declared: the workbook's unlocked cells with their labels.  
(3) A refusal in place, from the workbook's own rule and message: what was typed, the likely fix on Tab, the exact revert on Esc.  
(4) The fresh mark shows what moved; "was" keeps the opening value in view.  
(5) A frame: 3 of 360 rows, the count stated.  
(6) The Strip is still there, in plain words.

**S5.4 Scenarios** (M5.2). The chips are the scenarios Maria saved with Excel's Scenario Manager. Jo clicks **15 years** (keyboard: **Alt+↓**, arrows, **Enter**), then sets the rate to 6.25%; the chip reads "15 years, edited". **Compare with: Base** adds each Base figure and the difference. **Save as scenario…**, a name, **Enter**. **Reset** would restore every input as one step that Ctrl+Z reverses.

**M5.2 · Scenarios are Excel's own: compare with Base, then save the variation**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Scenario: Base  ▐15 years, edited▌  Rate +1 pt │ Compare with: Base ▼ │ Reset  Save as scenario… │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│    MONTHLY PAYMENT              TOTAL INTEREST                PAID OFF                           │
│    2,143.56                     135,840.29                    Dec 2041                           │
│    Base 1,580.17 · +563.39      Base 318,861.22 · −183,020.93   Base Dec 2056 · 15 y sooner (1)  │
│                                                                                                  │
│    · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·           │
│    : Name  ▐[ 15 years at 6.25%|          ]▌                                          :          │
│    : Saves an Excel scenario on sheet Loan. Changing cells: C4 6.25%, C5 15.  (2)     :          │
│    : Excel lists it under Data › What-If Analysis › Scenario Manager.                 :          │
│    : Enter saves · Esc cancels                                                        :          │
│    · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·           │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) Each output beside the same output under another scenario. It is Excel's scenario summary, shown in place.  
(2) The seam for someone who did not build the model: what is stored, where, and how Excel will show it.

**S5.5 When the model cannot answer.** She types `0` in *Term*, which has no rule in this workbook. The payment tile reads "Cannot be worked out: `#NUM!`", with the red tick, and from the trace: "Term (years) is 0. PMT needs at least one payment. **Undo my last change**". The tiles that depend on it say the same. None shows a stale number as if it were good. **Ctrl+Z**.

**S5.6 Where does that come from?** Resting on *Total interest*: "The sum of the Interest column of the schedule · `Loan!C9` · **See the cells (Alt+Shift+0)**". The grid is one key away and not in her way.

**S5.7 Print** (M5.3). **Ctrl+P**; page 1 previews within 300 ms. The schedule is an option that states its cost in pages. **Enter**.

**M5.3 · Ctrl+P in App: a one-page answer, the schedule optional**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Loan.xlsx · edited                           Print preview                          Esc closes│
├────────────────────────────────┬─────────────────────────────────────────────────────────────────┤
│  PRINT                         │   ┌──────────────────────────────────────────────────────────┐  │
│  What     App page ▼           │   │ Loan · 15 years at 6.25%                                 │  │
│  Paper    A4 portrait          │   │ Loan amount 250,000.00 · Annual rate 6.25%               │  │
│  Schedule (1)                  │   │ Term 15 years · Start 1-Jan-27                           │  │
│   ( ) all 180 payments, 4 pp   │   │                                                          │  │
│   (•) first year               │   │ Monthly payment   2,143.56     Base 1,580.17  +563.39    │  │
│   ( ) none                     │   │ Total interest  135,840.29     Base 318,861.22           │  │
│  Compare  [x] with Base        │   │ Paid off          Dec 2041     Base Dec 2056             │  │
│  Footer   file · scenario ·    │   │                                                          │  │
│           time of values       │   │ [ chart: Balance over time ]                             │  │
│                                │   │ Loan.xlsx · 15 years at 6.25% · 20 Nov 2026 15:40 (2)    │  │
│  ▐ Print ▌   Save as PDF       │   └──────────────────────────────────────────────────────────┘  │
├────────────────────────────────┴─────────────────────────────────────────────────────────────────┤
│  Ready · Up to date · No errors                                                 page 1 of 2 · ◄ ►│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) The schedule's 180 rows are a choice with its page count stated.  
(2) The footer ties the paper to a file, a scenario and a time.

**S5.8 Leave.** **Ctrl+W**: "Save changes to Loan.xlsx? You added 1 scenario and changed 2 inputs. **Save** · **Save a copy** · **Don't save**".

**S5.9 Access, and a note on touch.** The page is a form: labelled fields, and outputs in a live region ("Monthly payment 2,143.56, up 563.39 from Base"). At 200% the two columns become one. App is the view where touch will matter first. Its controls are already 36 px or taller, but no touch gestures are designed now.

*What an Excel user notices, and why it is a win.* The white fields are the unlocked cells, the limits are the validation rules, and the scenarios are the Scenario Manager's. The builder's Excel habits made the App; its user sees no cell reference unless she asks.

### Storyboard 6. Ade loads an add-in in the standalone Formula Explorer, tests a function, then uses it from a cell in Budget

*Start: Ade has built `Seasonal.xll`, three worksheet functions, and wants to know they behave before anyone models with them.*

**S6.1 Start, empty.** *DNA Calc - Formula Explorer* is ready in half a second: one formula line with a faint example, and a function list. `=PMT(6.5%/12,360,-250000)`, **Enter**: 1,580.17, with the steps beneath.

**S6.2 Load the add-in.** He drags `Seasonal.xll` onto the window; **Alt, T, I**, Excel's own path, does the same by keyboard. One card asks one question. It shows the file's path, its build time and "not signed", then: "An add-in is a program. It runs with your permissions. **Load once** · **Always trust this folder** · Cancel". **Enter**. The Add-ins facet lists three functions with the descriptions the add-in registered, and one remark for its author: "`SEASONAL.INDEX`: argument 2 has no description."

**S6.3 Test.** `=SEAS` narrows the list; add-in functions carry the `◊`. **Tab** completes the name. The tip shows the signature and the registered text for the current argument. `=SEASONAL.SPREAD(1200000,"retail")`, **Enter**: a 1 × 12 frame (M8.7). He names the line, `spread = …`, and adds `=SUM(spread)`: 1,200,000.

**S6.4 Edge cases** (M6.2). **Alt+Q**, `edge`, **Enter**. Eight awkward calls return in under a millisecond each. One, `periods = 0`, stops the add-in. The Explorer carries on, because add-ins run apart from it.

**M6.2 · Edge cases in one command, and an add-in failure that stays inside the add-in**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪● DNA Calc · Formula Explorer                                                 Ask or run…  Alt+Q│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  SEASONAL.SPREAD(annual_total, profile, [periods])    ◊ Seasonal.xll · not volatile · thread-safe│
│   Edge case            │ Call                            │ Result                │    Time │  (1)│
│   as typed             │ (1200000, "retail")             │ 1 × 12 numbers        │  0.4 ms │     │
│   profile misspelt     │ (1200000, "retial")             │ ▌#VALUE!              │  0.1 ms │     │
│   annual_total = text  │ ("abc", "retail")               │ ▌#VALUE!              │  0.1 ms │     │
│   annual_total empty   │ ( , "retail")                   │ 1 × 12 zeros          │  0.3 ms │     │
│   annual_total = error │ (#N/A, "retail")                │ ▌#N/A                 │  0.0 ms │  (2)│
│   periods = 0          │ (1200000, "retail", 0)          │ [!] add-in stopped    │         │  (3)│
│   periods = 13         │ (1200000, "retail", 13)         │ 1 × 13 numbers        │  0.4 ms │     │
│   a range for a number │ (A1:A3, "retail")               │ ▌#VALUE!              │  0.1 ms │     │
│                                                                                                  │
│  [!] Seasonal.xll stopped in SEASONAL.SPREAD(1200000,"retail",0): division by zero, not handled. │
│      The Explorer is unaffected. Your formulas and inputs are intact.                            │
│      ▐ Reload add-in ▌    Show the call    Copy as a test case                               (4) │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Ready · [!] 1 add-in stopped                                       9 calls · 1.4 ms in the add-in│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) "Edge cases" calls the function with the awkward values each argument invites.  
(2) Errors passed in come back out, as with Excel's own functions.  
(3) A stopped add-in is a warning, not a calculation error, so it is not red.  
(4) Recovery is one key, and the failing call can be copied out as a test case.

**S6.5 Fix and reload.** He rebuilds. The Explorer has noticed: "Seasonal.xll changed on disk. **Reload**". **Enter**: every line runs again, and that row now reads `#NUM!`, marked "was: stopped". Nothing was restarted.

**S6.6 Formats.** In *Number format* he types `#,##0;[Red]-#,##0` and sees it on the twelve results and on positive, negative, zero and text samples. In *Conditional format*, "top 3 values" marks three months and says which rule fired.

**S6.7 Into a workbook** (M6.3). In DNA Calc, with Budget open, the add-in he trusted is listed under **Alt, T, I**. **F5**, `Monthly!C20`. He types `=SEASONAL.SPREAD(`, clicks `O14`, types `,"retail")`, **Enter**. `#SPILL!`: the card names the cells the result needs and the one in the way. **Move the formula down one row**, and it spills over `C21:N21`.

**M6.3 · The same function in a cell in Budget: a blocked spill, explained at the cell**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Budget.xlsx · edited        ▐Classic▌  Inspector  Notebook  Map  App        Ask or run…  Alt+Q│
│ Monthly!C20     ▼ │ fx │ =SEASONAL.SPREAD(O14,"retail")                                  lens off│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│    │ A                    │         C │       D │  …  │       M │                N │       O │   │
│  4 │                      │       Jan │     Feb │  …  │     Nov │              Dec │      FY │   │
│ 14 │ Marketing            │    24,566 │  25,408 │  …  │  33,629 │           34,359 │ 354,654 │(1)│
│ 20 │ Marketing, re-phased │▐ ▌#SPILL!▌│ ·  ·  · │ · · │ ·  ·  · │ check with Maria │         │(2)│
│ 21 │                      │           │         │     │         │                  │         │   │
│    ┌──────────────────────────────────────────────────────────────────────────────────────────┐  │
│    │ ▌#SPILL!  The result needs C20:N20, 1 × 12. N20 is not empty: "check with Maria". (3)    │  │
│    │ Go to N20     Clear N20     Move the formula down one row                                │  │
│    │ ◊ Uses the add-in Seasonal.xll. Excel shows #NAME? where it is not installed. (4)        │  │
│    └──────────────────────────────────────────────────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Ready · Calculated · ▌1 error · ◊ 1 add-in in use                                            100%│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) `O14`, pointed at while typing.  
(2) The cells the result would fill are a ghost; the cell in the way keeps its content.  
(3) The range needed, the blocker by name, and three ways out. Excel's error text is kept.  
(4) The honest note about the file: what Excel will show without the add-in.

**S6.8 From the cell back to the Explorer.** **Alt+Shift+E** on `C21` opens it as a panel with the cell's inputs: `O14 = 354,654` and `"retail"`. He tries 400,000: "Trying. Workbook unchanged." **Esc**.

**S6.9 Save.** **Ctrl+S**. The Strip adds "◊ 1 formula uses Seasonal.xll. Excel shows `#NAME?` where it is not installed." A note, not a block.

**S6.10 Clipboard.** The formula line copies into an Excel cell as a formula, and the result as twelve values. A formula pasted in from Excel arrives with its references turned into inputs to fill (`O14 = ?`), there being no workbook for them to point at.

**S6.11 Keyboard and screen reader.** Formula line, inputs, result table, steps tree: **F6** moves between them and **Tab** within. A result is announced shape first: "1 by 12 numbers, 78,000 to 174,000."

*What an Excel user notices, and why it is a win.* Testing an add-in in Excel needs a workbook, shows one function's arguments at a time, needs a restart to load a rebuilt file, and goes down with the add-in. None of that is true here. In the browser build native add-ins cannot load, and the function list says so (section 11).

### Storyboard 7. The agent proposes a change to Budget. Maria reviews it, accepts part of it and undoes the rest

*Start: Budget in Classic, on Monthly. The price rise is hard-coded to July in twelve formulas.*

**S7.1 Ask.** **Alt+Q**: "make the price-rise month an input instead of hard-coded July, and tidy anything inconsistent in Monthly". No command matches, so the last row reads "Ask the agent: …". **Enter**. The Mast shows "Agent: reading Monthly…" and the Strip "Esc stops it". She keeps working. The agent reads through the same addresses and commands she uses, and what it writes goes into a proposal.

**S7.2 The proposal arrives** (M7.1). "Agent: 1 proposal", and the Changes panel opens. Nothing has been applied. There are three parts, each with its intent, its cells, the formula before and after, and its effect on the results, worked out in advance. The proposal also says what the agent chose not to propose.

**S7.3 Review.** **F6** to the panel. **↓** walks the parts and the grid follows. A and B read "no value changes"; **Space** ticks each. On C she presses **P**: the stage shows ghost values beside the current ones, under an amber band. Her 9,000 in June would go, and the year's profit would rise to 932,215. **Esc** leaves the preview.

**M7.1 · A proposal from the agent: nothing applied, three parts, each with its effect**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Budget.xlsx · saved    ▐Classic▌ Inspector Notebook Map App   ▫▫ Agent: 1 proposal  Ask… Alt+Q│
│ Monthly!H13     ▼ │ fx │ =H12*Inputs!$E$14+9000                                          lens off│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  PREVIEW of part C · not applied · Esc leaves the preview                                     (5)│
├───────────────────────────────────────────────────────────────┬──────────────────────────────────┤
│    │ A                │               H │                   O │ CHANGES · Proposal 1, Agent (1)  │
│  4 │                  │             Jun │                  FY │ "make the price-rise month an    │
│  5 │ Customers        │           5,796 │               6,689 │ input; tidy inconsistencies"     │
│  6 │ Price            │       : 49.00 : │           : 49.98 : │ 3 parts · 18 cells · none applied│
│  7 │ Subscriptions    │         283,996 │           3,518,394 │ ───────────────────────────────  │
│  8 │ Services         │          34,080 │             422,207 │ [x] A  Add input "Price rise     │
│  9 │ Total revenue    │         318,076 │           3,940,602 │        month"  Inputs!A17:E17    │
│ 11 │ Cost of sales    │          69,977 │             866,932 │        5 cells · no value changes│
│ 12 │ Headcount        │              20 │                  24 │ [x] B  Use it in Price           │
│ 13 │ People           │▐137,000→128,000▌│ 1,621,800→1,612,800 │        Monthly!C6:N6, 12 alike   │
│ 14 │ Marketing        │          28,627 │             354,654 │        >=7 becomes >=Inputs!$E$17│
│ 15 │ Rent and other   │          14,500 │             174,000 │        no value changes    (2)   │
│ 16 │ Total costs      │ 250,103→241,103 │ 3,017,387→3,008,387 │▐[ ] C  Make People consistent▌   │
│ 18 │ Operating profit │   67,972→76,972 │     923,215→932,215 │        Monthly!H13, 1 cell       │
│ 19 │ Margin           │     21.4%→24.2% │         23.4%→23.7% │        − =H12*Inputs!$E$14+9000  │
│                                                               │        + =H12*Inputs!$E$14       │
│  : … : proposed, same value  a→b proposed, value changes (4)  │        8 results change      (3) │
│  ▫▫ the agent's mark: teal, dotted, never amber               │ ───────────────────────────────  │
│                                                               │ Not proposed: turn Monthly into  │
│                                                               │ a table (would rewrite 214 refs) │
│ Inputs │ Monthly │ Summary │ +                                │ Space tick · P preview · R reject│
├───────────────────────────────────────────────────────────────┴──────────────────────────────────┤
│  Ready · Preview · Calculated · ▫▫ 1 proposal, 2 of 3 parts ticked    Ctrl+Enter accepts A, B (6)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) The proposal quotes its request. Nothing is written: the Mast still says "saved".  
(2) A and B change formulas and no values. The program worked that out before asking.  
(3) Part C: the formula before (−) and after (+), and how many results move. Signs and words, never red and green.  
(4) Ghosts on the grid: dotted sides for a new formula with the same value, `a→b` for a changed value.  
(5) The band names the one temporary state and how to leave it.  
(6) What is ticked, and the key that accepts it.

**S7.4 Accept part.** **Ctrl+Enter** accepts what is ticked. A and B are written: 17 cells, no values changed. Each is one named step in the undo history.

**S7.5 Try the rest, then undo it.** To read part C in her write-up she ticks it, **Ctrl+Enter**, **Alt+Shift+3**. The sentence in her notebook says 932,215. That is not the plan she agreed. **Ctrl+Z**: 923,215 again, and the Strip reads "Undid: Agent, part C (Monthly!H13). Pending again in Changes." (M7.2).

**M7.2 · After the undo: one history that says who and what kind, and a cell that remembers**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Budget.xlsx · edited           ▐Classic▌ Inspector Notebook Map App          Ask or run… Alt+Q│
├────────────────────────────────────────────────┬─────────────────────────────────────────────────┤
│  UNDO HISTORY                               (1)│ DETAILS · History                            (3)│
│  ● 14:10 Agent C  Monthly!H13, 1 cell  (undone)│ Monthly!E6 · Price, Mar                         │
│  ▐● 14:07 Agent B  Monthly!C6:N6, 12 cells   ▌ │ 20 Nov 2026 14:07                               │
│  ● 14:07 Agent A  Inputs!A17:E17, 5 cells      │ Agent · Proposal 1, part B                      │
│  ○ 13:58 You      Notebook: moved Summary      │ accepted by Maria                               │
│  ● 13:52 You      Monthly!C8:N8, 12 cells      │ − …IF(MONTH(E$4)>=7,1+Inputs!$E$9,1)            │
│  ────────────────────────────────────────────  │ + …IF(MONTH(E$4)>=Inputs!$E$17,1+Inputs!$E$9,1) │
│  ● the workbook   ○ layout or text only   (2)  │ value 49.00, unchanged                          │
│  Ctrl+Z undoes the top · Ctrl+Y redoes C       │ Show the proposal · Revert this change          │
│  Revert… undoes an older step by itself        │ 12 Mar 2026 09:31  Maria · typed                │
├────────────────────────────────────────────────┴─────────────────────────────────────────────────┤
│  Ready · Calculated · Undid: Agent, part C (Monthly!H13). Pending again in Changes.           (4)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) One history for the workbook, newest first: who, where, how many cells.  
(2) Filled steps changed the workbook; hollow ones changed only layout or text.  
(3) Any cell can show its own history. Revert writes a new step, and refuses if the cell was edited since.  
(4) The undone part is pending again, not lost.

**S7.6 Reject, with a reason.** **R** on part C: "Keep the 9,000 in June: one-off recruitment fee." `H13` becomes a noted exception, and the run check stops flagging it. She accepts the offer to add the reason as an Excel note on `H13`: a model change, previewed and undoable like any other.

**S7.7 Out of order.** **Revert** on an older step writes a new step that restores those cells. It refuses to destroy later work: "`Inputs!C17` was edited after this change (by you, 14:31). **Show me** · **Revert anyway**".

**S7.8 Trace.** A week later Sam opens **Details › History** on `Monthly!E6` (M7.2, right): who, when, through which proposal, before and after, with a link to the proposal and Maria's note.

**S7.9 The limits the agent shares with everyone.** It may not change calculation settings, allow macros, load add-ins or save without asking; those arrive as questions. A request about looks ("make Classic calmer") is a skin, tried on (section 7), and cannot touch a formula.

**S7.10 Screen reader and clipboard.** The Changes panel is a list of checkboxes: "Part B. 12 cells, Monthly C6 to N6. No value changes. Ticked." **Ctrl+C** on the proposal copies a plain-text summary with before and after.

*What an Excel user notices, and why it is a win.* Excel lists changes after they happen. Here the effect on the results is known first, part by part, and every accepted part stays an ordinary step that Ctrl+Z understands.

### Where each make-or-break moment is shown

| Moment | Steps |
|---|---|
| First run and empty states | S1.1 · S2.1 no notebook yet · S4.1 empty map · S6.1 empty Explorer · S5.1 first-use note |
| How features are discovered | S1.3 key tips teach the new layer · S3.2 an Excel habit leads to Trace · S2.1 the draft offer · S6.3 autocomplete · S7.1 the palette falls through to the agent |
| Feedback while loading or calculating | S1.2 · S1.8 · S4.7 the ripple · S5.1 · S7.1 · M8.3 |
| The switch between views | S2.1 where the selection lands · S3.6 panel to view and back · S4.8 grid alongside · S1.5 · S7.5 |
| Error, warning, refusal | S1.6 macro · S2.3 prose may not calculate · S2.5 unknown label · S3.7 overwrite · S4.4 loop · S5.3 validation · S5.5 `#NUM!` · S6.2 trust · S6.4 add-in stopped · S6.7 `#SPILL!` · S7.7 revert refused |
| Undo and recovery | S1.7 · S2.5 · S3.7 · S4.9 both kinds · S5.4 Reset · S6.5 reload · S7.5 · S7.7 |
| Copy and paste | S1.4 · S2.6 · S3.8 · S4.11 · S6.10 · S7.10; rules under R-90 |
| Hover, focus, selection | M0.1 · S1.5 · S3.1 · S3.4 shown against stored · S5.6 · M4.3 |
| How fast it feels | S1.2 · S2.1 · S2.5 · S3.3 · S5.2 · S6.1; budgets in section 6 |
| Accessibility | S1.2 · S1.9 · S2.9 · S3.10 · S4.11 · S5.9 · S6.11 · S7.10 |
| What an Excel user notices | the last paragraph of each storyboard; Appendix A, Part 12 |

---

## 3. The conceptual model

**What a user points at.** One pointer, seven kinds of thing, one grammar of addresses that a person types, a macro passes, a skin binds to and an agent sends.

| Thing | Address, as typed | In Excel? |
|---|---|---|
| Cell, range | `Monthly!H13`, `Inputs!C5:D16` | yes |
| Spill | `Monthly!C21#` | yes |
| Name | `ServicesShare`, `Store.Stock` | yes |
| Table, column, row | `Orders`, `Orders[Amount]`, `Orders[@Date]` | yes |
| Object | `Loan!"Balance"`, `Model!"Slider 1"`, `Report!"PivotTable1"` | yes, by its Excel name |
| Block | its range, `Monthly!B5:O9`, shown with its label `‹Revenue›` | no: a reading, until the user pins it as a name |
| View item | `view:notebook/Summary/text-1`, `view:map/Store` | no: presentation |

Where a view item stands for something in the model, the address field shows the model address. For a pure presentation item it shows the view path in muted ink, and the formula bar is empty.

**Selection and navigation that carry across views.**

- The selection is kept in model terms. Each view draws it on the finest item that contains it; if that item is coarser, the Strip says so ("`Monthly!C9`, shown in Revenue › Total revenue › Jan"), and switching back restores the exact cells.
- Scroll and zoom belong to each view and sheet. On a switch the view scrolls only as far as it must to show the selection.
- F5, Ctrl+G, the Name box and Alt+Q take any address in any view. Ctrl+[ and Ctrl+] go upstream and downstream. Alt+← and Alt+→ walk the places visited, across sheets and views. F6 cycles stage, companion, panel, Deck, Mast, Strip.
- At most two panes: a view and a companion, the grid by default, sharing the selection. *Not chosen:* free tiling (people get lost); a tab per view (hides that it is one workbook).

**What an edit means in a view that is not a grid.** One of four things, and the seam says which.

1. *Editing an item that stands for cells* is that edit on those cells. A run item writes its formula to the first cell and fills the run. A box writes its cell.
2. *Creating model content* uses the placement rule: a label cell and a value cell, plus a name where the view works by names, in the next free row of the block or frame it belongs to, on a visible sheet. The seam shows the place and lets it be changed.
3. *A structural gesture* is the Excel command it stands for. Rewiring replaces one reference with another. Renaming a box renames its name. Deleting a box clears its cell and deletes its name, and the seam lists what would then show `#NAME?`.
4. *Presentation* is free: layout, order, folding, prose, view sort and filter. It lives in the view part and can never change a value.

Anything else is refused, with the reason and the nearest thing Excel can express.

**Undo across views.** One model history per workbook, shared by all views, and a small presentation history per view. Ctrl+Z undoes the more recent of the last model step and the last presentation step of the view in focus. Steps are named: who, what, where, how many cells (M7.2). A macro run and an accepted agent part are one step each. Undo never reaches into another workbook, survives a save, and returns the Mast to "unchanged" at the opening state. **Revert** writes a new step that restores what an older step changed, and refuses if later work would be lost. *Not chosen:* one global stack (Ctrl+Z in the grid would undo a box drag that cannot be seen there); a model history per view (no coherent order).

**Errors, calculation state and live data as visible things.**

- *An error* belongs to a root, the cell where it first arises. Issues lists roots. Cells carry the red tick; boxes, notebook items, App tiles and sheet tabs inherit it with a count.
- *Calculation state* is per cell: fresh, settled, pending, stale (an input changed under manual calculation) or as saved. The last three share the hatch. For the workbook, the Strip states mode, iteration settings, progress and the last duration.
- *Outside data* has sources, and sources are things: a linked file, an RTD topic, a volatile function, an add-in. Cells they feed carry `◊`. Inspector › Live lists the sources with their state.
- *Warnings* have a kind and no hue: a run with an exception, a number stored as text, a pivot older than its source, a name built on OFFSET, a missing link.

**How an agent's actions are shown, reviewed, accepted and traced.**

- The agent uses the same addresses and commands as a person (Strand tenet 7). It has no private door to the model.
- *Shown:* a chip in the Mast (idle, reading where, working, proposals waiting). No cursor on the stage unless asked for.
- *Permission,* per workbook: **Look**, **Propose** (the default) or **Apply, then review**, in which changes are applied as steps marked "unreviewed" until kept or reverted. The panel and the verbs are the same.
- *Reviewed:* a proposal comes in parts. Each has its intent, cells, before and after, effect on the results, and three checks: opens in Excel, adds no errors, breaks no run. The verbs are tick, preview, accept ticked, reject with a note.
- *Traced:* an accepted part is a named undo step and appears in each cell's history. The proposal record stays with the session and, if the owner opts in, travels in the file.
- *Asked, never assumed:* calculation settings, macros, add-ins, saving, sending anything outside. Looks and layouts never pass through proposals; they are skins (section 7).

**The clipboard carries the thing, not the picture.** A copy puts on the clipboard what the item stands for: cells, or a frame of values, or a box with its name. What a paste does depends on where it lands, and a paste that would create model content shows the seam first. The full table is under R-90 in Appendix B.

---

## 4. The views and the Formula Explorer

Each view is shown full screen on a cast workbook other than the one its storyboard used, in a state that matters. The table after the mockups gives how each is entered and left, its own keys, and what it suits.

**M8.1 · Classic. Loan: a formula being typed, a cell being pointed at**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Loan.xlsx · edited         ▐Classic▌  Inspector  Notebook  Map  App         Ask or run…  Alt+Q│
│ File  Home  Insert  Layout  Formulas  Data  Review  View │ Paste▼  Fill▼  B I U  %  ,       More▼│
│ Loan!D12        ▼ │ × √ fx │ =IPMT(¹$C$4/12,²A12,³$C$5*12,-▐$C$3▌|                  lens off  (1)│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│    │       A │ B                │            C │ D                                │       E │    │
│  3 │         │ Loan amount      │▐  250,000.00▌│ (3)                              │         │    │
│  4 │         │ Annual rate      │      ¹ 6.50% │                                  │         │    │
│  5 │         │ Term (years)     │         ³ 30 │                                  │         │    │
│  6 │         │ Start date       │     1-Jan-27 │                                  │         │    │
│  8 │         │ Monthly payment  │     1,580.17 │                                  │         │    │
│ 11 │  Period │ Date             │      Payment │ Interest                         │  Princ. │    │
│ 12 │     ² 1 │ 1-Jan-27         │     1,580.17 │ =IPMT($C$4/12,A12,$C$5*12,-$C$3| │         │ (2)│
│                                                ┌──────────────────────────────────────────────┐  │
│                                                │ IPMT(rate, per, nper, pv, [fv], [type])  (4) │  │
│                                                │ pv: the present value, what the loan is worth│  │
│                                                └──────────────────────────────────────────────┘  │
│ 13 │       2 │ 1-Feb-27         │     1,580.17 │                                  │         │    │
│ 14 │       3 │ 1-Mar-27         │     1,580.17 │                                  │         │    │
│ Loan │ +                                                                                         │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Point · Calculated · No errors                                                          100%  (5)│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) Each reference has a number and a teal tone; the one at the caret is amber. No rainbow, no red: it works without colour vision.  
(2) The cell in edit shows the same text, with the caret.  
(3) The cell being pointed at. Arrows move the pointing; F4 has just made it `$C$3`.  
(4) The function tip: the current argument in bold, with its description.  
(5) "Point" is Excel's mode word. After Enter, a double-click on the fill handle fills 360 rows.

**M8.2 · Inspector. Stock: the Calc instrument, recording one recalculation per tick**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Stock.xlsx · saved         Classic  ▐Inspector▌  Notebook  Map  App         Ask or run…  Alt+Q│
│ Model!B11       ▼ │ fx │ =IF(Shop.Reset,Store.Initial,Store.Stock+Supply.Arrivals-Store.Sales)   │
├──────────────────────┬───────────────────────────────────────────────────────────────────────────┤
│  INSPECTOR        (5)│ CALCULATION  ● recording · 25 runs           ▐ Pause ▌ Step F9  Export (1)│
│  Overview            │ automatic · iterative: 1 pass, max change 0.001 · 6 formulas · 0.4 ms (2) │
│  Trace               │                                                                           │
│  ▐Calc         ● rec▌│ This recalculation (tick 25), in evaluation order           cause: F9  (3)│
│  Errors            0 │  1  Shop.Tick         B7       24 → 25•                                   │
│  Live              0 │  2  Supply.Arrivals   B8        0 → 420•    reads last pass: InTransit    │
│  Watch             1 │  3  Supply.InTransit  B9      420 → 0•      reads last pass: Order        │
│  Links             0 │  4  Store.Sales       B10     100            reads last pass: Stock       │
│  Names            11 │  5  Store.Stock       B11     100 → 420•    reads last pass: itself       │
│  ────────────────────│  6  Supply.Order      B12       0                                         │
│  Subject             │                                                                           │
│  Store.Stock         │ Watch: Store.Stock, last 24 recalculations                pin another… (4)│
│  Model!B11           │  460 ┤     ▄       █       █         ▄       ▄       ▄                    │
│  420                 │  345 ┤     █ █     █ █     █ █ ▄     █ ▄     █ ▄     █                    │
│                      │  230 ┤ ▄   █ █ █   █ █ █   █ █ █ ▄   █ █ ▄   █ █ ▄   █                    │
│                      │  115 ┤ █ █ █ █ █ █ █ █ █ █ █ █ █ █ ▄ █ █ █ ▄ █ █ █ █ █                    │
│                      │    0 ┼─────────────────────────────────────────────────                   │
│                      │       2       6       10      14      18      22    25                    │
├──────────────────────┴───────────────────────────────────────────────────────────────────────────┤
│  Ready · Iterates, 1 pass · tick 25 · No errors                      recording: 25 frames, 1.2 KB│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) An instrument: start, watch, pause, step, export. Recording stores nothing in the workbook.  
(2) The standing facts of this workbook's calculation, in Excel's terms.  
(3) The last recalculation as it ran: order, values before and after, and which reads saw the previous pass. The map's delay marks come from here.  
(4) A watched value over time: a readout, not a chart in the file.  
(5) Instruments with live counts. The same ones dock as panels in every view.

**M8.3 · Notebook. Sales laid across the page as a pipeline, recalculating after a paste**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Sales.xlsx · edited         Classic  Inspector  ▐Notebook▌  Map  App        Ask or run…  Alt+Q│
│ File Home Insert Layout … View │ Text  Value  Frame  Chart  Control │ Page · Across │ Reading    │
│ Report!B6:E9    ▼ │ fx │ =SUMIFS(Orders[Amount],Orders[Region],$A6,Orders[Date],…         lens on│
├──────────────────────────────┬────────────────────────────────────┬──────────────────────────────┤
│  1  Orders                ──►│ 2  Report                       ──►│ 3  PivotTable1               │
│  table 25,500 × 10 +500• (1) │ ▐Sales by region and quarter      ▌│ Sum of Amount                │
│  OrderID   text              │ = SUMIFS(Orders[Amount],           │ by Region × Quarter          │
│  Date      date and time     │    Orders[Region], ‹Region›,       │                              │
│  Region    text, 4 values    │    Orders[Date], ">=" & ‹start›,   │ [!] Source changed after     │
│  Rep       text, 38 values   │    Orders[Date], "<=" & ‹end›)     │ the last refresh (19 Nov     │
│  Amount    number            │ 16 cells, one formula  (2)         │ 16:02). Figures may be       │
│  … 5 more columns            │                                    │ out of date.     (4)         │
│  ─────────────────────────   │         Q1     Q2     Q3     Q4    │ Refresh  Alt+F5              │
│  rows 25,001–25,003 •        │  North ░░░░░  1.18m  1.09m  0.61m  │                              │
│  SO-125001 19-Nov West       │  South ░░░░░  0.90m  0.88m  0.46m  │        Q3                    │
│  SO-125002 19-Nov East       │  East  ░░░░░  1.29m  1.34m  0.70m  │  North 1,131,672.20          │
│  SO-125003 19-Nov East       │  West  ░░░░░  1.23m  1.20m  0.65m  │  South   905,665.00          │
│  view filter: none           │ calculating 12 of 16 · Esc  (3)    │  East  1,394,821.00          │
│                              │                                    │  West  1,251,907.25          │
│  reads: nothing              │ reads: Orders, 3 columns           │ reads: Orders (cache)        │
├──────────────────────────────┴────────────────────────────────────┴──────────────────────────────┤
│  Ready · Calculating 75% · Esc stops · [!] 1 pivot older than its source        Notebook · Across│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) A table is a frame: size, columns with types, a window of rows. The 500 rows just pasted carry the fresh mark.  
(2) Sixteen cells with one formula are one item. With the lens on, the header cells it reads appear as marked labels.  
(3) Live recalculation, shown truthfully: four results are pending and hatched, the rest are done.  
(4) A PivotTable is a snapshot, and the notebook says when it is older than its source.

**M8.4 · Model map. Budget, sheet Monthly at block zoom: an ordinary workbook with no names**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Budget.xlsx · saved         Classic  Inspector  Notebook  ▐Map▌  App        Ask or run…  Alt+Q│
│ Monthly!C9:N9   ▼ │ fx │ =SUM(C7:C8)                                                     lens off│
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  Budget.xlsx ► Monthly                     zoom:  sheets · blocks · cells                     (1)│
│                              ┌- - - - - - - - - - - - - - - - - - ┐                              │
│                              ¦ ‹Revenue›  B5:O9 · 12 periods (2)  ¦                              │
│                              ¦ Customers        [t-1]  (3)        ¦                              │
│                              ¦ Price                              ¦       ┌- - - - - - - - - - ┐ │
│                              ¦ Subscriptions                      ¦       ¦ ‹Result›  B18:O19  ¦ │
│  ┌────────────────────┐ 5    ¦ Services                           ¦──────►¦ Operating profit   ¦ │
│  │ Inputs (sheet)     │─────►¦▐Total revenue                     ▌¦   ┌──►¦ Margin             ¦ │
│  │ 12 assumptions     │      └- - - - - - - - - - - - - - - - - - ┘   │   └- - - - - - - - - - ┘ │
│  │ 2 scenarios        │                        │ Total revenue (4)    │              │           │
│  │ in use: Base (6)   │──┐                     ▼                      │              │ 8         │
│  └────────────────────┘  │   ┌- - - - - - - - - - - - - - - - - - ┐   │              │           │
│                          │   ¦ ‹Costs›  B11:O16 · 12 periods      ¦   │              ▼           │
│                       7  │   ¦ Cost of sales                      ¦   │   ┌────────────────────┐ │
│                          └──►¦ Headcount        [t-1]             ¦───┘   │ Summary (sheet)    │ │
│                              ¦ [!] People  11 alike, 1 differs (5)¦       │ 6 formulas         │ │
│                              ¦ Marketing · Rent and other         ¦       │ 1 chart            │ │
│                              ¦ Total costs                        ¦       └────────────────────┘ │
│                              └- - - - - - - - - - - - - - - - - - ┘                              │
│                                                                                                  │
│  13 rows in 3 blocks, all drawn · 4 bundles of references · [t-1] reads the previous period      │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Ready · Calculated · No errors · [!] 1 exception noted                        Map · blocks · 100%│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) Breadcrumb and semantic zoom. Enter on a row goes to its twelve cells.  
(2) A block is a reading: dashed, label in guillemets. The sheets either side are facts: solid.  
(3) `[t-1]`: a row that reads its own previous period. Deliberately not the iteration mark `[z-1]`.  
(4) Wires between blocks are bundles, labelled with what flows or a count.  
(5) An exception inside a run shows at every zoom.  
(6) The scenario switch: one input that selects a column.

**M8.5 · App. Budget, drafted from the file: Downside beside Base**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Budget.xlsx · saved            ▐App▌   View as ▼           Print  Ctrl+P    Ask or run…  Alt+Q│
│ Scenario: Base ▐Downside▌ │ Compare with: Base ▼ │ Reset   drafted from the file · Edit page… (1)│
├─────────────────────────────────────┬────────────────────────────────────────────────────────────┤
│  ASSUMPTIONS · Downside          (2)│ REVENUE FY     COSTS FY       OPERATING PROFIT  MARGIN (3) │
│  Opening customers  [    4,800 ]    │ 3,255,671      2,689,858      565,813           17.4%      │
│  New customers / mo [      180 ]    │ Base 3,940,602 Base 3,017,387 Base 923,215      Base 23.4% │
│  Monthly churn      [     2.6% ]    │ −684,931       −327,529       −357,402          −6.0 pts   │
│  Price per customer [    49.00 ]    │                                                            │
│  Price rise in July [     0.0% ]    │  Operating profit by month              █ Base   ▓ Downside│
│  Services share     [     8.0% ]    │   100k ┤                      ▄  ▄  ▄  ▄  █                │
│  6 more ▼                           │    75k ┤ ▄  ▄  █  ▄  █  ▄  █  █  █  █  █  █                │
│                                     │    50k ┤ █▓ █▓ █▓ █▓ █▓ █░ █▓ █▓ █▓ █▓ █▓ █▓               │
│  These are the Downside column      │    25k ┤ █▓ █▓ █▓ █▓ █▓ █▓ █▓ █▓ █▓ █▓ █▓ █▓               │
│  of Inputs. The Base column is      │      0 ┼─────────────────────────────────────              │
│  shown when Base is in use.         │          J  F  M  A  M  J  J  A  S  O  N  D     (4)        │
├─────────────────────────────────────┴────────────────────────────────────────────────────────────┤
│  Ready · Up to date · No errors                                             App · Reading density│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) The page was drafted from the program's reading of the file, and says so. "Edit page…" opens its declaration (section 7).  
(2) The inputs are the unlocked cells of the scenario in use, labelled from column A of Inputs.  
(3) Each output beside the same output under Base, with the difference.  
(4) A chart of the workbook's own series. The two differ in texture as well as tone.

**M8.6 · Formula Explorer inside a workbook: opened on `Report!D9`, a fix being tried**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪▪ Sales.xlsx · saved         ▐Classic▌  Inspector  Notebook  Map  App         Ask or run…  Alt+Q│
│ Report!D9       ▼ │ fx │ =SUMIFS(Orders[Amount],Orders[Region],$A9,Orders[Date],">="&D$3,…       │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  9 │ West        │  1,187,092.40 │  1,229,645.15 │▐ 1,204,318.50▌│    648,907.85 │               │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  FORMULA EXPLORER · D9       Steps │ Functions │ Number format │ Conditional format │ Add-ins (1)│
│  ────────────────────────────────────────────────────────────────────────────────────────────────│
│  In the cell  …,Orders[Date],">="&D$3,Orders[Date],"<="&D$4)                         1,204,318.50│
│  ▐Trying      …,Orders[Date],">="&D$3,Orders[Date],"<"&E$3|)                 ▌ 1,251,907.25 (2)  │
│  Trying. Workbook unchanged.  +47,588.75            Apply to cell  Ctrl+Enter · Copy · Reset  (3)│
│  ────────────────────────────────────────────────────────────────────────────────────────────────│
│  INPUTS, from the cell       in the workbook                     try                       (4)   │
│   D$3  Report                 1-Jul-26     stored 46204           [               ]              │
│   D$4  Report                 30-Sep-26    stored 46295           [               ]              │
│   E$3  Report                 1-Oct-26     stored 46296           added by the trial             │
│   Orders[Amount]              25,000 numbers · Σ 16,459,938.40                                   │
│  STEPS, for the trial                                                                         (5)│
│   ">="&D$3                    ">=46204"                                                          │
│   "<"&E$3                     "<46296"                                                           │
│   SUMIFS                      25,000 → 6,212 → 2,155 → 2,155 rows (was 2,114) → 1,251,907.25     │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  Ready · Calculated · No errors                                        Explorer: trying, 1 change│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) Five facets: steps, functions, number format, conditional formats, add-in functions.  
(2) The trial line is an editable copy. The cell is untouched until "Apply to cell".  
(3) The state is named and the way out is plain.  
(4) It starts from the cell's inputs, each with its shown and stored value; any can be overridden under *try*.  
(5) Every sub-expression with its value; for row functions, the ladder.

**M8.7 · Formula Explorer on its own: no workbook, an add-in loaded, a result named and reused**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ▪● DNA Calc · Formula Explorer                                            Ask or run…  Alt+Q  (6)│
├────────────────────────────┬─────────────────────────────────────────────────────────────────────┤
│  FUNCTIONS   [ seas     ]  │  1  spread = SEASONAL.SPREAD(1200000, "retail")           0.4 ms (1)│
│  ◊ SEASONAL.INDEX          │       1 × 12 numbers                                                │
│  ◊ SEASONAL.PROFILE        │        78,000   72,000   84,000   90,000   90,000   90,000    (3)   │
│  ▐◊ SEASONAL.SPREAD  ▌ (2) │        84,000   90,000   96,000  108,000  144,000  174,000          │
│  ───────────────────────── │  2 ▐= SUM(spread)|                             ▌  1,200,000         │
│  SEASONAL.SPREAD           │  3  type a formula. The = is optional.                              │
│  (annual_total, profile,   │  ──────────────────────────────────────────────────────────────     │
│   [periods])               │ Steps │ Functions │ Number format │ Conditional format │ Add-ins (4)│
│  Spreads an annual amount  │  STEPS, line 2                                                      │
│  over periods by a named   │   spread         {78000, 72000, 84000, …, 174000}   from line 1     │
│  profile.                  │   SUM(spread)    1200000                                            │
│  annual_total   a number   │  INPUTS   none yet. Name a value to reuse it:  rate = 6.5%          │
│  profile   "flat", "retail"│  CONTEXT  as if in cell A1 of an empty sheet · change…   (5)        │
│            or "4-4-5"      │                                                                     │
│  periods   optional, 12    │                                                                     │
├────────────────────────────┴─────────────────────────────────────────────────────────────────────┤
│  Ready · 2 lines · ◊ Seasonal.xll: 3 functions, 1 remark                        Save as workbook…│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) No sheets, no tabs, no file: numbered lines, each a formula, optionally named and usable below.  
(2) Excel's functions and those of loaded add-ins, marked `◊`, with the help the add-in registered.  
(3) An array result is a small frame, shape first.  
(4) The same five facets as inside a workbook.  
(5) The context a formula is assumed to sit in, stated, because ROW and CELL depend on it.  
(6) Its own identity badge (Strand's dot-long pair, assuming this is the product Strand calls OneCalc). "Save as workbook…" writes a plain `.xlsx`.

| | Enter and leave | Its own keys and gestures | Suits | Does not suit |
|---|---|---|---|---|
| **Classic** | Alt+Shift+1; the default for a file with no stated preference. | Everything in Appendix A. | All five. The only view where nothing is ever summarised. | Explaining a model to someone else; seeing Stock's structure. |
| **Inspector** | Alt+Shift+2, or by enlarging an instrument panel. | ↑ ↓ in lists and trees, → opens a node, Enter goes there, Space pins to Watch, R records, F9 steps. | Inherited (errors, links, names), Sales (trace), Stock (order, loops). | Authoring; Jo. |
| **Notebook** | Alt+Shift+3; the first visit offers a draft. | ↑ ↓ between items, Enter or F2 to edit, Alt+↑ ↓ to move, Alt, N to insert, → into an item's values, Page or Across. | Budget, Loan (an explanation), Sales (a pipeline), Stock (narrated). | Inherited as a whole: it declines and offers a part (section 6). |
| **Model map** | Alt+Shift+4; opens at the zoom that shows 60 boxes or fewer. | Arrows by position, Ctrl+[ and Ctrl+] along wires, Enter in, Esc out, typing `name = formula`, dragging a wire's end, pointing at boxes, + and − zoom. | Stock, Inherited at sheet zoom, Budget at block zoom. | Loan's schedule and Sales' rows: each is one box, by design. |
| **App** | Alt+Shift+5, or the file's preference. "View as" leaves. | Tab through inputs, Alt+↓ for lists and scenarios, Enter, Ctrl+P. | Loan, Budget's scenario switch, Stock's slider. | Sales, until a page is designed for Report's parameters; Inherited (declines). |
| **Formula Explorer** | Alt+Shift+E on a cell; Shift+F3 and Alt, M, V, where Excel opens its two dialogs; or the standalone program. Only "Apply to cell" writes. | Enter evaluates, F9 on a selected part evaluates it in place, F4 cycles a reference, Ctrl+Enter applies. | Any one formula. | A whole model: that is the Inspector and the map. |

Touch and pen are not designed now. They would matter first in App (large fields, sliders), then on the map (dragging boxes and wires with a pen) and in the notebook (marking up a write-up).

**The formula lens** (Alt+Shift+L, in any view) changes how a formula is shown, never what is stored. *Names:* references become defined names where they exist, and marked labels `‹…›` where the program read one from the sheet. *Layout:* the formula is broken and indented by its structure. *Steps:* repeated sub-expressions are shown once, under the notice "shown as steps; stored as one formula". Typing with the lens on accepts names and labels, and the seam shows the Excel text that will be stored. Resting on a chip shows the plain reference. The lens is off by default in Classic and on in Notebook and Map.

**How I react to the two previews.** *Keep:* boxes are names and wires are references; looking inside a box, with a breadcrumb back; the delay mark on a cell that reads itself; the Presented and Excel toggle, which is the lens; firing in evaluation order, which is Strand's ripple; a calculation tag on each group; the RTD event strip, which becomes Inspector › Live. *Change:* the palettes, to Strand's (teal structure, amber for now, signal orange for outside data only). "Director" becomes a chip that states the calculation in Excel's terms. Decorative motion goes; a gauge that carries a value stays. `Stock⁻` becomes `Stock[last pass]`, which can be typed and spoken. The plotter becomes the Watch instrument, stored nowhere in the model.

**The six experimental views.** Grid is Classic; notebook is Notebook; canvas is the Model map. Atlas is the map at sheet zoom, with the Overview. Tree is the Outline panel. Ledger divides between the Notebook's frames and the Inspector's Calc list.

---

## 5. Real spreadsheet models

**How the program reads a workbook.** It gathers evidence of three grades and never hides which is which.

- **Certain:** stated in the file. Tables, PivotTables, data tables, saved scenarios, form controls and their linked cells, validation, protection, names, print areas, iteration together with a real loop, RTD and external references, macros and what calls them.
- **Likely:** read from structure. Runs of identical formulas, period headers in steady steps, constants that formulas read, sheets with matching layouts, lookup shapes.
- **Guess:** read from meaning. "This sheet is a report." "This running balance is a stock."

Line style carries the grade everywhere: solid, dashed, dotted. A person usually recognises a pattern faster than the program; the design's job is to show the program's reading so that the person can agree or correct it in one gesture (M9.1). Corrections are kept in the file's view part. A reading never changes the model, and no view requires that a reading be confirmed first.

**M9.1 · A reading, opened: what the program thinks this is, why, how sure, and how to correct it**

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│  ‹Revenue›   calculation block · time series, 12 periods across                   likely  (1)│
│  Monthly!B5:O9                                                                               │
│  ────────────────────────────────────────────────────────────────────────────────────────────│
│  Why     row 4 holds 12 dates, one month apart                                   certain  (2)│
│          rows 5 to 9: each row is one formula filled across C:N                       certain│
│          column O: totals C:N in 3 rows, takes the last in 1, averages in 1            likely│
│          the label "Revenue" is the bold cell A4 above the block                        guess│
│  Not     a table: no table object covers this range                                          │
│  ────────────────────────────────────────────────────────────────────────────────────────────│
│  Correct Rename    Change kind ▼    Extend or shrink    Split    Merge    Not a pattern   (3)│
│          Pin with a name…   defines Revenue = Monthly!$B$5:$O$9 in the workbook           (4)│
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

(1) Kind, label and grade. The grade shown is that of the weakest evidence the reading rests on.  
(2) The evidence, line by line, each with its own grade. "Not" names the nearest reading that was ruled out.  
(3) A correction is one gesture, is kept with the file, and is open to an agent through the same commands.  
(4) The one correction that changes the workbook says so.

**The catalogue.** Patterns 1 to 17 are the brief's, with two of its entries split; 18 to 27 are those I would add. Appendix D gives for each how a person recognises it, what evidence the program uses, how the reading is corrected, and what each view does with it.

| # | Pattern | How sure the program can be | Shown by default as |
|---|---|---|---|
| 1 | Sheet as database | certain for a table, likely for a plain list | a frame; one box on the map |
| 2 | Inputs and assumptions | certain where unlocked, validated or tied to a control; else likely | App fields; the notebook's first section; constant ink |
| 3 | Calculation block | likely | a notebook section; a dashed frame |
| 4 | Time series, periods across | likely; certain with true dates | a frame with periods across; `[t-1]` |
| 5 | Scenarios | certain if saved in Excel; likely for selector columns | scenario chips; compare |
| 6 | Sensitivity table | certain | a frame naming its two inputs; "runs the model N times" |
| 7 | Reference table | likely | a frame marked lookup; join wires |
| 8 | Pipeline across sheets | likely | notebook Across; map at sheet zoom |
| 9 | Running balance | likely | one run item; `[t-1]` |
| 10 | Deliberate circular reference | certain | a loop with delay marks; a calculation chip |
| 11 | Simulation | likely | a "volatile" chip; `◊` |
| 12 | Control panel | certain for the controls, a guess for the sheet | the App's first draft |
| 13 | PivotTable | certain | a frame with its freshness |
| 14 | Live data | certain | `◊`; Inspector › Live |
| 15 | Macro-driven model | certain that macros exist; likely what they touch | "a macro writes here" marks |
| 16 | Report or dashboard | guess | App outputs; the notebook's last section |
| 17 | Stocks, flows, feedback, delays | guess, resting on 9 and 10 | state boxes and delay marks |
| 18 | Repeated sheets, 3-D sums | certain | a stack; one section with a sheet picker |
| 19 | Checks and reconciliations | likely | "7 of 8 checks pass" in the Strip |
| 20 | Overrides and plugs | certain that a run is broken; a guess why | "11 alike, 1 differs" |
| 21 | Linked workbooks | certain | `◊` boxes at the map's edge |
| 22 | Switches and modes | likely | a switch in App; a mode box listing its cases |
| 23 | Goal Seek and Solver set-ups | certain for Solver; a guess for Goal Seek | target, changing cells and limits in the Inspector |
| 24 | Form or document | guess | Classic and print only |
| 25 | Scratch and dead areas | certain that nothing reads them | "nothing reads these", folded |
| 26 | Named formulas, LAMBDA libraries | certain | definition boxes; the Explorer's function list |
| 27 | Two-way table | likely | one formula item with two header axes |

Three rules cover the places where patterns meet. A region may carry several readings at once: Loan's schedule is a time series, a running balance and a run. The most certain reading decides the default look. An exception inside any pattern is surfaced, never absorbed.

---

## 6. Big and busy workbooks

Inherited is the measure: 40 sheets, 301,482 formulas, one sheet packed `A1:IV900`.

| View | Shows everything | Simplifies, visibly | Declines, with a way forward |
|---|---|---|---|
| **Classic** | Always. Every cell is exact and reachable; only what is on screen is drawn. | Below 40% zoom, block outlines and labels replace cell text. The scroll gutter maps where formulas and errors lie. | Never. |
| **Inspector** | Lists of up to 2,000 rows. | Above that it groups: errors by root, dependents by block and run ("`C7:N7`, 12 alike"), each with a count, opened on demand. | The full precedents of `Summary!C8`: "Rests on 1,104,220 cells in 23 blocks. Showing blocks. **Export the full list** · **Narrow to one sheet**." |
| **Notebook** | A sheet of up to about 150 blocks. | Runs are single items. Frames show 20 rows and the count. Folded sections keep their errors and exceptions in view. `Curves` drafts as 6 items. | The whole workbook: "40 sheets and 1,907 blocks would be about 600 screens. Start from **one sheet** · **the outputs you pick, with what feeds them** · **an outline**." |
| **Model map** | A level of up to 60 boxes. | Like sheets stack; wires bundle, with counts; blocks cluster by region. Every level carries "x of y accounted for". | A level above 2,000 items after clustering: "**Only what feeds the selection** · **Only what has errors** · **Open as a list**." |
| **App** | Few, marked inputs and outputs, as in Loan. | With many candidates it drafts from certain evidence only and lists the rest: "not on the page: 212 other constants ►". | Inherited: "2,300 constants feed formulas and none is marked. **Pick the outputs you care about and I will find what drives them** · **Ask the agent for a draft**." |
| **Formula Explorer** | Arguments of up to 200 cells. | Larger ones are summarised by shape, type and first rows: "showing 20 of 25,000; results use all rows". | A formula beyond Excel's own limits, refused in Excel's words. |

Sampling is allowed in one place only, a windowed frame or argument. It always states "showing n of N" and never feeds a result.

**Budgets the experience must meet,** on a current mid-range laptop. The browser build may take twice as long on the first two lines only.

| Moment | Budget |
|---|---|
| Start to a usable shell | 1 s; the standalone Formula Explorer 0.5 s |
| Open: first paint of the active sheet from saved values, grid usable | 1 s for the four small workbooks; 2 s for Inherited; progress in the Strip within 100 ms |
| Open: every sheet reachable, Overview complete | 5 s; 8 s for Inherited, nothing blocked meanwhile |
| Keystroke to its echo | 16 ms aimed for, 50 ms at worst, in every view, during loading and recalculation too |
| Moving the selection; scrolling | within one frame; never blank for longer than 100 ms |
| Commit to dependents shown | 100 ms on the small workbooks. Otherwise the edited cell at once, dependents hatched, progress after 300 ms, Esc interrupts |
| Switching view | the selection in place within 150 ms; complete within 400 ms; Inherited's sheet map within 500 ms |
| Palette | open in 50 ms; results within 50 ms of each key |
| Anything longer than 1 s | shows progress and can be cancelled; longer than 10 s can continue in the background |

**Never acceptable:** a hang; content dropped without a stated count; a picture that misstates the model. The last has four concrete forms, each a release blocker: an estimate drawn as a fact, a run shown as uniform when it has an exception, a saved or stale value shown as fresh, and a wire that is not in a formula.

---

## 7. Customisation

**What a skin can declare.** A skin is a named declaration with no code in it:

- **theme:** values for Strand's tokens, density, type sizes;
- **layout:** panels open and where, the Deck's tabs and commands, nine quick slots on Alt+1 to Alt+9, extra Strip readouts;
- **keymap:** additions freely; a change to an Appendix A key is allowed but announced ("this skin departs from Excel in 2 keys");
- **commands:** named sequences of existing commands, which drive the interface and never the model directly;
- **panels:** arrangements of standard parts (value, input, frame, chart, trace, text) bound to addresses;
- **view settings,** per workbook and per sheet: the preferred view and the views the file has been set up for, lens and overlays, a notebook's order and prose, a map's layout, App pages. A view the file was not set up for still opens, drafted from the program's readings, and says so;
- **readings:** corrections to what the program inferred.

Code is needed only for a new component, such as a new instrument. A component is installed like an add-in, behind the same trust card, and is never carried inside a workbook.

**Making, trying, sharing.** A person chooses *Customise* (Alt, W, U) and drags panels, edits the Deck, records a command, or edits the declaration as text with live checking. An agent, asked in the palette, writes the same declaration. Either way the result is a **try-on**: the skin is applied under a band, "Trying *Calm review* · Keep · Edit · Discard (Esc)", and nothing is stored until Keep. A declaration that breaks a fixed rule is not applied, and the reason is given in plain words ("accent cannot be red: red is reserved for errors"). A skin lives with the user, or in the workbook's view part, where it travels with the file. It is shared as one text file.

**What stays fixed.** The calculation and the file's content, formats included. The five signals and their shapes. The Strip's readouts. The seam, and Esc as an exact revert. The address grammar. The trust cards. The way back to the grid. The contrast thresholds.

**Example 1: an agent restyles Classic.** Maria asks: "Make Classic calmer for long review sessions: no gridlines, taller rows, inputs in blue, Trace and Watch on the right." The agent returns:

```text
skin "Calm review"          based on: Classic
theme     cockpit-light     grid.lines: none      stage.zoom: 110%
overlay   provenance: on    # typed constants in the constant ink; cell formats untouched
panels    right: [Trace, Watch]
keymap    unchanged
note      "Row height is part of the workbook's formatting, so a skin cannot set it.
           I used 110% zoom instead. If you want the rows changed in the file,
           say so and I will propose that as a change to the workbook."
```

Under the try-on band the gridlines are gone and the constants are blue, as an overlay: switched off, it shows the author's own colours again. She chooses **Keep**. The note is the boundary at work: the one request that would have changed the file was not smuggled into the skin.

**Example 2: an agent builds an App view of Loan from a three-line brief.**

> Make a loan calculator page for branch staff.
> Inputs: amount, rate, term, start date. Show the monthly payment big, then total interest and the payoff date.
> Add the balance chart and a print button. Keep the schedule out of the way.

Starting from the program's readings (four unlocked, validated cells; three outputs; a chart named Balance; a 360-row run), the agent returns:

```text
app-page "Loan"             preferred view: App
inputs    Loan!C3 "Loan amount"     Loan!C4 "Annual rate"
          Loan!C5 "Term (years)"    Loan!C6 "Start date"
headline  Loan!C8 "Monthly payment"
outputs   Loan!C9 "Total interest"   Loan!C10 "Paid off"
chart     Loan!"Balance"
frame     Loan!A11:F371 "Schedule"     show: 3 rows, expandable
scenarios from the workbook            print: page; schedule optional
```

The try-on is M5.1. Maria asks for the first group to be headed "Your loan"; one line changes. **Keep** stores the page in the file and makes App its preferred view. One suggestion arrives separately, in Changes, because it is a change to the model: "Define the names `LoanAmount`, `AnnualRate`, `TermYears`, `StartDate`? 4 names, no value changes." Looks travel by try-on and the model by proposal. The two never mix.

---

## 8. Printing and macros

**What each view prints.**

| View | Prints |
|---|---|
| Classic | Exactly what Excel prints: print areas, titles, page breaks, scaling, headers and footers, page order. Ctrl+F2 and Ctrl+P preview Excel's pagination. |
| Inspector | A report of the open instrument: a trace as an outline with values, errors by root, the links list. Made for an audit file. |
| Notebook | The document in reading order (S2.7). Frames windowed or in full; folded sections as one line each. |
| Model map | The current level, on one page or tiled, with a legend and its "x of y accounted for" line. An optional appendix lists every box with its formula. |
| App | The page as a one-page answer (M5.3). Long frames are optional and state their page count. |
| Formula Explorer | A card: formula, inputs, steps, result. |

Every print except Classic's carries a footer with the file, the scenario in use and the time of the values. "Print as Excel would" is offered from every view.

**What a macro can do to views.**

- A macro's existing verbs keep their meaning in every view. `Select`, `Activate` and `Application.Goto` move the shared selection, and the view shows the item that contains it. `MsgBox`, `InputBox` and user forms appear over any view. `Application.StatusBar` text shows in the Strip, labelled as the macro's. `ScreenUpdating = False` defers painting but never hides "Macro running · Esc interrupts".
- Three verbs are new: switch view, select by address, show or hide a panel. They go through one late-bound call, so the same code is harmless in Excel: `Application.Run "DnaCalc.View", "Notebook"`, `"DnaCalc.Select", "Monthly!C8:N8"`, `"DnaCalc.Panel", "Trace", True`. In Excel the call fails in a way the macro can step over, and the workbook still opens and runs.
- A macro cannot dismiss a trust card, hide the Strip's fixed readouts or change a skin. A switch of view asked for in the middle of the user's edit waits for the commit.
- A macro run is one named undo step, which Excel does not offer. Macros are off until allowed, and a button says why it did nothing (S1.6).

---

## 9. Testing the experience

**The storyboards as scripts.** The participant gets the file in its starting state and the task in plain words: no interface terms, no coaching.

| | Participant | Task, as spoken | Success, and the pass mark |
|---|---|---|---|
| S1 | Excel power user, new to DNA Calc | "You have inherited this. In ten minutes: what is it, what is wrong with it, where do Summary's numbers come from?" | 8 of 10 name two error roots and the missing links, and say the values are as saved. Nobody thinks it recalculated or froze. |
| S2 | Analyst | "Make this readable for your director. Add a sentence with the profit. Services never below 2,500." | 8 of 10 in 15 min: a sentence quoting a live cell, twelve cells changed in one step. 9 answer "Will Excel show this text?" correctly. |
| S3 | Auditor | "D9 does not match the pivot. Why?" | 8 of 10 in 8 min state the time-of-day cause and show the 41 rows. No stray edit survives. |
| S4 | Engineer | "Build this sketch. Show me Stock's formula in the grid." | 7 of 10 in 20 min, stepping on F9. 9 can say where the boxes are stored and what the delay mark means. |
| S5 | Manager, no Excel | "The payment over 15 years at 6.25%? Print it." | 9 of 10 in 5 min, with a page naming the scenario. All recover from the 65% entry unaided. |
| S6 | Add-in developer | "Try awkward inputs, fix what breaks, use it in Budget." | 8 of 10 in 15 min find the `periods = 0` failure, reload without restarting, clear the `#SPILL!`. |
| S7 | Analyst | "Have the price-rise month made an input. Keep what you trust." | 8 of 10 in 8 min: A and B accepted, C not left applied. 9 predict whether a part changes a result. |

Every script is also run by keyboard alone, at 200% zoom and in the high-contrast theme; S2, S3, S5 and S7 with a screen reader, by people who rely on one. After each script comes a five-statement true-or-false quiz about the model ("Operating profit is 923,215"; "The pivot is up to date"), with a pass mark of 90%. It is the direct measure of whether a view misled.

**The screenshots a reviewer should see, for every view.** For each of the five cast workbooks, eight states: at rest; a selection with its peek card; mid-edit with the seam open; mid-calculation or as saved; an error with its root; the lens on; an agent's proposal as ghosts; the print preview. Once per view in addition: Inherited at its limit, with the notice; the first-visit state; the three themes; 200% zoom at 1280 × 720; keyboard focus on each kind of control. A cell of this matrix that cannot be produced is a gap in the design, not in the testing.

---

## 10. Where I would push back

1. **Inspector as a view is half the answer.** Tracing happens beside the thing being traced, as storyboard 3 shows. Build the instruments as panels that dock in every view first. The Inspector view is then those same instruments given the whole stage. Shipped only as a separate view, I expect it to be a place few people visit. I have designed it this way above and flag it here because it stretches what "a view" means.

2. **"Annotations are presentation, and are free" is too generous for annotations that carry meaning.** A write-up's prose and a reviewer's findings are content to the people who wrote them. Kept only where Excel ignores them, they are invisible in Excel, and Excel's own Document Inspector removes custom XML on request, as do other tools that rewrite the file. *Instead:* keep the default, but offer an Excel-visible mirror (cell notes for findings, a generated notes sheet for prose), treat everything in the view part as expendable or rebuildable, and say so at the first save.

3. **The map should be a reader long before it is an editor.** "Dragging an edge rewrites the formulas" is well defined when an existing reference is moved, and when a reference is inserted by pointing. It is not defined when a new wire is dropped on a formula such as `=A*B+C`: the wire has nowhere to go. *Instead:* ship reading, looking inside, calculation chips and the ripple first; then rewiring and pointing; then building on empty paper. Never give wires a meaning that formulas do not have.

4. **"Views can differ per sheet" should mean a preference, not an automatic switch.** If a click on a sheet tab can replace the whole surface, people lose their place. *Instead:* a sheet may carry a preferred view and its own settings in each view; its tab shows a small badge; the surface changes when the user asks, or once, when the file is first opened.

5. **The lens should not invent structure by default.** A formula shown "using LET or lambda structure" that the file does not contain has intermediate names nobody defined. As display it can be taken for the file's content. As an editing surface it leaves open what will be stored. *Instead:* by default the lens substitutes names and marked labels and lays the formula out. "Show as steps" is a further display with its own notice. "Rewrite as LET" is an explicit edit of the model, with a preview.

6. **"With no workbook at all" needs one qualification.** The standalone Formula Explorer needs named values and small ranges to test anything beyond scalars, and people will want to keep a session. *Instead:* no workbook surface, but lines and inputs that save as a plain `.xlsx`. No new file type.

---

## 11. Risks, open questions for the owner, and a milestone order

**Risks, and what limits each.**

1. *The long tail of Classic conformance.* Every missing gesture reads as a bug. Limit: Appendix A as an executable checklist in tiers; "Report a difference from Excel" in the palette.
2. *Readings that are wrong but look authoritative.* Limit: grades in line style, evidence one step away, no model change ever following from a reading alone.
3. *The view part is fragile.* It can be stripped, and its anchors go stale when the file is edited in Excel. Limit: anchor to names and tables where they exist, else to an address plus a fingerprint of labels and formulas; re-match on opening and say so; keep an orphan with a notice.
4. *Proposal fatigue.* Limit: effects on results come first; a part that changes values is never ticked by default.
5. *Inherited-size files in the browser.* Limit: the section 6 budgets as release gates; declining states designed from the start.
6. *Keys the browser keeps.* Limit: an installed mode that captures them, alternates, and every difference stated (Appendix A, Part 12).
7. *Five views feeling like five products.* Limit: one shell, one selection, one formula bar, never more than two panes.
8. *Add-ins and macros where they cannot run.* Limit: plain refusals; saved values, marked.

**Open questions for the owner.**

1. *May view anchors use hidden defined names, so that Excel keeps them in step with structural edits?* **Recommendation: no.** Use visible names and tables, fingerprints otherwise, and offer "Pin with a name" as the user's choice. Hidden names break the rule that every model change is one a user could make and can see.
2. *Should the agent's change trail travel inside the file?* **Recommendation: off by default, opt-in per workbook,** as a summary (who, when, which proposal, which cells), not as transcripts.
3. *Where do boxes made on the map live?* **Recommendation: as named cells on a visible sheet that the user names,** label and value in two columns, the place shown in the seam. Not as formula-only names: state, charts, controls and Excel's audit tools all need cells.
4. *Add-ins and macros in the browser build?* **Recommendation: desktop only at first.** In the browser show saved values with the `◊`, a one-line reason and "Open in the desktop app".
5. *Is key-tip parity with Excel's letters wanted, given "no Microsoft styling"?* **Recommendation: yes.** The letters are behaviour, not artwork, and they are the muscle memory Classic promises to keep. Hold them in a path table apart from the Deck's appearance.

**Milestone order.** No dates. Each ends in storyboard steps that can be tested with people.

| | Milestone | Delivers | Tested by |
|---|---|---|---|
| 1 | Shell and Classic core | Mast, Deck with key tips, Bridge, a Strip with true calculation state; tier 1 of Appendix A; staged opening within budget; addresses; named undo | S1.1–S1.3, S1.7–S1.9 |
| 2 | Reading the file | Blocks, runs, grades and corrections; Overview; Issues by root; Trace and Details as panels; place history | S1.4, S3 |
| 3 | Formula Explorer and lens | The Explorer in a workbook and standalone; the lens's first two levels | S6, S3.5, S2.4 |
| 4 | Notebook | Draft, run items, frames, prose with live quotations, Reading density, print | S2 |
| 5 | Changes and skins | Proposals with effects worked out first, acceptance by part, Revert, cell history; skins as declarations, with try-on | S7; section 7, example 1 |
| 6 | Model map | As a reader: levels, stacks, bundles, chips, the ripple. Then rewiring and pointing. Then building on empty paper | S1.5, then S4 |
| 7 | App | Draft from inputs and outputs, Excel scenarios, compare, print; pages by declaration | S5; section 7, example 2 |
| 8 | Inspector bench, hardening | Calc recording, Watch, Live; Inherited against every budget; components with code | S3.6; section 6 |

Two rules set the order. Each milestone is useful on its own to at least one member of the cast. The instruments that tell the truth about a workbook (1 and 2) come before the views that summarise it (4, 6 and 7), so that every simplification has something exact to point back to.

# Appendices

## Appendix A. Classic: keyboard and mouse conformance

The contract is simple: in Classic, each gesture below does what it does in Excel for Windows. **T1** marks what must hold at first release, **T2** what follows. Part 12 lists every deliberate difference; anything else that differs is a bug. Keys are given for a US keyboard. The Strip's first word is called "the mode word".

### Part 1. Modes and entry

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-1 | A printable key, in Ready | Mode word becomes Enter. The typed text replaces the cell's content. The caret is in the cell, and the same text shows in the formula bar. | T1 |
| A-2 | F2, in Ready | Mode word becomes Edit, caret at the end of the content, in the cell. F2 again switches between Edit and Enter, and to Point where a reference may be typed. | T1 |
| A-3 | Double-click a cell | Edit, with the caret where clicked. If in-cell editing is switched off in options, the double-click goes to the cell's precedents instead. | T1 |
| A-4 | Click in the formula bar | Edit, in the bar, caret where clicked. | T1 |
| A-5 | Arrow keys, in Enter | Commit and move one cell. In a formula, where the caret follows an operator, a comma or an opening parenthesis, the arrow starts pointing instead (Part 4). | T1 |
| A-6 | Arrow keys, in Edit | Move the caret. Home and End go to the ends of the line. Ctrl+← and Ctrl+→ move by word. Shift with any of these selects text. | T1 |
| A-7 | Backspace, in Ready | Clears the active cell only and starts an entry in it. | T1 |
| A-8 | Delete | In Ready: clears the contents of the whole selection and keeps formats, notes and validation. While editing: deletes the character to the right, or the selected text. | T1 |
| A-9 | Ctrl+Delete, while editing | Deletes from the caret to the end of the line. | T2 |
| A-10 | Alt+Enter, while editing | Starts a new line inside the cell. The cell wraps when committed. | T1 |
| A-11 | Insert, in Edit | Toggles overtype. | T2 |
| A-12 | The mode word | Reads Ready, Enter, Edit or Point exactly when Excel's status bar does. Extend Selection, Add or Remove Selection, End Mode and Scroll Lock show beside it when active. | T1 |

### Part 2. Committing, cancelling and what an entry means

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-13 | Enter, Shift+Enter | Commit; move down, or up. Direction, and whether to move at all, follow the Excel option "After pressing Enter, move selection". | T1 |
| A-14 | Tab, Shift+Tab | Commit; move right, or left. After a run of Tabs, Enter returns to the column where the run began, one row down. | T1 |
| A-15 | Ctrl+Enter | Commit and stay. With several cells selected, the entry goes into every one of them; relative references adjust cell by cell. | T1 |
| A-16 | Ctrl+Shift+Enter | Commit as a legacy array formula over the selection, shown in braces. Part of it cannot be changed alone; Excel's message says so. | T1 |
| A-17 | Esc | Cancels the entry. The cell is exactly as it was. | T1 |
| A-18 | Click another cell, while entering | For a value: commit, and select the clicked cell. For a formula, where a reference may go: insert the reference (Part 4). | T1 |
| A-19 | Enter or Tab inside a selection | Commit and move to the next cell within the selection, wrapping at its end. Shift reverses. Ctrl+. moves the active cell to the next corner. | T1 |
| A-20 | How an entry is read | As Excel reads it in the current locale. `5%` is 0.05 with a percent format. `1/2` is a date; `0 1/2` is a half. `1e3`, `$1,000` and `(100)` are numbers. A leading apostrophe forces text. `=`, `+`, `-` and `@` start a formula. Dates and times take the matching format. Digits beyond the fifteenth are lost exactly as in Excel. | T1 |
| A-21 | An entry Excel would refuse | Refused with Excel's message, and with Excel's proposed correction where it offers one. The cell stays in Edit. | T1 |
| A-22 | Text AutoComplete | In a column, the start of an existing entry proposes the rest. Enter accepts; continuing to type declines. Alt+↓ opens the list of the column's entries. | T2 |
| A-23 | Data validation | An entry that breaks a rule is handled by the rule's style. Stop refuses. Warning and Information ask. The rule's own title and message are used. | T1 |
| A-24 | Tables | Typing in the row under a table, or the column beside it, extends the table. A formula typed in a table column fills the column. Ctrl+Z once takes back the automatic part, twice the entry. | T1 |
| A-25 | Dynamic arrays | A formula that returns an array spills. A blocked spill gives `#SPILL!`. Cells of a spill other than its first show the formula greyed and cannot be edited. `@` appears where Excel shows it. | T1 |
| A-26 | A loop, with iteration off | Excel's result: zero, and the circular state reported, with the cell named. | T1 |
| A-27 | Protected sheets | Typing in a locked cell is refused with Excel's message. Tab moves among unlocked cells only. | T1 |

### Part 3. Formula keys: F2, F3, F4, F9 and the rest

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-28 | F4, while editing | Cycles the reference at the caret: `A1`, `$A$1`, `A$1`, `$A1`, `A1`. Both ends of a range cycle together. With text selected, every reference in the selection cycles. | T1 |
| A-29 | F4, in Ready | Repeats the last command, as Ctrl+Y does when there is nothing to redo. | T1 |
| A-30 | F9, while editing | Replaces the selected part of the formula, or all of it, with its value, inside the edit. Esc restores the formula. Enter would keep the value. | T1 |
| A-31 | F9, Shift+F9, Ctrl+Alt+F9, Ctrl+Alt+Shift+F9, in Ready | Calculate all open workbooks. Calculate the active sheet. Calculate everything whether changed or not. Rebuild dependencies, then calculate everything. | T1 |
| A-32 | F3, Ctrl+F3, Ctrl+Shift+F3 | Paste Name, while editing. Name Manager. Create names from the selection's labels. | T1 |
| A-33 | Shift+F3 | Insert Function. Here it opens the Formula Explorer at Functions, with Excel's search, categories and argument fields. Ctrl+A after a function's name opens its arguments. Ctrl+Shift+A inserts the argument names. | T1 |
| A-34 | Formula AutoComplete | After `=` and a letter, a list of functions, names and tables. ↑ and ↓ move. Tab inserts. Esc closes the list and keeps the entry. Inside a table reference, `[` lists columns, `@` means this row, `#` lists the special items. | T1 |
| A-35 | The function tip | Shows the signature with the current argument in bold. A click on an argument selects it in the formula. A click on the function's name opens its help. | T1 |
| A-36 | The range finder | While a formula is edited, each referenced range is outlined on the grid. Dragging an outline moves the reference. Dragging its corner resizes it. The text follows. | T1 |
| A-37 | Ctrl+\` | Shows formulas in cells in place of values, and widens the columns, as Excel does. Again restores. | T1 |
| A-38 | Ctrl+Shift+U | Expands or collapses the formula bar. Its lower edge can be dragged. | T2 |
| A-39 | Alt+= | AutoSum. Proposes `=SUM(` over the numbers above, or else to the left. With a range selected, fills in the totals. | T1 |
| A-40 | Ctrl+' and Ctrl+Shift+" | Copy the formula, or the value, of the cell above into this one, and stay in Edit. | T2 |
| A-41 | Ctrl+; and Ctrl+Shift+: | Insert today's date, or the time now, as a constant. | T1 |
| A-42 | Ctrl+[, Ctrl+], Ctrl+Shift+{, Ctrl+Shift+} | Select the direct precedents, the direct dependents, all precedents, all dependents. Where the first precedent is on another sheet, go there. | T1 |
| A-43 | Alt, M, P; Alt, M, D; Alt, M, A, A | Draw tracer arrows to precedents, or to dependents; remove them. A dashed arrow to a sheet symbol means another sheet. A double-click on an arrow follows it. | T2 |
| A-44 | Alt, M, V | Evaluate Formula. Here it opens the Formula Explorer at Steps, which offers Evaluate, Step In and Step Out as well as the whole tree at once. | T1 |
| A-45 | Error indicators | The cells Excel flags are flagged: a number stored as text, a formula unlike its neighbours, a formula that omits adjacent cells, an unlocked formula, a two-digit year in text. Alt+Shift+F10 opens the flag's menu, with Excel's choices, Ignore among them. | T2 |

### Part 4. Picking ranges while typing a formula

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-46 | Click a cell | Its relative reference is inserted at the caret. If the last thing inserted was itself pointed at, the new reference replaces it. Mode word: Point. | T1 |
| A-47 | Drag across cells; Shift+click | The range is inserted. Shift+click extends the pointed reference to the clicked cell. | T1 |
| A-48 | Ctrl+click | Adds another reference, after a comma. | T1 |
| A-49 | Arrow keys, in Point | Start from the cell being edited and move the pointed cell. Shift+arrows grow it to a range. Ctrl+arrows jump to the edge of the data. Ctrl+Shift+arrows grow to that edge. | T1 |
| A-50 | Click a row or column heading, or the corner | Inserts `5:5`, `C:C`, or the whole sheet. | T1 |
| A-51 | Click a sheet tab, then a cell | Inserts `Sheet!A1`, with the sheet's name quoted where Excel quotes it. Shift+click on a second tab makes a 3-D reference, `Jan:Dec!B5`. Ctrl+PageDown and Ctrl+PageUp change sheet without leaving the entry. | T1 |
| A-52 | Click a cell in another open workbook | Inserts an external reference, absolute, as Excel does: `[Book.xlsx]Sheet!$A$1`. | T2 |
| A-53 | Click in a table | Inserts a structured reference: `Orders[Amount]` for a column, `[@Amount]` for this row from inside the table, `Orders[[#Totals],[Amount]]` for a total. | T1 |
| A-54 | Select the whole of a spill | Inserts the spill reference, `A1#`. | T1 |
| A-55 | Select exactly a named range | Inserts the name, as Excel does. | T2 |
| A-56 | Click a PivotTable value | Inserts `GETPIVOTDATA(…)` when Excel's option for it is on. | T2 |
| A-57 | F4 straight after pointing | Cycles that reference (A-28). | T1 |
| A-58 | An operator, a comma or a parenthesis | Ends pointing and keeps the reference. Enter commits. Esc cancels the whole entry. | T1 |
| A-59 | Scrolling while pointing | The wheel, the scroll bars and Scroll Lock arrows scroll without changing the reference. | T1 |

### Part 5. Navigation

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-60 | Arrow keys | One cell. Merged cells count as one. | T1 |
| A-61 | Ctrl+arrow; End, then arrow | To the edge of the current block of data. From an empty cell, to the next filled cell, or the sheet's edge. | T1 |
| A-62 | Home, Ctrl+Home, Ctrl+End | Start of the row. Cell A1, or the first cell not frozen. The last used cell. | T1 |
| A-63 | PageDown, PageUp, Alt+PageDown, Alt+PageUp | One screen down, up, right, left. | T1 |
| A-64 | Ctrl+PageDown, Ctrl+PageUp | Next sheet, previous sheet. | T1 |
| A-65 | Ctrl+Backspace | Scrolls to show the active cell. | T2 |
| A-66 | Scroll Lock on | Arrow keys scroll the window; the active cell stays. | T2 |
| A-67 | F5 or Ctrl+G | Go To: names, recent places, a reference box, Special. F5 then Enter returns to the place before the last jump. | T1 |
| A-68 | Go To Special (Alt, H, FD, S) | Selects by kind: constants, formulas by result type, blanks, current region, current array, objects, row or column differences, precedents, dependents, last cell, visible cells only, conditional formats, validation. | T1 |
| A-69 | The Name box | Type an address or a name and press Enter to select it. The list shows the names. With cells selected, typing a new name defines it for them. | T1 |
| A-70 | Ctrl+F, Ctrl+H, Shift+F4, Ctrl+Shift+F4 | Find and Replace, with Excel's options: sheet or workbook, by rows or columns, in formulas, values, notes or comments, match case, whole cell, formats, and the wildcards `?`, `*`, `~`. Find next. Find previous. | T1 |
| A-71 | Ctrl+Tab, Ctrl+F6, with Shift | Next, or previous, workbook window. | T1 |
| A-72 | F6, Shift+F6 | Next, or previous, pane. Here the cycle is stage, companion, panel, Deck, Mast, Strip, and the parts of a split sheet. | T1 |
| A-73 | Hyperlinks | A click follows. Press and hold selects the cell. Ctrl+K edits. | T2 |
| A-74 | The wheel | Three rows a notch. Shift+wheel scrolls sideways. Ctrl+wheel zooms between 10% and 400%. Pressing the wheel and moving pans. | T1 |
| A-75 | Scroll bars | Dragging a thumb shows the row or column it will reach. | T2 |
| A-76 | Sheet tabs | A click goes to the sheet. The arrows scroll the tabs; Ctrl+click on an arrow goes to the first or last; a right-click on the arrows lists all sheets. | T1 |
| A-77 | Frozen and split panes | Ctrl+Home goes to the first cell that is not frozen. Frozen rows and columns stay put. Split bars drag; a double-click removes one. | T1 |
| A-78 | Ctrl+Alt+→, Ctrl+Alt+← | Move between the separate areas of a selection. | T2 |
| A-79 | Double-click a cell's border | Jumps to the edge of the data on that side. With Shift, selects up to it. | T2 |

### Part 6. Selection

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-80 | Shift with the navigation keys | Extends the selection by what the key would move: Shift+arrows, Ctrl+Shift+arrows, Shift+Home, Ctrl+Shift+Home, Ctrl+Shift+End, Shift+PageDown. | T1 |
| A-81 | Shift+Space, Ctrl+Space | The whole row, the whole column. In a table: the table's row or the column's data; again adds the header; a third time takes the sheet's column. | T1 |
| A-82 | Ctrl+A | The current region; again, the whole sheet. In a table: the data, then with headers, then the sheet. With an object selected: all objects. | T1 |
| A-83 | Ctrl+Shift+*, Ctrl+/ | The current region. The current array. | T1 |
| A-84 | Ctrl+\\, Ctrl+Shift+\|, Alt+;, Ctrl+Shift+O | Row differences. Column differences. Visible cells only. Cells with notes. | T2 |
| A-85 | F8, Shift+F8 | Extend mode: arrows and clicks extend, until F8 or Esc. Add mode: the next selection is added to the current one. | T2 |
| A-86 | Click, drag, Shift+click, Ctrl+click, Ctrl+drag | Select; extend from the active cell; add a separate area. Ctrl+click on a selected cell removes it from the selection. | T1 |
| A-87 | Headings and the corner | A click selects the row or column; a drag selects several; Shift and Ctrl work as on cells. The corner selects the sheet. | T1 |
| A-88 | Shift+Backspace | Collapses the selection to the active cell. | T2 |
| A-89 | Grouped sheets | Ctrl+click or Shift+click on tabs groups them. Ctrl+Shift+PageDown and PageUp add the next or previous sheet. What is typed goes to every sheet in the group. A click on a tab outside the group ends it. | T1 |
| A-90 | Selection statistics | The Strip shows Sum, Average and Count for the selection. A right-click chooses among those and Numerical Count, Minimum and Maximum. A click on a figure copies it. | T1 |
| A-91 | Objects | A click selects. Ctrl+click adds. Tab moves through the objects. Esc returns to the cells. Ctrl+click selects a form control without operating it. | T2 |
| A-92 | How a selection looks | The active cell is not tinted inside a tinted range. The headings of selected rows and columns are marked. Separate areas are each outlined. | T1 |

### Part 7. The fill handle and filling

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-93 | The handle | A small block at the lower right corner of the selection; the pointer becomes a cross over it. Excel's option can turn it off. | T1 |
| A-94 | Drag the handle | One number copies. A number with text counts up. Two numbers continue their step. Dates step by day. Day names, month names, quarters and custom lists continue. Text copies. Formulas copy with relative references adjusted. Formats travel with them. A tip shows the value the last cell will get. | T1 |
| A-95 | Ctrl while dragging | Swaps copy and series: a single number becomes a series, a date copies. | T1 |
| A-96 | The options button, after a fill | Copy Cells, Fill Series, Fill Formatting Only, Fill Without Formatting, the date units, Flash Fill. | T2 |
| A-97 | Drag with the right button | On release, the fill menu: the above and Linear Trend, Growth Trend, Series. | T2 |
| A-98 | Double-click the handle | Fills down as far as the neighbouring column has data, the left neighbour first, then the right. Stops at the first gap. | T1 |
| A-99 | Drag the handle back into the selection | Clears the cells passed over. They show greyed during the drag. | T2 |
| A-100 | Shift+drag the handle | Inserts cells and pushes the others along; dragged inward, removes. | T2 |
| A-101 | Ctrl+D, Ctrl+R, Ctrl+E | Fill down, or right, from the first row or column of the selection, or from the neighbour for a single cell. Flash Fill. | T1 |
| A-102 | The Fill menu (Alt, H, F, I) | Down, Right, Up, Left, Across Worksheets, Series (with step, stop, type and date unit), Justify, Flash Fill. | T2 |
| A-103 | Filling filtered rows | Only the visible cells are filled. | T1 |
| A-104 | A fill and undo | One fill is one step. | T1 |

### Part 8. Moving and copying by dragging

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-105 | Drag the selection's border | Moves the cells with their formats and notes. Formulas that referred to them follow. A tip shows the target address. Dropping on cells that hold something asks before replacing. | T1 |
| A-106 | Ctrl+drag the border | Copies. The pointer carries a plus sign. | T1 |
| A-107 | Shift+drag the border | Inserts between cells; a bar shows where. Rows and columns are reordered this way. Ctrl+Shift+drag copies and inserts. | T1 |
| A-108 | Alt+drag onto a sheet tab | Carries the cells to that sheet. | T2 |
| A-109 | Drag the border with the right button | On release, a menu: Move Here, Copy Here, Copy Here as Values Only, Copy Here as Formats Only, Link Here, Create Hyperlink Here, and the four shift-and-move or shift-and-copy choices. | T2 |
| A-110 | During any drag | Reaching the window's edge scrolls. Esc cancels the drag and nothing has changed. | T1 |
| A-111 | Dragging turned off | Excel's option "Enable fill handle and cell drag-and-drop" is honoured. | T2 |

### Part 9. The clipboard

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-112 | Ctrl+C | Copies. The source stays outlined until Esc, a new entry or another copy. | T1 |
| A-113 | Ctrl+X | Cuts. The source clears only when pasted, once. References to the moved cells follow them. Esc cancels the cut. | T1 |
| A-114 | Ctrl+V; Enter | Pastes everything: formulas with relative references adjusted, values, formats, validation, notes. One cell pasted into many fills them all. A block pasted into a selection that is a whole multiple of it tiles. Enter pastes once and clears the outline. | T1 |
| A-115 | Ctrl+Alt+V | Paste Special, with Excel's choices: All, Formulas, Values, Formats, Comments and Notes, Validation, All using the source theme, All except borders, Column widths, Formulas and number formats, Values and number formats; the operations Add, Subtract, Multiply, Divide; Skip blanks; Transpose; Paste Link. | T1 |
| A-116 | Ctrl+Shift+V; Ctrl after a paste | Paste values. After any paste, Ctrl opens the paste options, and a letter picks one. | T1 |
| A-117 | Copying several areas | Allowed where Excel allows it, that is, areas sharing rows or columns. Otherwise Excel's message. | T2 |
| A-118 | Ctrl+Shift+=, Ctrl+- | Insert, and Delete, cells, rows or columns, with the dialog that asks which way to shift. With cut or copied cells waiting, Insert places them and shifts the rest. | T1 |
| A-119 | Pasting into a filtered list | One cell copied into a filtered selection fills the visible cells only. A block is pasted into consecutive rows, hidden ones included, as Excel does. | T1 |
| A-120 | Alt+;, then copy | Copies the visible cells only. | T2 |
| A-121 | From other programs | Text with tabs and line ends splits into cells. An HTML table keeps its basic formats. A picture becomes a picture object. "Match destination formatting" is offered. | T1 |
| A-122 | To other programs | Tab-separated text as displayed, an HTML table with formats, and a picture. Formulas travel when the target is a spreadsheet. | T1 |
| A-123 | The format painter | One click paints once. A double-click keeps it until Esc. | T2 |
| A-124 | Objects | Charts, shapes and controls copy and paste as objects. Ctrl+drag duplicates one. | T2 |

### Part 10. Rows, columns, sheets and objects

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-125 | Drag a heading's edge | Resizes, with a tip showing the size. A double-click fits the content. With several headings selected, all take the size. Dragged to nothing, the row or column hides; dragged out again, it shows. | T1 |
| A-126 | Right-click a heading | Cut, Copy, Paste, Paste Special, Insert, Delete, Clear Contents, Format Cells, Row Height or Column Width, Hide, Unhide. | T1 |
| A-127 | Ctrl+9, Ctrl+0, Ctrl+Shift+9, Ctrl+Shift+0 | Hide rows, hide columns, show rows, show columns. | T1 |
| A-128 | Alt+Shift+→, Alt+Shift+←, Ctrl+8 | Group, ungroup, show or hide the outline symbols. The outline's level buttons and plus and minus signs work by click. | T2 |
| A-129 | Sheet tabs | A double-click renames. A drag reorders, with a marker for the target. Ctrl+drag copies. A right-click offers Insert, Delete, Rename, Move or Copy, View Code, Protect Sheet, Tab Color, Hide, Unhide, Select All Sheets. Shift+F11 adds a sheet. | T1 |
| A-130 | Right-click a cell; Shift+F10 | Excel's cell menu in Excel's order, from Cut to Link. DNA Calc's own entries (Trace, Explore formula, Show in) come after a rule at the end. | T1 |
| A-131 | Notes and comments | Shift+F2 adds or edits a note. Ctrl+Shift+F2 starts a comment. Resting on a marked cell shows the note. | T1 |
| A-132 | Filters | Ctrl+Shift+L turns the filter buttons on and off. Alt+↓ on a header opens its menu: sort, search, the list of values, and the number, date or text filters. | T1 |
| A-133 | Objects | Dragging moves; handles resize; Shift keeps proportions; Alt snaps to cell edges; arrow keys nudge; Delete removes. Ctrl+6 hides and shows all objects. A right-click offers Assign Macro and Format Control. | T2 |
| A-134 | Form controls | A click operates the control. A scroll bar's arrows step by its small change, its track by its page change, and its thumb commits on release. A check box writes TRUE or FALSE. A list writes its index. Each writes to its linked cell and the workbook recalculates once. | T1 |
| A-135 | Charts | A click selects the chart, a second its element. Arrow keys move among elements. Delete removes one. Alt+F1 makes a chart of the selection on the sheet; F11 on a sheet of its own. | T2 |
| A-136 | Views of the sheet | Normal, Page Layout and Page Break Preview (Alt, W, L, P or I). In Page Break Preview the breaks can be dragged. | T2 |

### Part 11. Key tips and command keys

| ID | Gesture | Expected behaviour | Tier |
|---|---|---|---|
| A-137 | Alt, or F10 | Key tips appear on the Deck. Letters are then typed one after another without holding Alt. Esc steps back one level. A path typed from memory runs at once, without waiting for the tips to draw. Arrow keys and Tab also move among the Deck's controls; Space or Enter activates one; Alt+↓ opens a list. | T1 |
| A-138 | The first letter | F File, H Home, N Insert, P Page Layout, M Formulas, A Data, R Review, W View. Q goes to the search field, which here is the palette. | T1 |
| A-139 | Home paths | V,V paste values · V,S Paste Special · V,T transpose · FP format painter · 1, 2, 3 bold, italic, underline · FF font · FS size · FC font colour · H fill colour · B borders (A all, O bottom, N none) · AL, AC, AR align · AT, AM, AB vertical align · W wrap · M,C merge and centre · N number format · P percent · K comma · AN accounting · 0 and 9 more and fewer decimals · L conditional formatting · T format as table · J styles · I insert (R rows, C columns, S sheet) · D delete (R, C, S) · O format (H row height, A fit row, W column width, I fit column, U hide and unhide, R rename sheet, M move or copy, T tab colour, P protect) · E clear (A all, F formats, C contents, M comments) · S sort and filter (S ascending, O descending, U custom, F filter, C clear) · FD find and select (F, R, G, S) · U AutoSum · FI fill. | T1 |
| A-140 | Other tabs' paths | Insert: V PivotTable · T table · R recommended charts. Page Layout: M margins · O orientation · SZ size · R,S and R,C set and clear the print area · I print titles · S,P page setup. Formulas: F insert function · N Name Manager · M,D define name · C create from selection · P, D and A,A trace and remove · H show formulas · K error checking · V evaluate · W watch · X calculation options (A, E, M) · B calculate now · J calculate sheet. Data: T filter · C clear · S,A and S,D sort · S,S sort dialog · E text to columns · F,F flash fill · M remove duplicates · V,V validation · W,S scenarios · W,G goal seek · W,T data table · G,G group · U,U ungroup · B subtotal · R,A refresh all. Review: S spelling · C new comment · P,S protect sheet · P,W protect workbook. View: L, P, I sheet views · V,G gridlines · V,H headings · V,F formula bar · Q zoom · J 100% · G zoom to selection · F freeze (F, R, C) · S split · N new window · A arrange · M macros. File: S save · A save as · O open · P print · T options · C close. | T1 |
| A-141 | The older menu paths | Still honoured, as Excel honours them: E,S Paste Special (then V, T, F and so on, and Enter) · E,D delete · E,A,A or C or F clear · E,L delete sheet · E,M move or copy sheet · I,R I,C I,W insert row, column, sheet · I,N,D define name · O,E format cells · O,C,A O,C,W O,R,A O,R,E fit and size columns and rows · O,H,R rename sheet · D,F,F filter · D,S sort · D,L validation · D,E text to columns · D,P PivotTable · D,B subtotals · T,O options · T,U,T T,U,D T,U,A trace and remove · T,G goal seek · T,I add-ins · T,M,M macros · T,P,P protect sheet · W,F freeze · W,S split · F,U page setup · F,V print preview · F,A save as. | T2 |
| A-142 | Command keys | Ctrl+N, O, S, P, W, Z, Y, F, H, G, K, B, I, U · Ctrl+5 strikethrough · Ctrl+1 Format Cells · Ctrl+T and Ctrl+L make a table · Ctrl+Q Quick Analysis · F12 Save As · Ctrl+F2 print preview · Ctrl+F4 close · Alt+F4 quit · F7 spelling · F1 help · Ctrl+F1 fold the Deck · Alt+F8 macros · Alt+F11 the macro editor · Alt+F5 refresh a PivotTable · Ctrl+Alt+F5 refresh all · Alt+' styles · Alt+↓ open a list · Alt+Backspace undo. | T1 |
| A-143 | Format keys | Ctrl+Shift+~ General · ! two decimals with separators · @ time · # date · $ currency · % percent · ^ scientific · Ctrl+Shift+& an outline border · Ctrl+Shift+_ no borders. | T1 |
| A-144 | Format Cells | Six tabs with Excel's fields: Number, with custom codes; Alignment; Font; Border; Fill; Protection. Ctrl+Tab moves between them. | T1 |
| A-145 | Dialogs generally | Tab order, Alt with the underlined letter, Enter for the default button and Esc to cancel, as in the Excel dialog each stands for. | T1 |
| A-146 | Quick slots | Alt+1 to Alt+9 run nine commands of the user's choosing, as the positions of Excel's Quick Access Toolbar do. | T2 |
| A-147 | Undo and redo | Ctrl+Z and Ctrl+Y, and a list from which several steps can be undone at once. | T1 |

### Part 12. Deliberate differences, and the browser

| ID | Where DNA Calc differs on purpose | Why it is a gain |
|---|---|---|
| D-1 | Undo belongs to one workbook, names its steps, and survives macro runs and saves. | Excel's undo is shared by every open workbook and is emptied by a macro. |
| D-2 | The copy outline stands still. The Strip says what is on the clipboard. | Strand allows three animations; this is not one. Nothing is lost. |
| D-3 | References in a formula are numbered and drawn in the teal family; the one at the caret is amber. | Works without colour vision; red stays free for errors. |
| D-4 | No dialogs on opening for links, macros, loops or old formats. One notice line, the Overview and the Strip. | The user is in the file at once, and the facts stay on screen. |
| D-5 | An error has a red tick on the cell's left edge. A note has a teal fold. | Red means one thing. The cell's own formatting is untouched. |
| D-6 | Validation refusals, blocked spills and loops are explained at the cell, with the choices Excel's dialogs offer. | Nothing to dismiss; the context stays in view. |
| D-7 | The seam warns before commit when an entry overwrites a formula inside a run, pastes into hidden rows, or writes to grouped sheets. | The three commonest silent accidents are shown before they happen. |
| D-8 | Grouped sheets show "Group: 3 sheets" in amber in the Strip and on each grouped tab. | Excel says it only in the title bar. |
| D-9 | Below 40% zoom, block labels replace cell text. | Excel shows range names at 39% and below; this goes one step further and keeps text legible. |
| D-10 | A form control shows a ghost value while dragged. It still commits on release. | Feedback during the drag without extra recalculations. |
| D-11 | Shift+F3, Alt, M, V and Alt, M, P open DNA Calc's fuller tools at the matching facet. | The habit is kept and leads somewhere better. |
| D-12 | Not provided: the Office Clipboard pane, Research and Thesaurus, ink. Their keys answer "Not in DNA Calc" in the Strip. | No dead keys. |

| ID | In the browser | What is done about it |
|---|---|---|
| N-1 | An ordinary tab cannot receive Ctrl+N, Ctrl+T, Ctrl+W, Ctrl+Tab, F11 and, in some browsers, Ctrl+PageDown and Ctrl+PageUp. | The installed window receives them all. In a tab, Ctrl+Alt+PageDown and PageUp change sheet and the palette covers the rest. The Strip says so once, naming the key. No key differs silently. |
| N-2 | Reading the clipboard may need the user's permission, and some formats may be withheld. | Asked on the first paste. If a format was not available, the Strip says which and what was pasted instead. |
| N-3 | Some browsers take Alt for their own menu. | Key tips can also be raised from the palette. The installed window has no such limit. |
| N-4 | The right mouse button. | DNA Calc's menu on the stage; Shift+right-click gives the browser's. |
| N-5 | The Back button. | It is the place history, the same as Alt+←. |
| N-6 | Native add-ins, macros that reach the machine, and links to local files. | Not available. Saved values are shown with the `◊` and a reason, and "Open in the desktop app" is offered. |

## Appendix B. Front-end requirements

Each is written so that it can be tested. "Model hash" means any exact comparison of everything Excel would read from the file. References in brackets point to where the requirement is shown.

### The shell and what every view shares

- **R-1** Every view is drawn inside the same shell, with Mast, Deck, Bridge, stage, panels and Strip in the same places. *Test:* the chrome regions occupy the same rectangles in all five views. [M0.1]
- **R-2** The Strip is visible in every view and under every skin, and always shows the mode word, calculation state, error count, outside-data state, macro state and agent state.
- **R-3** The address field shows the model address of the selection in every view. An address typed there, in Go To or in the palette selects that thing in the current view.
- **R-4** The formula bar shows the formula of the selected item in every view where the item stands for cells, and edits it with the keys of Appendix A, Part 3.
- **R-5** SELECTED and EDITING differ in three places at once: the item, the bar and the mode word. No other state changes all three.
- **R-6** After Esc the model hash equals the hash before the edit began. [A-17]
- **R-7** Esc steps back exactly one level, in this order: edit, preview, copy outline, popover, look-inside.
- **R-8** Every shortcut that is DNA Calc's own is Alt+Shift+key or Alt+Q. None takes a key listed in Appendix A.
- **R-9** The palette opens within 50 ms of Alt+Q. It finds commands, addresses, names, sheets, blocks and views, shows each command's key and key-tip path, and offers "Ask the agent" as its last row. [S7.1]
- **R-10** Alt shows key tips. On the first five uses in an installation it also shows the one-line summary of the Alt+Shift layer. [M1.3]
- **R-11** The peek card has the same content for the same object in every view. It appears after 500 ms of rest, never covers its object, and its content is also in the Details panel (Alt+Shift+D).
- **R-12** Keyboard focus is always visible: an amber ring at least 2 px wide with a 1 px offset. When focus leaves the stage, the stage's selection is drawn hollow.
- **R-13** In DNA Calc's own marks, amber is used only for selection, focus and "now"; red only for errors; signal orange only for outside or volatile data; green only for fresh values. Each has a shape as well. *Test:* an audit of every use of the five tokens.
- **R-14** In every view a solid line means stated in the file, a dashed line inferred, a dotted line a ghost.
- **R-15** Nothing animates except the settle of selection and focus, the recalculation ripple (400 ms at most) and the halo on a view switch. All three stop under the reduced-motion setting.

### Views and switching

- **R-16** Alt+Shift+1 to 5 switch view. After the switch the selection stands for the same model objects. There and back restores the exact selection and scroll position. *Test:* a property test over random selections on the cast workbooks.
- **R-17** After a switch the selected item is visible and outlined within 150 ms. Where the item is coarser than the selection, the Strip states the mapping. [S2.1]
- **R-18** Alt+Shift+0 opens and closes the grid alongside. The selection is shared both ways. There are never more than two panes. [M4.3]
- **R-19** A file's preferred view is applied when the file is first opened. A sheet's preferred view is shown as a badge on its tab and is never applied by a click on the tab.
- **R-20** From every view, App included, the cells behind the selected item are one key away.

### Classic

- **R-21** Every T1 item of Appendix A passes its scripted test on the cast workbooks. The differences from Excel are exactly those of Part 12.
- **R-22** Cell formatting is drawn as Excel draws it. No overlay is on by default in Classic.
- **R-23** The key-tip paths of A-139 to A-141 run the same commands whatever the Deck's layout or the skin.
- **R-24** While a formula is edited, each reference has a number and a tone shared by the text and by its outline on the grid. The reference at the caret is amber. [M8.1]
- **R-25** An entry that would overwrite a formula inside a run, paste into hidden rows or write to grouped sheets shows a warning in the seam before commit. [S3.7]

### Honest state

- **R-26** On opening, values from the file are shown within budget under an "as saved" mark that names the date of saving, until they are recalculated. [M1.2]
- **R-27** Under manual calculation, cells whose inputs have changed are hatched until calculated.
- **R-28** In a recalculation longer than 300 ms the Strip shows progress, pending cells are hatched, typing stays within its budget, and Esc interrupts. [S1.8]
- **R-29** After the first recalculation of an opened file, the number of values that differ from those saved is shown and leads to the list.
- **R-30** Opening a file never raises a dialog about links, macros, loops or file format.
- **R-31** Missing links, the macro state, manual calculation and hidden sheets are summed up in one notice line and in the Overview.
- **R-32** Errors are grouped by root. Each root states how many cells depend on it. Enter goes to the root. [S1.4]
- **R-33** Hidden and very hidden sheets are listed and can be looked at without changing the file.
- **R-34** The Mast's state word is one of: new, unchanged, saved, edited, "saved, layout edited", read-only. Undoing back to the opening state shows "unchanged". [S1.7]

### Edits outside the grid

- **R-35** Before commit, every gesture that changes the model, in any view, shows the cells, names and settings it will write, in Excel's syntax. Commit writes exactly that. *Test:* the seam's list equals the change journal.
- **R-36** Every model change made in any view can be reproduced by a person in Excel. A gesture that cannot is refused with a reason and an alternative. *Test:* the saved file opens in Excel without repair and shows identical values.
- **R-37** A presentation change alters no value, formula, name, format or setting. *Test:* the model hash is unchanged across 1,000 random presentation operations in each view.
- **R-38** Editing a run item writes relatively identical formulas to the whole run, as one undo step. [S2.5]
- **R-39** A run with an exception is shown as "n alike, m differ" in every view and print, with the exception one step away. [M2.2]
- **R-40** New model content is placed by the placement rule. The place is in the seam and can be changed before commit. [M4.1]
- **R-41** A view sort or view filter never changes Excel's filter state or row order, and is labelled as a view filter with its counts. [M3.2]
- **R-42** Prose can quote cells. A formula typed in prose is refused, with an offer to add it to the workbook. [S2.3]

### Notebook

- **R-43** A first visit offers a draft built from the readings, within 400 ms on the four small cast workbooks. Drafting leaves the model hash unchanged.
- **R-44** A frame shows its window of rows and "n of N". Folded columns and sections are named, with their exceptions and errors.
- **R-45** A value quoted in prose updates on recalculation and takes its cell's number format.
- **R-46** Reading density hides formulas and sets prose at about 68 characters. Changing density changes no content.
- **R-47** At 200% zoom prose reflows with no sideways scrolling; frames scroll inside themselves.

### Model map

- **R-48** Every wire stands for at least one reference in a formula, and every reference between drawn items is covered by a wire or a bundle. The "x of y accounted for" line is exact. *Test:* compared against the dependency data on the cast workbooks.
- **R-49** Frames of inferred groups and blocks are dashed. Sheets, names and tables are solid.
- **R-50** A delay mark is drawn on a wire only where the system reports that read as last-pass. Otherwise the loop is drawn without one. [S4.4]
- **R-51** Typing `name = formula` on empty paper creates a labelled, named cell as the seam describes. Names not yet defined can be created as empty boxes. [S4.2, S4.3]
- **R-52** Closing a loop while iteration is off raises the three-way choice at the box, with no dialog. [M4.2]
- **R-53** The map is navigable by arrow keys, Ctrl+[ and Ctrl+], Enter, Esc and Tab, and every box has a text equivalent listing what it reads and what reads it.
- **R-54** While a formula is being edited, a click on a box inserts its address.

### App

- **R-55** A drafted App page takes its inputs only from certain evidence: unlocked cells, validated cells and cells linked to controls. Others are added by the user or the agent.
- **R-56** Tab visits inputs only. A validation refusal appears in place with the workbook's own message, and Esc restores the previous value. [M5.1]
- **R-57** The scenarios listed are the workbook's Excel scenarios. "Save as scenario" writes one. Compare shows the difference for each output. [M5.2]
- **R-58** An output in error shows the error text, the red tick, a plain cause taken from the trace root, and an offer to undo. It never shows a blank or an earlier value. [S5.5]
- **R-59** Reset restores every input to its value at opening, as one undo step.
- **R-60** On closing with changes, the prompt lists the changes in plain words. [S5.8]

### Inspector and Trace

- **R-61** Trace opens on the active cell within 200 ms on the cast workbooks. It shows each argument with its value. For functions that test rows it shows the ladder, with the count that fails only each test. [M3.1]
- **R-62** Enter on a line of the ladder shows those rows as a view filter, with the count.
- **R-63** A hint carries its grade and never changes data.
- **R-64** The Calc instrument lists the last recalculation in evaluation order with values before and after. Recording writes nothing to the model. [M8.2]
- **R-65** Every instrument docks as a panel in every view, and enlarges to the Inspector view with its state kept. [S3.6]
- **R-66** With exactly two numeric cells selected, the Strip shows their difference.
- **R-67** Wherever a PivotTable appears, its last refresh and whether its source has changed since are shown.

### Formula Explorer and lens

- **R-68** Opened on a cell, the Explorer lists every precedent with its shown and stored value. Overriding one in the try column never changes the workbook. "Apply to cell" is the only action that writes. [M8.6]
- **R-69** Steps shows the value of every sub-expression and marks the first one that produced an error.
- **R-70** The number-format facet previews any format code on the cell's value and on positive, negative, zero and text samples, exactly as Excel would display them.
- **R-71** The conditional-format facet lists the rules that apply to the cell, in order, with which fired and why.
- **R-72** The standalone Explorer starts within 0.5 s, needs no workbook, loads an add-in behind the trust card, survives an add-in's failure, reloads a changed add-in without restarting, and saves as `.xlsx`. [S6]
- **R-73** The lens changes display only. Resting on a chip shows the plain reference. Inferred labels are visibly unlike defined names. With the lens on, the seam shows the Excel text that will be stored.
- **R-74** A formula that uses an add-in function is marked `◊`. Saving notes what Excel will show without the add-in. [S6.9]

### Readings

- **R-75** Every reading shows its kind, label, grade and evidence. Each correction takes one gesture, is kept in the file, and is there on reopening. [M9.1]
- **R-76** No correction changes the model, except "Pin with a name", which says that it does.
- **R-77** When a file has been edited outside DNA Calc, view items are matched again on opening. Items that moved are marked "re-located". Items that cannot be matched are kept as orphans with a notice.

### Big workbooks

- **R-78** Every budget of section 6 holds on Inherited, and is measured on every build.
- **R-79** Every simplification states a count. *Test:* what is shown plus what is stated as not shown equals what exists.
- **R-80** Every declining state gives the size, the reason and at least two ways forward.

### The agent

- **R-81** The default permission is Propose: nothing is written until accepted.
- **R-82** Each part of a proposal shows its cells, before and after, its effect on results as a count with examples, and the three checks. "No value changes" is computed, never asserted. [M7.1]
- **R-83** Acceptance is by part. Each accepted part is one named undo step. An undone part returns to pending. [S7.5]
- **R-84** Revert writes a new step. Where later edits conflict it refuses and explains. [S7.7]
- **R-85** A cell's history shows who changed it, when, through which proposal, and its content before and after. [M7.2]
- **R-86** The agent's marks are teal, dotted and badged, and never amber.
- **R-87** Changing settings, allowing macros, loading add-ins, saving, and sending data outside each require a question to the user.

### Undo and clipboard

- **R-88** Undo belongs to one workbook. Steps are named. A macro run, a fill, a paste and a run edit are each one step. Undo survives saving.
- **R-89** In any view, Ctrl+Z undoes the more recent of the last model step and that view's last presentation step. The Strip names what was undone and of which kind. [S4.9]
- **R-90** Copy places tab-separated text, HTML and a picture on the clipboard. Paste follows the table below. A paste that creates model content shows the seam first.

| Copied | Into Classic | Into Notebook | Into Map | Into another program |
|---|---|---|---|---|
| Cells | Excel's rules in full | a frame that quotes them, or text | boxes: cells and names, listed in the seam | tab-separated text, an HTML table, a picture |
| A frame or formula item | the cells behind it | a second showing of the item | the same | a table of values; the formula as text |
| A box or frame of the map | its cells and names | a quote of its value | boxes, with name clashes listed | a picture and `name = formula` lines |
| Trace or Issues rows | addresses and values | text with live quotes | not offered | an outline and a table |
| Text or a table from outside | Excel's rules | a text item; a table asks "text or cells?" | `name = formula` lines become boxes | |


### Skins

- **R-91** A skin is a declaration. Applying any skin leaves the model hash unchanged.
- **R-92** A new or changed skin is applied as a try-on with Keep, Edit and Discard; Esc discards. A declaration that breaks a rule is rejected with the reasons in plain words.
- **R-93** No skin can hide the Strip's fixed readouts, re-bind a signal role, break a contrast pair, or remove Alt+Shift+1 and Alt+Shift+0.
- **R-94** A skin that changes a key of Appendix A is labelled with the number of keys it changes.

### Printing and macros

- **R-95** Classic's printed pages match Excel's for the cast workbooks: the same count and the same breaks.
- **R-96** Every print from another view carries the file, the scenario in use and the time of the values.
- **R-97** Macros are off until allowed. A control that is blocked explains which procedure it would run and what that code calls. [S1.6]
- **R-98** A macro run is one undo step. "Macro running · Esc interrupts" is visible for as long as it runs.
- **R-99** The three view verbs work through `Application.Run`, and the same code fails harmlessly in Excel.

### Accessibility

- **R-100** Every function can be reached and operated by keyboard alone. Every storyboard passes without a mouse.
- **R-101** To a screen reader the grid gives each cell's address, value, formula and state. The other views present their items as documents, trees or lists, with what each reads and what reads it. Changes in calculation state and error count are announced without interrupting.
- **R-102** Contrast meets Strand's law in all three themes: 4.5:1 for text and small marks, 3:1 for large text and meaningful shapes.
- **R-103** The stage zooms from 10% to 400%. Interface scale is separate. At 200% on a 1280 × 720 screen no function is lost, and the views other than Classic reflow.
- **R-104** No information is carried by colour alone or by motion alone.

### Files

- **R-105** A saved file opens in Excel without a repair prompt. View settings are in a part that Excel preserves. Saving notes anything Excel will show differently.
- **R-106** If the view part is lost, the model is intact and every view still works from the program's readings.

## Appendix C. What the design needs from the system underneath

Plain needs, in the front end's words. Nothing here says how they are met. Where a need has a speed, it is the budget of section 6.

### Opening and state

- **C-1** Open a file in stages: first the sheet names and sizes, then the active sheet's saved values and formats, then the rest, reporting progress throughout.
- **C-2** Be given the values saved in the file separately from recalculated values, with the date and the program that saved them.
- **C-3** Know, for any cell, whether its value has been calculated since loading and since its inputs last changed.
- **C-4** Be told which cells changed after a recalculation, with old and new values, and what set the recalculation off: an edit, F9, a live source or a macro.
- **C-5** Be told the order in which cells were evaluated in the last recalculation.
- **C-6** Know how far a running calculation has got, be able to interrupt it, and be able to read values and accept typing meanwhile.
- **C-7** Know the workbook's calculation settings (mode, iteration, maximum iterations, maximum change, precision as displayed, the date system) and change them as a user could.

### Dependencies and structure

- **C-8** Know which cells a formula depends on, and which depend on it, directly and in full, and have this summarised by block and by sheet with counts.
- **C-9** Know the dependencies that pass through names, tables, spills and functions that build references, and have the dynamic ones marked as such.
- **C-10** Know references from name to name and from name to cell as names, not flattened to cells.
- **C-11** Know which formulas on a sheet are identical in relative terms, that is, the runs, and be told when a run gains or loses a member.
- **C-12** Get the structure of a formula, with each part's position in the text.
- **C-13** Turn a structure back into exact Excel text; replace one reference with another; rename a name everywhere it is used.
- **C-14** Evaluate a formula step by step: the value of every sub-expression, and which one first produced an error.
- **C-15** For functions that test rows (SUMIFS, COUNTIFS, AVERAGEIFS, FILTER and the lookups): which rows passed each test, which fail only one test, and which values were skipped and why.
- **C-16** For any error value, the cell or cells where it first arose.
- **C-17** Evaluate a formula, or a set of pending edits, without committing them, and report which values would change.
- **C-18** Know before commit whether a pending edit would close a loop, block a spill or break a validation rule.
- **C-19** Know each loop: the cells that form it.
- **C-20** Know for a spill its first cell, its extent and what blocks it.
- **C-21** Inside a loop under iteration, know for each reference whether its reader saw this pass's value or the previous pass's.
- **C-22** Know how an iteration ended: the passes run, and whether the change limit was met.

### What workbooks hold

- **C-23** Tables: name, range, columns with their types, totals row, filter and sort state; and be told when a table grows.
- **C-24** PivotTables: source, fields, time of last refresh, and whether the source has changed since.
- **C-25** Data tables with their input cells. Scenarios with their changing cells and values; apply one, create one, and produce a scenario summary without adding a sheet.
- **C-26** Form controls with their linked cells, limits and steps; write through one as a user's click would.
- **C-27** Validation rules and their messages for a cell; test a candidate value against them.
- **C-28** Conditional formats: the rules that apply to a cell, their order, and which fired.
- **C-29** Number formats: show any value under any format code exactly as Excel would, colours and locale included, and tell the stored value from the shown one.
- **C-30** Protection: locked and hidden flags, sheet and workbook protection, and what each permits.
- **C-31** Sheet visibility, very hidden included; read a hidden sheet without changing its state.
- **C-32** Charts, shapes and pictures: their names and source ranges, and a rendering of each.
- **C-33** Print set-up, and pagination exactly as Excel would paginate.
- **C-34** Comments and notes: read, add, edit, remove.

### The outside

- **C-35** External links: the target, found or missing, the values saved in the file, the last update. RTD topics: server, topic, last value and its time, and each update as an event.
- **C-36** Which functions in use are volatile, and which come from add-ins.
- **C-37** Add-ins: load, unload and reload one; what it registered (names, arguments, help text, flags); call one of its functions with given arguments; time the call; and be told when the add-in fails, without the application failing.
- **C-38** Macros: list modules and procedures without running anything; what each control and event calls; which object-model calls a procedure contains; run one with the ability to interrupt; and be told what the run changed.
- **C-39** Receive a macro's requests to switch view, select, show a panel, set status text or show a dialog.

### Changes and history

- **C-40** Apply a group of edits as one step; undo and redo it; name it; record who made it: the user, a macro or the agent.
- **C-41** Revert an earlier step if nothing later touched the same cells, and be told the conflicts otherwise.
- **C-42** Compare model content exactly, so that "unchanged" and "presentation only" can be proven and not merely claimed.
- **C-43** Keep the ranges that view settings refer to in step with inserts, deletes, moves, renames and sorts made here. Provide enough of a fingerprint (labels, formula shape, size) to find them again after edits made elsewhere.
- **C-44** Read and write a part of the file that Excel preserves and ignores, and detect when it is missing or older than the model.
- **C-45** Before saving, be told anything in the workbook that Excel would refuse, repair or show differently.
- **C-46** Put on the clipboard, and accept from it, the formats Excel uses for cells and objects.

### Readings, the agent, and size

- **C-47** Report recognised structures with their evidence: blocks, headers and labels, period axes, inputs, outputs, sheets of like layout, lookups, checks, cells that nothing reads.
- **C-48** Quick counts for a sheet and a workbook without a full calculation: formulas, constants, errors by kind, the used range.
- **C-49** Offer every command and address to an agent exactly as to the palette; let its reading proceed without blocking the user; hold its writes as proposals.
- **C-50** Answer all of the above on a workbook the size of Inherited within the budgets of section 6, or say that an answer is partial and how partial.

## Appendix D. The pattern catalogue in full

For each pattern: how a person recognises it; how the program does, and how sure it can be; how the reading is confirmed or corrected; and what each view does with it. Classic never changes what it draws because of a reading: there, a reading appears only in the Details panel, the Outline, and the optional block outlines below 40% zoom, so "Classic" below lists only what is added. The cast workbook where the pattern occurs is named in brackets.

**D-1 Sheet as database** [Sales, `Orders`]. *A person sees* one header row, one kind of thing per row, no gaps, and usually a filter. *The program sees* a table object (certain), or a filled rectangle with one header row and one data type per column that criteria functions or a pivot read (likely). *Correct by* moving the header row, extending the range, or "Make it a table" (Excel's Ctrl+T, a model edit). *Classic:* row count in Details. *Inspector:* column types, blanks, numbers stored as text, duplicate keys. *Notebook:* a frame; never one item per formula. *Map:* one box, with the columns that are read listed on it. *App:* a frame with search and a view filter; never an input.

**D-2 Inputs and assumptions** [Budget `Inputs`; Loan `C3:C6`]. *A person sees* labelled constants set apart, often coloured, often on a sheet named for them. *The program sees* constants with a label beside or above that formulas read: certain where the cell is unlocked on a protected sheet, validated or linked to a control; likely otherwise. *Correct by* "Not an input" or "Also an input" on any cell, and by editing its label. *Classic:* the provenance overlay shows them in the constant ink. *Inspector:* each input with the number of results it reaches. *Notebook:* the opening section, each input as a value item. *Map:* boxes in the constant ink at the left. *App:* the fields.

**D-3 Calculation block** [Budget `Monthly!B5:O9`]. *A person sees* a rectangle of formulas with labels down one side and a heading above. *The program sees* adjacent runs sharing an axis, bounded by blank rows or columns (likely). *Correct by* extend, shrink, split, merge, rename. *Inspector:* its inputs, its outputs and its exceptions. *Notebook:* a section, each row a formula item, with one frame of values. *Map:* a dashed frame, its rows as boxes one zoom level in. *App:* nothing, unless a row is chosen as an output.

**D-4 Time series, periods across** [Budget `Monthly`; Loan's schedule runs down]. *A person sees* months or years along the top and the same formula across. *The program sees* a header of dates or period labels in equal steps (certain when they are true dates) over rows that are each one run (likely). *Correct by* naming the header row, setting the period, marking where history ends and forecast begins. *Inspector:* runs with their exceptions; the first period where a row's formula changes. *Notebook:* a frame with periods across and the lens form `Customers[prev]`. *Map:* `[t-1]` on rows that read their own previous period. *App:* a chart over the periods.

**D-5 Scenarios** [Budget's two columns and selector; Loan's saved scenarios]. *A person sees* columns headed Base and Downside with a cell that picks one, or knows the Scenario Manager was used. *The program sees* saved scenarios (certain), or parallel input columns read through CHOOSE, INDEX or IF on one cell (likely). *Correct by* naming the selector and the columns. *Inspector:* the inputs that differ between scenarios, and the results that move. *Notebook:* a scenario control at the top; quoted values follow it. *Map:* a switch box feeding the "in use" column. *App:* chips, Compare, Save as scenario.

**D-6 Sensitivity table** [none in the cast; common beside a Loan]. *A person sees* a grid of results with input values along two edges and a formula in the corner. *The program sees* a data table (certain). *Correct by* nothing: it is stated in the file. *Inspector:* its row and column input cells; the cost, "recalculates the model 25 times". *Notebook:* a frame with both axes named. *Map:* a box marked "runs this part N times". *App:* a shaded table, read only.

**D-7 Reference table** [Sales `Reps`; Inherited `Lookup`]. *A person sees* a small list of codes and meanings, off to one side. *The program sees* a block of constants read by lookup functions or by a validation list (likely). *Correct by* naming the key column. *Inspector:* keys looked up and not found; duplicate keys. *Notebook:* a frame marked lookup, after the section that uses it. *Map:* a side box with join wires labelled by key. *App:* the source of a drop-down.

**D-8 Pipeline across sheets** [Budget: Inputs, Monthly, Summary; Inherited: Deals, Positions, P&L by Desk, Summary]. *A person sees* sheets ordered left to right from raw to finished. *The program sees* references between sheets that run one way (likely; certain that no reference runs back). *Correct by* reordering the stages or leaving a sheet out. *Inspector:* any reference that runs against the flow. *Notebook:* the Across layout. *Map:* the sheet zoom, laid left to right. *App:* inputs from the first stage, outputs from the last.

**D-9 Running balance** [Loan `F12:F371`]. *A person sees* opening, movement, closing, repeated down or across. *The program sees* a run in which each cell reads its neighbour in the run (likely). *Correct by* marking the opening cell. *Inspector:* where the chain is broken or overwritten. *Notebook:* one item, `Balance = Balance[prev] − Principal`, with its opening value stated. *Map:* a box with `[t-1]`. *App:* a line chart.

**D-10 Deliberate circular reference** [Stock]. *A person* knows it because they built it, or because Excel complained. *The program sees* a loop that exists while iteration is on (certain); with iteration off, a loop is reported as an error state, as in Excel. *Correct by* marking a loop "intended" or "accidental", which changes only how loudly it is reported. *Classic:* the Strip's state. *Inspector:* the loop's cells, the settings, how the last iteration ended. *Notebook:* a note on the item, "reads its own last value". *Map:* the loop, with delay marks where they can be reported. *App:* an F9 step control.

**D-11 Simulation** [none in the cast]. *A person sees* a column of random draws and summary statistics, and numbers that change at every keystroke. *The program sees* RAND, RANDARRAY or RANDBETWEEN feeding many rows, or a data table driven by an unused cell (likely). *Correct by* naming the trial count and the outputs. *Classic:* a "volatile" note in Details. *Inspector:* how many cells recalculate on every change, and how long that takes. *Notebook:* a frame of trials and a histogram of the output. *Map:* a box marked `◊ volatile`. *App:* a "Draw again" control, which is F9.

**D-12 Control panel** [Stock's slider; Budget's selector]. *A person sees* sliders, boxes and lists gathered on one sheet. *The program sees* form controls and their linked cells (certain); that the sheet is a panel is a guess. *Correct by* grouping or ungrouping the controls. *Inspector:* each control, its linked cell and what that reaches. *Notebook:* the controls, in line, live. *Map:* controls drawn on their boxes. *App:* the first draft of the page.

**D-13 PivotTable** [Sales]. *A person sees* the banded layout and field buttons. *The program sees* the pivot and its cache (certain). *Correct by* nothing. *Classic:* freshness in Details. *Inspector:* source, last refresh, whether the source has changed. *Notebook:* a frame with its freshness line. *Map:* a box, "snapshot of Orders". *App:* a read-only table with Refresh.

**D-14 Live data** [none in the cast; Watershed's clock and price feed]. *A person sees* numbers that move by themselves. *The program sees* RTD formulas, linked data types, web functions, NOW and TODAY (certain). *Correct by* nothing. *Classic:* `◊` in the overlay; the Strip's count of live topics. *Inspector:* the Live instrument: each source, its rate, its last value and time, and a strip of events. *Notebook:* values with their time of arrival. *Map:* source boxes in signal orange at the edge. *App:* a "live" mark and the time of the last update on any tile fed from outside.

**D-15 Macro-driven model** [Inherited `Module1`]. *A person sees* buttons, and values that appear after pressing them. *The program sees* modules, the controls and events that call them (certain), and the ranges named in the code (likely). *Correct by* nothing; the user chooses whether to allow macros. *Classic:* buttons that explain when blocked. *Inspector:* procedures, callers, and the calls each makes. *Notebook:* a note where a macro writes into a block. *Map:* a "written by a macro" mark on those boxes. *App:* a button, behind the same trust card.

**D-16 Report or dashboard** [Budget `Summary`; Sales `Report`]. *A person sees* formatting, charts, few inputs and a print area. *The program sees* a sheet that nothing reads, with charts and a print area (guess). *Correct by* "This is an output" on a sheet or a block. *Inspector:* everything the report rests on. *Notebook:* the closing section, or the opening one under "Conclusions first". *Map:* the right-hand end. *App:* the outputs.

**D-17 Stocks, flows, feedback and delays** [Stock]. *A person* thinks of it that way, or drew it that way. *The program sees* a state cell that adds some terms and subtracts others each period or pass, and chains of last-pass reads (guess, resting on D-9 and D-10). *Correct by* marking a cell as a stock, a flow or a delay. *Inspector:* the Calc instrument and Watch. *Notebook:* a narrated model, state items first. *Map:* its native form. *App:* controls, a step key and a chart of the stock.

**D-18 Repeated sheets and 3-D sums** [Inherited `Jan … Dec`]. *A person sees* a row of tabs named for months or branches that look alike. *The program sees* sheets whose formulas match cell for cell (certain), and 3-D references across them (certain). *Correct by* adding or removing a sheet from the set. *Classic:* a set badge on each tab. *Inspector:* cells where one sheet of the set differs from the others. *Notebook:* one section with a sheet picker. *Map:* a stack, "12 alike". *App:* a picker.

**D-19 Checks and reconciliations** [typical of a Budget; none in the cast file]. *A person sees* cells that say OK, or a difference that should be zero. *The program sees* a formula comparing two totals and returning a flag, or the difference of two sums (likely). *Correct by* "This is a check" on any cell, with the value that counts as a pass. *Classic:* the Strip, "7 of 8 checks pass". *Inspector:* the list, failures first, each with both sides traced. *Notebook:* a line at the end of its section. *Map:* a small mark on the frame. *App:* a warning line when any check fails.

**D-20 Overrides and plugs** [Budget `Monthly!H13`]. *A person* remembers typing it, or does not. *The program sees* a constant, or a formula with an added constant, inside a run (certain that the run is broken; a guess whether it is meant). *Correct by* "Noted exception", with a reason. *Classic:* Excel's own indicator, and the seam's warning when one is about to be made. *Inspector:* all exceptions, noted and not. *Notebook, Map:* "11 alike, 1 differs". *App:* a mark on any output that rests on an exception that has not been noted.

**D-21 Linked workbooks** [Inherited]. *A person sees* file names in formulas, or Excel's prompt. *The program sees* external references and the values saved for them (certain). *Correct by* pointing a missing link at a file, which is Excel's Edit Links. *Classic:* `◊`. *Inspector:* the Links list: found or missing, last updated, cells fed. *Notebook:* the source named under the frame it feeds. *Map:* boxes at the edge. *App:* a "from another file, as saved" mark.

**D-22 Switches and modes** [Stock `Shop.Reset`; Budget `Scenario`]. *A person sees* a cell holding 1 or 2, TRUE or FALSE, or a word. *The program sees* IFS, SWITCH, CHOOSE or nested IF on one cell (likely). *Correct by* naming the cases. *Inspector:* the formulas each case turns on. *Notebook:* a control with the cases named. *Map:* a mode box listing the cases, the current one marked. *App:* a toggle or a list.

**D-23 Goal Seek and Solver set-ups** [none in the cast]. *A person* remembers running it. *The program sees* Solver's saved settings (certain); for Goal Seek nothing is stored, so only a guess from a result that sits exactly on a round target. *Correct by* nothing for Solver. *Inspector:* target, changing cells and limits. *Notebook:* a note, "solved for X by changing Y". *Map:* a dashed link from the target back to the changing cells. *App:* nothing by default.

**D-24 Form or document** [some of Inherited's sheets]. *A person sees* an invoice or a statement: merged cells, borders, a logo. *The program sees* many merged cells, borders, a print area and few formulas (guess). *Correct by* "This is a form". *Classic:* as it is; this is Classic's ground. *Inspector:* the few cells that feed it. *Notebook, Map:* one item, "a printed form", with its inputs. *App:* its blank cells as fields, if the user asks.

**D-25 Scratch and dead areas** [Inherited's twelve unread sheets]. *A person sees* workings off to the right that nobody mentions. *The program sees* cells that nothing reads and that are not a report (certain that they are unread; a guess that they are scratch). *Correct by* "Keep in view" or "This is an output". *Inspector:* the list, with sizes. *Notebook:* folded under "not used by anything". *Map:* a collapsed group. *App:* nothing.

**D-26 Named formulas and LAMBDA libraries** [none in the cast]. *A person sees* names in the Name Manager that hold formulas. *The program sees* names bound to LAMBDA or to a formula (certain). *Correct by* nothing. *Inspector:* where each is used. *Notebook:* a definitions section. *Map:* definition boxes, drawn once and referred to by name. *App:* nothing. *Formula Explorer:* they appear in the function list and can be called and stepped into.

**D-27 Two-way table** [typical of price lists and allocations]. *A person sees* a grid with headings on two sides and one formula everywhere. *The program sees* a rectangle of formulas with one row-fixed and one column-fixed reference (likely). *Correct by* naming the two axes. *Inspector:* one run in two directions; exceptions. *Notebook:* one formula item with both axes named, and a frame. *Map:* one box with two inputs. *App:* a shaded table.
