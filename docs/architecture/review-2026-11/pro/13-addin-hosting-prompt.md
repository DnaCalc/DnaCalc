# Question for GPT-6 Pro: hosting Excel add-ins (.xll and COM) in DNA OneCalc and DNA Calc: gap analysis, architecture critique, implementation plan, risks

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet that must reproduce Excel's
observable behaviour exactly (bit-identical numbers, the same error values, the same invocation
counts where Excel exposes them through callbacks). The repository is public:
https://github.com/DnaCalc/DnaCalc (MIT). This question is pinned to commit
`1ece3d0ffeae28f27aa0ff28c2d873e3a6c2af84`
(https://github.com/DnaCalc/DnaCalc/tree/1ece3d0ffeae28f27aa0ff28c2d873e3a6c2af84). Every path below
is relative to the repository root, and every `file:line` refers to that commit. You may read the
repository if you can; everything you need to answer is also in the appendices, so do not depend on
browsing.

This topic is a **design review**, not a numeric one. The question is how a native Windows host
that is not Excel should load and run add-ins written for Excel, so that unmodified add-ins built with
**Excel-DNA, xlw and xlOil** work out of the box (PyXLL later), with **full .xll (Excel C API)
compatibility** and **broad COM support** as an early focus. User-defined functions (UDFs) come
first; commands, ribbons and the wider object model follow. The owner's wording: "full .xll
compatibility, including Excel-DNA, xlw and xlOil out of the box, and broad COM support are an early
focus."

The hosts are two Windows x64 desktop programs sharing one add-in host crate:
- **DNA OneCalc** (`apps/dna-onecalc-desktop`): a formula scratchpad with a WebView2 UI and no
  engine and no workbook dependency graph. It loads add-ins from a panel and evaluates one formula
  at a time. It is the first target.
- **DNA Calc** (`apps/dna-calc-desktop`): the spreadsheet, over `calc-host` and `calc-engine`
  (dependency graph, recalculation contract, spill, iteration, RTD admission, async tokens).

Ground truth is **Excel itself on the oracle machine** (AMD Ryzen 7 4800H, Windows x64; the
measurements in this prompt were taken on Excel 16.0 builds 20527 and 20150 as recorded in our
design documents). We measure Excel by running a native debugger-based tracer that records every
boundary call between Excel and an add-in (entry points, arguments, `Excel12` callbacks with return
codes and result operand bytes, thread ids, COM calls), then running the same add-in binary,
unmodified, in our host and comparing the two traces and the formula results. Everything below
that says "measured" comes from that instrument or from Excel results read through COM.

## What we ask

Please produce, in this order:

**A. A gap analysis** per framework: for **Excel-DNA** (classic .NET Framework hosting, .NET 6/8
hosting, and Native AOT), **xlw** (the C++ library, the generated sample, xlwDotNet), and **xlOil**
(native core, plugins, xlOil_Python, workbook-local VBA functions): which Excel C API calls
(function numbers, argument shapes, return codes), XLOPER12 behaviours (type bits, ownership flags,
reference forms, string conventions), registration features (type-string codes, flags, argument
metadata, hidden names, re-registration), threading and lifecycle expectations (export names,
unload/reload), and COM object-model members each needs **at load**, **in common use**, and **in
less common use**; compared against what we implement (Appendices B, C, F, G, I, J). Pro knows
these frameworks' public behaviour; we supply our side. Output a table keyed by our status IDs
(S1.. in Appendix B, R1.. in Appendix J): *call or feature / framework(s) and when / our status /
what happens in the framework if we are wrong / how likely it matters*. Flag every place where our
recorded belief in Appendix D is wrong or incomplete. Mark each claim **[public-doc]**,
**[public-code]**, **[recollection]** or **[inference]**: we test against the real binaries, and a
confident wrong claim costs us a day.

**B. An architecture critique** of our hosting design (Appendix A): the process model (everything
in-process; one STA main lane; the executable exports `MdCallBack12`; an `XLCALL32.DLL` shim beside
it), threading (the owner lane, the four-thread pool for `$` functions, async completions, the
"calc thread" of the engine), interaction with recalculation and volatility (Appendices A.4 and E),
error and crash isolation (there is none today: Appendix A.6), and the memory-ownership rules
(Appendix C). Say what is wrong, fragile, or merely untidy, and what you would change. In particular:
should some or all add-ins run in a **separate process** (the .NET-runtime-per-process limit,
Native AOT and xlOil Python that cannot be unloaded and reloaded, native crashes, hangs) with a
marshalling layer, and what would that break in the C API (references, `xlCoerce`, `xlfCaller`,
callbacks during a call, RTD, COM STA)?

**C. A prioritised implementation plan** with a test strategy built on **real add-ins and
black-box comparison with Excel**. For each step: what it implements (exact function numbers,
return codes, struct layouts, algorithms in prose or pseudocode, Rust-level interface sketches
welcome); which add-in, framework feature or user scenario it unblocks; which of our existing
oracle instruments tests it (Appendix H); how to know it matches Excel. Order by "unblocks the most
real add-ins per unit of risk", UDFs first. Separate what must be exact (ordered callback
numbers and return codes, invocation counts) from what may differ (pointer values, timing, module
paths).

**D. The risks and the behaviours most likely to differ subtly from Excel**, each with a
discriminating **experiment**: a small add-in function (give its type string, body in C against the
Excel12 API, or Excel-DNA/VBA snippet), the inputs, what to record (return codes, result operand
bytes, thread ids, invocation counts, callback order) and the expected outcome under each competing
hypothesis. We run them on our Excel answer service (a harness that loads a native fixture .xll
into Excel and reads results, return codes and operand bytes) and feed the results back, so
experiments should be runnable with our fixture: it can register any type string and call any
`Excel12` function number with any operands (Appendix H).

### Specific questions to cover inside A to D

1. **Uncalculated reads (xlretUncalced, status 64).** Excel measurement (Appendix F.3): an ordinary or
   `$` UDF whose `xlCoerce` reads a not-yet-calculated cell gets status 64, must unwind, and is
   called again after the precedent calculates; a `#` function reads the old value. Our
   `calc-xll-host` has no constant 64 and `XllFunction` does not override `reevaluation_policy` (the
   engine already supports `ReevaluationReason::XllUncalcedUnwind`, used by test doubles and the VBA
   adapter). Specify the exact behaviour to implement: what the add-in can and cannot call after 64,
   how many entries, thread, what Excel-DNA/xlw/xlOil do on 64, and how it interacts with `$`.
2. **Memory ownership.** Argument operands (who frees, flags Excel sets), callback result operands
   (does Excel set `xlbitXLFree` on `xlCoerce`, `xlfCaller`, `xlfGetName` results? we do not),
   `xlbitDLLFree` on results, `xlAutoFree12` per result and thread, Excel-DNA/xlw/xlOil free
   patterns, xlw's pool, in-place return codes, strings with embedded NULs, 32,767-character limits,
   `xltypeBigData`. Give a complete ownership table and the leaks or double frees our design allows.
3. **Events and lifecycle.** `xlEventRegister` is stored but never raised in our host
   (`CalculationEnded`, `CalculationCanceled`). `xlAddInManagerInfo12`, `xlAutoAdd`, `xlAutoRegister12`
   are never called. Which frameworks rely on them, and when does Excel raise them.
4. **Commands and macro-sheet functions.** `xlcRun`, `xlcOnKey`, `xlcAlert`, `xlcNew`,
   `xlcWorkbookInsert`, `xlfAddMenu` family, `xlSet`, `xlcFormula` are refused. What is the smallest
   command model that makes Excel-DNA (`QueueAsMacro`, `ExcelAsyncUtil`, menus) and xlOil
   (`xloRunInXLLContext`, workbook functions) work, and what are the exact permission rules per context
   (cell, `#` cell, `$` cell, command, async thread, event, outside)?
