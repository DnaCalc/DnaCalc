# VBA functions in DNA OneCalc

`dc-snz.3` value calls are closed; `dc-snz.5` remains in progress (2026-10-31).
Only `calc-vba` depends on the OxVba embedding API, pinned to
`c7ce134918f21038ec1f5f586ebfcac2d1851588`. Native builds prefer JIT;
browser builds omit JIT and use the interpreter. A native admission decline can select
the interpreter before any code runs. Runtime errors and exhausted budgets never retry.
The OxVba/CalcVba rename is deferred; OneCalc stays free of calc-engine/calc-doc/calc-host.

Load a standard module in **VBA functions**, enter 40 in A1 and `=Add2(A1,2)` in the
formula field to obtain 42. Load again to replace registrations and start fresh module
state; a compile failure leaves the prior module active. Unload removes registrations.
Nothing is persisted to a workbook. Public Functions register bare and qualified names
with ASCII case matching; Private functions and Subs stay private, and Main never runs.

The caller-owned calc-func registry and calc-fml resolver still own function registration
and invocation. Built-ins, locals and known defined names keep precedence. Plans retain
names rather than engine objects; callers recompile when adding a previously unknown
name. The default UserFunction entry still eagerly reads references for existing adapters;
the reference-aware entry borrows the evaluator's caller, reader and CalcFunc services.

One loaded VBA module retains globals, Static and Err across calls. Separate modules,
reloads and controller sessions have independent state. Optional arguments use folded
defaults, typed zero values or Variant Missing; explicit omitted positions differ from
blank cells. ParamArray packs a fresh zero-based array, including empty bounds 0..-1.
ByRef inputs remain private copies. Shared immutable code never shares mutable VBA state.

A single rectangular reference now becomes a live read-only Range object. Supported:
Value2, Count, Row, Column, Address, Cells/Item, and Rows/Columns Count. Variant reference
arguments have TypeName Range; a numeric literal is Double. Application.Caller identifies
the formula cell. VarType reflects the live Range default value (Number5, Empty0, text8, Variant array8204),
while TypeName/IsObject retain the Range. Date/Currency format-dependent defaults and other
object defaults remain open. Saved Range references read current inputs through the next call's
reader, so reads are recorded and missing inputs remain explicit refusals. Returning a
Range materializes Value2. Multi-area ranges, writes and other object members remain open. The bounded host
deduplicates Range identities by coordinates; VBA object-identity parity remains unmeasured.

Application.WorksheetFunction calls SUM/MIN/MAX/AVERAGE (1..30 arguments) or MATCH
(2..3 arguments) through the ordinary CalcFunc evaluator and its existing reader and
numeric policy. A failed MATCH raises catchable VBA error 1004. Scalar parameter positions
accept scalars or one-cell references; arrays and larger ranges are refused until measured.
Callbacks do not inherit the outer formula's implicit intersection or array lifting.
Other methods and unsupported argument counts are explicit host refusals. VBA On Error
and formula IFERROR cannot hide a missing read or a recorded host capability refusal.

Numbers, booleans, blanks, valid UTF-16, known worksheet errors and rectangular arrays
cross the adapter. CVErr keeps its code. VBA runtime failures, wrong arity, conversion
failures and exhausted budgets map to #VALUE! with diagnostics; ordinary VBA failures
can be caught by formula IFERROR. The per-call budget counts basic blocks, including
initialization and nested VBA calls; On Error cannot catch exhaustion. Compilation,
conversion and an executing host call are outside that cooperative budget.

A bounded per-thread cache retains up to 16 verified immutable modules and 8 MiB of
key/image descriptions. Keys include exact program/signature/defaults, backend, host
policy, target/CPU/codegen settings and helper/carrier ABI. This limit does not measure
executable arena memory. Reload can reuse code while starting new state; clearing or
evicting cached code leaves live registrations valid. No disk or cross-thread cache claim.

Excel run-3 evidence is retained in OxVba CORE-8/udf-run3 and udf-run3-objects. On Excel
16.0 build 20527, nonvolatile state stayed unchanged on ordinary recalculation without
changed inputs, advanced on changed input/explicit Range.Calculate, and volatile state
advanced each time. Initial calls happened twice; no fixed initial count is assumed.
Application.Volatile requests now reach the formula caller as evaluation effects.
An ordinary UDF plan is nonvolatile; its executed requests determine the caller cell's
volatility, separately from built-in volatility. Precise engine scheduling is still
in progress under dc-snz.6. Do not infer recalculation parity from persistent-storage tests.

