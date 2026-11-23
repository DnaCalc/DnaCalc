# Asynchronous functions in Excel

## Native XLL measurements

The primary run is `ww8/async-03` under `C:\Work\DnaCalc\codex-runs`, on GOVERT-OMEN with 64-bit Excel
16.0.20527.20196, en-ZA, and an AMD Ryzen 7 4800H. The Excel session wrapper launched PID 21088 on
a private desktop. Its guard counted zero windows and zero foreground hits on the user desktop.
AutomationSecurity read 1 before and after. The session host refused its post-job reset because
`RegisterXLL` left the fixture in the add-in and function inventories. It retired that owned
process. The measurements below come from the completed worker output and native trace that
preceded reset.

The lane cross-built the XLL from the repository's [P07 async fixture](../../crates/calc-engine/tests/fixtures/excel-calc-probes/P07/native-async.cpp)
and its [P03 logger](../../crates/calc-engine/tests/fixtures/excel-calc-probes/P03/native-probe.cpp), using clang-cl and
lld-link on dna-koderbot. The scratch sources and compiler output are in `ww8/async-build`.
The primary XLL SHA-256 is `0b2fd3a8dec17501228ad99ee3667dd4e281ddcd30f87a6e246fb8248caf660f`.
The three timed functions have separate entry addresses and Excel lists their actual registrations
as `AsyncUI >BBX`, `AsyncSafe >BBX$` and `AsyncVolatile >BBX!`. The first two pilots aliased
the addresses and Excel retained one `>BBX!` record, so the volatility and thread conclusions use
`async-03` exclusively. There are no worksheet VBA macros in these runs.

| Case | Observation | Run evidence |
| --- | --- | --- |
| Manual setup and dirty edit | The initial formula assignment computed live values during setup despite Manual. Once seeded at A1=10, editing A1=11 returned with zero async entries. B1/C1/D1 retained 20/21/42 and B2/D2 retained 13/113. CalculationState was 2 (Pending). A subsequent Calculate returned 22/23/46 and 14/114, with state 0 (Done). | `async-03` native rows 8–29, `manual-dirty`, `manual-edit`, `manual-dirty-calculate` |
| Two independent chains | `B1=XBefore(A1)+AsyncUI(A1,2100)`, `C1=XAfter(B1+1)`, `D1=C1*2`. `B2=AsyncUI(A1+3,800)`, `D2=XAfter(B2+100)`. Both providers entered at tick 125618140. D2's probe ran with 113 at 125618968, before the longer return at 125620250. C1's probe took 21 at 125620265, and D1 was 42 at command return. The long prefix ran once. The independent constant and NOW-based probes ran before either completion. | `async-03` `manual-calculate`, native sequence 30–44 |
| AppEvents | Dirty Calculate raised SheetCalculate followed by AfterCalculate at 125620281, after both returns. The fast branch alone had no AppEvents pair. A Manual edit raised SheetChange without calculation events. The Automatic argument edit ran both chains, then delivered SheetCalculate, AfterCalculate, and SheetChange in that order at 125627281. | `async-03/events.tsv`, `markers.tsv`, `native.tsv` |
| Native lifecycle events | Native CalculationEnded was absent from the dirty Application.Calculate at 125618125 and present after the final suffix in the CalculateFull at 125620468 (sequence 61) and Automatic edit at 125625125 (sequence 91). No native cancellation event occurred in these COM controls. | `async-03` `manual-calculate`, `calculate-full`, `automatic-edit` |
| Volatility and forced calculation | Clean Calculate after the ordinary chain was seeded ran only the NOW-based probe, with zero async entries. After adding the volatile and thread-safe registrations, each clean Calculate invoked only the volatile async input 213 plus the NOW-based probe. CalculateFull re-entered both ordinary, both thread-safe and the volatile call, and reran the independent nonvolatile constant. | `async-03` `manual-clean`, `volatile-clean-1/2`, `mtr-on/off` |
| Thread-safe entry | With MTR on, the two small thread-safe calls entered on two worker threads, separate from the ordinary and volatile entries. With MTR off, all five entered on thread 53184. A 96-cell sheet of `=AsyncSafe(1000+ROW(),120)` entered all 96 calls across 16 worker threads with MTR on, returning first/last values 1001 and 1096. Worker callback threads returned the results through xlAsyncReturn. | `async-03` `mtr-on`, `mtr-off`, `mtr-pool-full`; native kinds 25, 60 and 23 |
| Save and reopen | The completed workbook was saved and closed in Manual with CalculateBeforeSave false. The read-only reopen exposed the same completed values and CalculationState 2. No new async invocation occurred during save or open. Explicit Calculate re-entered only the volatile async call, then state read 0. | `async-03` `completed-save`, `reopen`, `reopen-manual-calculate`, `async-completed.xlsx` |
| Pending state and sheet text | The separate client timed CalculationState and worksheet Text reads independently. The 0 at 125632812 arrived after the volatile callback. The 2 at 125627843 coincided with one return and the start of another invocation during formula setup. The subsequent worksheet read spanned completion. Individual Text calls also straddled operations, yielding mixed snapshots such as 20/21/46. No captured Text sample contained #GETTING_DATA. The COM runs establish idle/dirty states and blocking. A stable CalculationState read before a noncanceled completion remains unmeasured. | `async-02/poll.tsv`, `async-03/poll.tsv` and matching native ticks |
| Painted cells while a worker is outstanding | After native F9 changed A1 to 11, an owned-window PrintWindow capture at ticks 126378765–126379734 showed the old 20/21/42. The callback returned at 126381781 and the final cells read 22/23/46, state 0. In the Esc arm the screenshot at 126382515–126382734 showed #CALC! across B1:D1 after cancellation at 126382500 and before the late return at 126385578. The suffix did not run. The independent repeat read state 2 and the three #CALC! cells repeatedly before its late return. Neither pending image contained #GETTING_DATA. | `async-05-display/complete.png`, `escape.png`, `paint.tsv`, `native.tsv`, and `async-04-ui/poll.tsv` |
| First display with no previous result | PrintWindow began during the newly entered formula's first invocation at 126521718 and finished at 126525140, after its return at 126524828. The result image showed 20. That sample leaves the cold in-flight grid state unresolved. | `async-06-cold/cold.png`, `paint.tsv`, `native.tsv` |
| UI F9, Esc and overwrite/undo | The retained P07 UI run recorded ten normal F9 controls with CalculationEnded. Ten Esc and ten overwrite/undo trials logged CalculationCanceled and CalculationEnded before the late return. The live continuation and its dependent showed #CALC!, the suffix never ran, and saved formula caches kept the previous 90/91. Overwriting and undoing also interrupted the calculation, so it left formula-generation-only cancellation unresolved. | [P07-ui3-safe controls and raw trace](../../crates/calc-engine/tests/fixtures/excel-calc-probes/P07/README.md#ui-controls) |
| COM edit and close while a callback is pending | The retained P07 controller entered mutation calls while the native worker waited, but each successful nontrivial edit or close returned after release. Those arms leave the outcome of a mutation applied while a continuation is still pending unresolved. P07's F9/Ctrl+S trials canceled the calculation and kept the old saved caches. | [P07 COM and save controls](../../crates/calc-engine/tests/fixtures/excel-calc-probes/P07/README.md#com-controls-and-remaining-scope) |

