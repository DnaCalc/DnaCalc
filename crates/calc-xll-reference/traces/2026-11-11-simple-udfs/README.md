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
| XLL, own X1 fixture (289) | 289 / 0 / 0 (page acceptance) | 289 / 0 / 0 |
| XLL, all OneCalc page sets (353: X1, Excel-DNA net48/net8/AOT/pool, xlw, xlOil, calcvba-udf) | 353 / 0 / 0 | not run (no DNA Calc page route) |

Routes:
- OneCalc: `cargo xtask app xll-accept onecalc` with `fixture/m5-acceptance.json` (sets
  `dnaexamples-xlam`, `dnaexamples-xlsm`, `simple-xlam`, `simple-xlsm`), driving the desktop exe's
  page in WebView2: Load path for the .xlam, the page's file input for the .xlsm, Enable macros,
  each formula in the formula field, Save; Excel then reopened each of the 136 saved .xlsm files:
  0 repaired, 0 repair logs, every one with its VBA project, Excel's value equal to the page's
  (`onecalc/m5-summary.json`, `onecalc/excel-reopen-*.json`).
- OneCalc XLL: the same harness with `fixture/m4-acceptance.json` and Excel's records of
  `codex-runs/x13-boundary/m4-excel-all`, after the last code change of this push
  (`onecalc/m4-summary.json`): 353 of 353, as before it; reload refused as before for Native AOT
  and xlOil Python. The page bundle used by both OneCalc runs was built at 06:45 from this push's
  tree before the per-function file-project refusal (a DNA Calc document-project change).
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

## 2026-11-12: DNA Calc's own app route, through its page (dc-hsa5.12, dc-0f1k.23)

What a DNA Calc user can now do: open an .xlsm in the desktop app and, at the macro level Off
(press Enable macros in the bar) or Unguarded (the project runs as the file opens), have the
workbook's own VBA functions answer in its cells; and load an .xll or a VBA .xlam from the page's
Add-ins panel (the native dialog, or a typed path and Load path), see each loaded add-in with the
worksheet functions it registered, and unload it.

| Set (cases) | Route | DNA Calc page, 2026-11-12 | Before (table above) |
|---|---|---|---|
| DnaSimple.xlsm, simple set (109) | open, Enable macros | 109 / 0 / 0 | 0 / 0 / 109 refused |
| DnaExamples.xlsm, simple set (109) | open at Unguarded | 109 / 0 / 0 | 0 / 0 / 109 refused |
| DnaExamples.xlsm, DnaExamples cases (26) | open, Enable macros | 26 / 0 / 0 | 0 / 0 / 26 refused |
| DnaExamples.xlam, simple set (109) | Add-ins panel | 109 / 0 / 0 | 109 (`--xll` only) |
| DnaExamples.xlam, DnaExamples cases (26) | Add-ins panel | 26 / 0 / 0 | 23 / 0 / 3 refused (`--xll`) |
| X1 fixture .xll (289) | Add-ins panel | 289 / 0 / 0 | 289 (`--engine`, one process) |
| OneCalc's other m4 sets (64: Excel-DNA net48, net8 with the native owner, AOT, pool; xlw, xlw types; xlOil utils/SQL, xlOil Python; calcvba-udf) | Add-ins panel | 61 / 3 / 0 | not run (no page route) |

(matched / mismatched / refused; fail = mismatched + refused + missing; no case was missing. In all: 729 of
732 matched.) The 3 mismatches are the generated add-in's `Native_ReferenceKind(A1)`,
`Native_OptionalReference(A1)` and `Native_CallerRow()` (Excel: `"Range:1"`, `"Range"`, `3`; DNA Calc:
`#VALUE!`): the add-in reads `Application.Range(address)` and `Application.Caller` through its COM view
of Excel, and since dc-svqh removed the pseudo object model from calc-com-host DNA Calc has no COM
`Range` or `Caller` of its own yet (bead filed). Report of these sets: `dnacalc/page-m4-summary.json`,
guard `guards/dnacalc-page-m4.guard.json`.

