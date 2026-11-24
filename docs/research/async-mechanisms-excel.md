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

On GOVERT-OMEN, owned private Excel sessions at 16.0.20625.20108 ran the
numeric RTD server from the existing X5 fixture. The scratch DLL used a
fixed test CLSID, tick-stamped callbacks and three notifications. The sessions
reported a 2,000 ms RTD throttle and 16 calculation threads with MTR enabled.
The runs removed the per-user CLSID and ProgID. The event and F9 runs recorded zero
owned Excel windows on the user desktop.

| Case | Excel observation |
|---|---|
| Automatic, two cells with the same topic | One ConnectData served both subscribers. The initial value was 1. The first notification refreshed 42 immediately. Excel coalesced the 43 and 44 notifications, then refreshed 44 about 2,000 ms after the first refresh. The two dependent chains ended at 45 → 90 and 54 → 162. The two refreshes each raised SheetCalculate and AfterCalculate. |
| Remove one subscriber and retarget the other | Removing the first formula left the original topic connected. The replacement topic connected before the old topic disconnected. Closing the workbook disconnected the replacement and terminated the server. |
| Manual | The server still refreshed 42 and 44 while the sheet kept showing 1 in both topic cells and the dependents retained their calculated values. Retargeting showed the new topic's 44 while its dependents stayed stale. Calculate published the two affected chains. CalculateFull kept the settled values. |
| Native F9 in Manual | On the focused private Excel grid, F9 changed the topic cells from 1 to 44 and both dependent chains from 2 and 4 / 11 and 33 to 45 and 90 / 54 and 162. CalculationState went from 2 to 0. See ww9/rtd-f9. |
| Save and reopen in Manual | The reopened topic initially showed saved 44 and dependents showed 54 and 162. Calculate reconnected and showed 1, 11 and 33. The server again refreshed 42 and 44 while the worksheet retained 1 until another calculation. |
| DNA Calc numeric control | The Win64 workbook harness admitted 1, 42 and 44, ended with 44 and dependent 45, and held zero RTD subscriptions after close. The comparison covers a single topic in the harness. |

Excel snapshots, tick-stamped callbacks and ordered AppEvents are in
`ww9/rtd-events/{excel.json,rtd.tsv,events.tsv}`. The DNA Calc results are in
`ww9/rtd-product/{engine.out,rtd.tsv}`. The remaining dc-26wtr.1 cases include
interruption during RTD work, Promise cancellation and streaming.
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

The owner installed manifest 1.0.0.2 in the trusted catalog, and ww11 used that catalog and the existing HTTPS fixture server. The JavaScript-only runtime did not deliver the fixture's HTTP event requests. For the remaining invocation measurements, use a fixture with cancelable and address metadata and an observable record of invocation IDs, addresses, cancellation order and results. Repeat the workbook controls below with keyboard F9, Esc, close and the remaining save variants. Capture worksheet Text, Value2, CalculationState and application event order.

## Office.js custom functions on Windows

The ww11 runs used Excel 16.0.20625.20108 on an AMD Ryzen 7 4800H. Each run opened a fresh copy of the worksheet with a reference to the owner's trusted catalog. Manifest 1.0.0.2 uses the JavaScript-only custom-functions runtime. The fixture returns a Promise after a nominal 1,600 ms and emits six stream values at nominal 450 ms intervals. Excel COM polling recorded cell text, values and calculation state. The private-desktop logs recorded zero owned windows on the user's desktop.

| Condition | Promise and its dependent | Stream and its dependent |
| --- | --- | --- |
| Cold Automatic entry, argument 4 | Both showed **#BUSY!** (Value2 error -2146826237). They first showed 40 and 140 at 2,406 ms and 2,604 ms in the two samples. CalculationState read 2 while waiting and 0 when the results appeared. | Both showed **#BUSY!**. The first sampled values were 41 and 141 at 1,346 ms and 1,386 ms. The stream reached 46 and the dependent reached 146. The stream published while the Promise was still pending. |
| Automatic edit, argument 4 to 5 | The old 40 and 140 stayed visible until 50 and 150 appeared at 1,904 ms. | The old 46 and 146 stayed visible until 51 and 151 appeared at 684 ms. The stream reached 56 and the dependent reached 156. |
| Automatic edit, argument 7 replaced by 8 after about 520 ms | The old 50 and 150 stayed visible and the final values were 80 and 180. No 70 was observed. | The cells showed 71 before the new argument started producing 81 through 86. |
| Overwrite while the argument 9 calls are pending | The literal 123 and its dependent 223 held through the sample. No late 90 was observed. | The overwritten cell and its dependent held at 456 and 556. A second cell with the same stream formula continued from 91 through 96. |
| Switch to Manual during an Automatic call | The in-flight Promise subsequently published 100 and its dependent published 200. | The first stream values, 101 and 201, held through the rest of the sample. |
| Manual edit, then Application.Calculate | Editing the argument from 10 to 11 left 100 and 200 visible. Calculate yielded 110 and 210. | Editing left 101 and 201 visible. Calculate yielded 111 and 211. No later ticks appeared in the Manual sample. |
| Manual formula entry, then CalculateFull | The new formula showed #BUSY! then 120. The dependent stayed at 210 until CalculateFull yielded 220. | The new formula showed #BUSY! then 121. The dependent stayed at 211 until CalculateFull yielded 221. |
| Manual save with CalculateBeforeSave off and reopen | The sampled reopen values were 120 and 220. They held on Calculate and after switching to Automatic. | Reopen displayed 121 and 221. They held on Calculate. After switching to Automatic the sampled values were 126 and 226. |

