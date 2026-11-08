# Add-ins in DNA OneCalc (milestones 4 and 5, dc-0f1k, dc-snz.2 and dc-0f1k.4)

How the desktop OneCalc loads, shows, runs and unloads `.xll` add-ins, what a saved session says about
them, and what the acceptance run showed. The XLL host itself (ABI, callbacks, pool, async, RTD) is
described in [XLL_COM_HOSTING.md](XLL_COM_HOSTING.md); this note is about the application on top of it.

## What a person can do

- **Load an add-in.** The page's "Add-ins" panel has "Load add-in..." (the native file dialog of the
  desktop exe, filtered to `*.xll` and `*.xlam`) and a path box with "Load path". A `.xlam` is a VBA
  add-in (milestone 5, below). The load of an `.xll` runs the add-in's open routine, waits
  until its registrations stop changing (a Python runtime registers after the open routine returns),
  and publishes every registered worksheet function to the session's one table of functions. A load
  that fails rolls back completely and the panel shows why (which registrations remain, if any).
- **See what is loaded.** One line per add-in: kind, path, SHA-256 of the file, and the worksheet
  functions it registered. A second add-in, or a VBA module, does not replace the functions of the
  first (the desktop host keeps both beside each other and publishes the union).
- **Unload an add-in.** The "Unload" button runs the add-in's close routine and removes its functions
  and its entry in the session's list. Reloading is refused, with the owning bead named, for the
  runtimes that cannot be started twice in one process: Excel-DNA Native AOT (dc-snz.19) and xlOil
  (dc-snz.21, which also crashes Excel itself on reload).
- **Reopen a session.** A saved session lists its add-ins (kind, path as chosen, hash). Opening it
  loads nothing. The panel shows each listed add-in that is not loaded with a "Load" button; the
  load checks the file against the listed hash and refuses a file that changed ("choose it with Load
  add-in to accept the change"). The session file is the constrained `.xlsx` of milestone 2; the list
  is in its custom XML part.

## Trust

There is no signing and no sandbox: an `.xll` is native code that runs in the OneCalc process with
the person's rights, as in Excel. What the application does about that is small and written here:

1. Nothing loads without the person's action (a button, a path they typed, a command-line `--xll`).
   An .xlam the person loads is enabled by that action. An .xlsm session's own VBA project is
   handled as Excel does by default: the file opens with macros disabled, so its project is held
   and saved back but not compiled and nothing in it, `Workbook_Open` included, runs until the
   person enables macros for that file (a bar above the session, "Enable macros"). A file that
   carries the Mark of the Web (a `Zone.Identifier` stream with ZoneId 3 or 4: it came from the
   internet or another computer) never runs its project, and the bar says why; the desktop
   application reads the stream, a browser cannot and treats the file as unmarked.
1a. COM is early bound. A VBA project with a reference to a registered type library can use that
   library's classes with `New` and typed variables; the calls go through the imported type
   library. Late binding is refused with a typed result that names the rule: `CreateObject` and
   `GetObject` in the project's source (the runtime sees them as activation by ProgID, as it does
   `New`, so it refuses the ProgIDs the source passes to those calls, and every ProgID when one
   call passes something that is not a literal; a class used both ways in one project is refused
   for both) and calls through `IDispatch` on a COM object. A project with COM references can do
   what the person can, as in Excel.
2. A session lists add-ins with the hash of each file when it was saved; reopening only offers.
3. A listed add-in whose file no longer has the listed hash is not loaded by the offer.
4. Commands, ribbons, menus and other add-in features are not offered. Their registration or load
   errors do not stop the add-in's worksheet functions from loading.

## Argument names, help and category

`xlfRegister` takes, after the type text and the function name, the argument names, the macro type,
the category, a shortcut, a help topic, a function description and one description per argument. The
host keeps them in `RegistrationInfo.docs` and hands them on as a `CallableDescriptor` in CalcFml
(`calc_fml::CallableTable::register_described`), next to the callable. They do not change a call.
CalcFml's assistance (`assist::complete_in`, `assist::call_in`) takes them in its `Scope`, so that:

- the completion list offers the function under the add-in's spelling, with its signature;
- signature help shows `Name(first, second)` with the active argument marked;
- function help shows the description, each argument's help, the category, the add-in it came from and
  whether it is volatile, thread-safe or asynchronous.

