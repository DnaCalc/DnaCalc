# X0: real Excel C API reference

This folder contains Windows x64 measurement tools and an Excel-DNA test add-in.
It is not a Cargo package or a DNA Calc XLL host. The owner direction is full
`.xll` compatibility (Excel-DNA, xlw and xlOil) and broad COM support early.

The [recorded run](traces/2026-11-01-net48/summary.json) contains 117 C API calls
and 1,207 paired boundary/COM calls. Recheck its retained text with:
`python verify.py traces/2026-11-01-net48/traced traces/2026-11-01-net48/control`.
COM forwarding thunks can produce several paired observations for one logical request.
Measured facts and remaining probes are in the
[hosting study](../../docs/architecture/XLL_COM_HOSTING.md#9-x0-measurements-2026-11-01-dc-snz16).

`native/trace.cpp` is a native debugger. `run.ps1` creates Excel through COM,
checks that its window belongs to a new PID, attaches the debugger, then loads
the fixture with `Application.RegisterXLL`. No Excel executable, DLL, COM
registration, Trust Center or security setting is changed. Breakpoints are
temporary process-memory changes. The debugger restores them before detaching;
the runner quits or stops only its recorded Excel process, checking its creation
time as well as its PID. PID 3144 is explicitly excluded. Other agents' Excel
processes are outside its scope.

The tracer observes `MdCallBack12`, `XLCallVer`, `LPenHelper`, exported XLL
lifecycle functions, and exports discovered from actual `xlfRegister` calls.
It reads PE export metadata and boundary arguments, without decoding or
disassembling instructions. Its JSONL includes entry/return identities, function
numbers, raw type/ownership bits, scalar/string/array/reference values, return
codes, native thread IDs and QPC timestamps/durations. Failed callback results
are marked invalid; their memory is still recorded when readable. BigData
handles remain opaque. Array decoding stops after 4096 items or four nested
levels and records any omission; verification rejects an omission in retained
captures. Unsupported signature shapes are identified explicitly.

COM observation starts at `AccessibleObjectFromWindow`. It follows discovered
interfaces, `QueryInterface`, `GetIDsOfNames`, `Invoke`, connection-point
discovery and subscriptions. An AppEvents sink lets the tracer see Excel's raw
`DISPPARAMS`, named DISPIDs, flags and HRESULTs. The first SheetChange handler
reads the worksheet name and range address to identify the raw argument order.
This is a bounded observation of these fixtures, not an exhaustive trace of
every COM vtable or Win32 call made by every possible add-in.

During single stepping, the debugger holds the other target threads, including
threads created while a step is pending. It checks the opened thread's process
ID before holding it. Native suspend status distinguishes a terminating thread
(`0xC000004B`) from access denial; every other failure invalidates the capture.
See Microsoft's [NTSTATUS table](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/596a1078-e883-4972-9bbc-49e60bebca55).
Failed memory reads invalidate a capture instead of silently supplying zero.
Queued watched breakpoints are rewound before detach; stop holds are separate
from the temporary holds used for individual steps.
Holds use independently opened handles; Windows manages the event-supplied
handles through [ContinueDebugEvent](https://learn.microsoft.com/en-us/windows/win32/api/debugapi/nf-debugapi-continuedebugevent).

Build and capture from this directory (Visual C++ x64 tools, a .NET SDK with
.NET Framework 4.8 targeting support, Python 3, and Excel x64 are required):

```powershell
./build.ps1
pwsh -NoProfile -Sta -File ./run.ps1 -RunDirectory ./runs/example-traced
pwsh -NoProfile -Sta -File ./run.ps1 -NoTrace -RunDirectory ./runs/example-control
python ./verify.py ./runs/example-traced ./runs/example-control --out ./runs/example-summary.json
```

Each run uses a new directory. The add-in is built from `fixture/Probe.cs` with
the pinned `ExcelDna.AddIn` 1.9.0 NuGet package, using its original native XLL and
embedded loader/integration assemblies. The owner's Excel-DNA checkouts were
read only; no build writes there. `environment.json` records binary and source
hashes, Excel's file version, bitness, culture and calculation-thread settings.
`fixture.tsv` records managed callbacks, native thread IDs and explicit fixture
COM accesses. `controller.jsonl` records actions and worksheet outcomes.

The Excel-DNA fixture covers hello-world, a thread-safe numeric UDF, native async
(`ExcelAsyncHandle`), a macro-type reference argument, caller lookup, mixed
array ownership, queued macros, background-thread context probes and AppEvents.
The mixed array includes one string. The measured NuGet binary returns it with
type `64`, without DLLFree; source expectations are not substituted for that
observation. `native/owned.cpp` is a second XLL fixture that explicitly allocates
a counted string and returns it with `xlbitDLLFree`. The trace records the exact
pointer and thread of the resulting `xlAutoFree12` call. `UNREGISTER(path)`
through `ExecuteExcel4Macro` tests
native `xlAutoClose`; it is not an Add-in Manager removal or proof that the CLR
or module was unloaded. Earlier diagnostic runs remain ignored under `runs/`.
Only verified captures belong under `traces/`.

QPC durations include debugger pauses, single stepping and thread suspension.
They are evidence of instrumented call order and duration, not normal Excel
latency or an unperturbed calculation schedule. The untraced control checks
worksheet outcomes and fixture completion. A missing export or an entry with
zero calls must not be described as an exercised implementation.

The standard Excel-DNA ribbon helper is not exercised: its
`AutomationSecurityOverride` can change `Application.AutomationSecurity`, contrary
to this session's instruction. Ribbon/COM-add-in activation, RTD-based async,
.NET 8/Native AOT, cancellation/late async returns, uncalculated-cell retry,
legacy Excel4, x86, and the rest of the study's probes remain open.

xlw and xlOil were not built or loaded. A later source-only xlw probe needs a
pinned checkout, its C++ library and InterfaceGenerator built with MSVC v143,
generated sample bindings and an x64 XLL; xlwDotNet adds its managed/interop
toolchain. See the [upstream repository](https://github.com/xlw/xlw).
For xlOil, pin the source and submodules, fetch dependencies with
`scripts/DownloadDependencies.cmd`, use Visual Studio 2022 or newer and ATL for
xlOil-COM, then build the loader/core and a C++ sample. Python support additionally
requires unpacking `external/python.7z` and matching Python development inputs.
Before tracing xlOil, extend export lookup to refresh its rewritten export-table
entry at each registration; this tracer's module-load export snapshot suffices
for the fixtures recorded here.
See its [development guide](https://github.com/cunnane/xloil/blob/master/docs/source/Developer.rst).
Those requirements were read from upstream on 2026-11-01, not tested here.

Original tool/fixture code is MIT. No Excel SDK header or third-party source is
vendored here. ABI facts were checked against Excel-DNA's permissively licensed
`Source/ExcelDna.Loader/{XlCallImpl,XlTypes}.cs` and Microsoft's
[registration documentation](https://learn.microsoft.com/en-us/office/client-developer/excel/xlfregister-form-1).
Recorded Excel outputs are black-box observations. Workbook calculation corpus
scores continue to use the existing `cargo xtask corpus` verdict path; these
boundary traces are not counted as DNA Calc formula passes.

## X1: DNA Calc's first native host

`calc-xll-abi` owns the wire layouts, registration parser and per-call value arena.
`calc-xll-host` loads native modules and adapts them to the existing caller-owned
`calc_func::UserFunctions` registry. `dna-onecalc-desktop` creates an STA and hidden
XLMAIN/XLDESK/EXCEL7 windows (`calc-platform-desktop`), loads the add-ins through
`calc-addin-host`, exports MdCallBack12 and calculates through OneCalc's ordinary controller.
The companion `calc-xll-shim` builds `xlcall32.dll` beside it. `dna-calc-desktop` is DNA Calc's
executable over the same add-in host: `--engine` runs the same case files through CalcHost
(`run.ps1 -Engine`, with `dna-calc-desktop.exe` as `-HostExecutable`) and `--app` hosts the page.
Each exports its own MdCallBack12 and carries its own version resource. These are minimal
executables; the browser applications acquire no native dependency.

```powershell
cargo build -p calc-xll-fixture -p calc-xll-shim -p dna-onecalc-desktop -p dna-calc-desktop
target/debug/dna-onecalc-desktop.exe --xll target/debug/calc_xll_fixture.dll --formula '=MyAdd(1,2)'
cargo test -p calc-xll-abi -p calc-xll-shim -p calc-addin-host -p dna-onecalc-desktop -p dna-calc-desktop
```

The original Rust fixture is loaded unchanged in Excel and DNA Calc. Its single
case list is `fixture/x1-cases.json`. The native tests invoke the actual executable,
including 10,000 extra callable names and a swallowed callback refusal under IFERROR.
Two registrations rewrite the fixture's own PE export metadata, then restore it;
their different results check registration-time address retention as xlOil needs.
This tests registry capacity, not an Excel-DNA 10,000-export binary in this host.

The [retained X1 comparison](traces/2026-11-01-x1/summary.json) binds product source
`523aa3cd` and the same fixture binary in both hosts. All 21 formulas agree; the
ordered 127 C API callback numbers and return codes agree, including the four
outside-context failures. Each host calls xlAutoFree12 once with the returned
pointer on the same thread. Traced and untraced controls agree. The raw async
result-operand difference remains visible in the captures and discrepancy note.
The retained source check has 1,490 passing tests. The structural sweep checks
layering, OS imports, each native package's own wasm refusal, provenance and the
dependency limits; browser tests, app smoke and acceptance were skipped. Corpus
summaries record 18,811 to 18,812 matches over 20,030 cells, with one IMSEC gain
corresponding to merged upstream `19b799bd` and zero losses.

Use the existing runner and tracer for four separate fresh directories:

```powershell
./build.ps1 -NativeOnly
pwsh -NoProfile -Sta -File ./run.ps1 -X1Xll ../../target/debug/calc_xll_fixture.dll -RunDirectory runs/x1-excel-traced
pwsh -NoProfile -Sta -File ./run.ps1 -X1Xll ../../target/debug/calc_xll_fixture.dll -RunDirectory runs/x1-excel-control -NoTrace
pwsh -NoProfile -Sta -File ./run.ps1 -X1Xll ../../target/debug/calc_xll_fixture.dll -HostExecutable ../../target/debug/dna-onecalc-desktop.exe -RunDirectory runs/x1-host-traced
pwsh -NoProfile -Sta -File ./run.ps1 -X1Xll ../../target/debug/calc_xll_fixture.dll -HostExecutable ../../target/debug/dna-onecalc-desktop.exe -RunDirectory runs/x1-host-control -NoTrace
python verify.py runs/x1-excel-traced runs/x1-excel-control --host runs/x1-host-traced --host-control runs/x1-host-control --out runs/x1-summary.json
```

The optional tracer image argument requires the exact explicitly named executable;
its default still accepts only Excel and excludes PID 3144. The runner waits for
detach before releasing its owned host. Existing X0 captures keep their original
environment records; `source-publication.json` identifies the commit that first
published the exact source hashes captured before that commit existed.

X1 checks are bounded to the following requirements, not the whole Excel C API:

| Requirement | Evidence |
|---|---|
| Wire offsets and deep value copies | ABI native tests, independent X0 C++ decoder |
| Registration types and incompatible flags | Table-driven parser tests, 19 observed registrations |
| Formula execution through CalcFml | 21 fixture formulas through the OneCalc controller and Excel |
| Current XLL, caller and reference reads | xlGetName, xlfCaller, xlCoerce boundary calls |
| Outside-context codes | Four callback probes returning 32 in each host |
| Companion exports and executable identity | XLCallVer, LPenHelper traces; version resource 16.x |
| DLLFree pointer and owner | Exact-pointer, same-thread xlAutoFree12 check |
| Callback storage release | xlFree calls and zero retained callback allocations |
| Address retention and registry size | Rewritten export fixture and 10,000-name process test |
| Refusals survive handlers | Native swallowing plus formula IFERROR test |

Known limits and the async callback result-operand difference are in
[DISCREPANCIES.md](DISCREPANCIES.md). Excel-DNA loading, COM objects, UI integration,
worker scheduling and complete async lifetime behavior remain later milestones.
The source follows the documented
[registration ABI](https://learn.microsoft.com/en-us/office/client-developer/excel/xlfregister-form-1),
[memory ownership](https://learn.microsoft.com/en-us/office/client-developer/excel/memory-management-in-excel),
and [async return contract](https://learn.microsoft.com/en-us/office/client-developer/excel/xlasyncreturn).

## X2: original Excel-DNA packages

The [X2 captures](traces/2026-11-01-x2/) run the original ExcelDna.AddIn 1.9.0
loaders on net48 and net8.0-windows, and ExcelDna.AddIn.NativeAOT 1.10.0-preview5
on net10.0-windows. Each managed runtime matches Excel in six Excel-DNA UDF
cases plus the native ownership control (7/7). Native AOT matches the hello case
(1/1). The same files run in Excel and DNA Calc; each traced run has a separate
control without the debugger. These are add-in boundary cases, not corpus scores.

The executable keeps its DNA Calc name and version 16.x. It sets
`EXCELDNA_ASSUME_EXCEL_PROCESS=1` before creating threads. The additional
1.10.0-preview6 identity fixture exercises the switch and ExcelDna.Interop
15.0.1's actual PIA calls. Framework runs in an isolated AppDomain; Version,
Hwnd and Run work through Excel's interfaces. The
[local COM package](../calc-com-host/README.md) records the measured layout.
No class registration or Excel security setting is changed.

The export-pool fixture registers every original f0 through f9999 and checks
their first and last delegate results in both hosts. It does not claim that
all 10,000 delegate outputs were compared. Empty-type commands read their native
Boolean return: TRUE in these net48 captures and FALSE in net8. SyncMacro's
explicit `>B` returns double zero. QueueAsMacro runs through that command on
the STA; `$` functions use calculation workers, with reads serviced by the
owning evaluator. This is worker placement, not concurrent workbook calculation.

Build and use the existing runner:

```powershell
./build.ps1 -X2
cargo build -p calc-xll-shim -p dna-onecalc-desktop
pwsh -NoProfile -Sta -File ./run.ps1 -ExcelDnaDirectory fixture/bin/Release/net48 -HostExecutable ../../target/debug/dna-onecalc-desktop.exe -Reload -NoTrace -RunDirectory runs/example-x2
```

Use `-AddInName X2Aot` with the Native AOT publish directory for hello,
`-AddInName X2Pool` for the export pool, or `-AddInName X2Identity -Cases
fixture/x2-identity-cases.json` for the PIA/identity control. Native AOT reload
is refused: the original preview aborts on a second import-resolver setup.
Process-lifetime payload/runtime identities also prevent reinitialization through
a fresh host or a renamed XLL copy. dc-snz.19 tracks safe runtime reinitialization.

Reproduce each retained comparison with `verify.py`: supply its Excel traced
and control directories, `--host` and `--host-control`, and `--x2-cases
fixture/x2-cases.json` (or `x2-hello-cases.json` for AOT). `--identity` and
`--pool` compare the two respective control directories. `source-publication.json`
binds dirty-tree captures to the commits publishing their exact source hashes.

The verifier preserves two raw async differences: Excel's direct xlAsyncReturn
result operand has type zero while the host supplies the documented Boolean;
net8's incoming opaque handle cbData also differs. Excel-DNA reads only hData on
this entry path. Defined arguments, results and ordered nested callbacks match;
the raw differences remain in the captures and summaries. RTD, cancellation,
overwrites/late completion, full callback permissions, delivered host AppEvents,
the general COM model, legacy Excel4 and x86 remain open.

The existing corpus is unchanged in all 20,030 cell records: 18,812 matches
(93.9191%), 1,131 failures (1,124 mismatches, six refusals and one not run),
73 external-link cells and 14 circular-rule cells. The retained comparison
includes the before/after report hashes. A read-only review reproduced the
profile, PIA and export-pool checks and checked ownership and source bindings.
The structural sweep passes layering, OS imports, native wasm refusals,
provenance and dependency ceilings. Its token scan flags four uses of the
external VBA HAL type name; dc-hsa5.4.1 records that failure. Browser, application
smoke and the long acceptance suite were skipped in that structural run.

The [final integration captures](traces/2026-11-01-x2/integrated/summary.json)
repeat every runtime, PIA, pool and AOT-refusal control after merging the VBA
reference-lifetime work at `053481ad`. The immutable host is identified by its
SHA-256 in each environment. Windows checks pass 1,529 tests with zero failures;
all 20,030 corpus records remain unchanged. Scalar argument reads use the
dependency reader, including worker requests; macro-equivalent callbacks use
the cached-read policy. Nested registered calls that change that policy return
a typed refusal. A dedicated native test with distinct live/cached readers has
not been run; that forwarding was reviewed in source.

## X3: original xlw and xlOil (dc-snz.20)

The [X3 captures](traces/2026-11-01-x3/) compare identical add-in files in Excel
16.0.20527.20088 and the desktop host, using the X0 debugger method plus separate
controls without the debugger. The original xlw 23.10.12-rc generator and library
build its unchanged upstream EchoShort sample. A separate MIT fixture adds scalar,
matrix and reference cases. Original xlOil 0.22.1 binaries run its native Utils
and SQL plugins, and its Python 3.12 plugin with a small MIT Python fixture.
The framework binaries are unchanged; source commits, release URLs, licences and
SHA-256 values are recorded in [the manifest](fixture/x3-upstream.json).

All 27 formulas agree in the traced and control runs. The verifier compares all
28 native UDF invocations, including repetitions, and their ordered nested
callbacks. Seventeen DLLFree returns are released exactly once through the
owning module, on the calling thread, before the next module entry. The four
profiles preserve callback number/code order before teardown. Raw initialization
records still differ: FILTER returns integer versus numeric array elements,
xlEventRegister's result payload differs, and Python registration order differs.
These records remain visible and are not counted as raw matches. Excel unregisters
by path; the host calls Remove then Close, so teardown is reported separately.

Build the pinned inputs outside the repository, then use the existing runner:

```powershell
./build.ps1 -X3 -UpstreamDirectory C:/Work/DnaCalc/codex-runs/x3-upstream
pwsh -NoProfile -Sta -File ./run.ps1 -AddInDirectory C:/Work/DnaCalc/codex-runs/x3-upstream/xloil-runtime -XllFileName xlOil.xll -Cases fixture/x3-xloil-cases.json -HostExecutable ../../target/debug/dna-onecalc-desktop.exe -RunDirectory runs/example-x3
```

Repeat without `-HostExecutable` for Excel and with `-NoTrace` for each control.
Use `verify.py` with the four directories and `--x3-cases`. The Python profile
requires an existing x64 Python 3.12 and NumPy; the builder installs neither.
Keep the same tracer binary for both traced runs. Following the history rewrite,
the verifier can recover a missing revision only from the exact recorded SHA at
the same path in retained history; recovered bindings are listed in the summary.

An unprotected xlOil reload crashes with C0000005. The host now refuses reopening
the exact measured 0.22.1 loader after teardown, including through a fresh host in
the same process. The refusal is a failure, not a compatibility pass. dc-snz.21
owns safe reinitialization and comparison of the same lifecycle in Excel.
Native AOT reinitialization remains dc-snz.19. RTD, event delivery, cancellation,
concurrent workbook calculation, ribbon/UI integration, the general COM model,
xlwDotNet, legacy Excel4 and x86 remain unverified.

Final integration at `ed838379` repeats all 27 formulas, 28 native invocations
and 17 owning-module frees after the shared evaluator changes. The native tests
pass 21 cases; Linux's full check at `65669173` passes 1,506, with zero failures,
five ignored and three not run. All 21 complete corpus verdict/evaluation files
remain byte-identical: 18,812/20,030 matches and 1,218 nonpasses.

The structural sweep passes on clean `65669173`. The full sweep on `97464996`
remains failed: the Chrome worker runner ran out of memory before startup, and
DNA Calc has three UI failures in each browser (dc-971p.6). The unchanged worker
WASM passes all 23 cases in a focused Chrome retry. OneCalc passes 52 cases in
each browser. The long acceptance suite was skipped. The validation records keep
the initial file-lock failure, runner failure, retry and exact UI failure messages.

## R7: strict pool/RTD traffic and bounded native xlOil reload

The [R7 evidence](traces/2026-11-03-xll7/) supersedes the earlier strict pool and
RTD failures. The pool profile matches all 15 native entries and two owned frees.
The original signaling RTD profile matches eight client calls. Its 22-second idle
extension disconnects the completed transport before idle; a separate persistent
numeric provider has seven calls including exactly one Heartbeat on each host.
The final admitted P07 input remains certified until formula replacement.

Use the existing four-directory verifier with `--workbook-pool-cases` or
`--rtd-cases`; the retained idle case files add `rtd_idle_ms: 22000` to the first
case. The numeric destructor's internal ServerTerminate body is retained under
its final owning Release, not counted as another client request. The negative
trace with a nonzero final reference count is rejected.

For the exact original native Utils/SQL profile, run `run.ps1 -Reload` or
`-ReloadNewHost` with `fixture/x3-xloil-cases.json`. Pass the Excel traced/control
and host traced/control directories to `verify.py --xloil-reload-cases`, with
`--source-publication` naming the retained publication. The verifier compares 12
formula outcomes and 13 ordered native calls before/after host reload, ownership
and actual DLL unload/reload cycles. Original Excel reload still crashes: that
failed lifecycle, its debugger diagnostics and both RPC failures remain explicit.
Only its complete first-load calls are used as the native reference.

The host repair requires exact loader/core/ini/plugin hashes, no settings override
or prior core, successful teardown, drained originating module leases and checked
process-local window ownership. It never changes framework binaries or Excel COM
registrations. Python and settings-override reload controls still refuse. A
renamed-loader control crashed and remains unqualified. Broader runtime teardown
and general safe reload stay open under dc-snz.21; the finite X5 results do not
close the remaining concurrency, cancellation and RTD scope of dc-snz.23.
