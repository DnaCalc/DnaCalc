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
1b. DNA Calc does the same for an opened workbook (dc-0f1k.22). Opening never compiles or runs the
   file's project (`apps/dna-calc-worker/src/workbook.rs`, `Macros`): it is held and saved back,
   the page shows a "Macros are disabled" bar above the formula bar, and its Enable macros button
   (`Request::EnableMacros`) compiles the project, publishes its functions beside the add-ins'
   and raises `Workbook_Open`. A file whose Mark of the Web says zone 3 or 4, or one opened from a
   UNC path, is blocked: the bar says why and offers no button, and the request is refused. The
   desktop reads the origin of a file named on its command line (`--open`;
   `calc_platform_desktop::origin`); a file the page's own file input opens has no path, so, as in
   a browser, its origin is unknown and it opens disabled. The desktop harness
   (`dna-calc-desktop --engine --workbook FILE`) holds the project unless `--enable-macros` is
   given, and reports a blocked file as `macros_refused`. Every calc-host-vba route that compiles
   a file's project takes a `ProjectGrant`, made only by the person's enabling
   (`ProjectGrant::person_enabled`, refused for a blocked origin) or by an evidence run's
   trusted-code manifest that lists the file's exact bytes (`ProjectGrant::evidence_manifest`, in
   `tools/xtask-harness/src/trusted_code.rs` only). `cargo xtask corpus` and `compare` without
   `--trusted-code` never run a file's project: a capture whose Excel could run it
   (AutomationSecurity 1) is refused as `TrustedCodePolicy:dc-0f1k.22`. There are no trusted
   locations and no per-file trust record.