Times are the first sample that observed a change. The cold clock began after formula entry and includes the remaining runtime startup. The polling reads occurred sequentially, so adjacent observations cannot establish ordering within a sample. The warm and Manual rows are from the first run. The second run confirmed the cold results, then yielded #NAME? on its first argument edit. The cause of the registration loss is unverified.

The fixture runtime did not deliver its HTTP event logs. The overwrite and retarget tests show that old results did not replace the edited cells during the sample. Callback delivery and ordering, whether duplicate streams share an invocation, Esc and keyboard F9, application event order, metadata options, errors, array spills and additional save and reopen variants remain open under dc-26wtr.1.3 and dc-26wtr.1.4. Raw samples, fixture inputs and guards are in the Windows lane under ww11/office-js/ and ww11/office-js-confirm/.

## ww12 follow-up on Excel 16.0.20625.20108

The runs used an AMD Ryzen 7 4800H and private Excel desktops. Timelines, inputs and
desktop guards are under `codex-runs/ww12/` on GOVERT-OMEN.

| Bead | Excel observation | Evidence and limits |
| --- | --- | --- |
| dc-26wtr.1.1 | For nonvolatile `>BBX`, CalculationEnded fired for CalculateFull, UI F9 and the Automatic argument edit. It was absent inside each dirty Application.Calculate, Worksheet.Calculate and Range.Calculate call. Those three routes all entered the provider and replayed the dependent. The first state immediately after Range.Calculate was 2. A clean Manual reopen with only AsyncUI and one ordinary dependent held 14/15 at state 0, with no native entry on Application.Calculate. | `xll-confirm/{ui.tsv,native.tsv,result.jsonl}`. The cold PrintWindow call began 344 ms after native entry and completed 156 ms after the return, displaying the settled result. The COM property read also finished after return. In the partial Esc attempt both native sites entered, but the controller posted Esc after the slow callback. Cold display, in-flight state, and the remaining save and ABI cases stay open. |
| dc-26wtr.1.2 | At the existing 2,000 ms throttle, both RTD subscribers advanced 1 → 42 → 44 in Automatic, and the unrelated NOW cell changed with both refreshes. In Manual, overwriting one of the two subscribers kept the topic. Clearing the last subscriber called DisconnectData and ServerTerminate within the 16 ms trace tick, before the next Calculate. RestartServers on the healthy server returned without reconnecting in the sampled Manual and Automatic arms. Manual held 1 even as the server refreshed to 44, and Automatic displayed 44. | `rtd/{excel.json,events.tsv,rtd.tsv,registry-cleanup.json}`. The RTD fixture keys were removed and verified absent. The server stayed healthy. Persistent throttle variants and the fault, interruption and wrapper cases remain open. |
| dc-26wtr.1.3 | Two staggered cold Promise formulas with identical inputs and their dependents were all pending until the sample that first showed 1510/1610 in both chains. Clean Calculate kept those values. CalculateFull put all four Value2 readings back at #BUSY! until they settled. Overwriting one pending formula kept 701/801 while the other completed at 1520/1620. Clearing a pending formula left a blank and 100. Automatic SaveAs with CalculateBeforeSave enabled returned while the Promise cells still read #BUSY! and the live cells later settled. | `office-js/{samples.jsonl,promise-save-true.xlsx,guard.json}`. The sample does not establish invocation sharing or callback order. The fixture did not deliver its HTTP invocation log. Esc, invocation metadata and errors, spills, and the remaining save and event arms stay open. |
| dc-26wtr.1.4 | With no Promise present, a cold delayed stream and its dependent read #BUSY! at CalculationState 2, then state 0 when the first tick became visible. A duplicate added after the first tick immediately showed the current 1561, and both cells advanced together through 1566. Switching to Manual after the first tick left 1571 visible. Manual CalculateFull and post-reopen Calculate each showed #BUSY! before returning to 1571. Switching to Automatic on reopen displayed 1576 in the first sample, 87 ms after switching began. | `office-js/{samples.jsonl,stream-reopen.xlsx}`. These observations cover sharing at the worksheet, with the count of provider invocations still open. Synchronous initial results, callback behavior, coalescing, errors and spills remain open. |
| dc-26wtr.1.5 | An XLL CalculationEnded callback coincided with each of two displayed Automatic RTD refreshes and each of six standalone JavaScript stream ticks. In the overlapping XLL/RTD run, the first UpdateNotify began during the native call in both modes. It returned after xlAsyncReturn and RefreshData followed. The XLL returned once in each arm. Automatic showed 42 and later 44, while Manual kept 1. | `mixed/{steps.tsv,native.tsv,rtd.tsv,samples.jsonl}`. Excel lost the JavaScript function registration during later mixed arms, so those arms did not measure the JS/XLL or JS/RTD overlap. The fixture keys were removed and verified absent. |

