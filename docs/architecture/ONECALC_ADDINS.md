# Add-ins in DNA OneCalc (milestone 4, dc-0f1k and dc-snz.2)

How the desktop OneCalc loads, shows, runs and unloads `.xll` add-ins, what a saved session says about
them, and what the acceptance run showed. The XLL host itself (ABI, callbacks, pool, async, RTD) is
described in [XLL_COM_HOSTING.md](XLL_COM_HOSTING.md); this note is about the application on top of it.

## What a person can do

- **Load an add-in.** The page's "Add-ins" panel has "Load add-in..." (the native file dialog of the
  desktop exe, filtered to `*.xll`) and a path box with "Load path". Only `.xll` files load; a
  `.xlam` or a VBA module is milestone 5 (dc-0f1k.4). The load runs the add-in's open routine, waits
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