1c. The blocks are easy to lift (dc-15dk). One per-user macro security level, shared by both
   applications (`crates/calc-macro-security`, stored by `calc_platform_desktop::macro_security`
   in `%LOCALAPPDATA%\DnaCalc\macro-security.txt`, or `DNACALC_SETTINGS_DIR`, or OneCalc's
   `--data-dir`), is always shown on the page and changed there: Off (the default, as above: the
   bar and Enable macros; a marked or network file offers Unblock and enable behind a short
   warning instead of a refusal), Safe (VBA runs with dangerous operating-system services
   refused; the policy is CalcVBA's, dc-10un, and until it lands Safe behaves as Off and says so)
   and Unguarded (full access, Excel's enable macros: the project runs on open, a marked or
   network file too, with an information notice). `--trust-macros` (Unguarded) or
   `--macros off|safe|unguarded` (both desktop executables, `--app` and `--engine`) or
   `DNACALC_TRUST_ALL_MACROS=1` (Unguarded; also read by `cargo xtask compare` and `corpus`) pin the
   level for a run, for scripts, agents and the harness; a pinned level is shown with what pins
   it, is never stored and the page cannot change it. The grant carries the single VBA host
   policy choice (`ProjectGrant::policy`, Full until Safe exists). Under the variable a capture
   whose Excel ran with macros disabled still compares with the project held. The trusted-code
   manifest stays as the other route for evidence runs.
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
| calcvba-udf generated add-in (15 exports) | 17 (+ 3 DNA Calc-only) | 17 | 0 |
| all | 350 (+ 3 DNA Calc-only) | 350 | 0 (100%) |

Re-run on 2026-11-11 after the dc-6332.10 callback-boundary changes (OneCalc a967db01): the same
350 of 353, set by set (`crates/calc-xll-reference/traces/2026-11-10-x13-boundary/m4-acceptance`).

Three cases of the generated add-in need a workbook: `Native_ReferenceKind(A1)`,
`Native_OptionalReference(A1)` and `Native_CallerRow()` (Excel gives `"Range:1"`, `"Range"` and `3`; the
generated code reads `Application.Range(address)` and `Application.Caller` through the add-in's COM view
of Excel, late bound). dc-6332.12 had answered them with a pseudo object model (Range, Union, Caller,
name-only Range with type information; commits ecd1d610 and b28341ab, 353 of 353). dc-svqh removed it:
OneCalc hosts functions, DNA Calc hosts workbooks, so these three are DNA Calc-only cases
(`dnacalc_only` in m4-acceptance.json), run and reported separately, neither matches nor failures.
The git history keeps the pseudo object model for DNA Calc's own.

**Values-only profile (dc-svqh, owner policy 2026-11-11).** One implementation of the Excel contract
in the shared host crates (`calc_vba::Profile`, `AddinHost::with_profile`), two profiles. OneCalc's
values-only profile refuses what needs a workbook, with the text "needs a workbook: open in DNA Calc":
a VBA function with an `As Range` (object) parameter is registered as a typed refusal (the module's
other functions load); `Application.Caller` is refused when it runs (VBA) and as a COM property
(`Application.Caller` and `Application.ThisCell` answer error 1004 with that text). Not refused:
`Application.Volatile`, `xlfVolatile` (OneCalc's volatile flag, F9), `Application.WorksheetFunction`,
and XLL reference parameter types (`R`, `U`) and `#` registrations: OneCalc never passes a reference.
A fixed fake value for Caller may replace the refusal later. DNA Calc takes the full profile.
The VBA acceptance (m5) has four more DNA Calc-only cases in each of `dnaexamples-xlam` and
`dnaexamples-xlsm` (`DnaCells`, `DnaWhere`, `DnaTryWriteArg`, `DnaWriteArg`, all `As Range`); the 109
simple-UDF cases are unchanged in both routes. The cross-built exe's run (2026-11-11/12): m4 350 of 350
plus 3 DNA Calc-only; m5 simple-xlam 109/109, simple-xlsm 109/109, dnaexamples 22/22 and 23/23 plus 4
DNA Calc-only each.

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
  error 1004, `ThisWorkbook.Name` is the file's name, `ActiveSheet` is the session's one sheet
  (`Name`, `Range("A1")`) and `Selection` is the cell the formula is entered in, and `Range.Value`
  reads are typed refusals. In Excel a worksheet function sees the user's sheet and selection, not its
  caller's (measured, `fixture/vba/active-objects-measured.json`, four scenarios); OneCalc's user has
  one sheet and has selected the formula cell, which is Excel's measured case where the selection is
  the caller. DNA Calc gives its session's selection as the view (2026-11-12, dc-hsa5.12): `ActiveSheet` is
  the active cell's sheet and `Selection` the active cell, the workbook's active sheet at A1 before the
  page selects.

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
were `ActiveSheet.Range(...)` (two cases) and `Selection`; since dc-0f1k.12 OneCalc answers them
(1004, `#VALUE!` and "Range") and the project's 26 cases all match. All 27 saved .xlsm files, the one with the edited project included,
opened in Excel without repair (no "[Repaired]" name, no new `error*.xml` log), each with its VBA
project, and Excel's recalculated value equalled the page's.

Not shown: `Workbook_AddinInstall` against Excel (installing an add-in in Excel writes its add-in
list, which these runs leave alone); the browsers run the .xlsm route without the registry's type
libraries, so DnaExamples' project does not compile there (the part is kept and saved back
unchanged); the native dialogs (the runs type the add-in's path, open through the page's file
input and save through `--save-dir`).

### Simple UDFs (2026-11-11, dc-0f1k.4 and dc-0f1k.10)

A set of 109 plain worksheet functions (`fixture/vba/DnaSimple.bas`: numbers, text, logicals,
Optional, ParamArray, arrays in and out, CVErr, Variant coercion, WorksheetFunction) in
DnaExamples.xlam and .xlsm, against Excel: OneCalc 109 of 109 through the page for both the .xlam
and the .xlsm session (and DnaExamples' own 26 + 1 edit, all matched); DNA Calc desktop 109 of 109
for the .xlam (`--xll` now takes an .xlam), 23 of 26 DnaExamples cases (ActiveSheet, Selection),
and 0 of 109 for an .xlsm, whose project DNA Calc's file-project route refuses whole (dc-hsa5.12).
Table, routes and the defects fixed:
`crates/calc-xll-reference/traces/2026-11-11-simple-udfs/README.md`. The "Macros are disabled"
bar now shows outside the collapsed session-file panel.

## Milestone 6: RTD (dc-snz.23)

A session with an RTD connector (the desktop application's, over the loaded add-ins' RTD servers)
reads real-time data for its one formula by the rules calc-fml defines for every host: the built-in
`RTD` and an add-in's `xlfRtd` join or start the topic through the connector, the first value
(`ConnectData`'s) shows at once, and the session admits it as sequence 1 when the evaluation ends.
The session owns what calc-fml holds no state for: the ledger of admitted inputs
(`calc_fml::rtd::RtdLedger`), the topics and leases the formula read, and the leases a replaced
formula stopped reading (`Session::update_rtd`, `retire_rtd`, `reads_rtd`, `take_unread_rtd`;
`apps/dna-onecalc/src/rtd.rs`). There is no engine and no dependents: when an admitted input is one
the formula read, the owner calculates the formula again. Without a connector (the browsers) `RTD`
and `xlfRtd` stay typed refusals.

The desktop harness (`dna-onecalc-desktop --rtd`, `run.ps1 -Rtd`, `src/rtd.rs`) drives a session over
the real servers the way `dna-calc-desktop --engine` drives CalcHost: it polls the RTD client between
turns, admits each update as the next sequence, calculates again, and, for a replaced formula,
disconnects what the new formula no longer reads after it has been calculated. `verify.py` reads a
host output with `single_formula` as (formula, value) rows with no dependents.

What the strict comparison with Excel (owned hidden sessions, traced and control, 16.0.20527) showed
through the OneCalc executable on 2026-11-08, with the numbers the DNA Calc host had for the same
conditions in `codex-runs/rtd1/m4`:

| Condition | Values | Ordered client calls matched / failed | DNA Calc host |
|---|---|---|---|
| X5 numeric UDF (`=10+X5.RtdValue()+3`) | 2 of 2, UDF answers 1 then 42 as Excel's | 6 / 0, "verified controlled numeric RTD protocol" | the same status |
| X5 typed `=RTD(...)` | 2 of 2 | 6 / 0, verified | the same |
| Excel-DNA original (8 formulas) | 8 of 8 | 8 / 18 | 11 / 15 |
| Excel-DNA delayed (6 formulas) | 6 of 6 | 20 / 2 | 20 / 0 |

Unequal ordered client traffic stays a failure. The original and delayed rows differ from Excel in
when `DisconnectData` and `UpdateNotify` fall relative to each other for the formulas that finish at
once; Excel's own samples differ between runs. Native call traffic (`xlAsyncReturn` and the managed
callbacks) is not matched in either host. Remote servers and array topics are refused as before.

## Milestone 7: the packaged folder (dc-0f1k.13)

`cargo xtask package onecalc` builds one folder, `DNA-OneCalc/`, and a zip of it, and records every
file's size and SHA-256 in `package-manifest.json`:

- `dna-onecalc-desktop.exe` carries OneCalc's page inside it (the bundle's five files packed into one
  block, `calc_platform_desktop::assets`, embedded by `build.rs` from `DNACALC_ONECALC_PAGE`; the
  WebView2 host answers the page's requests from memory at `https://dnacalc.app/`). It has an icon
  (`icon/make-icon.py` draws it) and a version resource, is a window program (no console), and is built
  in the release profile with the C runtime linked statically, so the folder needs no Visual C++
  redistributable; the command fails if the exe or the companion imports a `vcruntime`/`msvcp` DLL.
- `XLCALL32.DLL` (the `calc-xll-shim` companion) beside it, for .xll add-ins.
- WebView2 Evergreen is detected at start. When it is absent the app says what is missing and gives
  the download page (a box for a person; stderr, `--message-file` and a non-zero exit under a test flag).
- Started with no arguments it is the app. Test flags (`--debug-port`, `--data-dir`, `--save-dir`,
  `--close-answer save|discard|cancel`, `--message-file`) make it report on stderr and by exit code,
  never in a box that waits for a click.
- `dna-calc-desktop.exe` (DNA Calc, built in the same release profile and with the same static C runtime)
  is an optional companion in the folder, with its own page inside it (DNA Calc's bundle packed as
  OneCalc's is, `DNACALC_CALC_PAGE`, embedded by its `build.rs`; `--without-calc` leaves it out). It runs
  with no `--assets`, `--url` or `page` folder, takes `--open <file>`, and ends when its window is closed.
- "Open in DNA Calc" appears for a file OneCalc refuses. The host launches `dna-calc-desktop.exe
  --open <file>` when that exe is in the folder, otherwise says plainly that it is not (dc-0f1k.14).

Desktop polish: the window title is the session name with `*` for unsaved changes (the page sets
`document.title`, the host follows it); the window comes back where it was left (`settings.json`
beside the WebView2 profile under `%LOCALAPPDATA%\DNA OneCalc`, or `--data-dir`); recent files (eight,
newest first); Ctrl+O, Ctrl+S (writes the opened or last saved file in place) and Ctrl+Shift+S; closing
a window with unsaved changes asks Save, Don't save or Cancel. RTD now runs in the app: the host polls
the add-ins' RTD client on its timer and tells the page (`{"event":"rtd"}`) to evaluate again. A thread
timer's id is mapped back to the id its owner asked for (Windows gives a window-less timer its own).

Proof: `cargo xtask package onecalc-clean-run` (`app_clean.rs`) copies the folder to an empty directory
outside the repository and runs it from there with a fresh data folder, a minimal environment (checked
not to name the repository) and no `--assets`, driven through chromedriver on the app's WebView2 plus
the app's own window (title, place, WM_CLOSE). The run and the guard of
`tools/excel-session/on-private-desktop.ps1` (now `-ProcessNames`) kept every window off the owner's
desktop. The native file dialogs need a person and were not driven. Run E of the same command starts the
folder's `dna-calc-desktop.exe --open` on a workbook OneCalc refuses (two sheets and a pivot) and reads its
sheet tabs from the page inside the executable; run B checks that the process "Open in DNA Calc" starts is that
executable, shows a window, stays up and ends when its window is closed.
