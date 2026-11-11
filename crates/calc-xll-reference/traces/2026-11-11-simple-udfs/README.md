# Simple UDFs from .xlam, .xlsm and .xll in both desktop apps, against Excel (2026-11-11)

dc-0f1k.4 and dc-0f1k.10. Excel 16.0.20527 in an owned hidden session on a private desktop
(`tools/excel-session/on-private-desktop.ps1`; every file in `guards/` records 0 windows and 0
foreground hits on the owner's desktop). Pass = matched Excel; fail = mismatched + refused +
missing, over all cases of the set.

## The fixture

`fixture/vba/DnaSimple.bas`: plain worksheet functions, no object model beyond
`Application.WorksheetFunction`: numbers, text, logicals, `Optional` (default, omitted, skipped),
`ParamArray`, arrays out (2-D, 1-D, `Array()`, `Split`, a Double array in a Variant), arrays in
(2-D, one row, one column, `SEQUENCE`), `CVErr` with the XlCVError constants, raised and trapped
errors, overflow, Variant coercion of every argument kind into Double, Long, String and Boolean
parameters, `Date` and `Currency` results, `Round`/`Format$`, `Select Case`, recursion, a Private
function called from a cell, a module-qualified call. `vba-job.ps1 -Action build` puts it into
`DnaExamples.xlam`/`.xlsm` beside `DnaExamples.bas` and alone into `DnaSimple.xlsm`.
`fixture/vba/simple-udf-cases.json` holds the 109 cases with Excel's answers
(`excel/simple-xlam.*`: the .xlam open, formulas in a new workbook). The same 109 formulas entered
inside `DnaSimple.xlsm` give the same answers in Excel (`excel/simple-inbook-dnasimple-xlsm.*`,
`vba-job.ps1 -InBook $true`). `x9-vba-cases.json` (26 DnaExamples cases) was captured again with
the rebuilt .xlam and is unchanged (`excel/x9-xlam.controller.jsonl`).

## Result

| Set (cases) | DNA OneCalc desktop | DNA Calc desktop |
|---|---|---|
| VBA .xlam, simple set (109) | 109 matched, 0 mismatched, 0 refused | 109 matched, 0 mismatched, 0 refused |
| VBA .xlam, DnaExamples cases (26) | 26 / 0 / 0 | 23 / 0 / 3 refused |
| VBA .xlsm, simple set (109) | 109 / 0 / 0 (DnaExamples.xlsm session) | 0 / 0 / 109 refused (DnaSimple.xlsm) |
| VBA .xlsm, DnaExamples cases (26 + 1 module edit) | 27 / 0 / 0 | 0 / 0 / 26 refused |
| XLL, own X1 fixture (289) | 289 / 0 / 0 (page acceptance, see below) | 289 / 0 / 0 |

Routes:
- OneCalc: `cargo xtask app xll-accept onecalc` with `fixture/m5-acceptance.json` (sets
  `dnaexamples-xlam`, `dnaexamples-xlsm`, `simple-xlam`, `simple-xlsm`), driving the desktop exe's
  page in WebView2: Load path for the .xlam, the page's file input for the .xlsm, Enable macros,
  each formula in the formula field, Save; Excel then reopened each of the 136 saved .xlsm files:
  0 repaired, 0 repair logs, every one with its VBA project, Excel's value equal to the page's
  (`onecalc/m5-summary.json`, `onecalc/excel-reopen-*.json`).
- DNA Calc: `run-dnacalc.py` runs `dna-calc-desktop --engine --xll <file>` (or `--workbook <file>`)
  `--formula <case>` once per case (the engine harness stops at its first refusal): CalcHost's
  checked route with calc-addin-host's add-in set, which app mode loads with the same `--xll`;
  `--workbook` opens the file through calc-host-vba's open-time route, the one the page's worker
  takes. The page itself has no add-in panel, so DNA Calc is not driven through its page here.
  X1: one `--cases fixture/x1-cases.json` run (`dnacalc/x1-xll.json`) against Excel's record of
  2026-11-10 (`codex-runs/x13-boundary/m4-excel-all/x1-fixture`). X1's register and unregister
  cases depend on the cases before them, so X1 runs in one process; run one process per case, 9
  of those sequence cases differ (280 of 289), which says nothing about the add-in route.

DNA Calc's refusals:
- .xlam DnaExamples: `DnaTryWrite`, `DnaWrite` (`ActiveSheet.Range(...).Value = 1`) and
  `DnaSelection` (`Selection`): DNA Calc gives a worksheet function no active sheet or selection
  (ONECALC_ADDINS.md, milestone 5). Object model, left alone on purpose.
- .xlsm: DNA Calc's file-project route (calc-host-vba, CalcVBA's typed document route,
  dc-hsa5.12) refuses the whole project, so every case is refused. DnaSimple.xlsm: `TypeName` is
  not linked in CalcVBA's typed document route ("unlinked typed source project VBA ...
  TypeName"). DnaExamples.xlsm (the simple set run in it too, against Excel's reopen answers:
  0 of 109, `dnacalc/simple-dnaexamples-xlsm.json`): the project does not compile, because
  `Range.Value` assignment (`ActiveSheet.Range("Z99").Value = 1`) is not declared on this route.
  Before this push the first stop for DnaSimple.xlsm was any function with a Date, Currency or
  ParamArray signature, which refused the whole project; that now refuses only its own function
  (worker test `one_unsupported_signature_refuses_only_its_own_function`). An .xlsm whose project
  avoids these compiles and runs beside native add-ins (desktop test
  `a_macro_workbook_answers_its_own_functions_beside_a_native_addin`, local.xlsm with X1).

## What was wrong, and what changed (same commit as the tests)

1. Neither app could load the simple set at all: `CVErr(xlErrDiv0)` failed to compile
   ("Variable not defined") because DNA's Excel library declared only xlErrValue and xlErrRef, and
   a project that does not compile loads nothing. calc-vba now declares the XlCVError values.
2. `{1,2,3}` (and any one-row array) reached VBA as a 1x3 2-D array; Excel passes a 1-D array
   (1 To 3). Now 1-D.
3. A number that does not fit a typed parameter (`ByVal x As Long` given 3000000000) gave
   `#VALUE!`; Excel gives `#NUM!`. The body's own overflow stays `#VALUE!`, as in Excel.
4. DNA Calc desktop could not load an .xlam: `--xll` took only native add-ins. It now loads a VBA
   .xlam into the same add-in set (Workbook_Open, Workbook_AddinInstall).
5. DNA Calc: publishing add-in functions to an opened macro workbook replaced the workbook's own
   VBA functions; they now stand beside each other (`CallableTable::add_from`).
6. DNA Calc's file-project route refused the whole project for one unsupported signature; now that
   function alone is a typed refusal.
7. OneCalc: the "Macros are disabled for this file" bar and its Enable button sat inside the
   collapsed session-file panel, so a person who opened an .xlsm did not see it (the page
   acceptance could not press it: "element not interactable"). The bar is now outside the panel.

Fixture note: `SimpleGrid` returns its Double array `As Variant`. Declared `As Double()` with a
fixed local array, CalcVBA's verifier rejects the assignment (BadAssignRepresentation) and the
whole project does not compile, in both apps; Excel compiles it.