Route: `cargo xtask app xll-accept calc` (`tools/xtask-harness/src/app_xll_calc.rs`, DNA Calc's
twin of the OneCalc run) with `fixture/dnacalc-acceptance.json`, on a private desktop
(`tools/excel-session/on-private-desktop.ps1`; `guards/dnacalc-page.guard.json`: 0 windows and 0
foreground hits on the owner's desktop). It starts `dna-calc-desktop --app` with the page bundle
built from this push's tree, attaches chromedriver to its WebView2 and does what a person does: a
workbook set opens the .xlsm as a double-click does (`--open`, the level pinned with `--macros`) and
presses Enable macros; an add-in set opens the Add-ins panel, types the path and presses Load path,
and at the end unloads every add-in through the panel (all unloaded). For each case it types the
address into the name box, the formula into the formula bar, presses Enter, goes back to the cell
and reads the page: the value state (Calculated / Worksheet error / Refused), the value and its
kind, and the exact value where the shown text rounds a number (the result panel's new
"exactly ..." line, e.g. `SimpleFact(20)` shows `2.4329E+18 exactly 2.43290200817664E+18`).
Every case is entered in the same cell (H1; X1: B3 on a sheet renamed X1 through the page, A1 = 7,
as the Excel runs had). Excel's answers are the records of the table above: the in-book record
(`excel/simple-inbook-dnasimple-xlsm.controller.jsonl`), Excel's reopen of the OneCalc-saved
.xlsm files (`onecalc/excel-reopen-*.json`), the .xlam controller records, and X1's record of
2026-11-10. No Excel ran for this push. Report: `dnacalc/page-summary.json` (every case with what
the page showed).

**Measured on the tree before master's CalcVBA pins 9cf7dcfc and 7d554eb7 were merged** (calc-vba pin
e42bf5bb). After the merge the worker route answers 239 of 244: `SimpleKind(A1)` (both .xlsm),
`DnaCounterName()`, `DnaDictType()` and `DnaSelection()` regressed with those pins (TypeName of a Range,
a class and Selection; dc-hsa5.22, listed as the test's known differences). The page was not run again
after the merge.

The same three .xlsm sets on the worker route alone (the server the page talks to, no page):
`apps/dna-calc-worker/tests/opened_vba.rs`
`the_simple_udf_sets_answer_as_excel_does_in_the_macro_workbooks`, 109 + 109 + 26 of 244, which also
runs in the browsers' worker (244 of 244 before the merge, 239 after it).

What was wrong on the file route, and what changed:
1. Every UDF of a document project (an opened .xlsm, whose document events run) was bound to
   CalcVBA's owning typed entry, and one function the typed route cannot link refused the whole
   project: `TypeName` (no owning builtin in CalcVBA's typed linker), `Dim d As New DnaCounter`;
   a ParamArray, Date, Currency, Range or array signature was a typed refusal of its own. Such a
   function now
   runs on the dynamic runtime, as it does from an add-in or a module; the others keep the owning
   typed route (calc-vba `runtime/cache.rs`, `OwnedSession::dynamic`). Called inside a running
   document event, such a function is a typed refusal (it cannot enter the session the event holds).
2. `ActiveSheet.Range(...).Value = 1` did not compile: the automatic-event Range declared only
   `Value2`. `Value` (get and put) is declared; a worksheet function's cell write raises Excel's
   trappable error 1004, as in the add-in route.
3. The automatic-event types number Range members on their own (`Count` was answered as `Column`,
   `DnaCells(A1:B3)` gave 1, not 6, once the function ran): a worksheet function's Range members
   map onto the projection by name; `Application.WorksheetFunction` and `Application.Caller` are
   reached from a worksheet function of a document project.
4. DNA Calc gave a worksheet function no active sheet or selection (`ActiveSheet`, `Selection`
   refused). The session's selection is now the user view CalcFml passes to every user function
   (calc-engine `RecalcWork::with_user_view`, calc-host `HostState::user_view`): the active cell's
   sheet and the active cell, the workbook's active sheet at A1 before the page selects.
5. The page had no add-in panel: `Request::Addins`, `LoadAddinNative`, `LoadAddin`, `UnloadAddin`
   (dna-calc-model), answered by the desktop app over `calc-addin-host` (a browser worker says it
   loads none), the page's Add-ins menu beside File (`apps/dna-calc/src/addins.rs`), and the new
   table published to the open workbook, which recalculates. The per-XLL function list and the
   settling loop moved into `calc-addin-host` and OneCalc uses them too.

Not compared: number formats Excel applies to a UDF's Date or Currency result (the records hold
values; DNA Calc shows `SimpleDate()` as 46337).
