# P07: async continuation and invalidated returns

Result: normal completion resumes after the async site without repeating the prefix. UI Esc and UI overwrite
followed by Undo leave `#CALC!`; the late provider value does not install and the suffix does not run. Editing also
cancels calculation, so the Undo observation does not isolate formula generation from cancellation. SaveCopyAs
persists the held numeric caches 90/91 after these live errors, not the live error display or a late provider value.

Environment: Excel 16.0.20527.20088, Windows x64, AMD Ryzen 7 4800H (8 cores/16 logical processors), safe mode.
`machine.json`, the per-case profile rows and each raw run's environment record retain the full settings. Safe mode
avoids the installed TestCrashGuard startup crash; its disable prompt was answered No. No security setting or COM
registration was changed. COM add-in enumeration is unavailable in this mode, not evidence of an empty add-in list.

The XLL is newly written measurement code. `native-async.cpp` extends P03's ABI and logger; the P03 image is
unchanged. Computational arguments are doubles copied by value. The async handle is an opaque value copied before
the entry returns, not a buffer dereferenced through its union. Named events release callbacks without entering
Excel's STA. Thread IDs, native entry/exit, argument bits, opaque handle value and return status are retained.
Build with `./build-native.ps1`; the installed MSVC compiler and shared P03 builder are used. The authoritative
injected VBA is in each run's `OracleProbe.bas`; `driver-source` retains selected harness code and the native image
for the later runs. Source snapshots taken after a failed run are supplementary; the injected module hash is the
authority for that run.

## UI controls

`P07-ui3-safe` is the completed UI run. `ui-raw-results.tsv`, `ui-capture.json`, `ui-observations.json`, the
`ui-*-native.tsv` files, `ui-journal-P07-ui3-safe.jsonl` and the saved `ui-*.xlsm` files are its evidence. It repeats
each condition ten times in one owned Excel process, with fresh arguments and logs, retaining the warmed workbook.
The journal records acknowledged arm/collect commands, not keystrokes. Computer Use (Sky) sent the UI actions to
the exact window named in `ui-ready-P07-ui3-safe.json`; native lifecycle events and the resulting caches provide
independent evidence of the calculation route.

- Normal: arm with a changed A1 and a 1500 ms delay; F9; collect after return. Each trace has one Before, one async
  entry/exit, one return, one After and one native CalculationEnded. C1 equals A1 and D1 equals A1+1.
- Cancel: arm with a changed A1 and a 10000 ms delay; F9; Esc before release; collect after the provider returns.
  Each trace has Before and async entry, then CalculationCanceled and CalculationEnded, then the provider return.
  There is no After. Live C1/D1 are Error2051 (the UI shows `#CALC!`); saved C1/D1 retain numeric 90/91.
- Undo: arm, select C1, F9, paste 999 into C1, Ctrl+Z, collect after return. The first delay was 18000 ms and the
  other nine 12000 ms. The original formula text is restored. Editing cancels the calculation; the native trace
  has one Before/entry, canceled/ended, then the late return, with no After or replacement async entry. The final
  live result remains Error2051. The saved package retains numeric caches 90/91 and the restored formula text.

The raw rows named `reopened/after-open` come from calling the common ReadAfterOpen logger on the live workbook;
they are not reopen trials. All ten normal snapshots store their expected values (81..90 and 82..91). The twenty
cancel/Undo snapshots preserve the last normal trial's caches, 90/91, alongside the newly authored inputs.

Replay uses `run-calculation-probe.ps1 -SourceFile <P07/ui-probe.bas> -UIPause -Repetitions 1 -SafeMode` and
`ui-control.ps1 -RunId <run> -Seq <increasing number> -Operation arm|collect -Id <condition>`. In the owned window,
send F9; for cancel send Esc; for Undo select C1, send F9, type 999 and Ctrl+Z. Refresh the observed window state
between input actions. The controller waits for `ui-stop-<run>` before normal owned-process cleanup.

`xlAsyncReturn` status was zero in these normal and late-return traces, but its output operand stayed type zero;
no Boolean acceptance result was supplied. Cache installation and the suffix distinguish the outcomes. Do not
report the zero status as proof that an invalidated return was accepted.

## COM controls and remaining scope

The separate `save-ui` folder records ten UI F9 then Ctrl+S trials with CalculateBeforeSave=false and a 12000 ms
provider delay. Each cancels before the late return; live C1/D1 are Error2051 and no suffix runs. Both the immediate
`ui-save-NN-on-disk.xlsm` copy and the later Collect copy store edited A1=201..210 and held C1/D1=41/42, original
formulas, ca=1, calcCompleted=0 and calcOnSave=0. BeforeSave and AfterSave(success=True) are recorded. This is a
canceled save sequence, not evidence that Excel preserved a still-live pending continuation. Save(true) with async
pending and edits racing serialization remain open.

The direct COM controller calls `Application.Calculate` outside a VBA wrapper. A separate process attaches only
after verifying the supervisor's PID, process start and executable. It resolves ranges before calculation, waits
for native entry, then attempts the operation while a native thread releases the event independently. Busy COM
errors are retried. Operation start and return ticks must be compared with native completion; entering a COM call
before release does not establish that the edit reached Excel before release. COM Undo can fail after an overwrite
because automation does not supply a UI Undo record; such partial trials are explicitly logged.

`P07-core-safe` repeats ten conditions ten times (normal, nested, overwrite, Undo attempt, argument edit, move,
close/reopen, supersede, reverse-release attempt and save-pending attempt). The `special` folder repeats the
two-site and identical-duplicate conditions ten times each after a one-case two-site setup control. The two-site
formula evaluates its second async site after the first return, uses a distinct handle, and finishes at 85 with
dependent 86. Identical duplicate returns do not repeat the suffix. These are call-site observations, not evidence
that two requests from the same cell were simultaneously pending. Mutation completion and native ticks, rather
than method entry, determine what the COM trials establish. Every successful nontrivial core mutation/save call
completed after the first native return. All ten COM Undo attempts overwrote 999 after that return, then failed
Undo. None of this COM matrix isolates a mutation during pending work. The in-flight registration-replacement control did
not return and its owned process was stopped; it has no accepted replacement result.

Earlier experimental runs, including failed controller attachment, empty state, command-file replacement race and
the interrupted two-site setup, are retained in `supervisor-raw.zip`. They do not count as repeated measurements of
stale-handle behavior. The two-site seed needs its own release sequence because a later site may start after an
earlier callback. The raw intervention record identifies the exact owned process stopped to recover it.

Unregister/re-register here uses the actual numeric `xlfRegister` ID and logs `xlfUnregister` and re-registration
results. It is function registration replacement, not DLL unloading or replacing provider machine code. No claim
about a true DLL generation change follows from it. Pending UI CalculationState reads and native Boolean
acceptance remain unmeasured. The DNA-only invocation lease/suspension half is outside this Excel measurement task.

Contract: retain a continuation for normal async completion; do not wait for native CalculationEnded on a COM
route that does not raise it. Canceled returns must not install results. Keep the conservative SliceV1 identity
checks for states not isolated by these measurements.

API references: [asynchronous UDFs](https://learn.microsoft.com/en-us/office/client-developer/excel/asynchronous-user-defined-functions),
[event registration](https://learn.microsoft.com/en-us/office/client-developer/excel/xleventregister),
[programmatic event limitation](https://learn.microsoft.com/en-us/office/client-developer/excel/handling-events).
