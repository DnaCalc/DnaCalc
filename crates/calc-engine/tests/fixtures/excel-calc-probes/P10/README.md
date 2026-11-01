# P10 result

Stored Range objects track inserts, deletes above and cut/move, including both areas of a union. Deleting the held
cell or closing its workbook leaves an object whose TypeName is still Range and IsNothing is false, but its
properties fail with VBA error424. Reopening the same filename does not repair that old object. Replacing the
provider module clears globals/Static state and dirties callers; new calls use the changed String signature.
Retain tracked references qualified by workbook incarnation, and separate callable code/state generations.

Excel 16.0.20527.20088 x64, Windows 10.0.26200, AMD Ryzen 7 4800H, safe mode. `machine.json` and profile rows retain
the settings/add-in enumeration limit. No security or COM registry setting changed. Every case creates a provider
add-in and owner workbook, seeds a held A5=42 (A7=72 for the union), verifies ReadHeld=42 and F(5)=10, and records
module and Static counters before interpreting the operation. The provider source is newly written test code.

`P10-final-safe` repeats nine conditions ten times; `P10-trial-safe` adds nine setup controls. The original StateSummary
stops on its first invalid-object property. `P10-properties-safe` adds one companion per condition that attempts
TypeName, IsNothing, Parent type/sheet/workbook, external Address, Areas and Value2 independently. Those companion
results distinguish a surviving unusable Range wrapper (error424) from a reset Nothing variable (error91).
All 108 cases have no VBA fault rows and worker cleanup owned-process-zero. There are 528 retained packages.

| Operation | Before F9 | F9 / forced reader |
|---|---|---|
| Insert row3 | A5 tracks to A6, value42 | Hold runs once; clean ReadHeld/F/Static do not; forcing invokes them |
| Delete row3 | Tracks to A4, value42 | Same counter pattern |
| Cut A5 to D9 | Tracks to D9, value42 | Same counter pattern |
| Insert above union A5,A7 | Tracks to A6,A8, two areas, values42/72 | Value2 of union/ReadHeld uses first area42 |
| Delete held row5 | Old object unusable, error424 | Clean reader cache stays42; forced ReadHeld maps the failed read to #REF! |
| Switch active workbook | Still bound to Owner A5, value42 | F9 invokes no UDFs; forcing does |
| Close owner | Old object unusable, error424 | Cached42 survives F9; forced reader maps failure to #REF! |
| Close/reopen owner | Old object remains unusable before F9 | F9 invokes Hold with a new argument; reader counter stays3 and cache42 is old; forced read uses newly held object |
| Replace module F(Double) with F(String) | All counters0, held Nothing; cached formula values still old | F9 uses String arg5, F1="text:5", Static=1; reader first returns #REF!, then forced read returns42 |

The table's counter/summary patterns repeat in all ten primary trials. Module reload resets the whole provider
project's state, not only F's counter. F9 runs ReadHeld before Hold populates the new variable, leaving C1=#REF!
despite the provider later holding42; forcing the reader invokes it again and repairs C1. This is a measured
callback-order consequence, not an implicit graph edge from the hidden read. Likewise, matching cached42 after
reopening is not evidence that the old handle survived: its properties failed and Hold's count changed before
the reader was forced. ReadHeld deliberately maps its caught object-model error to CVErr(xlErrRef); error424 is
the direct object-model observation. The active-workbook case creates the empty other workbook during the
operation, so it also changes workbook membership; it does not isolate activation from every membership effect.

`raw-results.tsv` is the 90-case primary trace; `details-raw-results.tsv` is the independent-property companion.
Provider-state, provider-trace, source replacement, typed cells, event and held-properties rows retain the route.
Original and changed provider XLAMs, owner XLSXs and controller XLSMs are committed. `saved-caches.json` covers
the 484 primary/setup packages; details-saved-caches.json covers all528. Original provider source for the primary
run is retained as raw/P10-final-safe/provider-input.bas inside supervisor-raw.zip; the companion has its own
provider-input.bas. Injected OracleProbe.bas and source hashes identify each control version. Current provider.bas
adds only the property diagnostic to the earlier provider; it does not replace the earlier raw source evidence.

Replay the shared run-calculation-probe.ps1 with this directory, Repetitions=10 and SafeMode. Extract with the
shared inspector. The sealed OBSERVATION_ONLY expected-value mismatch is intentional; compile/run are ok.
The parent can report an exited-process identity race after worker owned-process-zero; the raw failure is kept.

Native provider replacement reuses P07's canonical failed control rather than creating a second XLL/harness.
The direct COM ReloadAsync request entered while a call was pending but did not return; its exact owned process
was stopped and no replacement result was accepted. xlfUnregister/re-register is also a function-registration
experiment, not unloading/replacing DLL code. A runnable isolation needs a bounded supported provider-replacement
operation with an independently persisted native trace proving replacement before the old return. The current
minimal callback bridge also supplies no Boolean xlAsyncReturn output despite status0; an SDK-bridge control
would be needed to resolve that output separately. See native-provider-control.json and P07's retained limits.

Excel cannot run the DNA-only suspended-invocation half: this VBA UDF route has no resumable suspended frame.
That requires CalcVBA's code/state lease test through the engine route, outside this measurements-only task. The
new-generation/state result above does not establish whether an old suspended DNA lease may resume.
