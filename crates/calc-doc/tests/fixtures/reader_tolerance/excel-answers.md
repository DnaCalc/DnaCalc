# What Excel does with four defects of real files

Measured 2026-11-11 on this machine (Excel 16.0.20527.20150) with workbooks we authored
(`make-fixtures.py`; no third-party content), each defect beside a control that differs from it
in the defect alone. Nothing here was opened from a corpus. The four defects are the ones that
`cargo xtask classify` over the Enron corpus reported as the reader's next blockers (the first
three kinds: 400 files, 108 files, 7 files; the real shapes are in the bullets).

## How it was asked

- `open-in-excel-job.ps1`: the session host's owned automation Excel (`Workbooks.Open`, read
  only), once with the default load and once with `CorruptLoad = xlRepairFile`. Output:
  `excel-answers/automation-normal-load.json`, `automation-repair-load.json`,
  `automation-repair-load-stale-array.json`.
- `open-by-ui-job.ps1`: a plain Excel process of our own started with the file on its command
  line, which is what File > Open does. Every dialog is recorded with UI Automation and dismissed
  through its Yes button; the workbook is then read through that process's own object model
  (process ID checked) and Excel's own save of it is kept (`SaveCopyAs`). Output:
  `excel-answers/file-open-ui3.json`, `-ui4.json`, `-ui5.json`, and the sheet lists of Excel's
  saved copies in `sheets-of-the-workbooks-excel-saved-after-opening.json`.
- Every run was on the private desktop (`tools/excel-session/on-private-desktop.ps1`); each guard
  file recorded 0 windows and 0 foreground hits on the owner's desktop. Only Excel processes these
  scripts started were touched; no Trust Center, macro or registry setting.
- Limit: the second dialog Excel shows after a repair ("Repairs to ..." with the removed
  records) is an Office dialog whose text UI Automation does not expose; its title is recorded,
  its text is not. The repaired workbook is read instead.

## Answers

| Defect (fixture) | Automation default load | File > Open | What the opened workbook shows |
|---|---|---|---|
| `r:id=""` on a sheet, with no part (`empty-rid-veryhidden`, `empty-rid-visible`; the shape of 687 sheets in 400 Enron files, VBA modules saved by Excel) | opens | opens, no prompt, title not "Repaired" | the sheet is there, empty, with its own state (a veryHidden sheet stays veryHidden, a visible one stays visible) |
| `r:id="rId9"`, no such relationship (`dangling-rid`) | refused (0x800A03EC) | repair prompt "We found a problem with some content in '...'. Do you want us to try to recover as much as we can? If you trust the source of this workbook, click Yes." (Yes, No), then a "Repairs to" dialog; title "Repaired" | the sheet is kept, empty; Excel's save writes it with its sheetId and a part |
| two sheets with one `sheetId` (`duplicate-sheetid`, `-gap`, `-lower`, `-triple`, `-two-pairs`; 108 Enron files) | refused | the same repair prompt | both sheets keep all their cells; Excel's save gives each later repeat the next id above the highest declared: 1,109,109 gives 1,109,110; 1,1,5 gives 1,6,5; 1,1,1 gives 1,2,3; 2,2,3,3 gives 2,4,3,5 |
| `<f ca="1"/>` (no text) on a member of a multi-cell array's area (`array-inner-empty-f`; what Excel itself writes, in 7 Enron files) | opens | opens, no prompt | the cell is a member of the array, like the control |
| a formula with text in a legacy array's area (`array-inner-formula`, `-stale`) | refused | the repair prompt | the formula is dropped; the cell shows the array's result (also with a wrong cached value in the area: Excel recalculates it) |
| a worksheet cut off inside a row (`watcher-control-malformed-sheet`) | refused, also with xlRepairFile | the repair prompt | sheet Two is empty; this file is the positive control for the dialog watcher |

Corpus note: the 7 Enron array files carry `<f ca="1"/>`, the member marker. The reader accepted
that marker only inside a dynamic array's area; Excel opens it inside a legacy array's area too.
The plain formula with text inside an array area is not in the corpus; it is measured here because
it is the same reader check.

## The reader

- `r:id=""`: an empty sheet that keeps its name and state; no repair is reported. It is carried
  as a preserved sheet of kind "sheet without a part": a save that leaves it alone writes the
  package as it was; an edit of that sheet is refused (`SaveRefusal::PreservedSheetChanged`).
- An `r:id` naming no relationship, a repeated `sheetId` and a formula with text in an array's
  area: read as Excel shows them after its repair, and reported in `LoadedWorkbook::repairs()`.
- Still refused with a typed error (Excel has no measured answer here): a sheet without an `r:id`
  attribute, a relationship that is external, a relationship whose part is missing.
- The array-member repair is limited to a plain formula with text in a legacy array's area; a
  shared, array or data-table formula there, and any formula in a data table's area, is still
  refused.
- Not measured, so not claimed: several formulas dropped from one array's area is the same rule
  applied per cell; a repeated `sheetId` together with a dangling `r:id`; a template merge
  (`save/template_merge.rs`) of a workbook whose file still holds the repeated ids.
