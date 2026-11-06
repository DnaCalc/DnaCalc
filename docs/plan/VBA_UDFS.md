# VBA functions in DNA OneCalc

`dc-snz.3` value calls are closed; `dc-snz.5` remains in progress (2026-10-31).
Only `calc-vba` depends on the OxVba embedding API, pinned to
`a4bf979239e1cb67326d13b760c04ecd13bbf55f`. Native builds prefer JIT;
browser builds omit JIT and use the interpreter. A native admission decline can select
the interpreter before any code runs. Runtime errors and exhausted budgets never retry.
The OxVba/CalcVba rename is deferred; OneCalc stays free of calc-engine/calc-doc/calc-host.

Load a standard module in **VBA functions**, enter 40 in A1 and `=Module1.Add2(A1,2)` in the
formula field to obtain 42. Load again to replace registrations and start fresh module
state; a compile failure leaves the prior module active. Unload removes registrations.
Nothing is persisted to a workbook. Public Functions register bare and qualified names
with the shared workbook-name comparison (dc-snz.11; measured Café/Ångström case pairs);
Private functions and Subs stay private, and Main never runs.
Bare names that spell grid cells keep reference precedence, as the original A1/XFD1
controls show. The example uses the default module's qualified name; a module loaded
under another name uses that qualifier instead. The VBA source is unchanged.

CalcFml owns the callable table and resolver; CalcFunc owns the neutral callable traits.
Built-ins, locals and known defined names keep precedence. Plans retain table, slot and
generation, plus spelling for diagnostics; invocation uses the checked slot directly.
Replacement preserves surviving slots and advances generations. Retained activations own
their previous code and project state; they cannot call an incompatible replacement.
Callers recompile when adding a previously unknown name. Metadata compilation preserves
the workbook's unresolved-macro refusal policy, including qualified misses; OneCalc's
explicit registry retains its existing name-error behavior. The default UserFunction entry still eagerly reads references for existing adapters;
the reference-aware entry borrows the evaluator's caller, reader and CalcFunc services.

One loaded VBA module retains globals, Static and Err across calls. Separate modules,
reloads and controller sessions have independent state. Optional arguments use folded
defaults, typed zero values or Variant Missing; explicit omitted positions differ from
blank cells. ParamArray packs a fresh zero-based array, including empty bounds 0..-1.
ByRef inputs remain private copies. Shared immutable code never shares mutable VBA state.

DNA Calc's file route is composed separately in `calc-host-vba`, shared by the worker
and corpus comparison. It admits the source of `.xlsm`/`.xlsb` projects before the
first calculation and binds document modules by the persisted VBA code names. UDFs
and admitted Open/Change/Calculate/BeforeSave handlers share one owning session;
reopen starts fresh state. Unsupported source, signatures, document events and
class/designer modules remain named refusals. The Preserve open policy executes no
VBA. File events refuse cached or uncertified Target values before copying them into
module state, including when VBA catches the read error; retaining that provenance in
state remains open. File-project worksheet lookup also admits the recorded private
standard-module functions through internal entries in their own modules, using the
same runtime and state. The original VBA visibility and saved source are retained.
Private scalar signatures are supported. Other private signatures register typed
refusals for their worksheet calls without blocking supported entries in the project.
Refused calls execute no procedure prefix and remain refusals through IFERROR.
Comparisons respect captured macro/event policy. See
[the recalculation contract](../architecture/RECALC_CONTRACT.md) for the event boundary
and remaining forced-UDF and object-model gaps. OneCalc's explicit
standard-module route keeps its existing dependency ceiling.

The Range, Application and WorksheetFunction implementation now belongs to DNA Calc in
calc-vba's runtime composition, extracted from OxVba 325257ea's frozen spreadsheet adapter.
There is no spreadsheet-compat dependency. The neutral host API carries bound procedure
handles, typed slots and owning activations. Admission prepares Long/Double and DNA numeric Range shims where
the actual body supports typed execution. Other signatures retain the explicit Variant
adapter; their synchronous interval is recorded. Errors never select a second entry.
CalcFml retains an admitted typed activation at an instruction boundary and resumes it,
with the same invocation identity and read-view lease, without re-entering the procedure.
The one-operation replacement test exercises both interpreter and native JIT. General
Variant object calls retain an explicit synchronous interval. Typed owning activations
use VM/JIT safepoints; arbitrary Variant bodies do not yet promise an internal safepoint.

A reference becomes a workbook-owned read-only Range object. Supported:
Value2, Count, Row, Column, Address, Cells/Item, and Rows/Columns Count. Variant reference
arguments have TypeName Range; a numeric literal is Double. Application.Caller identifies
the formula cell. VarType reflects the live Range default value (Number5, Empty0, text8, Variant array8204),
while TypeName/IsObject retain the Range. Date/Currency format-dependent defaults and other
object defaults remain open. Saved Range references read current inputs through the next call's
reader. Returning a Range materializes Value2. Multi-area handles support Areas.Count,
Areas.Item and Address; union Value2 and union worksheet callbacks remain typed refusals.
The host tracks identities through axis edits, partial cuts and cell shifts. A partial
cut that splits one Range across worksheets is a typed refusal; its Excel behavior is open. A deleted or
closed-workbook Range remains an object but raises VBA error 424 on access. Undo and
same-name reopen cannot revive its identity. Full VBA object-identity parity is unmeasured.

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
Excel dependency; its old value persisted until explicit calculation. The workbook host pins the cached-read view at invocation admission: hidden Range reads
observe that view without demanding or registering producer dependencies. Observations
retain imported, external and provisional qualifications and propagate to dependent results.
Scalar parameter readiness remains an ordinary dependency. Unavailable observations are
typed refusals; they cannot become blank or zero. Wider Excel object behavior remains open.

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

Engine integration, 2026-11-01 (`dc-hsa5.4`): CalcHost owns live callable registrations;
CalcEngine holds only binding metadata. Reload rebinds positive and negative watches,
invalidates their consumers and revokes old command publication authority. Unresolved
compilation refusals are retried conservatively when registrations change. Retained old
activations keep old code/state until discarded. Invocation certificates record
callable identities, call sites, completion and synchronous intervals. A typed activation
resumes against the same pinned model/cache/reference views without repeating its prefix.
A dependency or spill-placement wait discovered after an effect currently returns a named
continuation-unavailable refusal rather than restarting the procedure. General P03
provisional reevaluation, nested callback reentry and P07 async completion remain open.

The product worker accepts a deduplicated LoadVbaModule command. A failed compile preserves
the prior project; successful reload replaces it and starts new module state. Source is
not yet persisted. The interpreter-only browser path remains the OneCalc route as well.
The cache pin currently copies the workbook's cache map per user formula and the reference
view copies live handles; large-workbook cost and handle reclamation remain unmeasured.