Fresh observations are retained in
`crates/calc-vba/tests/fixtures/excel-recalculation/` (17 stages, 23 formula cells).
The existing VBE supervisor compiled explicitly with command578 and watched owned
dialogs before runtime. Normal startup crashed in the installed TestCrashGuard add-in;
the successful observations use safe mode and retain zero owned-process residue.
No security, Trust Center or registry setting changed. Raw results, source and hashes
remain separate from the derived observations. The exploratory expected text was
OBSERVATION_ONLY, so the supervisor's overall passed flag is false despite clean
compile/run; these are development observations, not certification.

Clean ordinary calculation and unrelated edits preserve a nonvolatile UDF's state.
Rewriting a referenced input with the same value still calls it again. A dirty upstream
formula also calls its UDF dependent even when the upstream result stays unchanged.
Explicit Range.Calculate and full/rebuild calculation call nonvolatile UDFs too.
Thus caching by argument values would be wrong. A zero-argument nonvolatile function
with Static state also remains unchanged during clean calculation.

Volatility is local to the formula cell, including calls of the same VBA procedure
with different flags. The last executed Application.Volatile request in the formula
wins; an ordinary call after a volatile call does not clear the request. An untaken
branch makes no request. Each reevaluation starts nonvolatile, so a conditional UDF
that stops requesting volatility becomes nonvolatile. A nested VBA helper's request
affects its caller; a request made outside a worksheet UDF has no scheduling effect.
Internal VBA reads of a cell absent from the formula's arguments did not create an
Excel dependency; its old value persisted until explicit calculation. Wider saved-Range
dependency behavior in the adapter remains an open parity question.

OneCalc deliberately calculates its one formula whenever its controller is asked.
That explicit call advances state even with unchanged inputs, like a forced range
calculation. It does not acquire a workbook graph or infer automatic dirty scheduling.
Workbook invalidation and ordinary/full recalculation belong to CalcEngine/CalcHost.
Microsoft's [Application.Volatile documentation](https://learn.microsoft.com/en-us/office/vba/api/excel.application.volatile)
and [recalculation documentation](https://learn.microsoft.com/en-us/office/client-developer/excel/excel-recalculation)
describe the broad rule; the retained observations specify the cases above.

Still open under dc-snz.5/OxVba bd-59co.2.10.6: exact volatility/effects and scheduling,
full Range/Caller/WorksheetFunction surfaces, locale-sensitive names, general object
reentry/disposal, End reset, and classes/UDTs in the embedding facade. Workbook/macros/
events and the desktop GUI shell remain in the broader dc-snz epic and dc-snz.1.

Validation: portable calc-vba tests explicitly compare interpreter/JIT calls on native;
apps/dna-onecalc/tests/vba.rs covers the controller route in native and browsers, including
optional/ParamArray/state, live objects/callbacks/refusals and cache reload ownership.
The existing dc-snz.3 acceptance and mounted UI cases still own load/reload/unload and
budget behavior. Final pinned revision and measured check results are in the run-3 report.

Native Git-pin verification: four adapter tests and seven controller tests pass;
the final repository check passes 1,307 tests, zero failed, three ignored.
Browser and structural verification passed on the previous pin (sweep with repository-wide acceptance skipped). The VarType correction now has final-pin native/browser checks; their results are recorded in the run-3 report.

Final pin native check: 1,314 passed, zero failed, three ignored. The controller
checks live default-type reflection and refusal propagation, with explicit native JIT
selection. Cells/Item currently admit positive indices; other offsets and full Range
identity/default-format behavior remain open. Final focused browser/acceptance and
corpus readings are retained in the run-3 report and codex-runs logs.

Final run-3 receipts: final-pin adapter4/controller7 pass in each Chrome/Firefox;
Windows and wasm dc-snz.3 acceptance pass, including the mounted load/reload/failed
load/budget/unload cases. Native CLI reports `5:Range:True` for Probe(A1).
The corpus remains 19,284/20,030 matched, 659 failed, with 73 external-link and14
circular-rule values separate. The repository-wide acceptance suite was skipped;
no whole-workset completion or Linux execution claim is made. Exact logs/report are
under C:/Work/DnaCalc/codex-runs (calcvba-report-3.md).