An argument the add-in did not name is `arg1`, `arg2`, ... (our choice, not Excel's). OneCalc sends the
functions once per version of the table (`Request::FunctionDocs`), not with every answer, because a
table can hold thousands of functions (the pool test add-in registers 10,000). DNA Calc's formula bar
gets the same descriptors through the outline reply of its worker (`Outline.functions`): its completion
list and signature line show them (checked in `dna-calc-desktop --app` with xlw's sample add-in:
typing `=Echo` offers `EchoShort(x)`, and `=EchoShort(` shows that signature). DNA Calc has no function
help panel yet, for the catalogue's functions either.

## Late replies, the pool and asynchronous add-ins

The session no longer waits inside a call. With an owner of returns (the desktop exe), a registered
function that answers later suspends the evaluation as an `EvalContinuation`:

- a **native asynchronous** function (`>` return with an async handle, `xlAsyncReturn`) parks on its
  request; the exe delivers the return when the add-in makes it, running the message loop meanwhile;
- a **thread-safe** (`$`) function runs on the host's pool; the evaluation polls it, serving the
  callbacks the worker makes from the add-in's own thread, until it is done.

A call that does not finish within 30 seconds fails the calculation with a diagnostic. A browser session
has no owner and calls every function to its end, as before.

## Acceptance

`cargo xtask app xll-accept onecalc` (tools/xtask-harness/src/app_xll.rs) starts the desktop exe, attaches
a WebDriver to its WebView2, and does through the page what a person does: opens the panel, types the
path, loads, enters each case's formula in the formula field and reads the result; then it reads the
signature help, the function help and the completion list for the first functions of each add-in,
unloads the add-in through the panel, and opens a session that lists it. Excel's answers come from
`crates/calc-xll-reference/m4-excel.ps1`, which runs `run.ps1 -NoTrace` for each set in an owned hidden
session. The sets and their files are in `crates/calc-xll-reference/fixture/m4-acceptance.json`.

### What it showed (2026-11-08, Excel 16.0.20527, final code)

Every set below was loaded through the page and its cases entered in the formula field; the page's
result was compared with Excel's answer for the same formula. Pass = matched Excel; fail =
mismatched + refused + missing; the rate is over all cases of the set.

| Set (file) | Cases | Matched | Fail |
|---|---|---|---|
| own fixture X1 (`calc_xll_fixture.dll` as X1.xll, 289 formulas) | 289 | 289 | 0 |
| Excel-DNA 1.9.0, net48 (+ the native ownership add-in) | 7 | 7 | 0 |
| Excel-DNA 1.9.0, net8.0-windows (+ the native ownership add-in) | 7 | 7 | 0 |
| Excel-DNA Native AOT 1.10.0-preview5 (net10, ILCompiler 10.0.14) | 1 | 1 | 0 |
| Excel-DNA, 10,000 thread-safe registrations (`X2Pool`) | 2 | 2 | 0 |
| xlw 23.10.12-rc sample, and its types sample | 5 + 5 | 10 | 0 |
| xlOil 0.22.1 Utils and SQL | 12 | 12 | 0 |
| xlOil 0.22.1 Python 3.12 | 5 | 5 | 0 |
| calcvba-udf generated add-in (15 exports) | 20 | 17 | 3 |
| all | 353 | 350 | 3 (99.2%) |

The three failures are all in the generated add-in: `Native_ReferenceKind(A1)`,
`Native_OptionalReference(A1)` and `Native_CallerRow()` return `#VALUE!` where Excel gives `"Range:1"`,
`"Range"` and `3`. The generated code gets a `Range` object from `Application.Range(address)` and
`Application.Caller` through the add-in's COM view of Excel (`xlGetHwnd`, then the EXCEL7 window), and
the object model the desktop hosts publish (calc-com-host) has no `Application.Range`, `Union` or
`Caller`. The owner's ruling is that OneCalc carries no full Excel object model; what these calls
should do instead of a silent `#VALUE!` is dc-snz.5's open question.

The page also showed, for the first functions of each add-in, the signature help, the function help
(description, argument help, category, origin) and the completion list; it unloaded each add-in
through the panel (every one unloaded); and it opened a session that lists the add-in twice (a wrong
hash and the right one): opening loaded nothing, the wrong hash was refused with the reason, the right
one loaded - except the two runtimes that cannot start twice in one process, which refused with their
beads (Excel-DNA Native AOT, dc-snz.19; xlOil Python, dc-snz.21). xlOil Utils and SQL reloaded.

Not shown: the native file dialog itself (a person must choose a file; the host logic runs in unit
tests with a scripted chooser, and the page's other route, a typed path, runs the same load);
commands and ribbons (not offered); RTD (milestone 6); xlOil Python's event cases (they toggle
`Application.EnableEvents`, which OneCalc has no control for; the page does tell add-ins of a changed
grid cell and of each calculation); the x5 permission and RTD case files.

## Milestone 5: VBA add-ins and .xlsm sessions (dc-0f1k.4)

- **Load a VBA add-in.** An `.xlam` loads through the same panel (dialog or typed path). calc-doc
  reads its `vbaProject.bin` part as a blob; CalcVBA reads and compiles the project with its
  references (the Excel library bound to DNA's objects; other type libraries, such as the Microsoft
  Scripting Runtime, read from the registry and called early-bound through COM). Its public
  functions join the session's table; its line in the list shows kind `xlam`, the functions and
  what its workbook events reported. The person's load installs it (`Workbook_Open`, then
  `Workbook_AddinInstall`, as Excel's Add-ins dialog); a load from a reopened session's list only
  opens it (`Workbook_Open`). Unload removes its functions.
- **Open an .xlsm session.** A session file with a VBA project opens like any session; its project
  compiles and runs `Workbook_Open`, and the VBA panel's status lists its functions. A module typed
  into the VBA panel goes into that project (a changed project). Save writes the session as an
  .xlsm under the file's own name: the project part unchanged, or the changed one after an edit.
  Where the project does not compile (in the browsers, without the registry's type libraries), the
  session still keeps the part and saves it back unchanged.
- **What add-in code sees** is listed in docs/plan/VBA_UDFS.md: cell writes raise Excel's trappable
  error 1004, `ThisWorkbook.Name` is the file's name, `ActiveSheet`, `Selection` and `Range.Value`
  reads are typed refusals.

The acceptance add-in is DNA Calc's own: `crates/calc-xll-reference/fixture/vba` holds its sources
(`DnaExamples.bas`, the class `DnaCounter.cls`, `ThisWorkbook.txt`), the `.xlam` and `.xlsm` Excel
built from them with the Scripting Runtime reference (`vba-job.ps1 -Action build`, owned hidden
session, VBE object model), and the cases with Excel's answers (`x9-vba-cases.json`). The plan is
`crates/calc-xll-reference/fixture/m5-acceptance.json`.

### What it showed (2026-11-08, Excel 16.0.20527, CalcVBA 18d06139)

| Set | Cases | Matched | Fail |
|---|---|---|---|
| DnaExamples.xlam loaded through the page, against Excel with the .xlam open | 26 | 23 | 3 refused |
| DnaExamples.xlsm opened as the session, each case saved and reopened by Excel | 26 + 1 edit | 24 | 3 refused |

Matched in both: plain UDFs, Optional arguments (omitted, given, skipped), ParamArray (empty, with a
skipped position), a class module, early-bound `Scripting.Dictionary` (`New`, `CompareMode =
TextCompare`, `Exists`, `Keys`, default-member assignment, `TypeName`), Range arguments (`Count`,
`Address`), `WorksheetFunction.Sum`, `CVErr`, `Err.Raise`, `Workbook_Open` having run once, a cell
write from a UDF (trappable 1004; `#VALUE!` unhandled) and `ThisWorkbook.Name`. The three refusals
are `ActiveSheet.Range(...)` (two cases) and `Selection`, which OneCalc does not model (Excel gives
1004, `#VALUE!` and "Range"). All 27 saved .xlsm files, the one with the edited project included,
opened in Excel without repair (no "[Repaired]" name, no new `error*.xml` log), each with its VBA
project, and Excel's recalculated value equalled the page's.

Not shown: `Workbook_AddinInstall` against Excel (installing an add-in in Excel writes its add-in
list, which these runs leave alone); the browsers run the .xlsm route without the registry's type
libraries, so DnaExamples' project does not compile there (the part is kept and saved back
unchanged); the native dialogs (the runs type the add-in's path, open through the page's file
input and save through `--save-dir`).