5. **Threading.** Excel's multithreaded recalculation assignment of `$` calls, thread-local state
   expectations in the three frameworks (xlw's pool reset, .NET thread statics, CPython GIL), and
   whether our design (four pool threads, private `State` per call, reads proxied to the owner) can
   expose a framework to a state it cannot see in Excel. When may Excel call a `$` function on the
   main thread?
6. **COM.** For each framework, the minimum Application/Workbook/Worksheet/Range/Window/COMAddIn
   surface at load and in common use, and how early-bound access works (which interfaces and
   vtable slots are touched by the .NET PIAs, `ExcelDna.Interop`, xlOil's `#import`). We answer
   Excel's IIDs from layouts read from Excel's type library, and refuse the rest per member
   (Appendix I): is "complete layout, honest per-member refusal" a workable contract, and what is
   the order of members to implement?
7. **Volatility and registration flags** (Appendix E): the C API side: `!`, `#`, `$`, `&`, `X`,
   `xlfVolatile` permissions, async volatility.
8. **Persistence and rebinding**: `_xll.` naming in the file, behaviour when the add-in loads after
   the workbook, Excel-DNA/xlOil naming and ProgIDs in saved files, what must be preserved.
9. **Add-in specific shims**: we already have one keyed by a SHA-256 for xlOil 0.22.1 (reload
   refusal). Policy: when is a framework-specific accommodation right, and how do we keep each
   one visible, counted and removable?

## Constraints that bind the answer

1. **Behavioural parity with Excel is the goal.** The answer is judged by recorded Excel answers:
   formula results, ordered callback numbers and return codes, invocation counts and thread
   placement, `xlAutoFree12` calls, COM call sequences. Pointer values, module paths, timings and
   register ids are not compared.
2. **Source rules** (from `docs/plan/RESEARCH_SOURCES.md`). Tier 1 (free to read): published papers
   and standards, textbooks, **Excel's own outputs (captures, witness banks, COM probes)**. Tier 2
   (permissive code): Boost.Math, fdlibm and similar. Tier 3 (facts only): Numerical Recipes, R nmath,
   glibc libm. Tier 4 (never used): Microsoft source, **disassembly or decompilation of Excel or
   Microsoft runtime DLLs**. For this topic: you may use **public documentation (Microsoft Learn's
   Excel C API and object-model documentation, the Excel 2010 XLL SDK documentation), the
   open-source add-in frameworks' public code (Excel-DNA, xlw, xlOil) and black-box measurement of
   Excel**. You may not use the XLCALL.H/XLCALL.CPP text of the Microsoft SDK as code to copy (we
   write layouts from the documented facts), nor anything learned by disassembling Excel. Our
   instrument calls Excel and observes its boundary; it does not read Excel's code.
3. **Method: evidence first.** Separate what the frameworks do (public code), what Microsoft
   documents, what we measured, and what you infer. Do not propose a design that passes only the
   frameworks we happened to test: derive it from the C API contract and check it against each
   framework's needs. A pass on a happy-path formula is not evidence that callback order, counts and
   ownership match.
4. **Pro cannot run anything.** Ask for experiments (Deliverable D) with inputs and expected outcomes
   per hypothesis; we run them and feed back.
5. **The deliverable must be implementable** by Sonnet agents under oversight, in Rust with a
   little C/C++ (the COM layer is C++): exact function numbers, return codes, operand layouts,
   algorithms, ordering rules, and test cases. No hand-waving about "robust" or "full"; say which
   member, which code, which order. Respect our layering (Appendix A.1): `calc-fml` never sees an
   XLOPER; the engine never depends on the XLL host; the application composes.
6. **No unmeasured guesses presented as facts.** Where Excel's behaviour is unknown to you, say so
   and design the experiment.

---

## Appendix A. The hosting design as built (assembler's description; the code is in Appendix K)

### A.1 Crates and layering (all native-only; none compile for wasm, so the browser builds carry no native dependency)

| Crate | Role |
|---|---|
| `crates/calc-xll-abi` | `#[repr(C)]` XLOPER12/FP12/XLREF12/XLMREF12 written from the documented layout; return codes; type-string parser to `XllSignature`; XLOPER12 <-> `CalcValue` conversion with a per-call `Arena` |
| `crates/calc-xll-host` | Module loading and lifetime; registration table (id, use count, hidden name, entry address, originating and free modules); the `dispatch` function that the executable's `MdCallBack12` export forwards to; per-thread callback frame; libffi call of the entry; result copying and `xlAutoFree12`; async handle table; pool for `$` functions; `xlfCall`/`xlfRegisterId` (`call.rs`); RTD bridge |
| `platform/calc-xll-shim` | The `XLCALL32.DLL` companion: `XLCallVer` (returns 3072), `LPenHelper` (answers `xlGetFmlaInfo` as ready mode), `Excel12`/`Excel12v` forwarding to the executable's `MdCallBack12`, `Excel4`/`Excel4v` that return 32 |
| `crates/calc-com-host` | Local COM objects answering `WM_GETOBJECT`/`AccessibleObjectFromWindow(OBJID_NATIVEOM)` on the `XLMAIN > XLDESK > EXCEL7` window chain; Excel's IIDs and measured vtable layouts; AppEvents connection point; COMAddIns + `IDTExtensibility2`; ribbon XML retention; a native IRtdServer client (`rtd.cpp`) |
| `crates/calc-addin-host` | `AddinHost`: owns the XLL host, the COM projection, the RTD client, VBA modules/add-ins and publishes one `CallableTable` to CalcFml |
| `crates/calc-xll-fixture` | Our own test .xll (Rust cdylib, ~70 exports) used for X1 comparisons |
| `crates/calc-xll-reference` | The Excel-side instrument: a native x64 debugger (`native/trace.cpp`), a runner, a verifier (`verify.py`), fixtures (C#, C++ for xlw, Python for xlOil), case files, retained traces | |
| `crates/calc-func` (`udf.rs`) | Engine-neutral traits: `UserFunction`, `UserFunctionContext`, `UserFunctionInvocation`, `RtdConnector` |
| `crates/calc-fml` | `CallableTable` (registry keyed by (provider, name) with generations), compile-time binding of calls to callable handles, evaluation of user calls (`eval/run.rs`) |

Rules: `calc-fml` never sees an XLOPER; `calc-engine` never depends on `calc-xll-host`; the application
composes. `tools/xtask/architecture.json` enforces a dependency ceiling (OneCalc may not depend on
`calc-host`, `calc-engine` or `calc-doc`).

### A.2 Process model

- **One process, one STA main thread.** The desktop executable creates an STA, a hidden window chain
  `XLMAIN > XLDESK > EXCEL7` (the page is hosted in WebView2 inside EXCEL7), pumps messages, and exports
  `MdCallBack12` (linker `/EXPORT`), which every framework finds with `GetModuleHandle(NULL)` +
  `GetProcAddress` and calls with `(xlfn, count, LPXLOPER12*, LPXLOPER12 result)`. It carries a version
  resource 16.x and sets `EXCELDNA_ASSUME_EXCEL_PROCESS=1` before creating threads. The executable keeps
  its own name (not `EXCEL.EXE`).
- **XLL load**: `LoadLibraryExW`, resolve `xlAutoOpen`, run it on the main thread inside a
  "lifecycle" callback frame (a thread-local pointer to a `Frame` naming the module); wait until the
  set of registrations stops changing (xlOil Python registers after `xlAutoOpen` returns); a load that
  fails (xlAutoOpen returns 0) rolls back names and use counts. Unload: `xlAutoRemove`/`xlAutoClose`,
  remove registrations, `FreeLibrary` only when no native call, async handle or RTD server is
  outstanding. Not called: `xlAddInManagerInfo12`, `xlAutoAdd`, `xlAutoRegister12`.
- **Registration**: the `xlfRegister` handler resolves `GetProcAddress` at register time (so xlOil's
  rewritten export works), keeps the address per register id, parses the type string, stores
  documentation (argument names, category, help), publishes a hidden name -> register id map used by
  `xlfEvaluate`/`xlUDF`/`xlfRegisterId`, and a `CallableDescriptor` for completion and signature help.
- **Calls**: a registered function is a `UserFunction` in CalcFml's `CallableTable`. The evaluator
  prepares arguments, the adapter marshals them into an arena (`XLOPER12`, `FP12`, wide strings,
  numerics), calls the entry through libffi on the owner thread (or on a pool thread, below), then
  copies the result out, calls `xlAutoFree12` of the module named in `xlfRegister` when
  `xlbitDLLFree` is set, and frees callback storage on `xlbitXLFree`.
- **No process isolation, no SEH handling, no timeouts inside native code** (A.6).

### A.3 Threading as built

- Evaluation of a workbook is single-threaded in the engine except for a bounded form of
  "independent scalar activations": a callable may declare `max_parallel_calls` > 1 when
  registered (thread-safe XLL adapters publish their pool capacity); only legacy (implicit
  intersection) scalar cells with eligible calls share a segment; dynamic spill formulas, legacy array
  producers, volatile plans, iteration and data tables stay ordered.
- **`$` (thread-safe) functions** run on a pool of 1 to 4 OS threads (`available_parallelism` clamped).
  Only owned values and request messages cross the boundary: a worker builds a **private `State`**
  (registrations empty), takes its own module leases, calls the entry, and any callback the add-in
  makes (`xlCoerce` of a reference, `xlfCaller`, `xlfVolatile`, worksheet functions) is sent as a
  message to the owner lane which services it on the evaluator's reader and replies. So callbacks of
  `$` functions are serialised on the owner. `xlfRtd`, `xlUDF`, `xlfEvaluate`, `xlfRegister`,
  `xlfUnregister`, `xlfSetName`, event registration from a `$` function return 128
  (`xlretNotThreadSafe`). The formula-entry evaluation of a `$` function runs on the owner (as Excel
  does at entry, measured). Thread-safe async entries are refused.
- **Non-`$` functions** run on the owner thread, in the evaluator's turn, in a "state lane" per
  originating module (a second activation of the same lane while one is occupied is a typed
  refusal, `permits_reentry` false).
- **Async (`X` argument, `>` return)**: the synchronous part runs on the owner; the argument is an
  `xltypeBigData` handle holding a never-reused request key; the evaluation parks as an
  `EvalContinuation`; `xlAsyncReturn` may be called from any thread, is checked against the
  owner-issued token (overwrite, undo, reload, cancel invalidate it), and its value is admitted by the
  engine (`CalcHost.complete_async`); `xlAsyncReturn` clears the whole result operand to 32 zero bytes
  (measured). A call that does not finish in 30 s fails the calculation (OneCalc); a bare wait has 10 s.
  An entered, cancelled call keeps its module lease until a late return is seen.
- **RTD**: a native `IRtdServer` client on the owner STA; `UpdateNotify` only sets an atomic; the owner
  polls `RefreshData`; admitted values enter the engine as inputs with sequence numbers (the "P07"
  admission route), and the formula that read them is evaluated again. Scalar values only; array and
  remote topics are refused (a controlled VARIANT-array server maps to `#N/A` as measured in Excel).

### A.4 Recalculation and volatility interplay

- The adapter reports `volatile` (static from `!`, and from `#` plus an `R`/`U` parameter), `thread_safe`,
  `asynchronous`, `macro_equivalent` (`cached_reads()` => `ReadPolicy::CachedMacroObservation`: a `#`
  function reads the cached value of an uncalculated cell instead of failing), `state_lane`,
  `permits_reentry`, `max_parallel_calls`, `permits_early_execution`, `reevaluation_policy`.
- `xlfVolatile` calls `UserFunctionContext::set_volatile(bool)` ("last executed request wins").
  Dispatch answers `xlfVolatile` (237) and `xlfGetWorkspace` (186) with `xlretInvXlfn` (2) from a
  non-`#` function (measured). The measured rules (Appendix E) say volatility is an ordered list of
  events and the last one wins; the engine's decision function is still the older sticky OR
  (dc-hsa5.20.2 is the open work item that replaces it).
- Referenced cells of a ref argument (`U`/`R`) are read lazily through the invocation's read view:
  `xlCoerce` -> `UserFunctionContext::values_of`. A read of a cell not yet calculated surfaces as an
  evaluator fault ("missing input"); how that maps to what the add-in sees is R1 in Appendix J.
- Argument preparation (a reference passed where the registration wants a value `Q`, `B`, `E`...) is
  done by the adapter before the call via `argument_values_of`; a not-yet-calculated precedent makes the
  evaluator wait (suspend) before the native entry is called at all.

### A.5 COM side

An in-process C++ object model (`calc-com-host`) reached by `WM_GETOBJECT` on EXCEL7. See Appendix I.
No class factory, no ROT entry, no registration of Excel's ProgIDs/CLSIDs/type libraries, nothing
written to HKLM; COM add-in discovery reads private HKCU classes whose in-proc server is an XLL
already loaded by this host. Owner constraint: nothing may change or shadow the machine's COM
registrations for Excel. COM activation (class objects, registration-free manifests, ROT, hooking)
is an open design question; none adopted.

### A.6 Isolation and failure handling today

- `catch_unwind` around `dispatch` converts a Rust panic into status 32; no structured exception
  handler or vectored handler exists: an access violation in an add-in kills the process.
- A hung `$` function keeps its module resident until process exit (the pool is not joined on final
  drop when work is outstanding).
- Runtimes that cannot be started twice in one process (Excel-DNA Native AOT, xlOil Python) refuse
  reload with a typed message naming a bead; xlOil 0.22.1 crashes Excel itself on UNREGISTER followed
  by RegisterXLL (measured), so the refusal is parity, not a bug. The xlOil accommodation is keyed by the
  SHA-256 of two exact binaries.
- Refusals are first-class: a callback or capability we do not model records a typed refusal (owner
  bead named) that survives the add-in's own error handling and formula `IFERROR`, and counts as a
  failure in the corpus headline.

---

## Appendix B. Excel C API surface: what `MdCallBack12` does with each function number (assembler's reading of `callbacks.rs`; IDs are for your table)

Status words: **Impl** implemented and compared with Excel; **Partial**; **Perm** a permission denial
modelled from measurement; **Refused** typed refusal (the add-in sees a failure code and the call is
recorded as a refusal); **Absent** falls to "unsupported callback" (`xlretInvXlfn` 2). "Measured"
means the ordered callback numbers and return codes agreed with Excel in the traced comparison for the
tested add-ins, not that every operand shape was compared.

| ID | Number and name | Status and notes |
|---|---|---|
| S1 | 0x4000+0 `xlFree` (16384) | Impl: frees our callback allocations by pointer key (strings, arrays, reference tables); ignores operands we do not own; returns no output operand. Outside any call context returns 32 (measured on Excel-DNA's route) |
| S2 | 16385 `xlStack` | Impl: returns bytes of headroom of the real thread stack as `xltypeInt`; Excel's value and in-context semantics unmeasured. Outside context 32 |
| S3 | 16386 `xlCoerce` | Partial: to Num/Str/Bool/Multi/Ref/Int; ref -> value via the invocation reader; text -> number only in an `en-US` environment; error/missing/nil conversions partly refused; `xltypeRef` target returns the reference untouched; result carries no `xlbitXLFree` |
| S4 | 16387 `xlSet` | Absent (no cell writes from the C API) |
| S5 | 16388 `xlSheetId`, 16389 `xlSheetNm` | Impl (name -> id, ref -> name, with the active/caller sheet) |
| S6 | 16390 `xlAbort` | Impl: always FALSE |
| S7 | 16391 `xlGetInst` | Absent; 16403 `xlGetInstPtr` Impl (module handle in an `xltypeBigData`) |
| S8 | 16392 `xlGetHwnd` | Impl (the XLMAIN handle as `xltypeInt`) |
| S9 | 16393 `xlGetName` | Impl: path of the module whose entry is running (the "current XLL" context) |
| S10 | 16394/16395 `xlEnableXLMsgs`/`xlDisableXLMsgs`; 16396/16397 `xlDefineBinaryName`/`xlGetBinaryName` | Absent |
| S11 | 16398 `xlGetFmlaInfo` | Only through the shim's `LPenHelper` (always "ready"); the `Excel12` route is Absent |
| S12 | 16400 `xlAsyncReturn` | Impl (token-checked; 256 `xlretInvAsynchronousContext` for stale, duplicate, revoked) |
| S13 | 16401 `xlEventRegister` | Partial: events 1 and 2 are stored per module; **never raised**; `$` caller gets 128 |
| S14 | 16402 `xlRunningOnCluster` | Impl: FALSE |
| S15 | 89 `xlfCaller` | Partial: a 1x1 `xltypeSRef` of the calling cell; `#REF!` error in command/queued-macro context (measured); array/multi-cell callers, names, conditional-format, data-validation, charts: not modelled |
| S16 | 149 `xlfRegister` (Form 1) | Impl for the measured shapes (Appendix C); Excel-DNA, xlw, xlOil registrations compared |
| S17 | 201 `xlfUnregister` | Partial: decrements the use count; the hidden name stays; no `xlfSetName` coupling beyond clearing |
| S18 | 88 `xlfSetName` | Partial: only the wizard-name removal pattern (clears the name) |
| S19 | 150 `xlfCall`, 267 `xlfRegisterId` | Impl (dc-gh22): 289 formulas compared; types `O`, `P`, `R` refused |
| S20 | 255 `xlUDF` | Impl: runs another registration by id or name; refused from a worksheet function when it changes the read policy (`#` vs ordinary) or targets a command; 128 from `$` |
| S21 | 257 `xlfEvaluate` | Partial: a registered name returns its register id; otherwise a **constant** expression only (no references, no names, no registered calls); 128 from `$` |
| S22 | 379 `xlfRtd` | Impl (scalar topics, local servers, via the owner connector); remote server and non-text topics refused; 128 from `$` |
| S23 | 237 `xlfVolatile` | Impl: status 2 from a cell call of a non-`#` registration (ordinary, `!`, `$`, async), status 0 from a `#` registration in a cell (sets the cell's request) and from command/lifecycle context (no effect), 32 from a thread with no frame; this matches the measured table in Appendix E (our design document still says the host answers everywhere: stale) |
| S24 | 186 `xlfGetWorkspace` | Partial: items 2 (version "16.0") and 37 (international settings array, from Windows NLS, English function syntax); every other item refused |
| S25 | 146 `xlfReftext` | Partial (single-area references) |
| S26 | 32890 `xlcMessage` | Impl (status-bar message state; `$` gets 128; non-`#` gets 2) |
| S27 | other `xlf` numbers (Ftab index) | routed to CalcFunc's catalogue by name through the evaluator's ordinary worksheet route (`EXCEL_FUNCTION_IDS`); 614 maps to FILTER; macro-sheet information functions (`GET.CELL`, `GET.WORKBOOK`, ...) are not catalogue functions: Absent |
| S28 | every other `xlc` command (0x8000+n): `xlcRun`, `xlcOnKey`, `xlcAlert`, `xlcNew`, `xlcWorkbookInsert`, `xlcFileClose`, `xlcEcho`, `xlcFormula`, `xlcFormulaArray`, `xlcSelect`, ... | Refused from worksheet functions with 2 (measured rule), Absent from commands/`xlAutoOpen` |
| S29 | `xlfAddMenu`, `xlfAddCommand`, `xlfDeleteMenu`, `xlfGetBar`, `xlfGetTool`, `xlfDeleteCommand` | Absent unless the Ftab table routes them to a catalogue function (none do) |
| S30 | `Excel4`, `Excel4v` (legacy XLOPER) | the shim exports them and returns 32 for everything; no legacy XLOPER, no `xltypeStr` 8-bit, no `R`/`P`/`C`/`D` legacy types |
| S31 | `LPenHelper` | `xlGetFmlaInfo` only |
| S32 | `XLCallVer` | 3072 (12*256), not an Office build number |

Return codes constants we know: Success 0, InvXlfn 2, InvCount 4, InvXloper 8, Failed 32, NotThreadSafe
128, InvAsynchronousContext 256. **No `xlretUncalced` (64) and no `xlretAbort` (1) constant exists in
the ABI crate.** Callback frame discovery uses one thread-local "current frame" pointer; a callback with
no frame (any other thread, outside a call) returns 32.

## Appendix C. Registration, type strings, argument conversion, result handling

**Type strings parsed** (`signature.rs`): result code first, then arguments; flags `!` `#` `$` `&` at
the end; `%` suffix for the wide forms. Parser accepts: `A B H I J` (bool16, double, uint16, int16,
int32 by value), `E L M N` (double, bool, short, int by pointer), `C D F G` (ANSI), `C% D% F% G%`
(UTF-16 null-terminated / counted / in-place variants), `K` (FP) and `K%` (FP12), `O`/`O%` (array,
argument only), `P` (XLOPER), `Q` (XLOPER12), `R` (XLOPER reference), `U` (XLOPER12 reference), `X`
(async handle, exactly one with a `>` return), `>` (void), digits 1..9 (in-place return naming the
argument). Validation: at most 255 arguments; `#` and `$` exclusive; async needs one `X`, a `>` return
and no `&`; `volatile |= macro_equivalent && any R/U argument` (static, see R4).

**Invocation ABI modelled** (a libffi call interface built at registration; `None` = not modelled):
by value `B` (f64), `A`/`I` (i16), `H` (u16), `J` (i32); pointers `E L M N`, wide strings `C% D% F% G%`,
`K%`, `Q`, `U`, `X`; `>` return. **Not modelled for worksheet calls**: ANSI strings `C D F G`, legacy
`K`, `O`, `O%`, `P`, `R`, and in-place returns (digit result, `F%`/`G%` result, `>` with a pointer first
argument is only valid for async/commands) -> the registration succeeds, the function is listed, and
a call returns `#VALUE!` plus a refusal naming dc-snz.2. (`call.rs` has its own marshalling that does
cover ANSI types for the `xlfCall` route.)

**Argument conversion**: a reference argument to a non-`U`/`R` parameter is replaced by its value(s)
(`argument_values_of`); a scalar-typed parameter given an error becomes `#VALUE!` result without
calling the add-in (measured with the unchanged fixture); other conversion failure -> `#VALUE!`; numbers
for integer types truncate (range-checked); missing arguments are padded as `xltypeMissing` up to the
declared count; more arguments than declared is an error. A `U`/`R` parameter receives an `xltypeSRef` when
the argument is a single-area reference on the caller's sheet and `xltypeRef` (with a sheet handle) otherwise;
multi-area references with several sheets are refused. Arrays become `xltypeMulti` with element
`Nil` for blanks; nested arrays refused. Non-finite numbers and LAMBDA values as arguments are refused.

**Result handling** (`convert_return`): the result operand is **copied before any further native call into
the module**; if `xlbitDLLFree` is set the module named in `xlfRegister` supplies `xlAutoFree12` which is
called once with the exact pointer on the calling thread (measured for the single-thread case); if
`xlbitXLFree` is set our `free` runs; a null result pointer is `#NUM!`; `Fp12` results are validated
(finite, at most 1,048,576 elements); `xltypeStr` over 32,767 units is refused; errors outside the
supported set are refused; nonfinite doubles are normalised (`normalize_return`).

**Callback result ownership**: a value returned through a callback (xlCoerce, xlfCaller of non-SRef,
xlfGetName, ...) is built in an `Arena`, stored in `State.allocations` keyed by its pointer (string, array
or reference table), and freed when the add-in calls `xlFree` on it or when the module is removed. **We
never set `xlbitXLFree` on these operands** (R3). Callback results Excel keeps valid "until the calling function
returns" (the `xlfCall` case) live in `Frame.transient`.

**Hidden names and use counts**: `names` maps the case-folded function name to the latest register id; a
second `xlfRegister` with identical (module, address, type text, name, macro type, origin) increments the use
count and returns the same id; `xlfUnregister` decrements; at zero the function evaluates as `#NAME?`.

**Persistence**: the xlsb reader maps add-in external names to `_xll.` names (`calc-doc/src/xlsb/formula.rs:732-739`);
the binder resolves `_xll.Name` through the callable table; OneCalc's session file (a constrained .xlsx) lists
loaded add-ins with kind, path and SHA-256 in a custom XML part but loads nothing on open (the person
loads them; a changed hash is refused). Behaviour of cells whose add-in is missing or loaded later is
**not** yet matched to Excel (design doc: [P]).



## Appendix D. What we believed about the frameworks and Excel before measuring (design document, verbatim, with our evidence tags)

Tags: **[V]** verified in source or Microsoft documentation, **[I]** inference, **[P]** must be measured. These are OUR beliefs; deliverable A should correct them where they are wrong or incomplete. (Excel's multithreaded-recalculation rules, the callback usage table and the COM tiers of the same document are omitted; you know them.)

### D.1 (our 1.3) Registration type strings and XLOPER12 [V]

- Codes used: Excel-DNA `Q U E D% K% L X` (`ExcelDna.Loader/XlTypes.cs:186-192`), return `Q`/`>`; suffixes `! # $ &`
  (`XlRegistration.cs:342-357`; `$` never with `#`; `&` never with `X`), commands with `>B` (SyncMacro, macro type 2).
  xlw: return `Q`, args `Q B K% U`, suffixes `! $ & #`, commands `A`. xlOil: `U Q K% X`, return `U K% A >` or digit
  (in-place), one of `! # $`.
- Excel rules: `#` and `$` exclusive; `#` + `R`/`U` arguments makes the function volatile by default; `X` with `>`
  marks async; async and cluster-safe exclusive; up to 255 arguments [V xlfRegister page].
- Types exchanged: Num, Str (counted UTF-16), Bool, Err, Multi, Missing, Nil, Ref (multi-area), SRef, Int, BigData
  (`xlGetInstPtr`, async handle), FP12. `xlbitDLLFree` returns: Excel copies, then calls `xlAutoFree12` of **the module
  named in `xlfRegister`** (xlOil: `xlOil.dll`, not the .xll) [V doc + xlOil]. xlw returns pool memory with no free bit,
  reset on the next top-level entry on that thread: copy the result before the next call into that module [V].

### D.2 (our 2) How an xll loads, and what DNA Calc must provide

Excel: `LoadLibrary` of the .xll, then `xlAutoOpen` on the main thread inside an XLL call context; Excel12 entry
points are resolved by name from the host exe [V xlcall.cpp]. For an **unmodified** xll DNA Calc must provide:

1. **`MdCallBack12` exported from the main exe image** (`int __stdcall(int xlfn, int count, LPXLOPER12*, LPXLOPER12)`),
   since every framework calls `GetModuleHandle(NULL)`. Rust: `#[no_mangle] extern "system"` plus `/EXPORT`. [V/I]
2. **An `XLCALL32.DLL` shim** beside the exe (app directory wins the search order): `XLCallVer` returning 12*256,
   `LPenHelper` (edit state; `xlModeReady` when not editing), `Excel4`/`Excel4v` (refuse with `xlretFailed` until legacy
   XLOPER support exists), `Excel12`/`Excel12v` forwarding. [V needs; layout I]
3. **Process identity**: version resource `FileMajorPart` >= 15 (Excel-DNA takes pre-15 paths otherwise,
   `Excel.cs:620-641`; cluster-safe needs >= 14, `LoaderHelpers.cs:41-67`); `xlfGetWorkspace(2)` = "16.0". Excel-DNA
   treats a process not named EXCEL or ET as an HPC cluster host unless `EXCELDNA_ASSUME_EXCEL_PROCESS=1`
   (`LoaderHelpers.cs:66-86`, `Host.Shared/utils.cpp:228-243`); that switch only exists since commit 51833b4a
   (2026-07-06), so older built add-ins see cluster mode in `dna-calc.exe` (effects found: cluster-safe flag, load
   errors suppressed; full effect [P]).
4. **Bitness**: x64 loads only x64 xlls. All three ship both builds; 32-bit DNA Calc is an owner decision (sec. 7).
5. **A "current XLL" context per thread**: `xlGetName`, `xlEventRegister`, `xlfRegister` and the rest carry no module
   argument, so the host tracks which module it called (xlAutoOpen, UDF, command, event, `Application.Run`) [I]. Calls
   outside any context must fail with Excel's exact codes: Excel-DNA probes with `xlGetName` (`Excel.cs:288-306`),
   xlOil with `xlStack` [P].
6. **Registration semantics**: resolve `GetProcAddress` **at `xlfRegister` time** and keep the pointer per register ID
   (xlOil rewrites the export between registrations); accept full path (Excel-DNA) or bare name of an already loaded
   module (xlOil); hidden application-scope name per function so `xlfEvaluate("Name")` and `xlUDF` work.
7. **No hostile process mitigations**: the CLR JIT, xlOil's `PAGE_EXECUTE_READ` thunks and export patching need
   dynamic code and writable images: no Arbitrary Code Guard, no Microsoft-signed-only image policy. COM and OLE
   initialised as STA on the main thread before `xlAutoOpen` [I].
8. **CLR hosting needs nothing from DNA Calc** beyond the above: the xll hosts its own runtime. One .NET (Core) runtime
   per process applies as in Excel; DNA Calc itself must not load a .NET runtime [I].

**COM pieces the frameworks use** [V]:
- **Application object**: Excel-DNA enumerates the main thread's `XLMAIN` windows, their `EXCEL7` children, and calls
  `AccessibleObjectFromWindow(OBJID_NATIVEOM, IID_IDispatch)`, then late-bound `.Application` (or protected-view
  `.Workbook.Application`) (`Excel.cs:404-489`, `701-706`); with no workbook window it creates one with `xlcNew`.
  xlOil: `xlGetHwnd` -> `XLDESK` -> `EXCEL7`, `IID Excel::Window`, `Window.Parent.Application`, retried every second.
  DNA Calc therefore needs the `XLMAIN > XLDESK > EXCEL7` window hierarchy answering `WM_GETOBJECT`/`OBJID_NATIVEOM`.
- **Ribbon and task panes** (Excel-DNA `ExcelComAddInHelper.cs:65-146`; xlOil `ComAddin.cpp`): write temporary
  HKCU `Software\Classes\<ProgID>`, `CLSID\{..}\InProcServer32` = xll path, and
  `Software\Microsoft\Office\Excel\Addins\<ProgID>` (LoadBehavior 0); set `Application.AutomationSecurity` low;
  `COMAddIns.Update()`, `.Item(progId)`, `.Connect = false/true`; Excel activates the class through the xll's
  `DllGetClassObject` (Excel-DNA) or `CoRegisterClassObject` (xlOil). Then `IDTExtensibility2`,
  `IRibbonExtensibility.GetCustomUI`, `IRibbonUI.Invalidate*`, `ICustomTaskPaneConsumer`/`ICTPFactory.CreateCTP`.
- **RTD**: Excel-DNA registers a fresh `RtdSrv_<guid>` ProgID per server and session in HKCU and calls
  `xlfRtd(progId, ...)` (`ExcelRtd.cs:128-148`); xlOil the same with `CoRegisterClassObject` plus volatile keys.
  DNA Calc must be an `IRtdServer` client: `CLSIDFromProgID`, in-proc activation, `ServerStart(IRTDUpdateEvent)`,
  `ConnectData`/`RefreshData`/`DisconnectData`/`Heartbeat`, on the main thread.
- **Macro context from outside**: Excel-DNA `QueueAsMacro` posts to a hidden window, then
  `Application.Run("SyncMacro_<guid>", 0.0)` and expects `0.0` back (`ExcelSynchronizationContext.cs:339-431`);
  xlOil `Application.Run("xloRunInXLLContext")`. `Application.Run` must run XLL commands with C API access.
- xlw: no COM.


## Appendix E. Volatility as Excel does it (measured; our design document, sections 4 to 6)

### E.4. Events, in execution order

1. **Registration default `!`**: true, applied when the call starts. A later request overwrites it: `BangHashClear()`
   (`Q!#`, requests false on every call) and `BangHuF(x)` are nonvolatile for every argument; `Bang()+VFalse()`
   is nonvolatile, `VFalse()+Bang()` volatile (`p3`, `p2h`). Not in the token; an untaken `IF(TRUE,1,Bang())` is
   nonvolatile. It also applies to `>EX!` async registrations.
2. **Registration default `#` with R/U parameter**: true, at call start, **only when a reference is actually passed
   in an R or U parameter** (`p4`). `Hu(C1)`, `Hu(C1:C2)`, `Hu(INDEX(..))`, `Hu(empty cell)`, `Hr(..)` are volatile;
   `Hu(1)` and `Hu(C1+0)` (a value) are not. Any R/U position counts (`Hub(C1,1)` yes, `Hub(1,C1)` no because
   position 2 is `B`; `Hbu(1,C1)` yes; `Hrr(1,C2)` yes). Q, P and B parameters never count; `Q#` with no
   parameter is nonvolatile; coercing the argument changes nothing. Same event order as 1: `HuF(C1)` (requests
   false) is nonvolatile for every argument, `HuT(1)` volatile, `HuTF(1)` not, `HuFT(1)` volatile.
3. **Requests**: `xlfVolatile(TRUE/FALSE)` and `Application.Volatile True/False`, any number per call; a call with
   a different argument per invocation behaves per invocation. `Application.Volatile` with no argument is true.
   Helper functions' requests count for the formula's cell.
4. **Name use** (section 5), **SUMIF shape**, **spill block**: events at execution that set true (name use: the
   name body's own flag, see 5).
No event clears a flag except a request of false. An untaken branch contributes no event.

**`xlfVolatile` is function 237 and Excel12 accepts it only in some contexts** (`p2x`, `p3`, `p4`, `x3`, `x5x`):
status 0 from a function registered with `#` called from a cell, and from any registration called outside a cell
(`Application.Run`, no effect on any cell); status 2 (xlretInvXlfn) from a cell call of an ordinary, `!` or `$`
registration, and from `>EX`/`>EX!`/`>EX$` entries; status 32 from the worker thread after the async return. So
`BangClear` (`Q!`, requests false) stays volatile: the request is refused, not obeyed. `$` with `#` was not
measured (DNA refuses the pair). DNA's `callbacks.rs` accepts it everywhere.

### E.5. Names (P5, `p5`, `p6`)

A defined name's body has its own token (`RAND()`, `IF(TRUE,1,RAND())`, `INDIRECT`, `LAMBDA(RAND())` yes; a body
calling a user function or `=1` no). A cell using a name gets no token from it. Executing a name use is an event:
it sets true when the body's flag is true. Hence `=NmRand` and `NmRand*0` are volatile; `IF(TRUE,1,NmRand)`,
`IF(FALSE,NmRand,1)`, `IF(TRUE,1,NmVba)` are not; a name whose body is `VFalse()` leaves the cell nonvolatile;
`NmVF+RAND()` is nonvolatile (token first, the name's request later); `VFalse()+NmRand` volatile,
`NmRand+VFalse()` not; `NmInd+VFalse()` not, `VFalse()+NmInd` volatile. **`LET(f,NmLam,1)` is volatile although `f`
is never called**: binding the name evaluates it. Redefining the names without touching the cells
(`NmRand` to `=1`, back to `=RAND()`) changed the cells' volatility at the next Calculate, and the dependents
recalculated once on the redefinition: names are late bound, the fact belongs to the body, joined at each
executed use.

### E.6. Persistence, commands, order, lifting, files

- **Calculate kinds (X1)**: a nonvolatile cell is not recalculated by Calculate or `Worksheet.Calculate`;
  `Range.Calculate` recalculates the target and leaves it dirty (target and dependents run again at the next
  Calculate); Full and Full-rebuild recalculate everything, requests are re-evaluated then (a function
  that now requests true becomes volatile only after one of these; one that now requests false ends the next
  Calculate). In Automatic mode volatile cells recalculate on an unrelated edit.
- **Entry and first calculation (X4, cal1)**: every formula is called once when entered and once more at the
  first Calculate; the events of the first Calculate decide the state (a request at that evaluation is
  honoured).
- **Abandoned attempt (X4, 3 runs)**: a reader of a three-level producer chain is evaluated twice in the first
  Calculate; a request made only in the abandoned attempt is lost, one made in the completed attempt counts.
  Only the completed evaluation's events are kept; never the abandoned one's.
- **Lifted calls (X2)**: with a Double-parameter user function lifted over a multi-cell range or array
  (`SUM(VElemD(C1:C3))`, `{1,2,3}`, 1x3 and 3x1, requests true and false mixed, all true) **no request
  is honoured: the cell is nonvolatile**; over a 1x1 reference the request is honoured; inside
  MAP/BYROW lambdas each call's request counts and elements run in ascending order, last wins (`MAP(C1:C3,..)`
  with requests F,T,T volatile, `MAP(E1:E3,..)` with T,T,F not). Repeated calls in one formula: last wins.
- **Outside a worksheet call (X5)**: `Application.Volatile` from a Sub, `Application.Run` or `Evaluate`
  raises no error and changes no cell; `Evaluate("VTrue()")` runs the function with no effect on cells.
- **Threads and async (X3)**: see section 4; `$` registered functions and async entries cannot clear or set
  by request; `>EX!` volatile; `AsyncLate` status 32.
- **File round trip (X6)**: saved, `ca` is written for requested, `!`, `#`+reference, SUMIF-shape and token
  cells alike (no token for the run-time ones). On open in Manual mode nothing is called; the first Calculate
  calls exactly the cells with `ca` (the reopened user-function cells included), none of the others. Automatic mode
  on open was not measured.
- **Dependents (X7)**: dependents of a volatile cell are recalculated every Calculate and carry `ca` in the
  file, to any depth, through ranges (`SUM(A1:A2)`); the bits clear when the source stops being volatile.
  Exception: the bare-reference user-function dependent of section 1.


## Appendix F. What we measured in Excel and what the host reproduced (condensed from our design document, sections 9 to 27)

All on Excel 16.0 x64 (builds 20527 and, for later cases, 20150), Windows, en-ZA, in owned hidden
Excel sessions, traced (debugger) and untraced controls; the untraced run checks worksheet outcomes.
"Host" is DNA Calc/OneCalc. QPC times include debugger pauses and are never compared.

### F.1 X0, boundary of Excel-DNA 1.9.0 and our own fixture (2,524 records, 117 C API callbacks)

- Registered type strings observed: `X0.Hello(string)` `QD%`; `ThreadSafe(double)` `QE$`;
  `Reference(object, AllowReference, IsMacroType)` `QU#`; macro-type `Caller()` `Q#`; `Big()` (mixed
  array) `Q`; `Async(double, handle)` `>EX`; Excel-DNA's `SyncMacro_<guid>` `>B` macro type 2.
- Excel-DNA exports 10,000 numbered `fN` stubs and no `xlAutoRegister12`; the actual entries are
  discovered from `xlfRegister` arguments. `$` calls ran on calculation worker threads (64 invocations
  over 16 threads); reference/caller and ordinary calls ran on the load thread.
- `XLCallVer` returns 3072. `xlfCaller` for `X0.Caller` in B3 gives zero-based rectangle `[2,2,1,1]` as an
  SRef; in a queued-macro or command context `xlfCaller` returns status 0 with an `xltypeErr` `#REF!` (23).
- On a ThreadPool thread outside any calculation or command, `xlGetName`, `xlStack`, `xlfCaller` each
  return **32** (`xlretFailed`), and the `xlFree` that Excel-DNA then attempts also returns 32.
- `QueueAsMacro` runs through `Application.Run("SyncMacro_<guid>")` on the main STA with `LPenHelper`
  (`xlGetFmlaInfo`) consulted first; the body has C API access and `xlGetName` names its own XLL even after a
  second XLL has loaded. `Application.Run` of zero-argument commands returns Boolean TRUE to the controller;
  of the `>B` SyncMacro returns `VT_R8` 0. Empty-type commands read the native Boolean return (TRUE for these
  Framework wrappers, FALSE for the net8 wrappers).
- A 1x4096 mixed array result: type 64 (`xltypeMulti`), no `xlbitDLLFree`. A small native fixture that returns
  an allocated counted string with `xlbitDLLFree`: Excel calls `xlAutoFree12` once, with the exact pointer, on the same
  native thread (single-thread case only).
- COM: the Application is found by `AccessibleObjectFromWindow(OBJID_NATIVEOM, IID_IDispatch)` on EXCEL7, then
  `Window.Application` through IDispatch (DISPID 148); Version 392, Hwnd 1950, Run 259. **AppEvents argument
  order**: `FindConnectionPoint` asks for `00024413-...`; SheetChange arrives with DISPID 1564 (0x61C),
  `wFlags=3`, two `rgvarg` entries with named DISPIDs [0,1]; `rgvarg[0]` is the worksheet, `rgvarg[1]` the range.
  Also SheetCalculate (0x61B) and AfterCalculate (0xA34).
- Saved xlsx: formulas are written with the `_xll.` prefix, nested too (`SUM(_xll.X0.Big())`).
- `ExecuteExcel4Macro("UNREGISTER(<path>)")` returns TRUE and calls native `xlAutoClose`; whether managed
  `AutoClose`, `Unadvise`, CLR unload or `FreeLibrary` follow is not proven.

### F.2 X1 to X3, host against Excel for unmodified binaries

- Own fixture: 289 formulas, every one agrees; the same ordered **599 callback numbers and return codes** in Excel
  and host (`xlfCall`/`xlfRegisterId` rules: both need a `#` registration or `xlAutoOpen`, a plain worksheet function
  gets return code 2 and `#VALUE!` in the result; zero CALL arguments or wrong REGISTER.ID count return code 4;
  other failures `#VALUE!` or `#NUM!` with code 0; integers round half up, unsigned types take the absolute value
  first; strings use the ANSI code page with `?` for unmappable; REGISTER.ID returns the first live registration
  of the module's export, module name case-insensitive with or without `.dll`, procedure name case-sensitive).
- Excel-DNA 1.9.0 under net48 and net8.0-windows (six UDF cases each plus the native ownership control, 7/7), Native AOT
  1.10.0-preview5 on net10 (1/1), the full 10,000-export pool registers and `f0`/`f9999` run. A loader/PIA fixture
  1.10.0-preview6 confirms process identity checks, Framework AppDomain isolation and ExcelDna.Interop 15.0.1 calls through
  Excel's `_Application` IID and vtable (Version, Hwnd, Run work). Measured: Window is a 7-slot dispatch interface;
  Application's dual interface has 474 slots; Worksheet 158; Workbook 289; Workbooks 26.
- xlw 23.10.12-rc (unchanged sample plus a scalar/matrix/reference types fixture), xlOil 0.22.1 Utils and SQL
  plugins, three xlOil_Python functions (Python 3.12): 27 agreeing formula cases, 28 matching native invocations,
  17 exact-pointer same-thread frees through the owning core/plugin modules. Host supplies the real executable
  instance handle, reads packed XLOPER12 operands unaligned, takes regional settings from Windows NLS with English
  function syntax. Other locales unverified.
- Remaining differences recorded as failures: raw `FILTER` element types from a lifecycle probe, the
  `xlEventRegister` result payload, Python registration order, teardown differences (counted separately). Original
  xlOil 0.22.1 **crashes Excel itself** after UNREGISTER then RegisterXLL (access violation: it destroys its COM
  connector on close and dereferences it on reopen); Excel-DNA Native AOT aborts on a second runtime initialisation.
  Both refuse reload in the host with a typed reason.

### F.3 Callback permissions and uncalculated reads (the P03 and X5 measurements)

- **Uncalculated reads, build 20527, safe mode, 110 primary workbooks and 60 controls:** ordinary and `$` registrations
  that `xlCoerce` a not-yet-calculated cell receive status **64 (`xlretUncalced`)**, unwind and are **entered again** after
  the precedent calculates (all ten reversed-area trials; `$` also on first calculation); `#` registrations coerce
  successfully (cached value); `xlfVolatile(FALSE)` in a `#` function suppresses clean F9 calls. After 64 only `xlFree`
  and native memory logging are safe in the tracer. VBA Range arguments can be Empty and then the real value across
  two entries; Double can be 0 then 11.
- **Callback matrix**: 13 requests each under normal, `#` and `$` registrations; Excel's flags, arguments and thread
  placement checked; the host matches 39 of 39 after the `EVALUATE` fix (`EVALUATE("1+1")` returns 2 in a normal or `#`
  registration; `$` denied with 128; `MESSAGE` from `$` 128). General `EVALUATE` with references/names/registered calls
  and volatile contexts are typed refusals.
- Cell entry runs the formula on the owner thread even for `$` functions; a later Manual Calculate uses the workers;
  an extra entry call at formula entry is part of the strict comparison. Pool test: 15 ordered native calls and two owned
  frees match, with a two-worker overlap witness (a fixture barrier proving two calls in flight).
- `xlAsyncReturn`: result operand after accepted completion is 32 zero bytes in Excel (even when the operand started as a
  number); the host now clears the operand. BigData `cbData` is a signed 32-bit field with four padding bytes (fixed in
  our ABI after being read as 64 bits). A net8 handle's incoming opaque `cbData` differs from Framework's; Excel-DNA reads only
  `hData` on that path.

### F.4 RTD

- Controlled numeric RTD server and original Excel-DNA/xlOil servers: values, dependents and admitted-input certificates
  agree; ordered `IRtdServer` method traffic: numeric controlled server and typed `=RTD()` 6 of 6 positions; xlOil
  signalling server 8 of 8 (with a 22-second idle extension: disconnect of a completed generator before the idle interval,
  a persistent numeric topic gets exactly one Heartbeat); **Excel-DNA original: 10 client positions matched, 16 failed;
  native invocations 0 matched, 26 failed; delayed 20 matched, native 0 of 22** (value, dependents and certificates agree,
  protocol traffic does not). Differences: first `RefreshData` signal 1 in Excel vs -2 in the host; replacement order
  (Excel connects the next topic before disconnecting the old) now matches for the tested scalar replacement.
- Excel exposes an empty COM RTD signal as numeric 0 to `xlfRtd`; a VARIANT SAFEARRAY of four numbers as `#N/A` without
  a spill; a topic is identified by (ProgID case-insensitive, server, topic strings after text coercion: the number 1
  and "1" are one topic; two strings and one string are two topics); a typed `=RTD()` and an XLL's `xlfRtd` with equal
  arguments are one topic. The host first returns a temporary `#N/A` before admitting the first input where Excel's native
  UDF returns the first value at once.

### F.5 Async, pool, spill, COM add-in loading

- Native async (`>EX`) with overwrite/undo/late-return rules goes through the engine's token (P07); a late return after cancel keeps the module leased.
  Concurrent independent scalar pool calls (up to pool size) and concurrent native spill producers with ordered placement run through the engine; broader
  workbook scheduling and cancellation cases are open.
- Excel-DNA's automatic ribbon/COM-add-in helper cannot run in our measurement sessions (it can change `Application.AutomationSecurity`, which we never alter),
  so `DllGetClassObject` activation and ribbon rendering in Excel are **unmeasured**. In the host, COM discovery reads private HKCU classes whose in-proc server is a
  loaded XLL, activates through `IDTExtensibility2`, and retains `GetCustomUI` XML; no rendering or button actions. Bounded ribbon startup and helper properties
  compared for Framework and net6 (seven helper property calls, exact XML).

## Appendix G. What is done, what is refused, and what tests and add-ins are actually exercised

### G.1 Epics and beads (read-only snapshot from the tracker)

- **dc-snz** (P2 epic, "Add-ins: UDFs hosted by DNA OneCalc, then VBA in DNA Calc"): 15 closed, 6 in progress, 8 open.
  Closed: X0 traces (dc-snz.16), X1 native ABI/dispatcher/own fixture (.17), X2 unmodified Excel-DNA runtimes + macro queue + local COM (.18),
  X3 unmodified xlw and xlOil boundary comparisons (.20), X4 xlOil Python, Excel IIDs, delivered AppEvents (.22), owned Excel sessions (.24, .25),
  CALL/REGISTER.ID (dc-gh22), VBA UDF value calls (.3), and several VBA/XLSB naming bugs.
  In progress: `.2` ".xll UDFs through the Excel C API" (accept: Excel-DNA sample and C sample give Excel's results), `.23` X5 pool, native
  async and RTD (stays open until the strict RTD and raw `xlAsyncReturn` differences close), `.21` xlOil safe teardown/reinit (needs upstream
  change or process isolation), `.5` VBA Range objects, `.6` VBA through the engine (open).
  Open: `.19` Excel-DNA Native AOT safe runtime removal and reinitialisation, `.4` rename Ox* crates, `.1` OneCalc as native UDF host, VBA
  naming/binding items (`.9`, `.10`, `.14`, `.15`).
- **dc-0f1k** (P1 epic, OneCalc on the Windows desktop): factored desktop done; milestone 4 ".xll add-ins from the app" (`.10`) in progress and
  accepted at 350 of 353 cases; milestone 5 VBA add-ins in progress (26 of 26 project cases match after dc-0f1k.12 answered ActiveSheet/Selection); milestone 6 RTD
  in the app; milestone 7 packaged folder in progress; `.12` VBA object members UDFs ask for (ActiveSheet, Selection, Range.Value)
  open; session file as constrained .xlsx open.
- Unscheduled and labelled `xll/com` themes: **commands, ribbons, menus and task panes are not offered** in OneCalc ("their registration or
  load errors do not stop worksheet functions loading"); Excel4/XLOPER legacy and 32-bit are "X7 later"; PyXLL later; cluster connector later.

### G.2 Acceptance through the OneCalc page, compared with Excel's answer for the same formula (2026-11-08)

Pass = matched Excel; fail = mismatched + refused + missing; rate over all cases of the set.

| Set (file) | Cases | Matched | Fail |
|---|---|---|---|
| own fixture X1 (289 formulas) | 289 | 289 | 0 |
| Excel-DNA 1.9.0, net48 (+ native ownership add-in) | 7 | 7 | 0 |
| Excel-DNA 1.9.0, net8.0-windows | 7 | 7 | 0 |
| Excel-DNA Native AOT 1.10.0-preview5 (net10) | 1 | 1 | 0 |
| Excel-DNA, 10,000 thread-safe registrations | 2 | 2 | 0 |
| xlw 23.10.12-rc sample + types sample | 5 + 5 | 10 | 0 |
| xlOil 0.22.1 Utils and SQL | 12 | 12 | 0 |
| xlOil 0.22.1 Python 3.12 | 5 | 5 | 0 |
| calcvba-udf generated add-in (15 exports, uses COM `Application.Range`, `Application.Caller`) | 20 | 17 | 3 |
| all | 353 | 350 | 3 |

The three failures: `Native_ReferenceKind(A1)`, `Native_OptionalReference(A1)`, `Native_CallerRow()` return `#VALUE!` where Excel gives
`"Range:1"`, `"Range"` and `3`: the host's COM object model has no `Application.Range`, `Union` or `Caller`. Owner ruling: OneCalc carries no
full Excel object model; what these calls should do instead of a silent `#VALUE!` is dc-snz.5's open question.

Also shown through the page: signature help, function help and completion list from `xlfRegister` metadata (names, category, help text);
unload through the panel for each set; reopen of a session that lists the add-in twice (wrong hash refused, right hash loaded) except the two
runtimes that cannot restart. Not shown: commands, ribbons, RTD through the page for managed servers, xlOil event cases that toggle
`Application.EnableEvents`, the x5 permission and RTD case files.

### G.3 Case files and add-ins in the repository (`crates/calc-xll-reference/fixture`)

Case files (JSON, formula -> Excel's answer): `x1` (289 formulas on the own fixture), `x2` (Excel-DNA hello, identity, pool), `x3` (xlOil, xlOil Python, xlw sample, xlw
types), `x4` (xlOil Python events), `x5` (callback permissions, RTD numeric/native/typed, workbook pool), `x8` (Excel-DNA runtime, AOT, types, RTD original and delayed,
native entries, registration RTD, ribbon, array RTD), `x12` (native async result), VBA (`DnaExamples.xlam/.xlsm`, 26 cases). Fixtures built from source: C# Excel-DNA projects
(`X0`, `X2Hello`, `X2Identity`, `X2Pool`, `X2Aot`), the Rust `calc-xll-fixture` (about 70 exports covering every modelled type code, DLLFree, volatile, async,
thread-safe overlap barriers, `Excel12` pass-throughs, `xlfCall`/`xlfRegisterId` probes, re-registration through one export name), an xlw-generated type library.

Unit and integration tests: ABI layout, BigData signed count, type-string parse/reject list, packed operand copying, DLLFree timing, callback coercion, async token rules (stale,
duplicate, revoked, cancelled call keeps its lease), documentation-to-descriptor mapping, early-execution grant, reload-UI cleanup, RTD client lifecycle, COM event-source and
Office-control lifecycle and ribbon re-entry (C++ called from Rust), AddinHost restart; portable engine tests with test doubles for the reevaluation reason, spill ordering
and preferred-producer sharing.

## Appendix H. The test instruments we can run for any experiment you propose

1. **Boundary tracer** (`native/trace.cpp`, `run.ps1`): a native x64 debugger that launches a new Excel process, registers an XLL with
   `Application.RegisterXLL` (or opens a workbook), and records every entry into the add-in (entry points discovered from `xlfRegister`),
   every `MdCallBack12`/Excel12 call with function number, operand bytes (32 per XLOPER12, packed), return code, result operand bytes,
   thread id, module, return address; COM calls (IDispatch::Invoke with DISPIDs/flags, QueryInterface IIDs, class-factory requests, RTD
   method ABIs with arguments); `xlAutoFree12` calls with pointer. Temporary breakpoints exist only in that Excel process.
2. **Fixture XLL** (`calc-xll-fixture`, Rust): registers any type string and any function through `xlfRegister`; `MyXlf*` exports call
   `Excel12` with arbitrary function numbers and operands taken from the formula and return the status and the result operand; a barrier
   fixture shows concurrent entry. Cases are JSON: formula -> expected value from Excel.
3. **Same-binary comparison**: the verifier runs the identical add-in binary in Excel (traced and control) and in the host (traced and
   control), normalises identifiers (register ids, pointers, sheet handles, paths), and compares ordered native entries, callbacks and
   return codes, values, dependents, admitted RTD certificates. Unequal ordered traffic stays a failure even when values match.
4. **Excel session service**: an owned hidden Excel with settings reset, add-in inventory checked, optional repeat in a fresh process;
   scripts read values, formulas, saved `ca` bits, `Formula`/`Formula2`, VBA project parts.
5. **The page-driven acceptance**: `xtask app xll-accept onecalc` drives the real desktop executable through WebDriver on WebView2:
   load, enter formula, read result, help, unload, reopen session.

## Appendix I. COM surface that exists

From `crates/calc-com-host` (README plus the dispatch name tables in `projection.cpp` and `projection-office.h`; the member list is
extracted from string tables and may miss some):

- **Interfaces with Excel's IIDs and measured layouts**: `_Application` (`000208D5-0000-0000-C000-000000000046`, 474-slot dual vtable; supported
  early-bound slots: Application 7, Parent 9, Run 42, Version 281 including LCID, Hwnd 351), `Window` (`00020893-...`, 7-slot dispatch-only),
  `_Worksheet` (158 slots), Workbook (289), Workbooks (26), a dispatch-only `Range` for single-cell event targets. Members not implemented fail with a fixed
  failing HRESULT and are counted as refusals.
- **Members answered (IDispatch name lookup)**: Application: `Version`, `Hwnd`, `Run`, `Workbooks`, `ActiveWorkbook`, `ActiveSheet`, `Path`, `EnableEvents`,
  `AutomationSecurity`, `COMAddIns`, `CommandBars`, `Parent`, `Application`, `Creator`; Window: `Application`, `Parent` (the workbook); Workbook:
  `Application`, `Name`, `FullName`, `Path`, `ActiveSheet`; Worksheet: `Name`; Range (event targets): `Address` (local and qualified, DISPID 0xec),
  `Row`, `Column`, `Rows`, `Columns`, `Count`, `Worksheet`, `Parent`; COMAddIns/COMAddIn: `Update`, `Item`, `Connect`, `ProgId`, `Description`, `Object`;
  CommandBars/Controls: `Count`, `Item`, `Add`, `Delete`, `Caption`, `OnAction` (bounded model for legacy menu use).
- **Events**: AppEvents connection point (`00024413-...`) with SheetChange (after committed edits), SheetCalculate and AfterCalculate, named arguments in
  the measured order, EnableEvents suppression, re-entrant Unadvise and cookie handling tested.
- **Not provided**: `Range.Value`, `Value2`, `Formula`, `Formula2`, `Cells`, `Offset`, `Resize`, `SpecialCells`, `Clear`, `Application.Range`, `Caller`, `Evaluate`,
  `Calculate*`, `Calculation`, `ScreenUpdating`, `StatusBar`, `DisplayAlerts`, `Selection`, `ActiveCell`, `WorksheetFunction`, `Intersect`, `Union`, `Quit`,
  `Worksheets` collection enumeration, `Names`, `Shapes`, `VBProject`, task panes, ribbon rendering. This is one local "Book1"/session workbook shell, not a general
  object model; DNA Calc's workbook object model over `calc-host` is future work (owner phase 2/3).
- **Activation**: nothing is registered; `AccessibleObjectFromWindow` on our own EXCEL7 window is the only door. `CreateObject("Excel.Application")`, `GetObject`,
  the ROT and moniker routes land in a real Excel if one is installed (the `excel-service` trap: bind via the launched process's EXCEL7 window and assert the PID).

## Appendix J. Candidate gaps and divergences the assembler found while reading the code (readings, not measurements; each may be wrong)

R1. **Uncalculated reads.** No constant 64 in `calc-xll-abi`/`calc-xll-host`. `XllFunction` does not override `reevaluation_policy()`; only the VBA adapter and
test doubles do. An `xlCoerce` of a not-yet-calculated cell from inside the native call goes through `UserCallContext::values_of`, which records a missing-input
fault and returns an `Err`, which the callback maps to status 32 (`err()`); the fault is returned by the evaluator after the call and the evaluation waits and
retries. So the add-in sees 32 instead of 64 and the entry-count/side-effect sequence may differ from Excel's "unwind and re-enter" (the engine supports
`ReevaluationReason::XllUncalcedUnwind` with prefix effects kept).
R2. `xlEventRegister` stores addresses (`State.events`), nothing ever calls them; `CalculationEnded`/`CalculationCanceled` are never raised.
R3. Callback result operands never carry `xlbitXLFree`; Excel's documented contract is that it sets that bit when the add-in must call `xlFree`. Frameworks that test the
bit (rather than freeing every result as xlOil does) would leak or skip frees; frameworks that always free work. Entries of `State.allocations` are keyed by raw pointer.
R4. Volatility default: `XllSignature.volatile |= macro_equivalent && has R/U parameter` is static; the measured rule is that `#` volatility applies only
when a reference is actually passed in an R/U position, and as an ordered event (Appendix E 4.2); `start_inline` calls `set_volatile(true)` when `signature.volatile`.
R5. `Excel4`/`Excel4v` return 32 for everything; `XLCALL32.DLL` `Excel12` forwards. xlw 5 and old commercial XLLs cannot run. Legacy types `C D F G K O P R`
and in-place returns are accepted at registration and refused at call time with `#VALUE!`.
R6. No process isolation or structured exception handling; native AV kills the process; wait timeouts (10 s bare wait, 30 s in OneCalc) cancel the calculation but
cannot stop native code.
R7. Pool: at most four threads; each call builds a private `State` and takes fresh module leases; callbacks from `$` functions are served by the owner (serialised with
evaluator reads). `xlUDF`/`xlfEvaluate`/`xlfRtd` from `$` give 128. Thread-local state in the add-in (xlw pool reset, .NET `[ThreadStatic]`, CPython) sees a thread
pool that differs from Excel's.
R8. `xlfCaller` returns a 1x1 SRef always; Excel's other forms (multi-cell array formula callers, names, conditional-format and data-validation callers, charts, `xlfCaller` in `#N/A` contexts) are not modelled.
R9. `xlCoerce`: text-to-number parsing uses a fixed en-US environment; error/missing/nil conversion targets partly refused; the 2-argument form with `xltypeRef` returns the reference untouched;
no check of the target-type bits Excel accepts as a mask.
R10. Registered-name semantics: one hidden name per function; no scoping by workbook; `xlfUnregister` use counts do not remove the name; `xlfSetName` only clears. `_xll.` rebinding when an add-in
loads after a workbook is not modelled.
R11. Commands: `Application.Run` of a registered `>B`/empty-type command works (STA, command frame), async `Application.Run` is refused; all `xlc*` operations are Absent, so Excel-DNA
`QueueAsMacro` bodies that call `xlc*` or menu functions, `ExcelAsyncUtil`'s macro-queued paths, and xlOil's `xloRunInXLLContext`-driven workbook functions work only to the extent measured.
R12. `xlAddInManagerInfo12`, `xlAutoAdd`, `xlAutoRegister12` are never called; Excel calls the first when the Add-in Manager lists the add-in.
R13. Add-in-specific accommodations: xlOil 0.22.1 reload refusal keyed by the SHA-256 of two binaries; Excel-DNA Native AOT repeat initialisation refused by process-lifetime payload/runtime identity; `EXCELDNA_ASSUME_EXCEL_PROCESS=1`
(only since an Excel-DNA commit of 2026-07; older builds treat a process not named EXCEL as a cluster host).
R14. `GetWorkspace` items other than 2 and 37, `xlGetBinaryName`/`xlDefineBinaryName`, `xlSet`, `xlGetInst` return unsupported-callback codes.
R15. COM object model is one session workbook; `Application.Range`/`Caller`/`Evaluate`/`Selection` absent (the 3 acceptance failures).
R16. Entire XLL parity is measured per add-in build; Excel-DNA's IntelliSense (UI Automation on the formula bar) is not supported and declines cleanly.


## Appendix K. Code excerpts at the pinned commit

### K.1 Type-string validation (`calc-xll-abi`; the constants are listed in Appendix B)

`crates/calc-xll-abi/src/signature.rs:137-179` (validation after parsing the type string)
```rust
        let result = codes.remove(0);
        if codes.len() > 255 {
            return Err("more than 255 function arguments".into());
        }
        if codes
            .iter()
            .any(|c| matches!(c, XllType::Void | XllType::InPlace(_)))
        {
            return Err("void/in-place marker used as an argument type".into());
        }
        if matches!(
            result,
            XllType::AsyncHandle | XllType::Array | XllType::Array12
        ) {
            return Err("argument-only type used as a return type".into());
        }
        let handles = codes.iter().filter(|c| **c == XllType::AsyncHandle).count();
        let asynchronous = handles != 0;
        if asynchronous && (handles != 1 || result != XllType::Void || cluster_safe) {
            return Err("async requires one X argument, > return and no & flag".into());
        }
        if macro_equivalent && thread_safe {
            return Err("# and $ are mutually exclusive".into());
        }
        if let XllType::InPlace(n) = result {
            if !codes
                .get(usize::from(n) - 1)
                .is_some_and(|c| c.is_pointer() && *c != XllType::AsyncHandle)
            {
                return Err("in-place return does not name a pointer argument".into());
            }
        }
        volatile |= macro_equivalent && codes.iter().any(|c| c.allows_reference());
        Ok(Self {
            result,
            arguments: codes,
            volatile,
            macro_equivalent,
            thread_safe,
            cluster_safe,
            asynchronous,
        })
    }
```

### K.2 Registration and the user-function seam

`crates/calc-xll-host/src/callbacks.rs:979-1078` (xlfRegister handling)
```rust
fn register(frame: &mut Frame<'_>, values: &[CalcValue]) -> Result<CalcValue, (i32, String)> {
    if values.len() < 4 {
        return Err((INVALID_COUNT, "xlfRegister requires four arguments".into()));
    }
    if frame.signature.is_some() {
        return Err((
            INVALID_FUNCTION,
            "dc-snz.2: registration from a worksheet function".into(),
        ));
    }
    let module_name = text(&values[0])?;
    let module = Module::loaded(&module_name).map_err(err)?;
    let procedure = if let Some(n) = values[1].as_number() {
        format!("#{}", n as u16)
    } else {
        text(&values[1])?
    };
    // Resolve now, on every registration, including re-registration of an export xlOil rewrites.
    let address = if let Some(n) = values[1].as_number() {
        if !(1. ..=65535.).contains(&n) || n.fract() != 0. {
            return Ok(CalcValue::error(WorksheetErrorCode::Value));
        }
        module.ordinal(n as u16)
    } else {
        module.address(&procedure)
    }
    .map_err(err)?;
    let type_text = text(&values[2])?;
    let name = text(&values[3])?;
    let macro_type = values.get(5).and_then(CalcValue::as_number).unwrap_or(1.) as i32;
    // Excel-DNA registers its zero-argument commands with an empty type string.
    // This form is only valid for commands. Excel reads its native Boolean return,
    // including a zero value from the .NET 8 wrappers; it is not unconditional success.
    let signature = XllSignature::parse(if type_text.is_empty() && macro_type == 2 {
        "A"
    } else {
        &type_text
    })
    .map_err(err)?;
    if !matches!(macro_type, 0..=2) || (macro_type == 2 && signature.thread_safe) {
        return Ok(CalcValue::error(WorksheetErrorCode::Value));
    }
    let mut state = frame.state.borrow_mut();
    let key = calc_value::fold_workbook_name(&name);
    let existing = state
        .registrations
        .values()
        .find(|r| {
            r.module.handle == module.handle
                && r.info.address == address
                && r.info.type_text == type_text
                && r.info.name == name
                && r.info.macro_type == macro_type
                && r.origin.handle == frame.module.handle
        })
        .cloned();
    let id = if let Some(existing) = existing {
        existing.uses.set(
            existing
                .uses
                .get()
                .checked_add(1)
                .ok_or_else(|| err("XLL registration use count overflow"))?,
        );
        existing.info.id
    } else {
        let id = state.next_id;
        state.next_id += 1;
        let info = RegistrationInfo {
            id,
            name: name.clone(),
            procedure,
            type_text,
            signature,
            address,
            module: module.path.clone(),
            origin: frame.module.path.clone(),
            use_count: 1,
            macro_type,
            docs: registration_docs(values),
        };
        let cif = crate::invoke::call_interface(&info.signature);
        state.registrations.insert(
            id,
            Rc::new(Registration {
                info,
                module,
                uses: Cell::new(1),
                origin: frame.module.clone(),
                cif,
            }),
        );
        id
    };
    state.names.insert(key, id);
    Ok(CalcValue::number(id as f64))
}

#[cfg(test)]
mod tests {
```

`crates/calc-func/src/udf.rs:185-232` (the UserFunction trait a registered function implements)
```rust
pub enum UserReevaluationReason {
    XllUncalcedUnwind,
    ProvisionalArguments,
}

/// A user function. The default host entry reads references eagerly; adapters can instead
/// use live reference/caller services for one invocation. An adapter
/// returns worksheet errors as values and execution failures as diagnostics (mapped to #VALUE!).
/// No Send/Sync promise is made: a desktop add-in may belong to its owning thread.
pub trait UserFunction: fmt::Debug {
    /// Owner-granted permission to run an argument-ready call before cached-read
    /// observers in the measured Full/Rebuild profile. This is independent of
    /// thread safety, memoization and permission to replay an invocation.
    fn permits_early_execution(&self) -> bool {
        false
    }
    /// Maximum independent scalar-cell activations allowed together. This is
    /// fixed when registering the callable; adapters opt in explicitly.
    fn max_parallel_calls(&self) -> usize {
        1
    }
    fn reevaluation_policy(&self) -> Option<UserReevaluationReason> {
        None
    }
    /// Adapters sharing project state return the same lane while their leases are alive.
    fn state_lane(&self) -> Option<u64> {
        None
    }
    fn permits_reentry(&self) -> bool {
        false
    }
    fn cached_reads(&self) -> bool {
        false
    }
    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String>;

    /// Prepare a cooperative activation when this exact signature/body supports it. None is
    /// a declared synchronous interval, chosen before entry; errors never fall back or replay.
    fn begin_call(
        &self,
        _arguments: &[CalcValue],
        _frame: UserInvocationFrame,
        _context: &mut dyn UserFunctionContext,
    ) -> Result<Option<Box<dyn UserFunctionInvocation>>, String> {
        Ok(None)
    }

    /// Reference-aware host entry. Eager adapters keep the original value-call behavior.
```

`crates/calc-func/src/udf.rs:259-373` (signatures only; doc comments and default bodies abbreviated by the assembler). `UserFunctionContext` is what a function
may ask of the evaluator during one call: `caller() -> Option<CellKey>`, `user_view()` (active sheet, selection), `sheet_name`, `reference_qualifier`, `callable_identity`,
`rtd_value`, `read_topic(topic, declared)` (join or start a topic through the owner's connector, return the admitted value or the provisional one), `request_reevaluation(reason)`
(authorised by the registration), `call_user_function(name, args)` (nested, invocation-aware, never queued behind the caller's activation), `take_async_result(request)`,
`retain_reference`/`resolve_reference`, `invocation() -> Option<&UserInvocationFrame>` (id, parent, read-view lease, `formula_entry`), `set_volatile(bool)` (last executed request wins),
`refuse`/`refuse_by(reason, owner)` (a capability refusal that survives error handlers and `IFERROR`), `values_of(&ReferenceLike)`, `argument_values_of(&ReferenceLike)` (scalar-argument
readiness, a dependency), `worksheet_function(_for)(name, args, owner)` (the catalogue through the ordinary call route), `evaluate_constant_expression(expr, owner)`.
`UserFunctionStep` is `Suspended | Awaiting { request } | Complete(CalcValue)`; `UserFunctionInvocation::resume(context, quantum)` is the cooperative activation ("resume never re-enters
the procedure or repeats a completed effect").


### K.3 Calling a registered function: the result

`crates/calc-xll-host/src/invoke.rs:329-367` (result copy, xlbitDLLFree and xlbitXLFree)
```rust
    unsafe fn convert_return(
        &self,
        kind: XllType,
        slots: &[u64],
        result: Returned,
        sheet: Option<calc_value::SheetId>,
    ) -> Result<CalcValue, String> {
        let r = &self.registration;
        let ptr = result.word as *mut XlOper12;
        let value = match kind {
            XllType::Oper12 | XllType::Oper12Reference => {
                // Copy before the next entry into this module, including xlAutoFree12.
                let flags = if ptr.is_null() {
                    0
                } else {
                    unsafe { ptr.read_unaligned().xltype }
                };
                let copy = unsafe { from_oper(ptr, &self.state.borrow().sheets, sheet) };
                if flags & DLL_FREE != 0 {
                    match r.module.address("xlAutoFree12") {
                        Ok(address) => {
                            let free: unsafe extern "system" fn(*mut XlOper12) =
                                unsafe { std::mem::transmute(address) };
                            unsafe {
                                free(ptr);
                            }
                        }
                        Err(why) => {
                            return Err(format!(
                                "dc-snz.2: DLLFree result has no xlAutoFree12: {why}"
                            ));
                        }
                    }
                } else if flags & XL_FREE != 0 {
                    callbacks::free(&self.state, &unsafe { ptr.read_unaligned() });
                }
                copy?
            }
            XllType::Double => CalcValue::number(result.number),
```

### K.4 The callback dispatcher (`MdCallBack12`)

`crates/calc-xll-host/src/callbacks.rs:186-251` (dispatch: frame lookup, permission denials measured on Excel 20527, special cases, result publication)
```rust
        FRAME.with(|active| {
            let ptr = active.get();
            if ptr.is_null() {
                return FAILED;
            }
            let frame = &mut *ptr;
            // These are modeled permission denials measured on Excel20527.
            // They leave the result operand undefined and can be handled by
            // the add-in; they are not absent-capability refusals.
            if let Some(signature) = &frame.signature {
                if matches!(function, 237 | 186) && !signature.macro_equivalent {
                    return INVALID_FUNCTION;
                }
                if signature.thread_safe && matches!(function, XLF_EVALUATE | 32890) {
                    return NOT_THREAD_SAFE;
                }
                if function == 32890 && !signature.macro_equivalent {
                    return INVALID_FUNCTION;
                }
            }
            if function == 16403 {
                if count != 0 {
                    return INVALID_COUNT;
                }
                if !result.is_null() {
                    result.write_unaligned(XlOper12 {
                        value: XlValue12 {
                            big_data: XlBigData {
                                data: crate::native::GetModuleHandleW(std::ptr::null()).cast(),
                                byte_count: 0,
                            },
                        },
                        xltype: XL_BIG_DATA,
                    });
                }
                return SUCCESS;
            }
            if matches!(function, 16388 | 16389) {
                return sheet_callback(frame, function, args, result);
            }
            if matches!(function, XLF_CALL | XLF_REGISTER_ID) {
                return crate::call::dispatch(frame, function, args, result);
            }
            match run(frame, function, args) {
                Ok(Some(value)) => {
                    if result.is_null() {
                        return SUCCESS;
                    }
                    match publish(frame, function, args, &value, result) {
                        Ok(()) => SUCCESS,
                        Err(why) => {
                            refuse(frame, &why);
                            FAILED
                        }
                    }
                }
                Ok(None) => SUCCESS,
                Err((code, why)) => {
                    refuse(frame, &why);
                    code
                }
            }
        })
    }))
    .unwrap_or(FAILED)
}
```

`crates/calc-xll-host/src/callbacks.rs:469-516` (run(): xlFree and the permission preamble)
```rust
fn run(
    frame: &mut Frame<'_>,
    function: i32,
    args: &[*mut XlOper12],
) -> Result<Option<CalcValue>, (i32, String)> {
    if function == XL_FREE_FN {
        for pointer in args {
            free(&frame.state, &unsafe { (*pointer).read_unaligned() });
        }
        return Ok(None);
    }
    if frame.signature.is_some()
        && function & 0x8000 != 0
        && !(function == 32890 && frame.signature.as_ref().is_some_and(|s| s.macro_equivalent))
    {
        return Err((
            INVALID_FUNCTION,
            "dc-snz.18: worksheet functions cannot call commands".into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe) && function == 16401 {
        return Err((
            NOT_THREAD_SAFE,
            "dc-snz.18: event registration from calculation workers is not modelled".into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe) && function == 379 {
        return Err((
            NOT_THREAD_SAFE,
            "dc-snz.23: RTD requires the owning STA".into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe)
        && matches!(function, XL_UDF | XLF_EVALUATE)
    {
        return Err((
            NOT_THREAD_SAFE,
            "dc-snz.18: nested registered calls from calculation workers are not modelled".into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe)
        && matches!(function, XLF_REGISTER | XLF_UNREGISTER | XLF_SET_NAME | 186)
    {
        return Err((
            NOT_THREAD_SAFE,
            format!("callback {function} from a thread-safe function"),
        ));
    }
```

`crates/calc-xll-host/src/callbacks.rs:350-359` (allocation key of a callback result)
```rust
pub(crate) fn allocation_key(oper: &XlOper12) -> Option<usize> {
    unsafe {
        match oper.xltype & TYPE_MASK {
            XL_STR => Some(oper.value.string as usize),
            XL_MULTI => Some(oper.value.array.values as usize),
            XL_REF => Some(oper.value.reference.table as usize),
            _ => None,
        }
    }
}
```

`crates/calc-xll-host/src/callbacks.rs:414-434` (publish: arena kept until xlFree; free)
```rust
    let mut arena = Arena::new();
    let oper = arena.oper(value, &mut frame.state.borrow_mut().sheets)?;
    if let Some(key) = allocation_key(&oper) {
        frame.state.borrow_mut().allocations.insert(
            key,
            crate::native::CallbackAllocation {
                owner: frame.origin.handle as usize,
                _arena: arena,
            },
        );
    }
    unsafe {
        result.write_unaligned(oper);
    }
    Ok(())
}
pub(crate) fn free(state: &Shared, oper: &XlOper12) {
    if let Some(key) = allocation_key(oper) {
        state.borrow_mut().allocations.remove(&key);
    }
}
```

### K.5 The pool for thread-safe functions

`crates/calc-xll-host/src/worker.rs:165-201` (submit(): a private State per call; only owned values cross threads)
```rust
        .send(Box::new(move || {
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if !completion.active.load(Ordering::Acquire) {
                    return Err("dc-snz.23: canceled before native worker entry".into());
                }
                // Acquire separate Windows module leases on this thread. No Rc, RefCell,
                // arena pointer, libffi interface or evaluator borrow was sent from the STA.
                let module = Module::loaded(info.module.to_string_lossy().as_ref())?;
                let origin = Module::loaded(info.origin.to_string_lossy().as_ref())?;
                let state = Rc::new(RefCell::new(State {
                    modules: Vec::new(),
                    registrations: BTreeMap::new(),
                    names: BTreeMap::new(),
                    next_id: 1,
                    sheets,
                    sheet_names,
                    active_sheet,
                    events: BTreeMap::new(),
                    allocations: HashMap::new(),
                    diagnostics: Vec::new(),
                    hwnd,
                    thread: unsafe { crate::native::GetCurrentThreadId() },
                    closed,
                    status_message: None,
                    workers: None,
                    pending_async: BTreeMap::new(),
                    async_continuations: false,
                    rtd: None,
                }));
                let cif = crate::invoke::call_interface(&info.signature);
                let registration = Rc::new(Registration {
                    info,
                    module,
                    origin,
                    uses: Cell::new(uses),
                    cif,
                });
```

### K.6 Reads from a user call during evaluation (cached versus live reader)

`crates/calc-fml/src/eval/run.rs:4269-4326`
```rust
    fn argument_values_of(&mut self, reference: &ReferenceLike) -> Result<CalcValue, String> {
        match self.run.reader.values_of(reference) {
            Ok(value) => Ok(value),
            Err(EvalFault::MissingInput { cell })
                if self.reevaluation
                    == Some(calc_func::UserReevaluationReason::ProvisionalArguments)
                    && self.run.reader.pending_producer(cell) =>
            {
                self.request_reevaluation(calc_func::UserReevaluationReason::ProvisionalArguments)?;
                Ok(CalcValue::empty())
            }
            Err(fault) => {
                let text = format!("{fault:?}");
                self.fault.get_or_insert(fault);
                Err(text)
            }
        }
    }
    fn values_of(&mut self, reference: &ReferenceLike) -> Result<CalcValue, String> {
        match (if self.cached {
            self.run.observation_reader.unwrap_or(self.run.reader)
        } else {
            self.run.reader
        })
        .values_of(reference)
        {
            Ok(value) => {
                // A failed cached read stays a fault; only an actual successful observation
                // can request a second semantic entry around an admitted producer.
                if self.cached
                    && self.reevaluation
                        == Some(calc_func::UserReevaluationReason::ProvisionalArguments)
                {
                    let explicit = reference.areas().iter().all(|read| {
                        self.provisional_arguments.iter().any(|argument| {
                            argument.contains(read.first()) && argument.contains(read.last())
                        })
                    });
                    let pending = if explicit {
                        self.run.reader.provisional_argument_pending(reference)
                    } else {
                        self.run.reader.provisional_observation_pending(reference)
                    };
                    if pending {
                        self.request_reevaluation(
                            calc_func::UserReevaluationReason::ProvisionalArguments,
                        )?;
                    }
                }
                Ok(value)
            }
            Err(fault) => {
                let text = format!("{fault:?}");
                self.fault.get_or_insert(fault);
                Err(text)
            }
        }
    }
```

### K.7 The xlcall32 shim

`platform/calc-xll-shim/src/variadic.c:1-22`
```
/* SPDX-License-Identifier: MIT. Original varargs adapter; no SDK header. */
#include <stdarg.h>
#include <stddef.h>
extern int __stdcall Excel12v(int, void*, int, void**);

__declspec(dllexport) int __cdecl Excel12(int function, void* result, int count, ...) {
    void* args[255];
    va_list list;
    int i;
    if (count < 0 || count > 255) return 4;
    va_start(list, count);
    for (i = 0; i < count; ++i) args[i] = va_arg(list, void*);
    va_end(list);
    return Excel12v(function, result, count, args);
}
/* Legacy XLOPER is a different layout. Refuse it until X7 implements that ABI. */
__declspec(dllexport) int __cdecl Excel4(int function, void* result, int count, ...) {
    (void)function; (void)result; (void)count; return 32;
}
__declspec(dllexport) int __stdcall Excel4v(int function, void* result, int count, void** args) {
    (void)function; (void)result; (void)count; (void)args; return 32;
}
```
