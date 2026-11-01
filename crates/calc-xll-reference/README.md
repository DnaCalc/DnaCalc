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