The native return trace records status 0 and an untouched output operand in these measured
callbacks, including P07's late returns. Execution of the suffix and the cell value establish
publication. The callback status by itself does not establish it.
Microsoft documents the async handle, continuation, and event APIs in its
[asynchronous UDF reference](https://learn.microsoft.com/en-us/office/client-developer/excel/asynchronous-user-defined-functions)
and [XLL event reference](https://learn.microsoft.com/en-us/office/client-developer/excel/handling-events).
The latter describes programmatic recalculation as excluding native lifecycle events. The
CalculateFull observations above record the behavior of this Excel build and route.

The primary run retains `result.json`, `native.tsv`, `events.tsv`, `markers.tsv`, `poll.tsv`,
`trace-hashes.tsv`, the workbook, the worker and poller scripts, and the ownership/reset receipt
`20261123T0724428025214-60281d38bba2404281bcf094e71d52ac`. Native records 10/11 are
prefix/suffix probes, 20/21 are entry/exit, 22/23 are release/return, 25 carries the argument bits,
50/51 are CalculationEnded/CalculationCanceled, and 60/61 mark the thread-safe and volatile exports.

The Win64 DNA Calc engine harness from 4094ef5c returned the same 26, 16 and 213 for the matching
ordinary, composed and volatile calls in `ww8/async-product`. Its two thread-safe cases returned
the typed refusal owned by dc-snz.23 while Excel returned 313 and 413. The exact arguments and
Excel readings are in `ww8/bank-rows/native-async-thread-safe.json`. This product replay covers
completed individual calls.

## Registered RTD measurements

On GOVERT-OMEN, two owned private Excel sessions at 16.0.20625.20108 ran the
numeric RTD server from the existing X5 fixture. The scratch DLL used a
fixed test CLSID, tick-stamped callbacks and three notifications. The sessions
reported a 2,000 ms RTD throttle and 16 calculation threads with MTR enabled.
Both runs removed the per-user CLSID and ProgID. The event run recorded zero
owned Excel windows on the user desktop.

| Case | Excel observation |
|---|---|
| Automatic, two cells with the same topic | One ConnectData served both subscribers. The initial value was 1. The first notification refreshed 42 immediately. Excel coalesced the 43 and 44 notifications, then refreshed 44 about 2,000 ms after the first refresh. The two dependent chains ended at 45 → 90 and 54 → 162. The two refreshes each raised SheetCalculate and AfterCalculate. |
| Remove one subscriber and retarget the other | Removing the first formula left the original topic connected. The replacement topic connected before the old topic disconnected. Closing the workbook disconnected the replacement and terminated the server. |
| Manual | The server still refreshed 42 and 44 while the sheet kept showing 1 in both topic cells and the dependents retained their calculated values. Retargeting showed the new topic's 44 while its dependents stayed stale. Calculate published the two affected chains. CalculateFull kept the settled values. |
| Save and reopen in Manual | The reopened topic initially showed saved 44 and dependents showed 54 and 162. Calculate reconnected and showed 1, 11 and 33. The server again refreshed 42 and 44 while the worksheet retained 1 until another calculation. |
| DNA Calc numeric control | The Win64 workbook harness admitted 1, 42 and 44, ended with 44 and dependent 45, and held zero RTD subscriptions after close. The comparison covers a single topic in the harness. |

Excel snapshots, tick-stamped callbacks and ordered AppEvents are in
`ww9/rtd-events/{excel.json,rtd.tsv,events.tsv}`. The DNA Calc results are in
`ww9/rtd-product/{engine.out,rtd.tsv}`. The remaining dc-26wtr.1 cases include
native F9 and interruption during RTD work, Promise cancellation and streaming.
Managed RTD remains dc-snz.23.

## Remaining experiment setup

### RTD

1. Use a disposable Excel profile. Build the repository's x64 `DNA_X5_RTD` variant and stage the
   authored worksheet. Its `xlAutoOpen` allocates a fresh CLSID and a ProgID of the form
   `DnaCalc.X5.<Excel PID>`. It creates the following keys with default REG_SZ values beneath
   `HKCU\Software\Classes`. `InprocServer32` points to the absolute staged XLL path. The
   fixture supplies `DllGetClassObject` and exposes `X5.RtdValue()` for the numeric topic.
   [Fixture registration and cleanup source](../../crates/calc-xll-reference/native/owned.cpp).

   | Relative key | Default value |
   | --- | --- |
   | `DnaCalc.X5.<Excel PID>` | `DNA Calc X5 controlled RTD fixture` |
   | `DnaCalc.X5.<Excel PID>\CLSID` | The generated braced CLSID |
   | `CLSID\{generated CLSID}` | `DNA Calc X5 controlled RTD fixture` |
   | `CLSID\{generated CLSID}\InprocServer32` | Absolute path to the staged x64 XLL |

2. Once authorized, load the RTD fixture on an owned private desktop. Enter
   `=RTD("DnaCalc.X5.<Excel PID>","","numeric")` in two cells with two levels of dependents.
   Log the display and app events against `ServerStart`, `ConnectData`, `UpdateNotify`,
   `RefreshData`, `DisconnectData` and `ServerTerminate`. Remove one subscriber, retarget
   the other, then close and reopen. Repeat in Manual and Automatic, recording state, repeated
   notifications, F9 and CalculateFull. Read the effective `Application.RTD.ThrottleInterval`
   first. Changing it persists across Excel restarts and needs separate owner approval on the
   disposable profile. Unload and verify removal of only the fixture's new keys.
   [RTD server methods](https://learn.microsoft.com/en-us/office/vba/api/excel.irtdserver),
   [RTD throttle property](https://learn.microsoft.com/en-us/dotnet/api/microsoft.office.interop.excel.rtd.throttleinterval).

### Office.js Promise and streaming custom functions

ww9 staged a local manifest, workbook, and Promise and streaming fixture. Automatic approval review rejected the Office.js catalog registration command, so the Excel sideload and measurements still belong to dc-26wtr.1.

1. On a disposable profile, prepare an Excel add-in-only XML manifest with a new add-in ID,
   namespace `DNACALCASYNC`, `ReadWriteDocument`, and the custom-functions script, metadata,
   page and shared runtime resources served over HTTPS. Add a delayed Promise function with
   cancelable metadata and a separate streaming function with streaming metadata, `setResult`,
   and `onCanceled`. Log invocation IDs, address when enabled, cancellation order and values.
   Put the manifest in an approved UNC share. [Custom-functions manifest](https://learn.microsoft.com/en-us/office/dev/add-ins/excel/custom-functions-overview),
   [invocation and cancellation](https://learn.microsoft.com/en-us/office/dev/add-ins/excel/custom-functions-web-reqs).
2. After approval, with Office closed on that profile, add
   `HKCU\Software\Microsoft\Office\16.0\WEF\TrustedCatalogs\{catalog GUID}` with
   `Id` (REG_SZ, the same braced GUID), `Url` (REG_SZ, the full UNC folder) and
   `Flags` (REG_DWORD, 1). Start a private Excel session, open Home → Add-ins → Advanced →
   Shared Folder and add the manifest. Use unique function names and remove the sideloaded entry
   and its catalog on that profile after the runs.
   [Microsoft's Windows catalog and removal steps](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/create-a-network-shared-folder-catalog-for-task-pane-and-content-add-ins).
3. Apply the same two dependent chains, argument edit, formula overwrite, F9, CalculateFull,
   Manual/Automatic, close and save/reopen. Log Promise settlement once and stream delivery
   for each result, with duplicate arguments as a shared-stream control. Record worksheet Text,
   Value2, CalculationState and callback/event order before making any equivalence claim.