The detached Esc controller then posted Esc 281 ms after the fast XLL return and
4,485 ms before the slow return. CalculationCanceled and CalculationEnded fired in that order.
The first post-Esc read showed CalculationState 2. The completed B2/C2 chain held 32/33.
B1/C1 displayed #CALC! through the slow return. Excel saved 31/32 as the dirty slow-chain caches
and 32/33 as the clean fast-chain caches. The session wrapper kept the output and flagged
baseline drift during reset. The desktop guard recorded no windows on the user's desktop.
See `ww12/xll-escape/{esc-post.tsv,native.tsv,result.jsonl,partial-escape.xlsx,guard.json}`.

The follow-up ran both `>BBX` and `>BBX$` through Worksheet.Calculate and Range.Calculate.
All four calls entered the provider and returned the new value and dependent. Worksheet.Calculate
returned with state 0 and Range.Calculate with state 2. In a second clean Manual reopen, the
nonvolatile-only workbook held 24/25 at state 0. Posting F9 to the focused grid left those values
and state unchanged, with no new native entry in the next 781 ms. See `ww12/xll-routes/`.

For the Automatic Promise SaveAs with CalculateBeforeSave enabled, the saved worksheet held
1540/1640 and 1520/1620, the last completed chains before the input changed to 155. Each
formula cache had `ca="1"`. The live cells later settled to 1570/1670 and 1550/1650.
The Manual stream save stored 1571/1671 in both chains with `ca="1"`, `calcCompleted="0"`
and `calcOnSave="0"`. See `ww12/office-js/saved-caches.txt`.

For `AsyncEcho`, the fixture called xlAsyncReturn twice per site with the same handle and
value. Both calls returned status 0 with a zeroed output operand in the two completed controls
and again after Esc cancelled a third site. The completed chain held 41/42. In the cancelled
case both cells held #CALC! and state 2 after the two late returns. The wrapper retained the
observations and reported baseline reset drift. See `ww12/xll-duplicate-final/`.

### ww13 Windows follow-up, 2026-11-24

Excel 16.0.20625.20108 ran on AMD Ryzen 7 4800H. The private desktop guards recorded zero
windows on the user's desktop.

- **dc-26wtr.1.1.** In Manual, `Save` with `CalculateBeforeSave=true` began at state 2
  with the old 7/8 values. It called the XLL with 11 and returned after `xlAsyncReturn`,
  with 11/12 at state 0. In the F9 overwrite arm, the COM setter began during the XLL
  call and returned after the callback and CalculationEnded. The cell stayed 123
  afterward. This establishes the setter's blocking order. The edit itself was not
  observed before release. Cold PrintWindow again completed after the callback.
  See `ww13/xll` and `ww13/xll-edit`. The latter's receipt rejected the JSON-lines
  output as a single JSON object after the measurements had been written.
- **dc-26wtr.1.1.** The inline `xlAsyncReturn` fixture produced 67/68 with a plain
  operand and 68/69 with `xlbitDLLFree`. In both flagged entries Excel called
  `xlAutoFree12` with the same operand pointer on the same thread, before
  `xlAsyncReturn` returned status 0. See `ww13/xll-abi-traced/{native.tsv,results.json}`.
  Other return types, the in-flight state and display, and the remaining save cases
  stay open.
- **dc-26wtr.1.2.** On the first open, both `ConnectData` calls received
  `GetNewValues=True`. Returning False for one topic gave it 0 while the True topic
  showed 1. Both reached 44. The workbook reopened in a fresh Manual process at
  state 2 with 44 in both cells. Calculate reconnected with `GetNewValues=False` on
  both calls. The provider returned False and True respectively. The first sampled
  cells after Calculate both showed 42 after `RefreshData`. Manual held 42 and
  Automatic showed 44. See `ww13/rtd` and `ww13/rtd-reopen`. The fixture keys were
  removed and verified absent. Fault, interruption, persisted throttle and wrapper
  cases remain open.
- **dc-26wtr.1.3–.1.5.** The Office.js catalog returned `#NAME?` in two fresh private
  sessions, including the scalar Promise control in the second session. Those runs
  provide no Promise, stream or mixed-provider timing result. See `ww13/office-js`
  and `ww13/office-retry`.
