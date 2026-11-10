# Hosting Excel add-ins in DNA OneCalc and DNA Calc

Design review against commit `1ece3d0ffeae28f27aa0ff28c2d873e3a6c2af84` · 10 November 2026

This review uses the supplied appendices as the record of the pinned host and its Excel measurements. The repository snapshot was not independently retrieved. Public framework documentation and source establish additional requirements, but a current source tree, historical source listing, and a particular shipped binary are distinct evidence. No Excel experiments were run for this review.

**Evidence notation.** **[public-doc]** identifies published documentation; **[public-code]** identifies inspected public framework code; **[recollection]** identifies an explicitly unverified technical recollection; **[inference]** identifies a design conclusion or proposed experiment. `Supplied F.3`, for example, means the measurement reported in your prompt, not an independent measurement. Host-status cells reproduce your inventory. A paragraph or table row's tag governs its associated explanation. Links such as [M01] resolve to the source register at the end. Likelihood ratings are engineering priorities, not prevalence statistics.

**Contents:** [A. Gap analysis](#a-gap-analysis) · [B. Architecture critique](#b-architecture-critique) · [C. Implementation plan](#c-prioritized-implementation-plan-and-acceptance-gates) · [D. Experiments](#d-discriminating-experiments) · [Sources](#source-register-and-evidence-boundaries).

## A. Gap analysis

### A.1 Assessment and scope

**[inference] The architecture is a credible foundation for a native compatibility host, but the present acceptance result establishes a narrow UDF profile. It does not establish full XLL or broad COM compatibility.** The next work should make invocation context, ownership, recalculation, registration and lifecycle exact before expanding the number of successful examples. Preserve the crate boundaries and the same-binary tracer.

The supplied 350/353 page result is dominated by the 289-case native fixture. Separately, F.4 records substantial Excel-DNA RTD protocol and invocation failures despite matching values. Those are different coverage dimensions. Keep the page score, ordered-boundary score, memory/lifecycle score, and framework-feature matrix visible together. **[inference]**

There are three important distinctions in the product promise:

- **Modern x64 UDF hosting** is attainable in OneCalc without a spreadsheet engine, for operations that have a real scratch-session meaning. **[inference]**
- **Full Excel C API compatibility** includes legacy Excel4 operands, commands, macro information functions, workbook state and lifecycle. An engine-free scratchpad cannot provide all workbook behavior merely by adding callback numbers. **[inference]**
- **Broad COM support** needs actual object behavior and both dispatch and typed ABI coverage. Answering Excel IIDs and retaining ribbon XML does not supply that behavior. **[inference]**

Keep these as explicit profiles with feature-level failures. Do not rename a refusal a parity success, including when the corresponding Excel run crashes.

### A.2 Frameworks and runtime variants

The callback numbers and host gaps are expanded in A.4. “Load” includes framework initialization that may be deferred to a posted macro; it is not limited to the duration of `xlAutoOpen`.

| Framework variant | Load requirements and exposure | Common use | Less common use and important qualification |
|---|---|---|---|
| Excel-DNA, .NET Framework | Your measured loader needs current-XLL identity, registration, version/process probes and its command dispatcher. Public loader code selects the default AppDomain on the cluster path, materially changing isolation and unload behavior. **[public-code]** [ED02] | Value/reference marshaling, `xlCoerce`, `xlfCaller`, per-thread returns, macro scheduling, and RTD if requested by the add-in. `Excel` throws on callback failure; `TryExcel` exposes the exact status. **[public-doc]** [ED01] | COM, menus, ribbon/task panes, custom marshaling and fast return signatures are feature-dependent. The framework name does not provide a complete list of callbacks used by arbitrary user code. **[inference]** |
| Excel-DNA, .NET 6/8 | Same Excel-facing contract, with CoreCLR hosting, runtime configuration and dependency resolution. One hosted CoreCLR can coexist with .NET Framework; load order and roll-forward settings determine compatibility. **[public-doc]** [ED03] | Keep native `X` async and RTD-backed `ExcelAsyncUtil` as separate protocols. Your net8 simple UDF pass does not cover RTD invocation traffic or runtime coexistence. **[inference]** (supplied F.2/F.4) | Assembly unloading, runtime unloading and XLL logical removal are different operations. Do not promise fresh process state after unregistering. **[inference]** |
| Excel-DNA, Native AOT | Self-contained native payload; retain its executable/runtime dependencies and exact registration ABI. Native AOT shared-library unloading is unsupported. **[public-doc]** [M12] | UDFs, native async/RTD and COM functionality depend on the particular generated package. Current Excel-DNA documentation includes commands, ribbons and dynamic COM access; the one preview5 formula proves none of those paths. **[public-doc]** [ED04]; **[inference]** (supplied G.2) | Logical disable while resident can be a separate supported operation; guaranteed fresh initialization requires a fresh process. Do not generalize a particular failed reinitialization into a ban on loading multiple distinct Native AOT libraries. **[inference]** |
| xlw native C++ library | C API registration, host-version selection, parameter/type conversion and any enabled menu hooks. It is not intrinsically a COM framework. **[public-doc]** [W01] | The supplied generated sample exercises numeric/matrix/reference conversion and its pool-backed return lifetime. Preserve copying before another entry on that thread. **[inference]** (supplied D.1/F.2) | User C++ can call additional C API, COM or native APIs. “xlw: no COM” describes the base library path, not every xlw add-in. **[inference]** |
| xlw generated sample | A separate acceptance binary, with its emitted type strings, automatic registration and cleanup sequence. Its success is useful evidence about that generator configuration. **[inference]** (supplied F.2) | Include missing/error/reference inputs, wide strings, FP12 matrices and two overlapping `$` calls with pool reuse. **[inference]** | Do not infer compatibility with all older xlw binaries from the sample, or infer failure of all xlw 5 binaries from missing Excel4. XLOPER12/MTR support predates xlw 5. **[public-doc]** [W01] |
| xlwDotNet | Generated C++/CLI bridge and native xlw wrapper, not Excel-DNA runtime hosting. Deployment/runtime selection needs its own fixture. **[public-doc]** [W02], [M30] | Same native operand lifetime plus managed/native transition, assembly resolution and exception behavior. **[inference]** | Test Framework and modern .NET separately, including `ijwhost.dll`, runtime configuration and mixed loading with Excel-DNA. Native unloading cannot be inferred from freeing one wrapper DLL. **[inference]** |
| xlOil native core | Keep origin XLL, registered procedure DLL, free DLL, export address and registration generation distinct. Your supplied traces establish rewritten-export and cross-module free behavior for 0.22.1. **[inference]** | `U/Q/K%` and in-place FPArray returns are documented. **[public-doc]** [O01] Reference conversion, thread/context tests, COM discovery and queued execution also need the supplied 0.22.1 acceptance paths. **[inference]** | Native callback/event handlers, runtime-generated thunks, COM add-in factories and re-registration all create lifetimes beyond one UDF entry. **[inference]** |
| xlOil native plugins | Core plus plugin binaries must remain a co-resident group. Utils/SQL passing is evidence for those plugins and formulas. **[inference]** (supplied F.2) | Preserve free-module routing and plugin registrations when another XLL becomes current. **[inference]** | Add plugin unload and re-registration tests; do not assume every plugin shares one allocator or export owner. **[inference]** |
| xlOil_Python | Deferred registration, Python/runtime dependencies, GIL, COM events and macro scheduling. Served documentation identifies itself as 0.19.0 and defaults `macro=True`; this must be checked against the 0.22.1 binary. **[public-doc]** [O02] | Explicitly test ordinary, macro, threaded, native-async and RTD functions. Documented Python async defaults to RTD; the basic decorator is not a representative ordinary-UDF status-64 test. **[public-doc]** [O02], [O03] | Python source/module reload and re-registration are distinct from unloading the native core or restarting CPython. Treat the supplied 0.22.1 crash as binary-specific evidence. **[inference]** |
| xlOil workbook-local functions | Generated VBA stubs, workbook scope and a bridge to Python. These are automatically macro-type; the documented local route does not support thread-safe/native-async functions, although RTD works. **[public-doc]** [O04] | Existing-stub execution, dynamic stub installation, and workbook reopen/rebinding are three separate acceptance features. **[inference]** | `Application.Run` alone is insufficient. Dynamic installation may need the actual VBIDE members used by the binary; local functions require macro-enabled persistence. **[public-doc]** [O04]; **[inference]** |

### A.3 Corrections to Appendix D and related premises

1. **The type inventories are examples, not closed framework ABIs.** Excel-DNA has exception-safe numeric/array paths and commands with return values; capture their emitted registration strings. xlOil also documents in-place FPArray returns. The phrase “one of `! # $`” is misleading: volatility and execution-class flags are not a single mutually exclusive choice; `!` can accompany an execution-class flag. Your own `Q!#` and `>EX!` measurements demonstrate this. **[public-code]** [ED05]; **[public-doc]** [O01]; **[inference]** (supplied E).

2. **`#` plus a declared reference parameter is not an unconditional volatility property in your measured profile.** It is a call-start event when an actual reference arrives in an `R/U` position. Your static parser OR and sticky engine OR both need replacement. **[inference]** (supplied E.4, K.1).

3. **Do not “fix” callback ownership by universally setting `xlbitXLFree`.** Documentation assigns that flag a role when the XLL returns Excel-allocated memory to Excel. It separately requires freeing callback allocations. Historical xlOil code explicitly adds the bit in its own `fromExcel()` wrapper, so wrapper-visible bits need not be bits supplied by Excel. The definitive documented issue in K.4 is different: `xlFree` nulls the operand's contained pointer; your function only removes a map entry. **[public-doc]** [M02], [M03]; **[public-code, historical v0.8.2]** [O05], [O06]. Probe raw callback returns before wrapper mutation.

4. **“Every framework calls the executable export directly” is too categorical.** Your binaries establish the need for `MdCallBack12`; a different binary may import a shim function or use an older API route. Verify its actual route with the tracer. **[inference]**

5. **The application directory does not unconditionally win DLL resolution.** Loaded-module lookup, redirection and SxS can precede it; alternate search flags affect dependency lookup. Load and retain the intended shim explicitly, and verify the bound module. Do not replace the system's Excel files. **[public-doc]** [M13]; **[inference]**

6. **Old Excel-DNA process-name detection affects AppDomain isolation.** The public classic loader's cluster path uses the default AppDomain and does not unload it. That is more serious than suppressing an error dialog or changing `&`. The prompt's exact switch-introduction commit/date remains supplied information, not independently verified here. **[public-code]** [ED02]. A renamed executable is not automatically equivalent to Excel for older binaries.

7. **`xlfRegister` does have a module argument.** The current originating-XLL context remains necessary for module-relative operations and ownership, but the sentence grouping it with APIs carrying no module argument is wrong. Likewise, `xlGetName` **16393** is the current-XLL path operation; `xlfGetName` **107** is a different XLM operation. **[public-doc]** [M04]; **[public-code]** [ED06].

8. **One .NET runtime per process is too broad.** The practical conflict is hosted CoreCLR selection/configuration. .NET Framework can coexist with it, and Framework has its own side-by-side hosting rules. A Rust executable avoids selecting a runtime itself, but another add-in or COM component can still select one first. **[public-doc]** [ED03], [M14]; **[inference]**

9. **Dynamic-code restrictions are framework-dependent.** CLR JIT and runtime-generated xlOil thunks need compatible process policy; a Native AOT-only profile has different requirements. Check the actual image/thunk behavior rather than disabling unrelated mitigations globally. **[inference]** (supplied D.2).

10. **`xlGetInst` is expected to fail on x64 Excel.** Its absence is not a missing success implementation. Measure the precise failure status and bytes, while retaining working `xlGetInstPtr` support. **[public-doc]** [M15], [M16].

11. **`xlStack` has a documented 64 KiB cap through Excel12.** Returning raw OS stack headroom can be observably different even though xlOil merely uses success/failure as a context test. **[public-doc]** [M17].

12. **`xlAutoAdd` and `xlAutoRemove` are manager actions, not universal lifecycle calls.** Ordinary shutdown must not be implemented as an unconditional manager removal. Excel-DNA's managed `IExcelAddIn.AutoClose` is also distinct from native `xlAutoClose`. **[public-doc]** [M18], [M19], [ED07].

13. **Calculation events have an origin rule.** The published event contract says cancellation is followed by ended, and neither event is raised for programmatic recalculation. This may need a modern-build witness, but a blanket “fire on every epoch” implementation is unjustified. COM `AfterCalculate` is another event contract. **[public-doc]** [M05], [M06], [M20].

14. **Thread-safe async is valid.** Refusing `>...X$` is a host limitation, not an Excel registration rule. `&` is not a synonym for `$`; async and cluster-safe are incompatible. **[public-doc]** [M07].

15. **“xlw 5 cannot run” is incorrect as a family-wide claim.** XLW's history records XLOPER12, Unicode and MTR in 3.0 and x64 in 5.0. Actual Excel4 calls or legacy signatures determine exposure. **[public-doc]** [W01].

16. **Framework COM minima are feature-dependent.** Pure Excel-DNA and native xlw UDFs can avoid broad COM; Excel-DNA scheduling and xlOil discovery can need it before user-visible COM features. The exact parent identity of your Window object should be probed rather than inferred from the successful `.Parent.Application` chain. **[inference]**

17. **Refusing a reload that crashes Excel is an intentional divergence.** It is a reasonable product choice, but not an equality result. The Native AOT no-unload rule is a runtime contract; the xlOil 0.22.1 crash is a particular measured lifecycle failure. Neither justifies disabling Python source reload. **[public-doc]** [M12]; **[inference]** (supplied F.2).

18. **Validate `#` with `&` as well as `#` with `$`.** The registration documentation excludes macro-equivalent cluster-safe functions; current Excel-DNA registration code also clears cluster safety for macro functions. The supplied parser does not show that check. Measure `Q#&` and `QU#&` rejection, including raw status/output, rather than inventing the exact failure code. `Q$&` is a distinct permitted combination in the documented model. **[public-doc]** [M04]; **[public-code]** [ED05].

### A.4 Complete S-status table

Abbreviations: **ED** = Excel-DNA variants; **W** = xlw/native and generated wrappers; **WD** = xlwDotNet; **O** = xlOil core/plugins; **OP** = xlOil_Python. L/C/R mean load/common/rarer or feature-dependent use. Unless a source is explicitly identified, framework exposure and likelihood are **[inference]**, grounded in the supplied D/F/G traces; they are not claims that every binary makes the call.

| ID | Call or feature and essential shape | Frameworks and when | Supplied host status | Consequence if wrong; likelihood |
|---|---|---|---|---|
| S1 | `xlFree` 16384, pointers to callback-result operands; no value result | All L/C; cleanup after 64/1 and inside auto-free | Impl; pointer-key removal; outside frame 32 | Pointer mutation is missing; worker-store lifetime can invalidate later frees. Universal ownership path: **highest**. **[public-doc]** [M03] |
| S2 | `xlStack` 16385, zero args, Int result | O context probe L/C; all recursion checks R | Impl using real stack headroom | Compare capped value, type, outside-context failure. **Medium**, easy contract fix. **[public-doc]** [M17] |
| S3 | `xlCoerce` 16386, source plus optional Int type mask | All C; reference conversion especially ED/W/O | Partial; locale/conversion/readiness gaps | Wrong 32 versus 64 changes native execution, exceptions and side effects; mask/blank/error differences alter inputs. **Highest**. **[public-doc]** [M01], [M08] |
| S4 | `xlSet` 16387, rectangular reference plus value or omitted clear | ED reference writes; W/O commands C/R | Absent | Queued writes fail; fake success would lose changes. Command permission, fill shape and undo effects matter. **High for commands**. **[public-doc]** [M09] |
| S5 | `xlSheetId` 16388 / `xlSheetNm` 16389 | All reference-aware C | Impl for tested shapes | Active sheet and caller sheet must differ correctly; preserve stable sheet identity and quoted workbook/sheet names. **High**. **[inference]** |
| S6 | `xlAbort` 16390, optional retain Bool, Bool output | Long-running native/managed work C/R | Always false | Cooperative cancellation never works; leases remain indefinitely. Query and clear are distinct, and true Bool is not status 1. **High**. **[public-doc]** [M10] |
| S7 | `xlGetInst` 16391; `xlGetInstPtr` 16403, zero args | O/ED host identity L; native custom code R | Old call absent; pointer call Impl | Preserve x64 failure for old API; BigData handle for new one. Do not truncate to fabricate success. **Medium**. **[public-doc]** [M15], [M16] |
| S8 | `xlGetHwnd` 16392, zero args, Int | ED/O discovery, UI L/C | Impl | Handle must identify the matching host/window/process; Excel4 returns a different-width representation. **High**. **[public-doc]** [M21] |
| S9 | `xlGetName` 16393, zero args, counted string | All L; macro queues/nested callbacks C | Impl, current module | Origin restoration across nested calls, auto-free and multiple loaded XLLs matters. **High**. **[inference]** |
| S10 | 16394/16395 message enable/disable; 16396 define binary name `(name, BigData)`; 16397 get `(name)` | Custom ED/W/O commands R | Absent | Binary names need persisted bytes, sheet-scope behavior and a real handle/free path. GetBinaryName is not an async-handle operation. **Low for supplied samples; required for full API**. **[public-doc]** [M22], [M23] |
| S11 | `xlGetFmlaInfo` 16398 | ED queue/UI checks; other edit-sensitive add-ins | Only LPenHelper route | Do not assume Excel12 route must succeed just because the constant exists. Probe route and result ABI separately. **Medium**. **[inference]** |
| S12 | `xlAsyncReturn` 16400 `(handle, value)`; batched arrays also documented | ED/O/OP native async C | Token support; stale/duplicate 256; zeroed result observed | Need `$` launch, immediate-completion race, batch semantics and synchronous copying. Background thread has no ordinary C API privileges. **High**. **[public-doc]** [M11]; **[inference]** |
| S13 | `xlEventRegister` 16401 `(procedure-name Str, event Int)` | ED/O L; async cancellation C | Stored, never raised; `$` 128 | Leaks/cancelled work and callback counts differ; result is documented Int >0 on success, exact value requires witness. **High**. **[public-doc]** [M05], [M06] |
| S14 | `xlRunningOnCluster` 16402 | ED and cluster-aware add-ins L/R | False | Correct for local host profile; do not equate this API with old process-name heuristics. **Low**. **[inference]** |
| S15 | `xlfCaller` 89, zero args | All caller/reference/RTD-aware C | Single-cell SRef; command #REF | CSE, spill, menu and nested-call forms differ. Caller must be an invocation attribute, not active selection. **High**. **[public-doc]** [M24] |
| S16 | `xlfRegister` 149; Form 1 module/procedure/types/name/metadata, Form 2 module only | All L/re-registration | Measured Form 1 subset | Missing optional names, omitted-type AutoRegister, macro-type 0 and invalid-operand results remain gaps. Current minimum four arguments excludes documented forms. **Highest**. **[public-doc]** [M04] |
| S17 | `xlfUnregister` 201 by ID or module form | All remove/reload R | Partial ID use counts | Hidden names and live procedure lifetimes differ; module-form lifecycle is not just decrementing IDs. **High**. **[inference]** (supplied C/F.1) |
| S18 | `xlfSetName` 88 `(name[, definition, ...])` | All unregister; ED/O dynamic names C/R | Clear-name subset | Hidden-name cleanup is useful, but defining/evaluating names and scope are missing. **High for wrappers; medium general names**. **[inference]** |
| S19 | `xlfCall` 150 / `xlfRegisterId` 267 | Native interoperability R; registration helpers | 289-case subset; O/P/R refused | Preserve the measured CALL-specific conversions; do not reuse worksheet integer truncation. Legacy/array expansion remains. **Medium**. **[inference]** (supplied F.2) |
| S20 | `xlUDF` 255 `(name-or-id, args...)` | All composition C/R | Denies policy changes; `$` 128 | Legitimate nested calls need child context and inherited attempt state. Blanket same-module/policy refusal blocks composition. **High**. **[inference]** |
| S21 | `xlfEvaluate` 257 `(text)` | ED/O dynamic names/formulas C | Constant-only; hidden register IDs; `$` 128 | Reference, scoped-name and registered-call evaluation need the current reader and caller. Preserve measured `$` denial; probe documented safe-expression exceptions separately. **High**. **[inference]** |
| S22 | `xlfRtd` 379 `(ProgID, server, topic...)` | ED/OP async and streaming C | Values largely work; strict ED traffic fails | Initial value, topic sharing/coercion, refresh/disconnect order and wrapper reentry remain blockers. **Highest for async coverage**. **[inference]** (supplied F.4) |
| S23 | `xlfVolatile` 237 `(Bool)` | Macro-equivalent ED/W/O C | Callback permission measured; engine sticky OR | Fix downstream event reduction, not the measured permission table. Abandoned attempts and lifting are distinct. **Highest**. **[inference]** (supplied E/K) |
| S24 | `xlfGetWorkspace` 186 `(item)` | ED/O probes L; user metadata R | Items 2/37 only | International settings need consistent parsing/formatting; other successful Excel queries remain refusals. **High for startup items; demand-driven remainder**. **[inference]** |
| S25 | `xlfReftext` 146 `(reference[, A1-style])`; inverse `xlfTextref` 147 | ED/O reference-to-COM bridges C | Single-area only | Multi-area references, quoting, external qualification and caller locale can break Range lookup. **High**. **[public-doc]** [ED01]; **[inference]** |
| S26 | `xlcMessage` 32890 `(display[, text])` | Framework load/status reporting; `#` helpers | Impl with measured `$` 128/ordinary2 | Preserve the measured exception to a simple “all xlc denied” rule. **Medium**. **[inference]** (supplied F.3) |
| S27 | Worksheet callback numbers; macro info `GET.CELL` 185, `GET.WORKBOOK` 268 | All; macros especially ED/O C | Catalogue route; macro info absent | CalcValue conversion loses ABI distinctions; worksheet compatibility does not automatically give callback-byte compatibility. **High**. **[inference]** (supplied FILTER mismatch) |
| S28 | Commands: RUN32785, ON.KEY32936, ALERT32886, NEW32887, WORKBOOK.INSERT33122, FILE.CLOSE32912, ECHO32909, FORMULA32864, FORMULA.ARRAY32866, SELECT32877 | ED/W/O command and discovery paths L/C/R | Most absent; cell denials | Needed in an ordered capability rollout, not fake-success stubs. Queue transport already partly works. **High for selected commands**. **[public-code]** [ED06]; **[inference]** |
| S29 | ADD.MENU152, ADD.COMMAND153, DELETE.MENU158, DELETE.COMMAND159, GET.BAR182, GET.TOOL259 | ED/W legacy menus; O/custom UI R | Absent | Optional initialization may fail before UDFs register. A model must retain objects, actions and deletions if it claims success. **Medium**. **[public-code]** [ED06]; **[inference]** |
| S30 | Excel4/Excel4v and XLOPER/ANSI/FP ABI | Legacy/custom code in any family | All return 32 | Cannot claim full XLL while this remains. x64 process architecture does not imply Excel12-only callbacks. **High for full-API milestone**. **[public-doc]** [M01]; **[inference]** |
| S31 | LPenHelper, edit-state structure | ED queue and some UI integrations L/C | Always ready | Queued macros can run at the wrong edit/point state or retry incorrectly. **High for queue fidelity**. **[public-code]** [ED08]; **[inference]** |
| S32 | XLCallVer, no arguments | All API-version selection L | 3072 | Keep API version separate from executable version and Office build. Supplied behavior is correct for this profile. **[inference]** (supplied F.1) |

### A.5 Disposition of every R item

| ID | Review disposition and action |
|---|---|
| R1 | **Confirmed structural gap.** Implement typed not-ready reads, status 64, unwind state and explicit reevaluation permission; do not translate every missing-input fault to64. **[inference]** (F.3/K.6) |
| R2 | **Confirmed, with origin qualification.** Implement actual event dispatch using measured UI/programmatic/cancel rules. **[public-doc]** [M05] |
| R3 | **Premise unproved; different definite defect.** Measure callback flags; fix xlFree pointer mutation and allocation lifetime. **[public-doc]** [M02], [M03] |
| R4 | **Confirmed.** Invocation-dependent default plus ordered evaluation events replaces both static ORs. **[inference]** (E.4/E.6) |
| R5 | **Confirmed ABI gap; xlw-family claim corrected.** Implement modern in-place and legacy support in separate increments. **[public-doc]** [W01]; **[inference]** |
| R6 | **Confirmed containment gap.** Timeout is not termination; arbitrary AV recovery in the same process is not credible. Use process isolation for restartable failure containment. **[inference]** |
| R7 | **Partly implementation choice, partly observable.** Four persistent threads are legal as a chosen profile but cannot reproduce a 16-thread trace. Per-call host State is a lifetime/context risk independent of pool size. **[inference]** |
| R8 | **Confirmed.** Caller must be a tagged descriptor, not always a CellKey/1×1 SRef. **[public-doc]** [M24]; **[inference]** |
| R9 | **Confirmed.** Destination is a mask; default coercion, references, locale and errors need independent witnesses. **[public-doc]** [M08] |
| R10 | **Mixed.** Hidden-name retention after unregister can be correct; a single global map cannot express all workbook-local names/VBA or late rebinding. Keep these namespaces separate. **[public-doc]** [M04], [M25]; **[inference]** |
| R11 | **Confirmed feature gap, not total queue failure.** Preserve working Run/SyncMacro transport; add real body operations and context rules. **[inference]** (F.1/G.1) |
| R12 | **Route-dependent gap.** Implement manager info/add/remove and omitted-type auto-registration when those routes occur, not on every load. **[public-doc]** [M18], [M19], [M26] |
| R13 | **Keep visible, split mechanisms.** Old identity adaptation, AOT residency and xlOil-specific crash avoidance are three policies with different evidence and exit conditions. **[inference]** |
| R14 | **Split by contract.** xlGetInst success is not required on x64; binary names, metadata and writes remain real full-API gaps. **[public-doc]** [M15]; **[inference]** |
| R15 | **Confirmed; prioritize coordinate/read services.** The three current failures do not require implementing all of Excel. They do require actual Caller/Range semantics. **[inference]** (G.2/I) |
| R16 | **Coverage statement only.** Clean IntelliSense decline in one tested version is useful, not proof that all UI extensions safely decline. Maintain a separate UI integration profile. **[inference]** |

## B. Architecture critique

### B.1 Preserve the layering, strengthen the invocation contract

**[inference] Keep `calc-fml` independent of XLOPER and `calc-engine` independent of the XLL host.** The missing abstraction is a sufficiently expressive invocation service between them, not an Excel-specific dependency inside the evaluator.

The current `Option<CellKey>` caller and `Result<CalcValue, String>` reference read are too narrow for all the required distinctions. Add engine-neutral descriptors and typed read outcomes; let the XLL adapter map them to ABI bytes and return codes.

```rust
// Interface sketch, not a patch against the pinned repository.
enum ReferenceRead {
    Value(CalcValue),
    Pending { producers: Vec<ProducerKey> },
    InvalidReference,
    Refused(CapabilityRefusal),
}

enum CallerDescriptor {
    Cell(ReferenceDescriptor),
    LegacyArray(ReferenceDescriptor),
    SpillAnchor(ReferenceDescriptor),
    RegisteredCall { callable: CallableIdentity },
    Macro(MacroCaller),
    None,
}

struct InvocationIdentity {
    invocation: InvocationId,
    parent: Option<InvocationId>,
    evaluation_attempt: AttemptId,
    read_view: ReadViewLease,
    caller: CallerDescriptor,
    origin: InvocationOrigin, // entry, recalc, command, lifecycle, etc.
}

enum NativeDisposition {
    Returned,
    Uncalculated { producers: Vec<ProducerKey> },
    Aborted,
}
```

**[inference] Separate three channels:** the callback's integer status; its optional/partially modified output operand; and the evaluator's invocation disposition. A native function may return an ordinary error after receiving 64, but that cannot erase the scheduled unwind. Conversely, `xlretSuccess` with an `xltypeErr` is often an ordinary Excel result, not a host refusal.

**[inference] Introduce an ABI-preserving callback value inside `calc-xll-host`.** Do not make `CalcValue` carry ownership flags or pointers. The present `run -> CalcValue -> publish` path cannot necessarily preserve Int versus Num, SRef versus Ref, output untouched versus zeroed, reference-table order, or allocator provenance. The supplied raw FILTER mismatch is evidence that a worksheet-value projection is insufficient at this boundary.

```rust
enum CallbackWrite {
    Unchanged,
    ZeroAll32,                      // measured xlAsyncReturn profile
    Operand(OwnedCallbackOperand), // preserves ABI kind and raw UTF-16
}
struct CallbackReply {
    status: XlReturn,
    write: CallbackWrite,
    disposition: Option<NativeDisposition>,
}
```

### B.2 ABI details and registration contracts

**[recollection] The following is the expected modern Windows x64 layout to check against your existing ABI fixture, not a claim that new layout measurements were performed.** Your supplied evidence already establishes the 32-byte XLOPER12 and signed 32-bit BigData count. I could not independently retrieve the framework's complete layout file, so do not replace an already-tested layout from this table alone. Turn each row into an offset/stride assertion and a raw-operand witness before changing ABI code.

| Structure/field | Expected x64 byte layout |
|---|---|
| XLOPER12 | 32 bytes; value union at 0; 32-bit `xltype` at 24; tail padding to 32 |
| Numeric/string | f64 or pointer at value offset 0 |
| Bool/error/integer operand | 32-bit field at value offset 0; distinct from 16-bit registration types A/I/L/M |
| Multi | pointer at 0; signed 32-bit rows at 8; signed 32-bit columns at 12; elements have 32-byte stride |
| Ref | reference-table pointer at 0; pointer-sized sheet identifier at 8 |
| SRef | 16-bit count at 0; rectangle at 4 |
| XLREF12 | four signed 32-bit fields at 0,4,8,12: first row, last row, first column, last column; endpoints inclusive |
| XLMREF12 | 16-bit count at 0; rectangles begin at 4, stride 16; variable-size allocation |
| FP12 | signed rows at 0, columns at 4, f64 elements beginning at 8 |
| BigData | pointer/handle at 0; signed 32-bit byte count at 8; it is not a 64-bit byte count |

**[inference] Preserve unaligned access at all foreign-memory boundaries.** Rust `read_unaligned` on the outer descriptor does not authorize creating aligned references to packed nested operands. Validate dimension products and allocation sizes with checked arithmetic; validate coordinates and area count independently. The current one-million-element FP12 limit is a resource policy, not proof of Excel's actual array limit. Keep such refusals visible.

**[recollection] Relevant type values are Num `0x0001`, Str `0x0002`, Bool `0x0004`, Ref `0x0008`, Err `0x0010`, Flow `0x0020`, Multi `0x0040`, Missing `0x0080`, Nil `0x0100`, SRef `0x0400`, Int `0x0800`, BigData `0x0802`; free flags are XLFree `0x1000` and DLLFree `0x4000`.** Verify these with the fixture's existing independent constants. BigData is a composite code: a test such as `type & xltypeStr != 0` must not mistake it for a string. Strip only recognized ownership flags before validating the base kind; retain raw bits for tracing.

**[public-doc]** Counted Unicode strings and null-terminated Unicode strings are different ABIs. In-place F%/G% buffers have room for 32,768 16-bit units including terminator/count. O/O% each expand one registered parameter into three native arguments. A synchronous `>` can mean modify the first parameter in place; it is not exclusively an async marker. [M04]

**[inference] Implementation consequences:**

- Preserve counted strings as UTF-16 code units at the ABI boundary, including embedded NUL. Do not use a `wcslen` path for D%/XLOPER12 strings or count Unicode scalar values instead of code units. A raw UTF-16 representation also avoids accidental replacement of lone surrogate units before the oracle has established behavior.
- Build a separate ABI-argument plan from logical parameters. O/O% expand to row-pointer, column-pointer and double-array-pointer. Digit returns select a logical argument; synchronous `>` selects the first argument; F%/G% returns select the first argument with the corresponding string code, which need not be argument 1. These paths do not read the ordinary native return register.
- Do not infer worksheet scalar conversion rules from CALL. Your F.2 explicitly establishes different integer behavior on the CALL route.
- Preserve reference area order and one sheet identity per XLOPER12 Ref. A collection of areas on multiple sheets needs another representation or an explicit limitation; it is not a legal single Ref made by dropping sheet IDs.
- Legacy XLOPER is a separate ABI, including narrower integer/reference fields and byte strings. Implement it with its own layout witness; do not cast XLOPER to XLOPER12.

**[inference] Registration should be a state machine, not just a name map.** Keep `RegistrationId`, signature, macro type, metadata, name binding, use count, origin module, code module, free module, address snapshot and callable generation separate. Add documented module-only registration and missing-type auto-registration without breaking the already-measured Form 1 path. Keep macro type 0 callable but hidden from the wizard; do not conflate it with a command. Resolve an export on each registration as you already do, and preserve every existing registration's chosen address until the measured unregistration rules remove it.

### B.3 Complete ownership model

The following ownership table is the proposed host contract **[inference]**, grounded in the documented rules and supplied free traces. Exact flag patterns and result-byte mutations must still be matched per callback/build. In particular, no row assumes that every callback allocation arrives with XLFree set.

| Object or boundary | Owner and valid lifetime | Release/host action |
|---|---|---|
| Ordinary Q/U input descriptor, string, array, reference table | Host-owned borrowed buffers; do not assume foreign code preserves their contents | Adapter releases after return and required result handling; add-in acquires no ownership and must copy anything needed asynchronously |
| Scalar by-value argument | Native call value | No auxiliary release |
| E/L/M/N pointer, ordinary C%/D%, FP12 input | Host call arena; ordinary arguments borrowed | Keep through entry and any designated in-place result extraction |
| Explicit in-place parameter | Host writable buffer governed by registration | Extract from the specified parameter; apply measured conversion/lifetime rules; do not treat register return bits as a pointer |
| Successful callback scalar/SRef result | Descriptor supplied by add-in; usually no auxiliary allocation | `xlFree` remains safe for valid callback results; preserve exact operand mutation policy |
| Successful callback Str/Multi/Ref | Host allocation, separately tracked | Add-in calls xlFree, or returns Excel-owned memory with XLFree; recursively owned backing storage released once |
| Callback result retained across entries | Host allocation until its applicable release boundary | Do not destroy merely because the worker's private invocation State is dropped; probe lifetime and free from later valid context |
| UDF returns static/TLS/pool memory without free bits | Add-in | Copy complete value before next relevant native entry; never free with the host allocator |
| UDF returns DLLFree descriptor, including a scalar descriptor | Add-in | Normal-return path: copy, then call the correct module's `xlAutoFree12` with the exact pointer and flag intact, on the entry thread |
| UDF returns callback-owned memory with XLFree | Host owns backing storage; returned descriptor may be add-in-owned | Copy and release backing allocation; do not free an add-in static descriptor |
| Both free bits or malformed flag/type combination | Not established by supplied evidence | Probe in a disposable process; do not invent recursive double cleanup |
| Return after status 64/1 | Calculation result discarded; native pointer may still have a lifetime obligation | Separate discard cleanup from normal conversion; auto-free behavior on this path is an explicit experiment, D.2 |
| Native async input handle | Host-issued opaque identity, copied by add-in if retained | Never treat handle as an ordinary data buffer or infer meaning from cbData |
| Value passed to xlAsyncReturn | Add-in memory during callback | Host must copy before returning; add-in frees its own value memory afterward; ordinary UDF DLLFree protocol is not automatically applicable |
| xlGetInstPtr BigData | Borrowed process module identity | No buffer allocation to free just because type is BigData |
| xlGetBinaryName BigData | Host-owned global memory handle under its documented contract | Support GlobalLock-compatible handle behavior and xlFree release; distinct from instance/async handles |
| BSTR, SAFEARRAY, VARIANT and COM interface results | COM allocation/reference rules | Correct SysAllocString/VariantClear/SafeArray/Release ownership; never pass through the XLL arena free path |

**[public-doc]** `xlFree` releases auxiliary memory and nulls the contained pointer while preserving other operand parts. Normal `xlAutoFree12` runs on the original UDF thread before another worksheet cell is evaluated there; only `xlFree` may be called back during it. Async result memory remains the XLL's responsibility. Binary-name results have their own handle ownership rule. [M03], [M27], [M11], [M23]

**[inference] Concrete changes to K.3/K.4:**

1. Change the explicit xlFree path to operate on the caller's mutable operand pointer, using unaligned reads/writes as needed. Removing an allocation by a copied value is not enough. Preserve non-pointer bytes until the oracle proves a broader mutation.
2. Give auto-free an explicit `FreeOnly` frame. Merely retaining the preceding worksheet/lifecycle frame can grant illegal callbacks. Save and restore the originating XLL and allocation registry around cleanup.
3. Keep persistent allocation ownership independent of per-call worker State. Prefer a process/group allocator registry with allocation kind, owner generation and lifetime metadata, plus worker-local construction where useful.
4. Do not let an owner thread publish a pointer from its arena directly into a worker reply. Marshal an owned representation; construct the callback operand on the calling worker with a tracked lifetime.
5. Prevent premature teardown while native result copying/freeing is active. Metadata and event returns can also require correct cleanup; the worksheet return path is not the only pointer-return boundary.
6. Add an explicit binary-name allocation kind if S10 is implemented. The current pointer-key classifier only understands Str/Multi/Ref.

**[inference] Pointer provenance improves diagnostics but cannot make invalid native pointers safe.** An allocation generation stored only in the registry cannot distinguish a stale raw pointer from a later allocation reusing the same address. Nulling the caller's operand fixes the documented repeated-free case; it does not legalize stale shallow aliases or arbitrary pointers. Process containment remains necessary.

**[public-doc]** xlOil's `xloSort` example mutates the supplied array's row order without changing its allocation, outside an explicit in-place-return registration. [O01] **[inference]** Therefore do not expose the foreign input as an active shared Rust reference, impose read-only pages universally, or let a mutation corrupt an engine-owned shared value. Build isolated call buffers and test the documented mutable paths.

### B.4 Context and permission model

Use a stack of frames, not an unstructured single “current frame”. A TLS pointer to the top can remain an implementation detail if push/pop/restoration are exception-safe and nesting is explicit. Permissions depend on why control entered the add-in, registration flags, calculation attempt and cleanup state, not merely which OS thread services a request. **[inference]**

| Context | Reference reads | Worksheet/value callbacks | xlfVolatile 237 | Commands/writes | Other constraints |
|---|---|---|---|---|---|
| Ordinary cell, including `!` | Pending in-body read ->64; ready read normal | Supported worksheet callbacks; xlfCaller | 2 in supplied profile | General xlc denied2; xlSet denied | No macro information privileges |
| `#` cell | Valid pending-cell observation uses cached view in supplied witness | Macro information allowed where implemented | 0; ordered cell event | Does not grant general command permission; supplied MESSAGE exception works | No `$`; do not confuse with command context |
| `$` cell | Pending in-body read ->64 | Only allowed thread-safe operations | 2 in supplied profile | General command refusal; supplied MESSAGE128 | Permissions remain restricted even when entry is on owner thread |
| Native async synchronous entry | Same rules as its registration | Same registration restrictions | 2 for tested async registrations | No automatic macro privileges | `!` default event can still apply |
| Native async completion thread | No worksheet reads | xlAsyncReturn 16400 only under documented async contract | Outside-frame32 in supplied witness | No | Current host returns 256 for invalid/revoked/duplicate handles; confirm each oracle rejection scenario |
| Registered command / QueueAsMacro entry | Command-context view | Supported C API operations | 0 but no cell effect in supplied witness | Class-3 operations allowed by implementation capabilities | Command may be nested, but must not be synthesized inside a cell call to bypass restrictions |
| Lifecycle entry | Defined lifecycle capability profile | Registration and startup callbacks | 0/no cell effect in supplied witness | Permitted startup operations according to measured route | Not an ordinary cell signature |
| XLL calculation event | Must be measured | Must be measured | Must be measured | Do not assume command privileges | Event identity, trigger and order are separate from COM AppEvents |
| xlAutoFree12 | No | xlFree only | Forbidden | Forbidden | Same native thread, exact returned pointer |
| After64 or1 in active attempt ancestry | No further read/evaluation | xlFree only | Forbidden | Forbidden | Return naturally; sticky disposition cannot be caught away |
| Unrelated thread/outside a granted call | No | Supplied probes return 32 | 32 | No | xlAsyncReturn has its explicit token-based route |

The cell status values above are **supplied E/F measurements**. The command/worksheet distinction, free-only restrictions and async exception have **[public-doc]** support in [M01], [M07], [M11], [M27], [M28]. The event row is deliberately unknown.

**[inference] Make permission resolution table-driven with exact precedence.** For example, the supplied GetWorkspace/Volatile preamble returns 2 before the `$` check; replacing it with a global “thread-safe caller =>128” changes traces. Keep the measured MESSAGE exception. Preserve `Unchanged` result bytes for the cases that leave the result untouched, and the measured zeroed async output despite the older documentation's Bool description.

**[inference] Classify refusals separately from modeled denials.** `UnsupportedCapability` should remain visible and survive IFERROR as your policy requires. Expected Excel permission failures and ordinary Excel error values should not accidentally call the same `refuse()` path and poison an otherwise compatible invocation. Audit the early dispatcher returns against the `run()` error path for inconsistent classification.

### B.5 Exact status 64 protocol and its limits

**[public-doc]** After `xlretUncalced` 64 or `xlretAbort` 1, the add-in must return promptly and may call only xlFree meanwhile. A non-macro function can be entered again after its precedent calculates. [M01], [M04]

**[inference] Implement this sequence:**

```text
1. Prepare Q/B/E/etc. value arguments using argument readiness.
   Pending value input -> suspend BEFORE native entry; do not count an entry.

2. Enter ordinary/$ native procedure with a fresh attempt-scoped frame.
   U/R references remain references.

3. xlCoerce asks the invocation reader for values.
   Ready -> return the normal result.
   Valid but pending calculation -> register required producer(s),
       latch Uncalculated on this attempt and its native-call ancestry,
       return status 64 with the measured result-write behavior.
   Invalid reference/capability/other error -> its own measured result/status.

4. While latched, service xlFree; no further calculation/command callbacks.
   Do not throw or longjmp across the add-in/Rust/libffi stack.

5. When native code returns, do not commit its apparent result.
   Perform only the measured discard-result cleanup protocol.
   Do not deep-convert a discarded operand as if it were a successful value.

6. At the established evaluation boundary, abandon its volatility events,
   preserve already-performed irreversible native effects and trace entries,
   and schedule a fresh semantic invocation once the precedent is ready.

7. Fresh invocation gets fresh frame/attempt state and, if async, a fresh handle.
```

**[inference] `reevaluation_policy()` should authorize `XllUncalcedUnwind` for ordinary and `$` XLL functions.** Permission alone is insufficient: the read service must distinguish pending calculation from invalid reference, and the adapter must explicitly request reevaluation when returning64. A macro-equivalent registration uses the cached observation policy; the supplied cached-read witness does not imply that every invalid/cancelled/missing reference succeeds.

**[inference] Do not promise a universal total of two entries.** The one-blocked-read witness expects one rejected entry and a later successful entry. Formula entry, the first Calculate, multiple newly discovered precedents, nested UDFs, rebuild and cancellation can add other semantic entries. The proposed scheduler keeps ordinary entries on the owner lane and permits `$` retries on eligible calculation threads. Measure retry placement separately; the established same-thread guarantee covers an entry and its normal auto-free, not all retries.

**[inference] Native effects and evaluation events need different journals.** A local counter increment or external write before 64 already occurred. Do not undo it, hide it or replay unrelated completed native calls. Separately, supplied E.6 says abandoned evaluation volatility events are lost. Reconcile that with the existing prefix-effect mechanism using the actual P03/E.6 witness; retaining only the innermost function's events or restarting the entire formula blindly is not justified.

**[inference] Nested calls and async races need explicit experiments.** An inner64 must preserve the attempt's terminal reason, but the outer `xlUDF` callback's exact status/output cannot be guessed. An async handle must be valid before native entry because a completion may arrive immediately. Stage such completion until synchronous entry ends successfully; if that entry later gets64, revoke the handle and discard the staged value. This is the proposed admission design, not an already-measured Excel sequence. D.2/D.7 distinguish its alternatives.

### B.6 Volatility belongs to ordered evaluation events

**[inference] Keep declaration metadata immutable and compute default volatility at invocation.** Replace the parser's combined `volatile` flag with, for example:

```rust
struct VolatilityDefaults {
    explicit_bang: bool,
    reference_sensitive_macro: bool,
}

fn call_start_events(sig: &Signature, args: &[PreparedArgument], log: &mut EventLog) {
    if sig.explicit_bang {
        log.push(SetVolatile(true));
    }
    if sig.macro_equivalent && sig.reference_positions()
        .any(|i| args[i].is_actual_reference()) {
        log.push(SetVolatile(true));
    }
}
```

**[inference] An ordinary nonvolatile call emits no clearing event.** It must not erase earlier RAND/name/default/request events. Valid `xlfVolatile(false)` emits a false event; denied requests emit none. Reduce the completed evaluation's events in execution order, not with OR. Preserve the supplied rules for untaken branches, defined-name bodies, implicit scalar lifting versus MAP/BYROW, async `!`, file `ca` bits and volatile dependents. This needs the existing engine work item as well as the XLL adapter fix.

### B.7 Threading, reentry and liveness

**[public-doc]** `$` is permission to schedule concurrently, not an assertion that every invocation occurs on a non-main thread. Single-thread configuration runs formulas on the main thread; Excel can also use it for load balancing. [M29] Supplied F.3 additionally establishes main-thread formula-entry behavior.

**[inference] Keep long-lived OS workers, but stop treating their callback environment as an empty new host.** A private call arena and owned RPC messages are good. Empty registrations, names and per-call allocation stores are not a complete logical session. Expose immutable registration snapshots or owner-mediated queries, persistent allocation ownership, the correct caller/read view, and an explicit original-thread role.

**[inference] Do not equate a module's serialized lane with “cannot be reentered”.** Synchronous `xlUDF`, a legal command calling another command, and COM callback reentry can target the same module while its parent is active. They need a nested call stack and a per-operation reentry policy. Queueing a child behind its waiting parent deadlocks; rejecting every same-module child changes behavior. Preserve a typed refusal for genuinely unsupported reentry until the targeted fixture proves the allowed path.

**[inference] Never hold a mutable engine lock, Rust `RefCell` borrow or exclusive reference to a shared frame across foreign code or a message-pumping wait.** Nested dispatch may revisit it. `catch_unwind` can convert a Rust panic, but does not make aliasing violations, arbitrary native exceptions or access violations safe.

**[inference] Use a constrained wait/pump.** While the owner waits for a worker, it must service that invocation's callback requests. It must not arbitrarily run queued structural edits, unloads or unrelated Python events inside the active evaluation. If a Python binding retains the GIL while awaiting an owner-serviced callback, and that owner enters another handler requiring the GIL, both can block. This is a conditional risk, not a verified defect in xlOil 0.22.1's GIL guards. A deterministic barrier test is more useful than a four-thread throughput test here.

**[inference] Match a declared scheduling profile.** The current four-worker cap cannot reproduce a witness using sixteen distinct workers. Add a conformance thread-count setting, and include main/worker assignment, stable thread-equivalence relations and per-thread free ordering in comparison. Do not claim exact Excel-wide scheduling from one overlap witness. C.1 explains how to retain the strict ordered-trace requirement without mistaking nondeterministic Excel-to-Excel differences for deterministic contract facts.

### B.8 Process isolation

**[inference] Yes: add process isolation, but preserve an in-process conformance mode.** Make the unit a calculation session or a compatible add-in/runtime group, not each individual DLL. Keep xlOil core, plugins, Python and their factories together; likewise an Excel-DNA XLL, its payload and generated RTD/COM infrastructure. Split groups only where their interactions can be represented faithfully.

Start OneCalc with one native hosting surrogate per session. For DNA Calc, consider co-locating the calculation session and add-in host in that surrogate through application composition; that avoids turning every reference read into IPC. A later multi-runtime arrangement can use the same services remotely. Neither arrangement introduces an XLL dependency into `calc-engine`.

The surrogate must itself provide the main-image export, private shim, STA message pump, matching hidden window hierarchy, local COM projection, stable workers, allocation stores and module lifecycle. Providing the EXCEL7 window only in the UI process does not satisfy frameworks that enumerate their own process/thread windows. **[inference]**

| Boundary | What stays local / what crosses | Failure of a value-only RPC design |
|---|---|---|
| Native ABI | Pointers, handles, libffi entry, export snapshots and free callbacks stay local | Calling/freeing pointers in another process is invalid |
| References | Logical workbook/sheet/areas plus invocation/read-view identity cross if needed | Eagerly converting everything to values loses lazy reads, cached observations and64 |
| Callbacks | Bidirectional nested requests carrying parent ID, sequence and permission context | A single blocking request/reply cannot service a callback during the UDF |
| Caller | Tagged caller descriptor, translated to process-local operands | A synthetic A1 caller changes results and RTD identities |
| Async | Token includes surrogate incarnation, request ID and document generation | Restart can otherwise accept an old completion as a new request |
| RTD | COM server/update object/STA together; logical topic admission sent to document owner | Duplicate topics, wrong apartments and extra native entries |
| COM | Local facades hold stable logical object IDs; permitted document operations cross | Raw IDispatch pointer forwarding is invalid; arbitrary Office interface marshalling needs its own contract |
| UI | Explicit command/dialog/ribbon/task-pane bridge | HWND subclassing, modality, UI Automation and embedded ActiveX cannot be transparently reduced to data |

**[inference] A timeout cancels admission; it does not stop native code.** A crash/hang supervisor may terminate the surrogate, but must not silently replay an invocation that may have performed external side effects. Keep the committed document/edit journal or a recoverable checkpoint in the controller even if the calculation session runs in the surrogate. Otherwise the crash can destroy both computation and the only document copy. Restore committed edits independently of native invocation replay. In-process SEH can improve fault reporting; continuing normal operation after arbitrary AV is not a general recovery design.

**[inference] Your current lease condition is insufficient for physical unloading.** The last async completion can return to add-in code that still frees resources or exits its worker thread. “Late return observed” does not prove that no instruction pointer remains in the module. Queued Windows messages, timers, class factories, COM references, event subscriptions and plugin thunks are additional leases. For modules without a proven quiescence protocol, logical deactivation plus residency until process exit is the defensible policy. Native AOT already requires that residency under the documented runtime contract. [M12]

### B.9 COM contract and member order

**[inference] Complete ABI layout with honest per-member refusal is a workable partial contract.** It is unsafe to promise an IID whose methods have the wrong calling convention, parameters or out-parameter locations. Generate typed C++ entry points and dispatch adapters from one schema derived from permitted type-library metadata. Include IID, interface kind, slot, DISPID, invocation flags, parameter order, optional/byref/LCID details and ownership. Do not guess additional vtable slots from method names.

Retain the supplied measured `_Application` slots: Application7, Parent9, Run42, Version281 including LCID, Hwnd351. Compare those through a PIA, native `#import` client and late binding independently. The public Window and Range IIDs are dispatch-only; seven slots are not inherently an error. Their distinct internal interfaces must not be answered with the same vtable. **[public-doc]** [M31], [M32]; **[inference]** (supplied I).

**[public-doc]** COM requires stable interface support and one controlling IUnknown identity per object. Dispatch optional arguments, reverse positional argument order, property-put named arguments, EXCEPINFO and type information have their own contracts. Native OM acquisition on EXCEL7 returns a Window object. [M33], [M34], [M35], [M36]

**[inference] Preserve logical identity, not raw addresses across processes.** Workbook/Worksheet/Range facades should retain logical IDs and areas; never retain a borrowed evaluator context. A closed workbook can make operations fail without changing an existing object's QI identity. `Application.Caller`, ActiveCell and Selection are distinct. Your measured AppEvents argument ordering is a specific event invocation shape; do not “correct” it by applying the positional-reversal rule to named arguments indiscriminately.

| Priority | Exact member group to implement/audit | Purpose |
|---|---|---|
| COM0 | IUnknown, IDispatch, type information, connection-point identity; Window.Application/Parent; Application.Application/Parent/Version/Hwnd; Workbook.Application/Name/FullName/Path; Worksheet.Application/Parent/Name | Stable discovery and navigation; much already exists |
| COM1 | Application.Run by registered name/ID; COMAddIns.Update/Item/Count/_NewEnum; COMAddIn.Connect/ProgId/Guid/Description/Object; IDTExtensibility2 state transitions | Existing macro transport, xlOil context entry, actual COM add-in lifecycle |
| COM2 | Application.Caller/Range/Union/Intersect; Worksheet.Range/Cells; Range.Application/Worksheet/Parent/Address/Row/Column/Rows/Columns/Count/Areas/Item/_Default/Value2 getter; Workbooks/Worksheets Count/Item/_NewEnum; ActiveWorkbook/ActiveSheet/ActiveCell/Selection | Common UDF/reference use and the three current acceptance failures |
| COM3 | Range.Value2 setter, Value conversion policy, Formula/Formula2 get/set, ClearContents, Offset/Resize; Application/Worksheet.Evaluate; Calculation/CalculationState/Calculate/CalculateFull/CalculateFullRebuild; Names basics | Document-backed commands, evaluation and recalculation |
| COM4 | StatusBar/ScreenUpdating/DisplayAlerts; Workbooks.Open/Add, Workbook.Close; sheet mutation; OnKey; CommandBars actions; ribbon callbacks/invalidation; task panes | Broader workflows and UI integration |
| COM5 | Workbook.VBProject, VBComponents, CodeModule members actually used to create/update/remove wrappers, appropriate access/trust behavior | xlOil dynamic workbook-local VBA installation |

This order is **[inference]**, not a claim that all COM add-ins need exactly this subset. Native RTD and the command queue remain early priorities even if broader UI waits.

**[inference] OneCalc can implement COM0–2 through a narrow session/document-view trait without a dependency graph.** A Range's type identity, Count/Row/Address and a real invocation Caller can resolve the three supplied failures without implementing general workbook editing. Only expose values where the scratch session actually has values. Do not make missing cells zero or invent worksheets to satisfy arbitrary calls. DNA Calc supplies the document-capable implementation of COM3 onward through `calc-host`.

**[inference] Keep the no-shadowing rule for Excel registration.** Native OM is sufficient for the intended local discovery route. An add-in that explicitly activates `Excel.Application` is asking COM for that registered server; redirecting it globally would change unrelated applications. If DNA Calc later offers external automation, use its own explicit identity/moniker. A local registration-free strategy needs a separate compatibility test and must not silently hijack the installed Excel server.

**[inference] Preserve both activation paths used by frameworks:** direct `DllGetClassObject` and process-registered class factories. `COMAddIns.Update` must not merely enumerate matching registry strings and assume all class factories behave alike. Runtime connect/disconnect and application startup/shutdown have different callback modes; get exact order/SAFEARRAY arguments from the oracle. A COM add-in implementing IDTExtensibility2 is also not the same feature as an Automation add-in exposing worksheet functions.

**[inference] Add a no-Office-installed acceptance lane.** Correct local vtables do not automatically supply cross-apartment/process marshaling or type-information availability. Test NativeOM discovery, PIA/native typed calls and a second-apartment client using private metadata and an explicit marshaling design. Do not acquire an undeclared dependency on installed Excel's type library or proxy registration, and do not solve it by shadowing Excel's global registrations.

The supplied F.5 automatic Excel-DNA COM helper, `DllGetClassObject` activation in Excel and ribbon rendering remain **unmeasured** because the harness does not alter `AutomationSecurity`. Seven helper-property calls and XML retention are useful partial evidence, not end-to-end activation evidence. Preserve that harness policy and carry the activation gap explicitly. **[inference]**

### B.10 Lifecycle, persistence and accommodations

**[inference] Replace “wait until registrations stop changing” with incremental publication.** A quiet interval is not a reliable load-completion protocol, especially with Python imports, timers or queued macros. `xlAutoOpen` return determines its immediate activation outcome; subsequent valid registration events publish new callable generations. A UI can show that functions are still arriving. On failed startup, preserve the trace and apply measured partial-initialization cleanup rules. Until those are established, rollback of host registrations/names is an explicit product policy with a recorded divergence; initialization may already have exposed them to nested callbacks. Native initialization, COM factories and external effects cannot be assumed undone.

**[inference] Track activation reason.** Manager enable/disable, RegisterXLL, REGISTER(module), file open, programmatic unregister, reload and process shutdown are distinct transitions. Dispatch only the corresponding lifecycle exports, on the owner STA with origin context. Keep `xlAutoRegister12` conditional on the missing-type registration route. Keep manager-description queries independent of loading functions.

**[inference] Split persistence identities:**

- Formula-visible name, namespace/scope and `_xll.` representation.
- Persistent add-in/provider identity and optional user-approved path/hash policy.
- Ephemeral registration ID, export address, async token and session-generated RTD ProgID.

Do not serialize ephemeral register IDs or assume `RtdSrv_<guid>` is a stable function identity. Preserve an explicit RTD formula's ProgID/text exactly when that is what the workbook contains. Preserve workbook-local VBA stubs, macro project data and their scope separately from global XLL names. Your load-nothing-on-open session policy is a product policy; test and report it separately from Excel's add-in discovery behavior.

**[inference] Rebinding requires callable-generation invalidation.** On a newly loaded matching provider, dirty/recompile only the unresolved or stale call sites that depend on it according to the measured open/manual/recalc profile. Do not destructively replace a missing add-in formula with `#NAME?`; keep the formula and cached value so later rebinding is possible. Do not automatically recalculate on load if the relevant Excel Manual-mode witness does not.

**[inference] Keep framework-specific accommodations small and auditable.** Each needs: identifier; exact match predicate; binary/source version evidence; oracle trace; reason generic behavior is insufficient; changed observable behavior; affected tests and failure count; owner; removal criterion. A sha256-keyed xlOil refusal is preferable to “all Python cannot reload”. Process-name adaptation for old Excel-DNA is a separate compatibility profile. Neither should leak into the engine or become an unrecorded special case.

## C. Prioritized implementation plan and acceptance gates

All priorities and implementation sketches in this section are **[inference]**. Public constants/contracts are cited where introduced. A gate means the corresponding change is not declared compatible until the same binary agrees with Excel under H.3; a proposed oracle experiment is not a passing result.

### C.1 Establish a comparison profile before widening the surface

Keep the strict ordered trace as the headline. Add a manifest containing Excel build/channel, bitness, OS, locale, calculation mode, MTR configuration, entry versus recalc trigger, workbook and add-in hashes, framework/runtime configuration, load order, process-identity adaptation, open documents, and cancellation/input schedule. Preserve traced and untraced controls. Do not merge results from builds 20527 and 20150 into one unconditional contract.

| Must compare exactly within the declared profile | Normalize, while preserving relationships |
|---|---|
| Formula value, error code, binary64 bits where defined, string code units, array dimensions/elements | Pointer values, but preserve equality/aliasing and which pointer was returned/freed |
| Native entry count, order in deterministic witnesses, selected registration/export generation | Registration IDs, but preserve same/different identity, use counts and references to them |
| Callback number including command/special/international/prompt bits; count; operand base kinds, flags and meaningful bytes; status; output write/untouched behavior | Module paths, but preserve origin versus code/free-module relationships and bare/full-path lookup cases |
| Per-thread entry/callback/free sequence; owner-versus-worker placement; overlapping-call witnesses | Numeric thread IDs, but preserve same-thread relations, distinct workers and owner identity |
| COM IIDs, DISPIDs, invocation flags, named arguments, VARTYPE/byref shapes, HRESULTs and method order | Sheet/workbook handles, but preserve identity, area order and qualification |
| RTD method/order/initial values and native reentries; async admission/rejection; XLL events; lifecycle order | Wall-clock durations and asynchronous arrival latency, unless deliberately controlling their order |

A globally ordered MTR trace may vary between two Excel runs. Do not silently replace the user's strict contract with a partial-order comparator. First repeat Excel-to-Excel; then construct a deterministic single-thread or barrier-controlled witness for the disputed rule. Keep nondeterministic schedules visible as an unresolved profile dimension. A causal/per-thread view can explain failures without turning them into passes.

Report at least four counts: value cases; ordered boundary cases; lifecycle/ownership cases; framework feature lanes. Weighting by 289 native cases must not obscure 26 native RTD invocation failures. Classify expected Excel denials separately from unsupported-host refusals and intentional safety divergences.

### C.2 Delivery order

| Step | Concrete change and code boundary | Real use unblocked | Instruments and exact completion gate |
|---|---|---|---|
| **0. Preserve the boundary** | `calc-xll-abi` constants/layout checks; host-only `CallbackReply`; snapshot-before-conversion; context/origin trace. Audit result buffers initialized to sentinels. | Every framework; prevents apparently correct values from concealing ABI mismatches | H.1/H.2/H.3, D.0/D.3/D.4. All existing known-good traces unchanged; corrected traces agree in bytes/status/free relationships. |
| **1. Ownership and persistent call context** | Fix 16384 operand mutation; allocation registry outlives private worker State; `FreeOnly` frames; stack/origin restoration; isolated mutable input buffers. Correct cheap contract probes 16385 and 16391 after witnesses. | All ED/W/O returns, xlw pools, concurrent callbacks, safe nesting foundations | H.1–3, D.3/D.5/D.21/D.22. No missing/extra auto-free; exact thread and pointer relation; repeated same-operand free follows Excel mutation. |
| **2. Status64 and ordered volatility** | Typed reference readiness in engine-neutral seam; ordinary/`$` XLL reevaluation policy; attempt disposition; separate argument preparation. Replace static `#` volatility and sticky OR with B.6 events. | Correct reference UDFs and engine recalculation; avoids wrong native exceptions/reentries | H.1–4, existing P03/E banks plus D.1/D.2/D.6. Match entry count, callback64, cached `#` result and clean-F9 behavior under each controlled witness. |
| **3. Useful COM and command entry** | COM0/COM2 discovery/Caller/Range metadata, COM1 Run and lifecycle audit; preserve `>B` numeric-zero result; edit-aware LPenHelper; safe queued command turn. | Three OneCalc failures; ED QueueAsMacro; xlOil owner-context entry | H.1/H.3/H.5, D.11/D.15/D.16. Same late/early-bound call sequence and values; queue executes once at the observed safe point; no artificial workbook engine dependency. |
| **4. Exact RTD protocol** | One topic key/admission service for typed RTD and379; ConnectData byref/initial value; UpdateNotify/RefreshData/Heartbeat/disconnect order; avoid invented provisional value. | ED async/streams and xlOil Python's default async route | H.1/H.3/H.4, original ED failing corpus plus D.14. Require ordered IRtdServer and native-entry equality, not only value/certificate equality. |
| **5. Native async and calculation events** | Admit `>...X$`; handle valid before native entry; stage completion; batch16400; cancellation/revocation; 16401 dispatch with trigger origin; cooperative16390. | ED/O native async; cancellation cleanup | H.1–4, P07 plus D.7/D.8. Exact completion statuses/output, event order, native entry placement and token invalidation. Physical unload remains independently gated. |
| **6. Coercion, nested evaluation, names** | Full measured16386 type matrix/locale/ref forms; nested255/257; general88 name definitions/scopes;149 optional forms; callable generations and partial-startup behavior. | ED/W/O reference and composition features; dynamic Python registration | H.1–4, D.4/D.9/D.10/D.12/D.13. No generic32 replacing a modeled result/64; exact parent/child context and hidden-name behavior. |
| **7. Modern in-place/expanded ABI** | Separate logical/native argument plan; digit return; synchronous `>`; F%/G% result; O% expansion; full K% mutation/return handling. | Documented xlOil FPArray path and native performance UDFs | H.1–3, D.5/D.17. Exact argument count/layout, buffer identity, result extraction and next-call lifetime. |
| **8. Legacy C API** | Real Excel4/Excel4v, XLOPER/FP/ANSI types and xlAutoFree; route-specific conversion; O expansion; Excel4 callbacks sharing logical services without sharing raw layouts. | Actual legacy xlw/custom/commercial XLL paths; required for full-XLL claim | H.1–3, D.18 and captured legacy binaries. Pair modern/legacy semantic tests, but compare each to its own Excel boundary bytes. |
| **9. Document commands and information** | Implement the command subset below through a capability trait; COM3; GET.CELL185/GET.WORKSPACE186/GET.WORKBOOK268 items selected from actual traces; binary names16396/7. | Useful queued writes, workbook-aware UDFs, menus and command add-ins | H.1–4/H.5, D.9/D.16 plus shape tables. Test success, refusal, wrong context, undo/events and persistence. |
| **10. Lifecycle, saved binding and local VBA** | Complete route-specific exports, provider/name generations, `_xll.` persistence, missing/late-loaded add-ins, existing local stubs then VBIDE installation; COM4/5 by use case. | Reload/open workflows; xlOil workbook functions; broader COM | H.1/H.3–5, D.12/D.13/D.19/D.20. Correct counts/order before/after save/open, with Manual-mode controls and original macro project preserved. |
| **11. Ship supervised isolation** | Session/runtime-group surrogate; owner STA/windows/COM/shim; bidirectional invocation protocol; controller document checkpoint; crash/hang state and explicit restart. | Reliable OneCalc sessions, incompatible runtimes, logical reload of resident runtimes | H.3/H.5 with H.1-equivalent surrogate trace, D.19/D.21. Same compatible traces as in-process mode; deterministic crash containment and no unauthorized invocation replay. |

Design the surrogate boundary during steps 0–2, and start its implementation in parallel once the invocation/context model stabilizes. Step 11 is its release gate, not advice to leave native crashes untreated until all COM work is finished. Conversely, moving the current value-only contract into another process before fixing64 and callbacks would preserve its defects and add latency.

### C.3 Return-code and output-write rules

**[public-doc]** The callback return codes form the following published set. They are not worksheet error operands. [M01]

| Numeric code | Meaning | Implementation rule |
|---|---|---|
| 0 | `xlretSuccess` | Output may itself be an Excel error; do not classify it as a host failure. |
| 1 | `xlretAbort` | Latch aborted disposition; only xlFree thereafter; no committed native result. |
| 2 | `xlretInvXlfn` | Invalid/disallowed callback for the measured context, including supplied Volatile/Workspace non-`#` behavior. |
| 4 | `xlretInvCount` | Exact argument-count error where Excel returns it; distinguish API count from registration argument metadata. |
| 8 | `xlretInvXloper` | Invalid operand where measured/documented; do not substitute for an ordinary returned `#VALUE!`. |
| 16 | `xlretStackOvfl` | Stack/resource condition under the documented API; do not map every host resource refusal to it. |
| 32 | `xlretFailed` | General callback failure; not a catch-all for pending calculation or unsupported host capability. |
| 64 | `xlretUncalced` | B.5 attempt protocol, restricted cleanup and reevaluation. |
| 128 | `xlretNotThreadSafe` | Exact callback/thread-safe permission rule; preserve denial precedence. |
| 256 | `xlretInvAsynchronousContext` | Invalid native async context/handle; current host uses it for stale/duplicate/revoked tokens, requiring oracle coverage per cause. |
| 512 | `xlretNotClusterSafe` | Cluster-safety contract where applicable; not a local-host scheduling synonym. |

Store an exact `XlReturn` integer rather than OR-ing unrelated failures. Whether a failed callback leaves the output untouched, writes an error or zeros it belongs to its tested rule. Never let `catch_unwind` overwrite a terminal64/1 with32 or convert every successful callback into a freshly normalized CalcValue operand.

### C.4 Smallest useful command model

There are two independent minima. **Transport minimum:** registered command resolution, the existing `Application.Run(name, 0.0)` path, a real command frame, numeric-zero return for the measured void macro, safe-turn queueing, edit-state retry and origin restoration. **Work minimum:** the actual operations performed by that command. QueueAsMacro alone does not require every `xlc*` operation, but a queued edit cannot work until its write API exists.

Add an engine-neutral `CommandServices` capability alongside the read view, composed by `calc-addin-host`. OneCalc can provide its real session selection/status/command functions and bounded scratch edits; DNA Calc provides document operations through its application layer. No command implementation in `calc-xll-host` should import `calc-host`.

| Operation | Native shape to support first | Required state/effect and oracle gate |
|---|---|---|
| RUN32785 | `(macro-name-or-measured-id-form, arg...)` | Resolve actual command/UDF semantics without laundering a cell call into command privileges; preserve return kind. Compare to Application.Run independently. |
| SET16387 | `(single rectangular Ref/SRef[, value])` | Omitted value clears; implement documented scalar/array fill behavior and then measure edge shapes. Requires actual writable document/session service. [M09] |
| FORMULA32864 / FORMULA.ARRAY32866 | `(formula-text[, target-reference])` | Formula parsing, destination, array extent and calculation/event/undo effects; do not replace by a value write. |
| SELECT32877 | `(reference[, active-cell])` | Selection and active cell are separate state; required only where supported. |
| ON.KEY32936 | `(key-text[, macro-text])` | Install/remove key-to-command mapping with lifetime; verify omitted versus empty macro separately. |
| MESSAGE32890 / ECHO32909 | `(display[, text])` / `(enabled)` | Existing status/message exception and UI-update state; keep their different permissions. |
| ALERT32886 | `(message[, dialog-type, help-reference])` | Real dialog/action result through the UI bridge; no fabricated acknowledgement. Optional shapes need the oracle. |
| NEW32887 / WORKBOOK.INSERT33122 / FILE.CLOSE32912 | Start with exact argument vectors recorded from the framework discovery/command route | Create/insert/close real supported objects and restore active state. Do not implement guessed whole-command overloads from names. |
| Menu152/153/158/159; GET.BAR182/GET.TOOL259 | Start with the actual framework's recorded menu arrays and identifiers | Retain identity, ordering, action binding and deletion, then test activation. Successful construction followed by inert actions is partial support. |

For SET16387 specifically, the documented algorithm fills the rectangle for a scalar; repeats a horizontal source down or a vertical source right; pads an undersized two-dimensional source with `#N/A`; crops an oversized source; clears cells for Nil elements or an omitted entire value. It writes constants only and destroys undo history. Measure the array-formula/spill boundaries before integrating it with general edit/undo transactions. **[public-doc]** [M09], [M25]

The numeric command IDs are from permitted public Excel-DNA definitions [ED06]; table shapes not explicitly cited are proposed initial fixture shapes **[inference]**. Exact error/status/result behavior for optional and invalid forms remains a measurement task. `#` permission to inspect macro information is not permission to edit a worksheet. Test mutation in ordinary, `#`, `$` and true command frames independently.

### C.5 RTD and async algorithms to implement, not approximate

For RTD, retain one logical topic key `(case-normalized ProgID, server according to its measured rules, ordered topic strings after Excel-compatible coercion)`. Do not merge one topic string containing a separator with two topic strings, or conflate key identity with an individual cell's subscription. Initial `ConnectData` returns a VARIANT plus byref state: record and admit what Excel does before manufacturing any pending result. The original Excel-DNA signalling server needs its own trace; do not “fix” all servers by replacing `-2` with `1` globally. That difference can be a consequence of the wrong connection/admission state machine.

UpdateNotify schedules owner-STA work; RefreshData yields a batch whose VARIANT/SAFEARRAY bounds and topic identities must be decoded exactly. Admission and dirtying must not add an extra native entry merely to deliver a value that Excel supplies synchronously. Preserve connect-before-disconnect for the measured replacement case, but probe simultaneous/multi-topic replacement before generalizing. Heartbeat, completed-generator disconnect and remote-server refusal remain visible features.

For native async, use state such as:

```text
Created -> InSynchronousEntry -> Pending -> Admitted
                       |          |
                       +-----> Revoked

completion during entry: copy value, keep a staged result
successful synchronous exit: publish staged result or become Pending
64/abort/overwrite/cancel/reload: revoke admission, discard staged result
completion after revocation or admitted duplicate: current host256;
    confirm each cause in Excel, especially the new early-completion/64 case
callback return: preserve measured status/output-write protocol
module residency: independent native-execution/quiescence lease
```

Handle allocation, admission and native-execution lifetime are different concerns. Add sequence/incarnation fields internally without exposing a guessed interpretation of Excel's opaque BigData layout. Batch16400 must follow measured partial-success/all-or-nothing behavior; do not choose one by implementation convenience.

### C.6 Registration, persistence and lifecycle work packages

Use separate records for a native registration and a formula-visible name binding. `Unregister(id)` can remove an active registration while a hidden name remains, as supplied; clearing a name does not necessarily unload native code. A registration's numeric ID is session-local. Omitted display name, omitted type text, macro type 0/1/2, ordinal/string procedure, argument help, duplicate registration and invalid flag combinations each need a witness.

Publish callable generations on registration changes and connect them to the existing binder invalidation mechanism. A saved formula containing `_xll.Name` must survive missing-provider evaluation; load-later rebinding must not depend on a cached numeric register ID. Persist workbook-local names/stubs and explicit RTD formula text separately. Test whether loading dirties or recalculates a particular Manual-mode cell instead of assuming automatic reevaluation.

Make `Deactivate(reason)` distinct from `UnloadNativeWhenQuiescent`. Track native entries, free callbacks, event subscriptions, queued work, class factories, RTD servers, external COM objects and runtime residency. Run manager add/remove exports only for their measured routes; an ordinary process shutdown is a separate reason. A failed `xlAutoOpen` needs its own partial-startup trace before rollback can be called parity.

Do not change the harness's `AutomationSecurity` policy to obtain a passing COM activation test. Mark those lanes unmeasured and use allowed explicit probes where they can establish a narrower fact. Full activation/ribbon acceptance remains a separate gap.

## D. Discriminating experiments

Everything below is a **proposed experiment [inference]**, not an executed test. Expected outcomes are explicitly separated into supplied measurements, documented predictions and unknown alternatives. Run the same compiled fixture binary in Excel and the host. Keep the native baseline before adding a framework wrapper, because a wrapper can add flags, catch exceptions, perform frees or introduce its own RTD state.

### D.0 Common fixture conventions and recording

Use the fixture's independently maintained ABI definitions. The snippets use conventional C field names (`xltype`, `val.num`, `val.w`, `val.str`, `val.array`, `val.sref`) as aliases for those definitions; **they do not include or reproduce SDK header text**. The examples are Windows x64 C/C++ bodies to add to the existing fixture, not a claim that a standalone project has been compiled here. Rust equivalents can use the same types/registration strings.

One small harness adapter keeps subsequent bodies short:

```cpp
// Include the project's OWN ABI declarations and Excel12v import/forwarder.
#include "fixture_owned_abi.hpp"
#include <windows.h>
#include <cstdint>
#include <cstring>
#include <cwchar>
#include <initializer_list>
#include <vector>

using Op = XLOPER12;
#define EXPORT extern "C" __declspec(dllexport)

// Implement with the fixture's native-only log. Neither may call Excel/COM.
// Trace copies all 32 descriptor bytes. Deep=true additionally copies valid
// payload/code units/areas before any free. Deep=false never follows pointers.
void Mark(const char* label);
void Trace(const char* label, int rc, const Op* value, bool deep);

static Op Num(double v) { Op x{}; x.xltype=xltypeNum; x.val.num=v; return x; }
static Op Int(int32_t v) { Op x{}; x.xltype=xltypeInt; x.val.w=v; return x; }
static Op Bool(bool v) { Op x{}; x.xltype=xltypeBool; x.val.xbool=v?1:0; return x; }
static Op Missing() { Op x{}; x.xltype=xltypeMissing; return x; }
static Op Sentinel() { Op x; std::memset(&x,0xA5,sizeof x); return x; }
static Op* ReturnNumber(double v) {
    static thread_local Op x; x=Num(v); return &x;
}
struct Text {
    std::vector<wchar_t> units;
    Op x{};
    explicit Text(const wchar_t* s) {
        size_t n=std::wcslen(s); // literals only; NOT the tested input decoder
        units.resize(n+1); units[0]=static_cast<wchar_t>(n);
        std::memcpy(units.data()+1,s,n*sizeof(wchar_t));
        x.xltype=xltypeStr; x.val.str=units.data();
    }
};
static int Call(int fn, Op* result, std::initializer_list<Op*> args) {
    return Excel12v(fn,result,static_cast<int>(args.size()),
                    const_cast<Op**>(args.begin()));
}
static void FreeResult(Op& x) {
    int rc=Call(16384,nullptr,{&x});
    Trace("after-free",rc,&x,false);
}
```

Use a native log record containing process/thread ID, a monotonic sequence, invocation/attempt label where the fixture knows it, nesting depth, export, registration string and phase. H.1 supplies actual callback entry/exit and COM ABI details. Fill callback outputs with `A5` before calls and snapshot immediately afterward. Deep-decode only successful, valid operands; failed outputs can still contain the sentinel. For strings log length and every UTF-16 unit; for numeric operands log bits; for references log sheet identity, count and rectangles; for arrays log shape and each element kind. Record free-pointer equality and mutation separately from allocation counts.

Log BigData as an opaque descriptor by default. Only the explicitly owned binary-name test locks and reads its known data buffer; never follow an instance/async handle as arbitrary bytes or inspect executable-image contents. Snapshot functions must validate the base kind before following any pointer, including successful callbacks whose result is documented as absent.

Managed snippets use fixture helpers `NativeLog(string,string)` and invariant int/long overloads, `NewEntry(string)` for a native-only sequence ID, and externally signaled `AwaitRelease(string)` where specified. They make no Excel/COM calls. Place the methods in the existing public fixture class, keep helpers nonpublic, and reuse the pinned framework's working initialization. These names are harness requirements, not purported Excel-DNA APIs.

**Foundation inputs:** scalar1, -0, TRUE, missing argument, blank-cell reference, `#N/A`, text `a`, 2×3 array, same-sheet reference, other-sheet reference and two-area reference. Assert the B.2 offsets/strides in the fixture build and compare their observed bytes; an assertion against one's own definitions alone is not an oracle check. D.22 adds function-number flags and host probes.

### D.1 Status64, cached macro reads and pre-entry readiness

Register the same body three times: `ReadOrd`=`QU`, `ReadMacro`=`QU#`, `ReadSafe`=`QU$`, macro type 1. The body deliberately returns the callback status; the coerced value is in the native log.

```cpp
EXPORT Op* __stdcall ReadRef(Op* reference) {
    Mark("ReadRef-enter");
    Op r=Sentinel();
    int rc=Call(16386,&r,{reference});
    Trace("coerce",rc,&r,rc==0);
    if (rc==64 || rc==1) return ReturnNumber(rc); // no more Excel calls
    if (rc==0) FreeResult(r);
    return ReturnNumber(rc);
}
EXPORT double __stdcall Late(double x) { Mark("Late-enter"); return x; } // BB
EXPORT Op* __stdcall ReadValue(Op* value) { // QQ, and separately QQ$
    Mark("ReadValue-enter"); Trace("value-input",0,value,true);
    return ReturnNumber(0);
}
EXPORT Op* __stdcall ReadMixed(Op* reference, Op* value) { // QUQ / QUQ$
    Trace("prepared-value",0,value,true);
    return ReadRef(reference);
}
```

**Inputs and trigger.** Reuse the exact existing P03 reversed-area workbooks first. Add a small control: Manual mode, initially `A1=7`; `B1=ReadOrd(A1)`; then change `A1` to `=Late(11)` and calculate the two-area union with B1 listed before A1. Record formula-entry calls separately and reset native counters only after setup. If Excel resolves the dependency before entering ReadOrd and no64 occurs, that run is a readiness control, **not evidence against64**; use the already established P03 pending-read witness for the discriminating case. Repeat ReadMacro/ReadSafe and one-thread/two-worker settings. Run ReadValue(A1), ReadMixed(A1,A1) and the corresponding B/E value-typed variants separately.

**Predictions.** Supplied F.3 predicts64 and a later fresh entry for an ordinary/`$` in-body pending read; `#` succeeds with the cached value. Current host R1 predicts32 in that native callback even if the evaluator subsequently retries. A fully prepared value parameter predicts no native entry until ready; a mixed U+Q call can make the shared precedent ready before the U read. Record actual total entries and retry thread placement; do not assert a universal total of two across setup/recalc triggers.

For framework behavior, build the following Excel-DNA pair with `[ExcelArgument(AllowReference=true)]`, ordinary and `IsThreadSafe=true` variants; capture the emitted strings, expected `QU`/`QU$` for object return/reference argument:

```csharp
[ExcelFunction(Name="ED_TRY_READ")]
public static object TryRead([ExcelArgument(AllowReference=true)] object r)
{
    object value;
    var rc = XlCall.TryExcel(XlCall.xlCoerce, out value, r);
    NativeLog("TryExcel", (int)rc); // fixture logger, no Excel/COM
    return rc == XlCall.XlReturn.XlReturnSuccess ? value : (object)(int)rc;
}
[ExcelFunction(Name="ED_THROW_READ")]
public static object ThrowRead([ExcelArgument(AllowReference=true)] object r)
{
    try { return XlCall.Excel(XlCall.xlCoerce, r); }
    catch (XlCallException e) { NativeLog("caught", (int)e.xlReturn); return -999.0; }
}
```

**[public-doc]** Excel-DNA's throwing/nonthrowing APIs differ as documented [ED01]; the public exception field is `xlReturn` [ED06]. The native tracer remains authoritative for callback64 and cleanup added by the wrapper. **[inference]** If catching the managed exception lets the host commit -999, attempt terminal state is wrong. Repeat equivalent conversion paths in the exact xlw and xlOil builds; do not assume a particular C++ wrapper throws rather than returning an error. For xlOil Python set `macro=False` explicitly for this witness, because the documented default macro path intentionally takes a different read policy.

### D.2 Sticky unwind, nested64 and discarded-result cleanup

Use D.1's pending input. Register this as `QU`, and separately `QU$`:

```cpp
EXPORT Op* __stdcall ReturnOwnedAfterRead(Op* reference) {
    Op r=Sentinel(); int rc=Call(16386,&r,{reference});
    Trace("coerce-owned",rc,&r,rc==0);
    if (rc==0) FreeResult(r);
    Op* p=new Op(Num(777));
    p->xltype |= xlbitDLLFree;
    Mark(rc==64 ? "return-owned-after64" : "return-owned-normal");
    return p;
}
// Use a separate fixture module whose returned pointers all use this allocator.
EXPORT void __stdcall xlAutoFree12(Op* p) {
    Trace("AutoFree",0,p,true);
    delete p;
}
```

**Alternatives to discriminate:** Excel ignores the returned value but still invokes auto-free; ignores it without auto-free; or applies another cleanup sequence. The supplied evidence does not decide this. Normal ready-input controls establish the module's valid DLLFree path first. Do not choose a cleanup policy from the ready-input result alone.

Register a parent `QU` body that calls255 with `(Text("ReadOrd"), reference)`, logs the outer status/output and returns a scalar. This distinguishes an inner64 propagated directly by255 from a different outer status with the same abandoned evaluation. Keep a native effect counter before the child; compare prefix-effect preservation and retries.

The managed catch case in D.1 is the safe sticky-state witness. A deliberately nonconforming variant that calls another worksheet API after 64 may be run **only as a separate disposable-process contract-boundary test**, with the violation explicit. Its undefined/unsupported behavior must not become the specification for compliant add-ins. No production test needs that violation to prove that the host must not commit a caught-away64.

### D.3 Raw ownership flags, xlFree mutation and return cleanup

Register `RawOwned`=`QU#` (or `QU` with already-ready reference). Test each callback in a separate run to isolate its bytes:

```cpp
EXPORT Op* __stdcall RawOwned(Op* reference) {
    Op r=Sentinel(); int rc=Call(16386,&r,{reference});
    Trace("raw-before-wrapper",rc,&r,rc==0);
    if (rc==0) {
        FreeResult(r);
        FreeResult(r);  // same descriptor after documented pointer nulling
    }
    return ReturnNumber(rc);
}
EXPORT Op* __stdcall ReturnHostOwned() { // Q, non-command
    static thread_local Op r;
    r=Sentinel(); int rc=Call(16393,&r,{}); // xlGetName, not xlfGetName107
    Trace("GetName-before-flag",rc,&r,rc==0);
    if (rc!=0) return ReturnNumber(rc);
    r.xltype |= xlbitXLFree;
    return &r;
}
```

**Inputs.** Coerce a text cell, 2×3 array and a reference table; substitute89 and16393 in separate raw-return probes. For Caller include a reference form that allocates auxiliary storage if Excel produces one. Run before any framework RAII wrapper.

**Predictions.** “Excel always marks owned callback memory XLFree” predicts that bit in every relevant raw result; “the wrapper adds it / ownership is API-contract based” predicts unflagged results on at least some routes. Do not assume which route sets it. The documented xlFree rule predicts its contained pointer is null after the first call, with unrelated bytes retained; current copied-operand removal predicts an unchanged pointer. A second call on the *same mutated operand* tests the documented safe path; a stale shallow alias is a different, invalid-lifetime question.

Add normal DLLFree numeric and string returns in a separate fixture allocator; run `Q` and `Q$` with two overlapping calls. Record exact pointer, flags at auto-free, calling/freeing thread and order before the next entry. A third module returns TLS/pool memory without free bits and poisons it at its next entry: the earlier cell result must already have been copied. Finally retain one successful callback allocation across two entries on the same persistent worker and free it on the second valid entry; determine its actual supported lifetime and expose a per-call-State lifetime bug if present. Do not retain an argument buffer as though it were callback-owned memory.

### D.4 Coercion matrix and reference identity

Register `CoerceMask`=`QUQ#`, then ordinary and `$` variants with ready inputs:

```cpp
EXPORT Op* __stdcall CoerceMask(Op* source, Op* targetMask) {
    Op r=Sentinel(); int rc=Call(16386,&r,{source,targetMask});
    Trace("mask-source",0,source,true);
    Trace("mask-result",rc,&r,rc==0);
    if (rc==0) FreeResult(r);
    return ReturnNumber(rc);
}
EXPORT Op* __stdcall CoerceIntMask(Op* source, int32_t mask) { // QUJ#
    Op target=Int(mask), r=Sentinel();
    int rc=Call(16386,&r,{source,&target});
    Trace("int-mask-result",rc,&r,rc==0);
    if (rc==0) FreeResult(r);
    return ReturnNumber(rc);
}
```

**Inputs.** Target operands must include native Int masks, not only worksheet numeric operands: Num/Str/Bool/Int/Ref/SRef/Multi, zero, combinations such as Num|Str, and masks containing a free flag. CoerceIntMask constructs that Int inside its normally invoked registered frame. For arbitrary source/target kinds use D.9's case-driven constructor inside the same ordinary/`#`/`$` context; do not rely on worksheet Q/Multi preparation to preserve an arbitrary nested Ref or Int. Source cases: literal text `1,5` and `1.5`, blank reference, explicit Missing/Nil/Err, Int1 versus Num1, mixed array, same-sheet SRef, other-sheet Ref and ordered two-area Ref. Repeat under en-US and a decimal-comma locale with settings recorded. Include one-argument Coerce and omitted versus Missing second argument.

**Competing predictions.** Fixed en-US parsing differs on at least one locale-sensitive text input; target-bit-mask validation may reject or select differently from equality testing; returning an SRef unchanged for targetRef leaves the wrong base kind/sheet qualification if Excel converts it. Exact statuses and output-write behavior for invalid masks are unknown until measured. A successful error-valued operand must remain distinct from a failed callback. [M08]

### D.5 Strings, arrays, integer types and input aliasing

For counted strings register `CountedEcho`=`QQ`; for null-terminated input register a separate `WideLength`=`BC%`:

```cpp
EXPORT Op* __stdcall CountedEcho(Op* value) {
    Trace("echo-input",0,value,true);
    return value; // borrowed input returned; no ownership flags added
}
EXPORT double __stdcall WideLength(const wchar_t* value) {
    Mark("wide-enter"); return static_cast<double>(std::wcslen(value));
}
EXPORT Op* __stdcall CountedText(const wchar_t* value) { // QD%, separate ABI
    static thread_local Op r; r=Op{}; r.xltype=xltypeStr;
    r.val.str=const_cast<wchar_t*>(value); // borrowed through result copying
    return &r;
}
```

**Inputs.** H.2 constructs counted units `[3, 'A', 0, 'B']`, lengths 0/1/32767, a surrogate pair crossing a length boundary, and isolated surrogate units. Construct special operands within a valid registered fixture call where formula entry cannot express them. Compare QQ, the separately prototyped QD% CountedText, C% and F%/G% in D.17; never register CountedEcho(Op*) as QD%. Test illegal32768-unit return separately with an allocated descriptor/buffer; keep it out of the ordinary corpus if Excel's behavior is unstable. Record whether truncation/refusal occurs at registration marshaling, native return conversion or worksheet admission.

To exercise input marshaling, have a registered Q-result producer return the selected counted string and evaluate `CountedEcho(Producer(caseId))` / `CountedText(Producer(caseId))`, or invoke the target via255 from a registered command with that operand. A direct C call to the target only tests its body and cannot establish Excel's argument preparation.

Run a 2×3 mixed array containing Nil, Num, Int, Bool, Err and embedded-NUL Str. Native input types/flags and result types matter even where displayed values coincide. Test integer registration boundaries A/H/I/J and pointer L/M/N with -1,0,1, fractional values and range limits; retain the supplied CALL-versus-worksheet conversion distinction. Test return NaN/infinity/signed-zero bit patterns through B, Q and K% separately; do not generalize one route's normalization.

**Predictions.** A `wcslen` implementation loses B in the counted string; counting Unicode scalars instead of UTF-16 units produces a different length; normalization through CalcValue can erase Int/Nil distinctions. Echoing a borrowed operand tests whether the host copies before releasing input storage. Add an xlOil-style input-array mutation that swaps two existing rows without reallocating; its effect must remain in the call buffer and must not corrupt another invocation's shared engine value. The exact worksheet result/alias behavior is an oracle result, not a license for arbitrary native memory mutation.

### D.6 Dynamic default volatility and ordered requests

Register `VolRef`=`QUB#`, `VolBang`=`QUB!#`, and a literal-input control with the same declared U position:

```cpp
EXPORT Op* __stdcall VolRef(Op* argument, double request) {
    Mark("VolRef-enter"); Trace("actual-U-kind",0,argument,true);
    if (request!=2.0) { // 2 means emit no explicit request
        Op flag=Bool(request!=0.0), r=Sentinel();
        int rc=Call(237,&r,{&flag}); Trace("volatile",rc,&r,rc==0);
        if (rc==0) FreeResult(r);
    }
    return ReturnNumber(1);
}
```

**Inputs.** `VolRef(1,2)` versus `VolRef(A1,2)`; true/false requests; `VolBang(A1,0)`; expressions ordering a volatile primitive and a clearing request in both directions; untaken IF branches; the existing P5/p5/p6 name bodies; implicit scalar lifting versus MAP/BYROW. Use the exact E-bank formulas where their evaluation order is already established. Count calls after initial setup, clean F9, dirty dependency, full calculate and saved/reopened `ca` bits.

**Predictions from supplied E.** Static declared-reference volatility incorrectly marks the literal call; sticky OR cannot clear a preceding true event; clearing all state on every ordinary call incorrectly erases earlier volatility. Explicit last request wins in the completed evaluation. Add D.1's pending-read path after a volatility request to check that abandoned evaluation events disappear while native log increments remain. Native async `!` must use the same call-start event model, not a permanent post-completion flag.

### D.7 Native async: immediate, delayed, stale and batched completion

Register `NativeNow`=`>EX`, then `>EX$` under another name. This explicitly tests pointer-double input E, unlike the managed B example below.

```cpp
EXPORT void __stdcall NativeNow(double* input, Op* handle) {
    Mark("async-sync-enter"); Trace("handle",0,handle,false);
    Op value=Num(*input+1), r=Sentinel();
    int rc=Call(16400,&r,{handle,&value});
    Trace("immediate-completion",rc,&r,false);
    Mark("async-sync-leave");
}
struct Job { Op handle; double input; HANDLE release; };
HANDLE TrialReleaseEvent(); // fixture/controller-owned, unique to trial
void RetainThreadUntilExit(HANDLE thread); // NONBLOCKING lifetime registration
static DWORD WINAPI CompleteLater(void* raw) {
    Job* j=static_cast<Job*>(raw);
    DWORD wait=WaitForSingleObject(j->release,30000);
    if (wait!=WAIT_OBJECT_0) { Mark("INVALID-trial-timeout"); delete j; return 1; }
    Op value=Num(j->input+1), r=Sentinel();
    int rc=Call(16400,&r,{&j->handle,&value});
    Trace("held-completion",rc,&r,false);
    Mark("after-callback-native-code");
    delete j; return 0;
}
EXPORT void __stdcall NativeHeld(double* input, Op* handle) { // >EX / >EX$
    Job* j=new Job{*handle,*input,TrialReleaseEvent()};
    HANDLE t=CreateThread(nullptr,0,CompleteLater,j,0,nullptr);
    if (!t) { delete j; Mark("INVALID-thread-creation"); return; }
    RetainThreadUntilExit(t);
}
```

**Inputs/runs.** Input7; immediate completion; release after synchronous native return; user Esc while held followed by release; overwrite/undo/reload invalidation where permitted; duplicate completion with the same captured handle; one-thread and two-worker settings. An external controller signals the event—do not require Excel to run a macro while native async calculation is pending. Keep module residency until the fixture worker actually exits.

The timeout/thread-creation branches mark an invalid trial and can leave its async calculation pending. The controller must terminate/recycle that disposable test process; ending the worker alone is not cleanup. `RetainThreadUntilExit` stores the handle for later observation and must not join synchronously inside NativeHeld.

**Predictions.** Supplied F.3 establishes the accepted completion's zeroed32-byte output in Excel. Supplied S12 describes the host's256 for stale/duplicate/revoked tokens; D.7 must establish Excel's exact rejection status/output for each cause, especially early-completion followed by64. A host that creates the handle only after native entry returns risks rejecting the immediate call. A host that refuses `$` async fails the documented registration/invocation feature. Exact pre-return admission ordering is a new observation, not established by the final value8.

For batching, capture two valid handles from two held entries and call16400 once with a 1×2 Multi of handles and a matching 1×2 Multi of values, then repeat with one revoked handle and finally a duplicate handle. Keep all arrays alive until the callback returns. **[public-doc]** Batched handles/values are supported [M11]. **[inference]** Record whether the mixed batch rejects entirely, accepts valid elements before/after an error, or reports another result. No supplied evidence selects an atomicity policy. Return-data memory remains owned by the add-in; do not infer an ordinary UDF auto-free callback for it.

Add a combined edge case: complete an ordinary async invocation immediately, then Coerce a pending U reference and return as soon as64 is observed. Compare final admission, returned status of the early completion, handle identity on retry and event order. This decides the staging/revocation proposal in C.5; it has no asserted Excel outcome.

The managed counterpart is deliberately a native-handle function, not a Task/RTD registration:

```csharp
[ExcelFunction(Name="ED_NATIVE_NOW", IsThreadSafe=true)]
public static void NativeNow(double x, ExcelAsyncHandle handle)
{
    bool accepted = handle.SetResult(x + 1);
    NativeLog("SetResult", accepted ? 1 : 0);
    NativeLog("sync-return", 0);
}
```

Capture the actual registration; simple expected shape is `>BX$`, with an ordinary `>BX` control. The returned bool does not uniquely identify raw256; current framework code maps multiple callback failures to false. **[public-code]** [ED10] The tracer must retain the integer status.

### D.8 Calculation events, cooperative abort and trigger origin

In `xlAutoOpen`, register these exports with16401 and the fixture's independent `xleventCalculationEnded` / `xleventCalculationCanceled` constants (expected numeric1/2; retain the actual operands):

```cpp
EXPORT void __stdcall CalcEnded() { Mark("XLL-ended"); }
EXPORT void __stdcall CalcCanceled() { Mark("XLL-canceled"); }
static void RegisterEvents() {
    Text ended(L"CalcEnded"), canceled(L"CalcCanceled");
    Op e=Int(1), c=Int(2), r=Sentinel();
    int rc=Call(16401,&r,{&ended.x,&e}); Trace("reg-ended",rc,&r,rc==0);
    if (rc==64 || rc==1) return;
    if (rc==0) FreeResult(r);
    r=Sentinel(); rc=Call(16401,&r,{&canceled.x,&c});
    Trace("reg-canceled",rc,&r,rc==0); if (rc==0) FreeResult(r);
}
```

**Runs.** UI F9/full calculate with a synchronous function; UI F9 with held native async; Esc during it; COM `Application.Calculate`/`CalculateFull`; VBA Calculate; formula entry; Manual-mode dirtying without calculation. Record each trigger rather than calling all “recalc”. Use native-only event handlers first, then in separate trials probe89,16385,237 and a write from the handler to establish its permissions. Do not assume an event is a command frame.

**Documented prediction:** cancel precedes ended immediately, and programmatic calculation does not raise these XLL events. [M05], [M06] **Competing modern-build hypothesis:** a particular trigger/build behaves differently; keep its observed profile instead of forcing the documentation. Current host R2 predicts no event calls for every trigger. Record XLL and COM SheetCalculate/AfterCalculate on the same timeline, without treating them as equivalent subscriptions.

For cooperative abort, add an ordinary `B` no-argument function that loops native work and periodically calls16390 with BoolTRUE (retain abort state), logs the result, and stops when true. Run a separate BoolFALSE variant to measure clearing behavior. After any callback status 1, only native logging and return are allowed. Contrast an abort BoolTRUE at status 0 with status 1; always-false host behavior cannot reproduce the former. Record timing only to locate order, not as an exact latency contract. [M10]

### D.9 Permissions, result sentinels and denial precedence

Reuse the fixture's arbitrary-callback support or add this case-driven entry, registered `QJ`, `QJ#`, `QJ$` and command macro type 2 `QJ`. Excel invokes the export normally with an integer case ID. `BuildFixtureCase` constructs the table's exact raw operand vector **inside that registered frame**, retaining its buffers until return; it uses native fixture configuration and makes no Excel/COM calls. Its interface supplies `number`, `arguments` (a vector of Op pointers) and `free_result` (callback-specific cleanup policy). Do not send nested Ref operands through an ordinary Q/Multi worksheet argument, or directly invoke the export from the controller and claim to have tested a worksheet permission context.

```cpp
EXPORT Op* __stdcall ApiProbe(int32_t caseId) {
    auto request=BuildFixtureCase(caseId); // owned buffers, native-only teardown
    Op r=Sentinel();
    int rc=Excel12v(request.number,&r,static_cast<int>(request.arguments.size()),
                   request.arguments.data());
    Trace("permission",rc,&r,rc==0);
    if (rc==0 && request.free_result) FreeResult(r);
    return ReturnNumber(rc); // no callback after64/1
}
```

| Callback | Operand vector for one fresh trial |
|---|---|
| 16385 Stack | `[]` |
| 16386 Coerce | `[ready reference]` |
| 89 Caller | `[]` |
| 237 Volatile | `[BoolFALSE]` |
| 186 Workspace | `[Int2]` |
| 32890 Message | `[BoolTRUE, Str"probe"]` |
| 257 Evaluate | `[Str"1+1"]`, then `[Str"SafeChild(2)"]` |
| 255 UDF | `[Str"SafeChild", Num2]`, where SafeChild=`BB$` |
| 379 RTD | `[controlled ProgID, Str"", Str"key"]` |
| 149 Register | Valid D.12 seven-operand vector with a unique name |
| 201 Unregister | `[ID of a different disposable registration]` |
| 88 SetName | `[disposable name]`, then a separate define-name vector |
| 16401 EventRegister | `[Str"CalcEnded", Int1]` |

Do not unregister the running probe. Each mutating test gets a fresh fixture registration/session; its cleanup belongs to the recorded lifecycle. Repeat appropriate probes from lifecycle, event, auto-free and an unrelated native thread only under their documented constraints. For auto-free and after 64, the compliant baseline uses xlFree only; forbidden-operation tests are separately labeled contract-boundary trials.

Set `free_result=true` for the valid callback-result cleanup cases. Keep it false for baseline16403 borrowed instance identity and16400 zeroed completion output; GetBinaryName 16397 has its separate owned-handle cleanup. A deliberately separate “xlFree on a borrowed instance result” trial can observe descriptor behavior without treating that module handle as a releasable global allocation.

**Predictions.** Supplied ordinary237/186 gives2; `#` gives0; `$` 237/186 remains2 under the measured precedence. MESSAGE gives ordinary2/`#` 0/`$` 128. Literal Evaluate ordinary/`#` succeeds with2 and `$` returns 128. The documented MTR discussion permits some safe nested/evaluated operations [M29]; SafeChild versus unsafe child discriminates that broader contract from the supplied blanket128 implementation. Do not relax the existing39/39 profile without a witness. Outside-frame32 is the supplied baseline; native async completion has its explicit16400 exception. Invalid count/operand permutations and output sentinel behavior complete the rule, rather than inferring all failures from function name alone.

### D.10 Nested calls, origin restoration and general EVALUATE

Register the parent as `QQ`/`QQ#` and SafeChild as `QQ$`; also register an ordinary child and a child in another module. Use ready values first:

```cpp
static void RecordOrigin(const char* label) {
    Op r=Sentinel(); int rc=Call(16393,&r,{});
    Trace(label,rc,&r,rc==0); if (rc==0) FreeResult(r);
}
EXPORT Op* __stdcall Child(Op* value) {
    Mark("child-enter"); RecordOrigin("child-origin");
    Trace("child-arg",0,value,true); return ReturnNumber(12);
}
EXPORT Op* __stdcall Parent(Op* value) {
    Mark("parent-enter"); RecordOrigin("parent-before");
    Text child(L"Child"); Op r=Sentinel();
    int rc=Call(255,&r,{&child.x,value}); Trace("nested-return",rc,&r,rc==0);
    if (rc==64 || rc==1) return ReturnNumber(rc);
    if (rc==0) FreeResult(r);
    RecordOrigin("parent-after"); return ReturnNumber(rc);
}
```

**Inputs.** `Parent(5)`; same-module versus other-module child; child registered through an origin XLL but whose code/free module differs. Separately Evaluate `1+1`, `A1`, `MyLocalName`, `'Book2.xlsx'!MyName` in the actual supported syntax, `Child(5)` and a name containing a volatile function. Execute on a non-active sheet with distinct local/global name values. Use exact known-good textual forms from H.4 rather than treating syntax failures as evaluation failures.

**Predictions.** A blanket same-lane refusal prevents a child Excel permits. A single unstacked current-module pointer makes parent-after report the child. A value-only evaluator can erase reference kind or caller scope. A second hidden evaluator pass changes child counter/volatility. A pending-read child is D.2's separate test because its outer255 result is unmeasured. Record every child entry and free; a matching final0 and traced child result 12 alone are insufficient.

### D.11 Caller shapes and active-sheet independence

Register this body `Q`, `Q#`, `Q$`; run a separately named macro-type 2 version through Application.Run:

```cpp
EXPORT Op* __stdcall CallerRaw() {
    Op r=Sentinel(); int rc=Call(89,&r,{});
    Trace("caller-raw",rc,&r,rc==0);
    if (rc==0) FreeResult(r);
    return ReturnNumber(rc);
}
```

**Inputs.** Single cell Sheet2!D3; legacy CSE formula over D5:E6; dynamic-spill producer and downstream call; a defined name calling CallerRaw; command invoked by Run; selected Sheet1 while evaluating Sheet2. Add conditional-format/data-validation/chart routes only where the service can install a valid caller. Their precise forms are not established here.

**Predictions.** Supplied single-cell and command controls should retain SRef and error23 respectively. Published caller behavior distinguishes array-formula and non-cell routes [M24]; an always-one-cell implementation fails the CSE witness. Do not infer native call count from the number of displayed CSE outputs or assume a spill's caller rectangle equals its final spill extent. Preserve caller identity through nesting and worker RPC.

COM/VBA control:

```vb
Public Function CallerShape() As String
    Dim r As Range
    If TypeName(Application.Caller) = "Range" Then
        Set r = Application.Caller
        CallerShape = r.Parent.Name & "!" & _
            r.Address(True, True, xlA1, False) & ";" & CStr(r.Count)
    Else
        CallerShape = TypeName(Application.Caller)
    End If
End Function
Public Sub RunCallerShape()
    Debug.Print Application.Run("CallerShape")
End Sub
```

For a CSE D5:E6 control, the documented COM prediction is one caller Range of count 4, not four independent Caller objects. [M37] The exact VARIANT error for Run is established by H.1, and must not be inferred from a managed wrapper's representation.

### D.12 Registration forms, hidden names and use counts

Add `EXPORT double __stdcall RegTarget(double x) { return x+100; }`, type `BB`, using D.0's unmangled export macro. A registered command performs this valid baseline (the command may be type `A` and return short1):

```cpp
static Op RegisterAlias(const wchar_t* visibleName, double macroType) {
    Op module=Sentinel(), out=Sentinel();
    int rc=Call(16393,&module,{});
    Trace("register-module",rc,&module,rc==0);
    if (rc!=0) return out;
    Text proc(L"RegTarget"), type(L"BB"), name(visibleName);
    Text args(L"x"), category(L"Probe"); Op mt=Num(macroType);
    rc=Call(149,&out,{&module,&proc.x,&type.x,&name.x,&args.x,&mt,&category.x});
    Trace("register",rc,&out,rc==0); FreeResult(module);
    return out; // successful registration ID is retained by the command
}
```

**Runs and hypotheses.** Register the exact vector twice, then unregister the captured ID once and twice; after each step evaluate the hidden name with257 and call it with255/formula syntax. This distinguishes ID use counts from name lifetime. Clear only the hidden name with88, re-register, and compare the new callable generation. Supplied behavior predicts identical duplicate IDs/use counts and a hidden-name distinction; additional cases remain unknown.

Vary one field at a time: omitted function name; Missing name; omitted type text versus Missing; macro type 0/1/2; same export under another name; same name with another export; actual exported ordinal; fractional/invalid ordinal; metadata/help arrays; `$#`, `#&`, `X&`, duplicateX, valid `$&`; maximum argument count with a matching generated prototype. **Do not invoke an intentionally mismatched native prototype.** Capture registration status, result operand and all calls to AutoRegister. Module-only Form2 should target a separate disposable XLL from the command/controller, avoiding an accidental recursive AutoOpen of the currently initializing module.

For omitted-type testing, `xlAutoRegister12` logs its input and invokes RegisterAlias for the known RegTarget prototype, returning that ID. This makes the successful automatic-registration path observable; a handler that only returns zero would test failure, not the complete route. Correct flag validation is documented, but the exact invalid-registration statuses/outputs must be obtained from Excel. [M04], [M26]

### D.13 Lifecycle reasons, late registration and partial startup

Use separate fixture builds for normal startup and deliberate startup failure:

```cpp
EXPORT int __stdcall xlAutoOpen() {
    Mark("AutoOpen");
    Op id=RegisterAlias(L"LifeTarget",1);
    // Optional: call LifeTarget here and define a disposable hidden name.
    // Trial config selects success1 or failure0 after those observable effects.
    return TrialStartupSucceeds() ? 1 : 0;
}
EXPORT int __stdcall xlAutoAdd() { Mark("AutoAdd"); return 1; }
EXPORT int __stdcall xlAutoRemove() { Mark("AutoRemove"); return 1; }
EXPORT int __stdcall xlAutoClose() { Mark("AutoClose"); return 1; }
EXPORT Op* __stdcall xlAddInManagerInfo12(Op* action) {
    Trace("ManagerInfo",0,action,true);
    static Text description(L"Lifecycle probe");
    static Op error; error=Op{}; error.xltype=xltypeErr; error.val.err=15;
    Op target=Int(xltypeNum), value=Sentinel();
    int rc=Call(16386,&value,{action,&target});
    Trace("ManagerInfo-action",rc,&value,rc==0);
    bool wantsInfo=(rc==0 && value.xltype==xltypeNum && value.val.num==1);
    if (rc==0) FreeResult(value);
    return wantsInfo ? &description.x : &error;
}
```

`TrialStartupSucceeds()` is a native fixture configuration, fixed before load. ManagerInfo returns a description only for an action coercible to1, otherwise #VALUE15, as documented [M41]; use a base-kind check after stripping ownership flags if the scalar coercion witness requires it. Add the D.12 AutoRegister handler and a late-register command released by the controller after AutoOpen has returned.

**Routes.** Add-in Manager list/enable/disable; RegisterXLL; opening an XLL; REGISTER(module); UNREGISTER(module); file close where applicable; logical reload; process shutdown. Run each in a fresh process before testing repeated transitions. Record export order, caller context, COM mode arguments, remaining registrations/names and actual native residency.

**Predictions.** Docs distinguish manager add/remove from ordinary session startup/shutdown [M18], [M19]. The exact ordering for each route comes from the oracle. A blanket Add/Remove/Close sequence fails at least those distinctions. A quiet-window loader can miss the controller-gated registration; incremental publication exposes it without declaring a second load. The startup-failure variant discriminates full host rollback from retained partial registrations/names and specific cleanup calls. Do not assert that Excel erases all earlier effects merely because AutoOpen returns 0. Repeat with Excel-DNA's managed AutoOpen/AutoClose log: managed cleanup semantics need not equal every native export transition. [ED07]

### D.14 RTD initial values, sharing and signal traffic

Register a native wrapper `RtdProbe`=`QQ` (topic argument; runtime ProgID captured by the fixture):

```cpp
const wchar_t* ControlledProgId(); // configured before the trial
EXPORT Op* __stdcall RtdProbe(Op* topic) {
    Mark("RtdProbe-enter"); Text prog(ControlledProgId()), server(L"");
    static thread_local Op result; result=Sentinel();
    int rc=Call(379,&result,{&prog.x,&server.x,topic});
    Trace("rtd-first-result",rc,&result,rc==0);
    if (rc!=0) return ReturnNumber(rc);
    result.xltype |= xlbitXLFree; // add-in explicitly transfers callback storage
    return &result;
}
```

**Inputs and server schedule.** Use the existing controlled server, with ConnectData initially returning numeric17. Delay the first UpdateNotify until the controller sees the first native return; then publish23. Put `=RtdProbe(1)` beside typed `=RTD(progId,"","1")`, and separately one-topic `"a,b"` versus two topics `"a","b"`. Repeat with an empty VARIANT signal, a 2×2 SAFEARRAY, key replacement, two subscribers, one subscriber removal and final removal. Record ConnectData's byref argument before/after, SAFEARRAY bounds, topic IDs, RefreshData/Heartbeat/DisconnectData and every native entry.

**Predictions from supplied F.4.** Numeric1 and string `"1"` share a topic, as do typed RTD and callback379 with equal arguments. One versus two topic strings remains distinct. Empty signal maps to0; array signal maps to `#N/A` without spilling. For the measured first-value path, an invented provisional `#N/A` produces an extra native entry. Reproduce the exact original Excel-DNA server next: an initial signal1 versus-2 and failed native entry counts are not fixed merely by the controlled numeric-server pass. Keep generator cleanup and the22-second heartbeat witness as separate protocol scenarios.

For Excel-DNA add an `ExcelAsyncUtil.Run("Probe", key, () => controlledResult)` fixture with its work gated externally; for xlOil use the documented default async/RTD function and a separately configured native-async function. This prevents the label “async” from hiding which protocol is under test. [ED09], [O03]

### D.15 COM: PIA, dispatch, identity and no-Office operation

Add these methods to the already-working Framework/net6/net8 Excel-DNA fixture with a PIA reference. This is not the Native AOT dynamic-binding test. `Describe` must avoid reading a COM object's default property; the H.1 trace retains raw VARIANT distinctions.

```csharp
using System;
using System.Runtime.InteropServices;
using ExcelDna.Integration;
using Excel = Microsoft.Office.Interop.Excel;

static string Describe(object v)
{
    if (v == null) return "null";
    if (Marshal.IsComObject(v)) return "COM-object";
    if (v is Array a) {
        string s = "rank=" + a.Rank;
        for (int d=0; d<a.Rank; ++d)
            s += ";bounds=" + a.GetLowerBound(d) + ".." + a.GetUpperBound(d);
        foreach (object element in a) s += ";" + Describe(element);
        return s;
    }
    if (v is ErrorWrapper e) return "error=" + e.ErrorCode;
    return v.GetType().Name + ":" + Convert.ToString(v,
        System.Globalization.CultureInfo.InvariantCulture);
}
static void Observe(string label, Func<object> f)
{
    try { NativeLog(label, Describe(f())); }
    catch (Exception e) { NativeLog(label, "HRESULT=" + e.HResult.ToString("X8")); }
}
[ExcelFunction(Name="COM_PIA", IsMacroType=true)]
public static string Pia()
{
    Excel._Application app=null;
    Observe("acquire", () => app=(Excel._Application)ExcelDnaUtil.Application);
    Observe("Version", () => app.Version);
    Observe("Hwnd", () => app.Hwnd);
    Observe("Caller", () => app.Caller[Type.Missing]);
    Observe("Range.Value2", () => app.Range["A1:B2",Type.Missing].Value2);
    return "COM probe complete";
}
[ExcelFunction(Name="COM_DYNAMIC", IsMacroType=true)]
public static string Late()
{
    object raw=null;
    Observe("acquire", () => raw=ExcelDnaUtil.Application);
    dynamic app=raw;
    Observe("Version", () => app.Version);
    Observe("Hwnd", () => app.Hwnd);
    Observe("Caller", () => app.Caller);
    Observe("Range.Value2", () => app.Range["A1:B2"].Value2);
    return "COM probe complete";
}
```

Place helpers/methods inside a public fixture class. The shared native-only diagnostic adapter provides `NativeLog(string,string)` plus int/long overloads that format invariantly and forward to it; these are harness helpers, not Excel-DNA APIs. The PIA indexed property signatures are documented [M38], [M39]. Capture their emitted XLL registrations rather than forcing a guessed return-string code.

**Inputs.** A1=1, B1=`x`, A2=TRUE, B2=`#N/A`; call each function in D3 in a fresh equivalent run. Repeat via Application.Run and while another sheet is active. Compare each binary/route with its own Excel trace; a dynamic call's native sequence is not expected to equal a PIA call's sequence.

**Predictions.** Dispatch-only implementation can pass dynamic access while failing an `_Application` slot. Correct ABI with missing members passes Version/Hwnd but refuses Caller/Range. A fabricated ActiveCell caller fails other-sheet/CSE controls. Record exact IID/slot/DISPID, flags, LCID, optional-argument presence versus `VT_ERROR/DISP_E_PARAMNOTFOUND`, `VT_BYREF`, BSTR length, SAFEARRAY rank/lower bounds/elements, HRESULT/EXCEPINFO/arg-error and reference lifetimes. Add native `#import` access using permitted type-library metadata as a third client, not a fourth guessed ABI.

For identity, acquire NativeOM from the correct EXCEL7 HWND; QI public Window and compare canonical IUnknown. Read Window.Parent and Application separately, inspect type information and QI the Parent for the actually supported interfaces. A successful Parent.Application chain does not identify Parent's class. Marshal the acquired object to a second STA with `CoMarshalInterThreadInterfaceInStream` / `CoGetInterfaceAndReleaseStream`, keeping the owner STA pumping; repeat from a second process. Then repeat the host on a machine without Office. The hypotheses are local-only support, accidental dependence on installed Office metadata, or a working private metadata/marshaling arrangement. No test changes Excel's COM registration.

In the AppEvents sink, record `cArgs`, `cNamedArgs`, `wFlags`, LCID, each raw VARIANT in order and each named DISPID paired with its associated value. Compare the supplied SheetChange invocation shape directly; do not reverse named arguments based on positional-call rules. Add reentrant Unadvise and closed-workbook retained-Range controls to protect identity/lifetime behavior.

### D.16 QueueAsMacro, edit mode and the first real commands

Use the existing fixture's working ExcelAsyncUtil initialization. Do not assume the same bootstrap or cleanup call in every framework version. [ED08], [ED11]

```csharp
[ExcelFunction(Name="ED_QUEUE")]
public static double Queue(double ticket)
{
    long entry=NewEntry("queue-UDF"); // fixture native-only sequence allocator
    ExcelAsyncUtil.QueueAsMacro(() => {
        NativeLog("macro-entry", entry);
        object name;
        var rc=XlCall.TryExcel(XlCall.xlGetName, out name);
        NativeLog("macro-GetName", (int)rc);
    });
    NativeLog("UDF-returning", entry);
    return ticket;
}
[ExcelCommand(Name="ED_ARM_QUEUE")]
public static void ArmQueue()
{
    System.Threading.ThreadPool.QueueUserWorkItem(_ => {
        if (!AwaitRelease("queue")) { NativeLog("INVALID-timeout",0); return; }
        ExcelAsyncUtil.QueueAsMacro(() => NativeLog("gated-macro",1));
    });
}
```

`AwaitRelease` waits on a process/trial-scoped Windows event signaled externally, with timeout marking an invalid trial. First enter `=ED_QUEUE(17)` while ready. Then run ED_ARM_QUEUE, enter formula-edit mode, release the gate, and finally leave edit mode. Trace LPenHelper, COM Run arguments/result VARIANT, retry HRESULTs, generated SyncMacro `>B` entry and body count relative to UDF return and edit commit/cancel.

**Predictions.** Executing immediately can look correct while ready yet run during edit. Returning integer0 instead of the measured double0 can cause a framework retry. Blocking the owner can prevent delivery. This fixture queues **once per native semantic entry**, not once per cell: extra UDF entries must remain visible instead of being hidden by a deduplication guard.

After transport matches, replace the body with one real operation at a time. A native command/cell comparison uses this worker:

```cpp
static int PermissionWrites(Op* target) {
    Op item=Int(2), show=Bool(true), value=Num(77), r=Sentinel();
    Text message(L"probe"), formula(L"=99");
    int rc=Call(186,&r,{&item}); Trace("workspace",rc,&r,rc==0);
    if (rc==64 || rc==1) return rc;
    if (rc==0) FreeResult(r);
    rc=Call(32890,nullptr,{&show,&message.x}); Trace("message",rc,nullptr,false);
    if (rc==64 || rc==1) return rc;
    int setRc=Call(16387,nullptr,{target,&value}); Trace("set",setRc,nullptr,false);
    if (setRc==64 || setRc==1) return setRc;
    rc=Call(32864,nullptr,{&formula.x,target}); Trace("formula",rc,nullptr,false);
    return setRc;
}
EXPORT double __stdcall PermCell(Op* target) { return PermissionWrites(target); }
// Register PermCell aliases BU, BU#, BU$.
EXPORT short __stdcall PermCommand() { // A, macro type2
    Op b1{}; b1.xltype=xltypeSRef; b1.val.sref.count=1;
    b1.val.sref.ref.rwFirst=b1.val.sref.ref.rwLast=0;
    b1.val.sref.ref.colFirst=b1.val.sref.ref.colLast=1;
    PermissionWrites(&b1); return 1;
}
```

Use B1=5 and separate fresh runs for each cell variant and for Application.Run("PermCommand") with no probe formulas. This avoids a command write triggering the probe itself. **Predictions:** `#` gains the supplied information/MESSAGE access but not general worksheet mutation; a real command can set77 and then formula `=99` if implemented. Current absent command support fails only the true-command case after modeled worksheet denials. Record final value/formula, callback statuses and SheetChange/dependent activity; the sample passes NULL result pointers deliberately, leaving output-byte tests to D.9.

Next apply SET16387 to a 3×3 target with scalar, 1×3, 3×1, 2×2, oversized array, Nil elements and omitted value; compare C.4's fill/crop/clear predictions and undo-history loss. Add one OnKey install/invoke/remove and one real legacy menu action. Retain callback-created menu identity and test its action after initialization, rather than accepting menu creation alone.

### D.17 Modern in-place and expanded-argument ABI

Use fixture-owned FP12; dimensions below are restricted to a small positive matrix by the test harness. The bodies do not resize allocations.

```cpp
EXPORT void __stdcall InplaceScalar(double* x) { *x=10*(*x)+7; } // 1E, >E
EXPORT void __stdcall InplaceSecond(double* a,double* b) { // 2EE
    *a=-111; *b=10*(*b)+7;
}
EXPORT void __stdcall InplaceFp(FP12* a) { // 1K%, >K%
    int n=a->rows*a->columns; // fixture supplies at most16 elements
    for (int i=0;i<n;++i) a->array[i]=1000+10*a->array[i]+i;
}
EXPORT uint16_t* __stdcall FirstWide(double tag,uint16_t* a,uint16_t* b) {
    // F%BF%F%: first corresponding F% buffer is logical argument2.
    static uint16_t ret[]={'R','E','T',0};
    const uint16_t one[]={'F','I','R','S','T',0}, two[]={'S','E','C','O','N','D',0};
    std::memcpy(a,one,sizeof one); std::memcpy(b,two,sizeof two); return ret;
}
EXPORT uint16_t* __stdcall FirstCounted(double tag,uint16_t* a,uint16_t* b) {
    // G%BG%G%
    static uint16_t ret[]={3,'R','E','T'};
    const uint16_t one[]={3,'O','N','E'}, two[]={3,'T','W','O'};
    std::memcpy(a,one,sizeof one); std::memcpy(b,two,sizeof two); return ret;
}
EXPORT void __stdcall EmbeddedZero(uint16_t* a) { // 1G%
    const uint16_t s[]={3,'A',0,'B'}; std::memcpy(a,s,sizeof s);
}
EXPORT void __stdcall ExpandedScalar(int32_t* rows,int32_t* cols,
                                     double* data,double* scalar) { // 2O%E
    if (*rows>0 && *cols>0) data[0]=-222;
    *scalar=10*(*scalar)+7;
}
```

**Inputs and discriminators.** Scalar3 gives37; `(1,3)` under2EE gives37 rather than-111. A 2×3 matrix1..6 under1K% gives rows `1010,1021,1032` and `1043,1054,1065` if the observed element order is row-major. Error/text controls determine whether native entry occurs. F%BF%F% yields FIRST under the documented matching-buffer rule, rather than SECOND or native-return RET; G%BG%G% yields ONE. EmbeddedZero distinguishes counted-unit preservation from null termination. `2O%E(matrix,3)` has **two logical inputs and four native parameters** and returns 37; choosing native parameter2 would read the columns pointer incorrectly. [M04], [O01]

Add aliases `1O%`/`>O%` to a matching `void(rows*,cols*,double*)` body that modifies only existing elements, and record data order instead of guessing a Fortran convention. Return-code O% itself is invalid as an ordinary pointer return; do not register it to invoke the wrong prototype. Digit/synchronous `>` functions use native void; F%/G% examples deliberately return a distinct valid pointer to prove it is ignored. Check source cells remain unchanged by call-buffer mutation, then run the real xlOil FPArray and xloSort examples as framework acceptance cases.

### D.18 Legacy Excel4, ANSI and independent free ABI

Use a separate `Oper4`/`Fp4` alias to the fixture's independently defined legacy layout. Do not reuse Op/FP12 or cast them. Register this body as `PP`, `RR`, `RR#`:

```cpp
static thread_local Oper4 legacyResult;
EXPORT Oper4* __stdcall LegacyKind(Oper4* input) {
    Oper4 local; std::memcpy(&local,input,sizeof local);
    std::memset(&legacyResult,0,sizeof legacyResult);
    legacyResult.xltype=xltypeNum;
    legacyResult.val.num=local.xltype & ~(xlbitXLFree|xlbitDLLFree);
    return &legacyResult;
}
EXPORT double __stdcall LegacyMatrix(Fp4* a) { // BK
    double sum=0; unsigned n=unsigned(a->rows)*unsigned(a->columns);
    for (unsigned i=0;i<n;++i) sum+=(i+1)*a->array[i];
    return sum;
}
```

**Inputs.** Numeric/text/blank scalar, one-cell and 2×2 old-grid references, two-area reference and a short non-ASCII string in the recorded Windows code page. PP versus RR isolates value versus reference preparation; RR versus RR# isolates macro permissions. BK tests legacy FP input without conflating its return lifetime. Add separately typed KK and1K bodies only with the corresponding pointer-return/void prototypes.

A legacy signature does not itself exercise Excel4. Add this explicit callback body, registered RR and RR#; `Trace4` is the legacy-size native-only logger and Excel4v is the fixture's independent import:

```cpp
EXPORT Oper4* __stdcall LegacyRead(Oper4* reference) {
    Oper4 result{}, target{};
    target.xltype=xltypeInt; target.val.w=xltypeNum;
    Oper4* args[]={reference,&target};
    int rc=Excel4v(16386,&result,2,args);
    Trace4("legacy-coerce",rc,&result,rc==0);
    legacyResult=Oper4{}; legacyResult.xltype=xltypeNum;
    legacyResult.val.num=rc; return &legacyResult;
}
```

Use numeric ready/pending/`#` references for this baseline: the requested scalar numeric result has no auxiliary allocation. Follow legacy xlFree when extending to allocated results, and add legacy stride/payload decoding to H.1 rather than always capturing a modern32-byte descriptor. Current shim predicts32 for every call; a functioning legacy route reproduces its own Excel bytes/statuses and64 behavior where observed. Modern callback rules do not license casting between descriptors. [M40]

Finally return a legacy string/array with DLLFree and export **xlAutoFree**, separately from xlAutoFree12. Check exact pointer/thread, narrow string count/terminator, legacy integer limits and callback flags. Add C/D/F/G and O (16-bit unsigned row/column pointers) after the narrow baseline. This gate, plus a real binary that actually uses Excel4, establishes legacy support; the modern xlw sample cannot substitute for it.

### D.19 Runtime coexistence, process identity and logical reload

For the classic Framework fixture use an ordinary managed observation function; its log is diagnostic, and the formula result is fixed:

```csharp
static readonly string loadToken=Guid.NewGuid().ToString("N");
static int calls;
[ExcelFunction(Name="CLASSIC_IDENTITY")]
public static string ClassicIdentity()
{
    NativeLog("process", System.Diagnostics.Process.GetCurrentProcess().ProcessName);
    NativeLog("integration", typeof(XlCall).Assembly.FullName);
    NativeLog("corelib", typeof(object).Assembly.FullName);
    NativeLog("runtime", Environment.Version.ToString());
    NativeLog("domain", AppDomain.CurrentDomain.Id);
    NativeLog("default-domain", AppDomain.CurrentDomain.IsDefaultAppDomain()?1:0);
    NativeLog("load-token", loadToken);
    NativeLog("calls", System.Threading.Interlocked.Increment(ref calls));
    return "identity logged";
}
```

**Matrix.** Exact supplied Excel-DNA 1.9 XLL; a separately identified loader predating the process-identity override; fresh processes with override unset/set before startup; manager-disable/re-enable versus unregister/register versus process restart. Use two add-ins with different assembly dependencies to expose shared-domain consequences. Normalize random identity tokens only in the diagnostic channel while preserving equality across calls/reloads; do not normalize away differences in initialization count or default-domain status.

**Predictions.** Current public classic loader's cluster branch selects the default AppDomain and inhibits normal AppDomain unloading [ED02]. An old loader that lacks the override cannot be repaired by merely setting the environment variable. A process restart gives fresh managed state; a logical reactivation may retain it. Neither source proves the exact supplied binary's branch—record its behavior.

For net6/net8 log CoreLib version and the fixture assembly's `AssemblyLoadContext` name/collectibility. Run6→8 and8→6 with actual runtimeconfig/roll-forward settings, plus Framework alongside each. The question is which runtime/configuration is selected and whether later loading succeeds, not simply whether both DLL files load. Test xlwDotNet separately with its generated C++/CLI bridge, ijwhost and runtime configuration. [ED03], [M14], [M30]

For Native AOT compare logical deactivate/reactivate while resident with a fresh-process control and two distinct payloads. Do not call physical FreeLibrary merely to test a reset that the runtime explicitly does not support. [M12] For xlOil 0.22.1 distinguish the already-measured native reload crash route from Python module/source reload and new function registration. The current hash accommodation should affect only its recorded predicate. Keep “Excel crashes / host refuses” an intentional divergence, and verify the refusal leaves the session usable.

### D.20 Saved names, missing providers and workbook-local Python

Register a native counter with `BB`:

```cpp
static volatile LONG savedCalls=0;
EXPORT double __stdcall SavedCounter(double x) {
    LONG n=InterlockedIncrement(&savedCalls);
    Mark("SavedCounter-enter"); return x+1000*n;
}
```

**Workbook.** A1=7; B1=`=SavedCounter(A1)`; C1 depends on B1; a global defined name calling SavedCounter; one same-spelling sheet-local name; one explicit RTD formula. Record formulas, cached values and entry counts after setup. Save `.xlsx` and `.xlsb` controls under Manual and Automatic modes. Reopen with provider absent, recalculate or leave untouched, load provider later, recalculate explicitly, unregister, and register a changed implementation under the same name. Keep file-load policy separate from formula-binding behavior.

**Alternatives.** A correct saved binding retains the formula and appropriate cached state when a provider is absent, then resolves by provider/name/generation under the observed trigger. An ID-based binding breaks across sessions; destructive conversion to `#NAME?` loses recoverability; unconditional dirty/recalculate on load changes the Manual-mode invocation count. Exact Excel behavior for each missing-provider/open/load sequence is explicitly unmeasured in the supplied prompt, so record it before choosing invalidation rules. Inspect `_xll.`/external-name records and preserve explicit RTD ProgID text; do not substitute session-generated identifiers from a different run.

For local Python, use two workbook-linked modules, `Book1.py` and `Book2.py`, with the same function name and different bodies:

```python
import xloil

@xloil.func
def LocalValue(x: float):
    return x + 1       # Book2.py uses x + 1001
```

In the documented workbook-module route the local default generates a VBA stub [O04]. Save each as `.xlsm`; invoke each with2; close/reopen; reload source; rename/move the workbook under the supported path policy; open both together. Capture generated VBA source/project parts and actual registration/COM/Run traffic. Run **existing stub execution** independently of **creating/updating the stub**. The latter requires observed VBProject/VBComponents/CodeModule operations and applicable trust behavior; a working global Python UDF does not establish either. Local functions' no-threaded/no-native-async limits are separate from their supported RTD route. Do not claim compatibility with macros stripped by a scratch-session `.xlsx` save.

### D.21 Reentry, owner pumping, native quiescence and crash containment

Register this reference-reading worker `BU$` and run two instances with the existing barrier fixture. Set A1=7 and pass its ready reference from two other cells:

```cpp
static double ReadReadyReference(Op* reference) {
    Op r=Sentinel();
    int rc=Call(16386,&r,{reference});
    Trace("worker-coerce",rc,&r,rc==0);
    double value=(rc==0 && (r.xltype & ~(xlbitXLFree|xlbitDLLFree))==xltypeNum)
        ? r.val.num : -1000-rc;
    if (rc==0) FreeResult(r);
    return value;
}
EXPORT double __stdcall CallbackReference(Op* reference) { // BU$
    FixtureBarrierBeforeCallback(); // controller can observe both entries
    return ReadReadyReference(reference);
}
// Companion called by Python; its formula is on the sheet containing ready A1.
EXPORT double __stdcall CallbackNumber(double unused) {
    Op a1{}; a1.xltype=xltypeSRef; a1.val.sref.count=1;
    a1.val.sref.ref.rwFirst=a1.val.sref.ref.rwLast=0;
    a1.val.sref.ref.colFirst=a1.val.sref.ref.colLast=0;
    return ReadReadyReference(&a1);
}
```

While the owner waits, inject an unrelated queued command and an unload request, then release the callback barriers. Confirm in the host's internal diagnostic trace that the read actually traverses the owner proxy; scalar-to-scalar Coerce can be resolved locally and is not a sufficient pump witness. **Predictions:** a non-pumping wait deadlocks; an indiscriminate pump runs a structural operation inside the active calculation; a constrained callback pump services the worker and defers inappropriate work. Record owner/worker call stacks at the boundary, nested request IDs and command/unload order. Add D.10's same-module nested call; a queued child behind its waiting parent is a separate deadlock witness.

For the Python/GIL branch, expose an ungated `CallbackNumber` helper in a small native companion and call it from an xlOil global threaded function through `ctypes.PyDLL`, deliberately retaining the GIL, while an unrelated owner-side Python command is queued:

```python
import ctypes, xloil
probe = ctypes.PyDLL(r"<absolute fixture companion path>")
probe.CallbackNumber.argtypes = [ctypes.c_double]
probe.CallbackNumber.restype = ctypes.c_double

@xloil.func(local=False, macro=False, threaded=True)
def OwnerCallbackWithGil(x: float):
    return probe.CallbackNumber(x)
```

Provide the companion's Excel12 forwarder from the same host main-image lookup used by the fixture; it is part of the test group. **[public-doc]** PyDLL retains the GIL during the foreign call [P01]. **[inference]** If owner pumping enters Python before servicing the proxied callback, it can wait for the GIL held by the waiting worker. Compare a CDLL-release control. This diagnoses the proposed transport/pump, not a claim that xlOil 0.22.1 currently retains the GIL in every internal callback.

For the lease bug, change D.7's worker to signal completion, then block at a second external gate *after*16400 returns but before worker exit. Request unload while blocked. **Predictions:** a lease released on “last completion seen” allows physical unload with live add-in instructions; explicit native quiescence or residency retains it. A controlled fixture can signal actual thread exit; an arbitrary production XLL may offer no equivalent proof. Repeat with a queued timer, event callback and retained COM object.

For containment, use a disposable fixture `BB` whose argument selects a native noncontinuable exception or a wait on a never-signaled native event. One possible controlled exception body is:

```cpp
EXPORT double __stdcall FaultProbe(double mode) {
    if (mode==1) RaiseException(EXCEPTION_ACCESS_VIOLATION,
                               EXCEPTION_NONCONTINUABLE,0,nullptr);
    else WaitForSingleObject(NeverSignaledTrialEvent(),INFINITE);
    return 0;
}
```

This tests exception/hang containment, not recovery from arbitrary heap corruption. In a supervised host, terminate the failed surrogate when required, preserve committed controller edits, invalidate its tokens/COM facades, and make restart explicit. Verify no native invocation with possible side effects is replayed automatically. In-process Excel failure remains recorded as failure/crash behavior; surviving it is a deliberate product difference. Do not test termination against the user's real workbook session.

### D.22 Low-cost contract probes and remaining surface

Use D.9's arbitrary-call body for this table; every row has an explicit input and competing outcome. These close selected S2/S7/S10/S11/S24/S27/S29 gaps without pretending the whole XLM/COM catalogue is measured.

| Probe | Inputs and observation | Discriminator |
|---|---|---|
| Stack16385 | Zero arguments in ordinary/`#`/`$`/command and outside frame; repeat with bounded native recursion, no actual overflow | Documented Excel12 cap64 KiB versus raw available OS stack; type/status and context failures are separate. [M17] |
| Instance16391/16403 | Zero arguments on x64; descriptor sentinel; compare16403 handle with actual main-image identity | Documented old-call failure versus fabricated truncated success; BigData handle versus owned byte buffer. [M15], [M16] |
| Hwnd16392 | Zero args; relate returned identity to NativeOM hierarchy and PID | A numeric-looking result can point to the wrong window/process; normalization must retain this relation. [M21] |
| Number flags | A known worksheet callback (for example SUM4) with original number and separate international `0x2000` variants; command/special-bit misuse controls | Blanket bit stripping may execute a different callback. Retain complete incoming function number and locale. Prompt `0x1000` trials belong to an explicitly interactive fixture run, with no automatic dialog acknowledgement. |
| LPenHelper /16398 | Ready, formula edit and committed/cancelled edit; call actual framework LPenHelper path and Excel12 route separately | Always-ready breaks queue retry; aliasing the two routes may invent API support. Record structure bytes from the existing independent layout. |
| Worksheet ABI | Existing raw FILTER mismatch plus SUM, logical/error/text functions and mixed arrays | Same CalcValue can hide Int/Num, Nil/Missing, flags or output-mutation differences; exact callback bytes decide. |
| Macro information185/186/268 | Replay exact framework item/reference operands; one valid and one unsupported item in ordinary/`#`/command | Missing capability differs from expected ordinary permission denial; one successful Workspace item does not validate the family. |
| Menu152/153/158/159/182/259 | Replay captured framework initialization arrays; query, invoke action, delete, query again | A success stub creates no usable object/action or has wrong IDs/order/lifetime. |

For binary names add a true command with this body, using two fixture-owned helpers to set/read the already-validated BigData union fields:

```cpp
EXPORT short __stdcall BinaryRoundTrip() { // A, macro type2
    unsigned char bytes[]={0,1,127,255};
    Text name(L"ProbeBinary");
    Op data=BigDataFromBytes(bytes,4), r=Sentinel();
    // Helper sets raw byte pointer and signed32 cbData, not a global handle.
    int rc=Call(16396,nullptr,{&name.x,&data}); Trace("define-binary",rc,nullptr,false);
    if (rc!=0) return 1; // preserve failure; no further callback after64/1
    rc=Call(16397,&r,{&name.x}); Trace("get-binary",rc,&r,false);
    if (rc==0 && (r.xltype & ~(xlbitXLFree|xlbitDLLFree))==xltypeBigData) {
        HGLOBAL h=BigDataHandle(r); // returned handle, NOT raw byte pointer
        if (BigDataSignedCount(r)==4 && GlobalSize(h)>=4) {
            void* p=GlobalLock(h);
            if (p) { RecordFourBytes(p); GlobalUnlock(h); }
        } else Mark("INVALID-binary-size");
        FreeResult(r);
    } else if (rc==0) FreeResult(r);
    return 1;
}
```

Define in Sheet1, retrieve in Sheet1/Sheet2, save/reopen, redefine and delete with Missing/omitted data in separate trials. **[public-doc]** Define consumes bytes while Get returns an Excel-owned global-memory handle requiring xlFree [M22], [M23]; published known issues qualify its scope [M25]. **[inference]** A universal “BigData is opaque handle” rule gets Define wrong; a universal “BigData is raw pointer” rule gets Get wrong. The instance/async BigData cases must not enter this allocation/free path. Record persisted bytes and scope rather than inventing a generic workbook-name implementation.

### D.23 First oracle batch to send back

The highest-information first batch is **D.1, D.2, D.3, D.6, the original Excel-DNA D.14 RTD case, and D.16's queue transport**, followed by D.15 for the three OneCalc COM failures. Include registration records, the complete trace surrounding the first divergence, the repeated Excel control, and the manifest from C.1. These results settle the highest-risk semantic decisions before a broad callback rollout.

The outstanding outcomes most likely to change implementation are: discarded DLLFree cleanup after 64; outer255 status when a child gets64; early async completion followed by64; mixed-validity batch16400; event trigger/permission differences on the target build; partial AutoOpen-failure cleanup; hidden-name/rebinding behavior under Manual mode; and actual COM marshaling without installed Office. None is presented here as already measured.

## Source register and evidence boundaries

Public sources below were consulted on 10 November 2026. Microsoft pages are documentation, not implementation-source evidence. Excel-DNA GitHub links identify inspected current/indexed public source unless explicitly historical; no immutable framework source commit is invented. xlOil's served `latest` documentation labels itself 0.19.0, and O05/O06 are explicitly historical 0.8.2. Neither automatically describes the tested0.22.1 binary. W01's release history establishes availability across xlw releases, not the emitted signatures of every add-in.

The pinned DNA Calc excerpts, case counts, exact tested statuses, native/COM traces and Excel build numbers come from the supplied prompt. The repository snapshot was not independently inspected. ABI details explicitly tagged recollection require the fixture gate. Proposed bodies were not compiled or executed. No Excel/Microsoft-runtime disassembly, decompilation or SDK header implementation text was used as evidence or copied into the proposed code.

### Microsoft public documentation

| Area | Sources |
|---|---|
| Callbacks and permissions | [Excel4/Excel12][M01], [callback classes and states][M28], [MTR][M29], [legacy/modern callback prototypes][M40] |
| Memory and operands | [Memory management][M02], [xlFree][M03], [auto-free][M27], [Coerce][M08], [Caller][M24] |
| Registration and lifecycle | [Register Form1][M04], [AutoAdd][M18], [AutoRemove][M19], [AutoRegister][M26], [ManagerInfo][M41] |
| Async and events | [Native async][M07], [AsyncReturn][M11], [event handling][M05], [event registration][M06], [Abort][M10], [COM AfterCalculate][M20] |
| Commands and probes | [Set][M09], [Stack][M17], [GetInst][M15], [GetInstPtr][M16], [GetHwnd][M21], [DefineBinaryName][M22], [GetBinaryName][M23], [known issues][M25] |
| Runtime and loading | [Native AOT libraries][M12], [CLR side-by-side][M14], [C++/CLI][M30], [Windows DLL search][M13] |
| COM | [Window][M31], [Range][M32], [QI identity][M33], [Invoke][M34], [type information][M35], [NativeOM][M36], [Caller behavior][M37], [PIA Caller][M38], [PIA Range][M39] |

[M01]: https://learn.microsoft.com/en-us/office/client-developer/excel/excel4-excel12 "Excel4/Excel12 callback status and restrictions"
[M02]: https://learn.microsoft.com/en-us/office/client-developer/excel/memory-management-in-excel "Memory management in Excel"
[M03]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlfree "xlFree: auxiliary allocation release and pointer mutation"
[M04]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlfregister-form-1 "xlfRegister Form 1: types, flags and in-place conventions"
[M05]: https://learn.microsoft.com/en-us/office/client-developer/excel/handling-events "Handling XLL events and calculation origin"
[M06]: https://learn.microsoft.com/en-us/office/client-developer/excel/xleventregister "xlEventRegister"
[M07]: https://learn.microsoft.com/en-us/office/client-developer/excel/asynchronous-user-defined-functions "Native asynchronous UDFs"
[M08]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlcoerce "xlCoerce"
[M09]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlset "xlSet fill, clear and undo behavior"
[M10]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlabort "xlAbort"
[M11]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlasyncreturn "xlAsyncReturn, batch and ownership"
[M12]: https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/libraries "Native AOT library unloading restriction"
[M13]: https://learn.microsoft.com/en-us/windows/win32/dlls/dynamic-link-library-search-order "Windows DLL search order"
[M14]: https://learn.microsoft.com/en-us/dotnet/framework/deployment/in-process-side-by-side-execution "In-process side-by-side CLR execution"
[M15]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlgetinst "xlGetInst x64 restriction"
[M16]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlgetinstptr "xlGetInstPtr"
[M17]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlstack "xlStack cap"
[M18]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlautoadd "xlAutoAdd"
[M19]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlautoremove "xlAutoRemove"
[M20]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.aftercalculate "COM Application.AfterCalculate"
[M21]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlgethwnd "xlGetHwnd"
[M22]: https://learn.microsoft.com/en-us/office/client-developer/excel/xldefinebinaryname "xlDefineBinaryName"
[M23]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlgetbinaryname "xlGetBinaryName global-memory ownership"
[M24]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlfcaller "xlfCaller"
[M25]: https://learn.microsoft.com/en-us/office/client-developer/excel/known-issues-in-excel-xll-development "XLL known issues: binary-name scope, xlSet and names"
[M26]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlautoregister-xlautoregister12 "Automatic registration exports"
[M27]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlautofree-xlautofree12 "Auto-free timing and callback restrictions"
[M28]: https://learn.microsoft.com/en-us/office/client-developer/excel/excel-commands-functions-and-states "Excel callback classes and states"
[M29]: https://learn.microsoft.com/en-us/office/client-developer/excel/multithreaded-recalculation-in-excel "MTR scheduling and callback permissions"
[M30]: https://learn.microsoft.com/en-us/dotnet/core/porting/cpp-cli "C++/CLI modern .NET deployment"
[M31]: https://learn.microsoft.com/en-us/dotnet/api/microsoft.office.interop.excel.window?view=excel-pia "Public dispatch-only Window interface"
[M32]: https://learn.microsoft.com/en-us/dotnet/api/microsoft.office.interop.excel.range?view=excel-pia "Public dispatch-only Range interface"
[M33]: https://learn.microsoft.com/en-us/windows/win32/com/rules-for-implementing-queryinterface "COM QueryInterface identity rules"
[M34]: https://learn.microsoft.com/en-us/windows/win32/api/oaidl/nf-oaidl-idispatch-invoke "IDispatch Invoke ABI"
[M35]: https://learn.microsoft.com/en-us/windows/win32/api/oaidl/nf-oaidl-idispatch-gettypeinfo "IDispatch type information"
[M36]: https://learn.microsoft.com/en-us/windows/win32/api/oleacc/nf-oleacc-accessibleobjectfromwindow "NativeOM acquisition"
[M37]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.caller "COM Application.Caller"
[M38]: https://learn.microsoft.com/en-us/dotnet/api/microsoft.office.interop.excel._application.caller?view=excel-pia "PIA indexed Caller signature"
[M39]: https://learn.microsoft.com/en-us/dotnet/api/microsoft.office.interop.excel._application.range?view=excel-pia "PIA indexed Range signature"
[M40]: https://learn.microsoft.com/en-us/office/client-developer/excel/calling-into-excel-from-the-dll-or-xll "Legacy and modern callback prototypes"
[M41]: https://learn.microsoft.com/en-us/office/client-developer/excel/xladdinmanagerinfo-xladdinmanagerinfo12 "ManagerInfo action and returned description contract"

### Public framework documentation and code

| Framework | Sources and scope |
|---|---|
| Excel-DNA | [C API wrappers][ED01], [classic loader][ED02], [runtime selection][ED03], [AOT support][ED04], [registration metadata][ED05], [callback definitions][ED06], [managed AutoClose][ED07], [queue implementation][ED08], [historical async notes][ED09], [async handle][ED10], [queue guide][ED11] |
| xlw | [Project/release history][W01], [public repository including xlwDotNet][W02] |
| xlOil | [Special arguments][O01], [Python API][O02], [RTD][O03], [local/global functions][O04]; historical 0.8.2 [call wrapper][O05] and [ExcelObj ownership wrapper][O06] |
| Python | [ctypes/PyDLL GIL behavior][P01] |

[ED01]: https://excel-dna.net/docs/guides-basic/excel-programming-interfaces/excel-c-api/ "Excel-DNA Excel versus TryExcel"
[ED02]: https://github.com/Excel-DNA/ExcelDna/blob/master/Source/ExcelDna/ExcelDnaLoader.cpp "Classic loader AppDomain/cluster branch, current public code"
[ED03]: https://excel-dna.net/docs/guides-basic/dotnet-runtime-support/ "Excel-DNA runtime support and selection"
[ED04]: https://excel-dna.net/docs/guides-basic/dotnet-native-aot-support/ "Excel-DNA Native AOT feature support, current documentation"
[ED05]: https://github.com/Excel-DNA/ExcelDna/blob/master/Source/ExcelDna.Loader/XlMethodInfo.cs "Current registration metadata and exception-safe return selection"
[ED06]: https://github.com/Excel-DNA/ExcelDna/blob/master/Source/ExcelDna.Integration/XlCall.cs "Public callback IDs and XlCallException status field"
[ED07]: https://excel-dna.net/docs/guides-advanced/detecting-excel-shutdown-and-autoclose/ "Managed AutoClose versus shutdown"
[ED08]: https://github.com/Excel-DNA/ExcelDna/blob/master/Source/ExcelDna.Integration/ExcelSynchronizationContext.cs "QueueAsMacro Run/result/edit-state protocol, current public code"
[ED09]: https://excel-dna.net/docs/archive/wiki/Async-Notes/ "Historical explanation of native async versus RTD async"
[ED10]: https://github.com/Excel-DNA/ExcelDna/blob/master/Source/ExcelDna.Integration/ExcelAsyncHandle.cs "Native async SetResult status mapping, current public code"
[ED11]: https://excel-dna.net/docs/guides-advanced/performing-asynchronous-work/ "QueueAsMacro guide; initialization is version-sensitive"
[W01]: https://xlw.github.io/ "xlw project and release history"
[W02]: https://github.com/xlw/xlw "xlw and xlwDotNet public project"
[O01]: https://xloil.readthedocs.io/en/latest/xlOil_Cpp/SpecialArgs.html "xlOil special arguments, FPArray in-place and input mutation"
[O02]: https://xloil.readthedocs.io/en/latest/xlOil_Python/ModuleReference.html "xlOil Python decorator defaults and APIs, served 0.19 documentation"
[O03]: https://xloil.readthedocs.io/en/latest/xlOil_Python/Rtd.html "xlOil Python RTD"
[O04]: https://xloil.readthedocs.io/en/latest/xlOil_Python/Functions.html "Workbook-local VBA stubs and Python registration variants"
[O05]: https://xloil.readthedocs.io/en/v0.8.2/doxygen/_excel_call_8h_source.html "Historical xlOil call wrapper, v0.8.2"
[O06]: https://xloil.readthedocs.io/en/v0.8.2/doxygen/_excel_obj_8h_source.html "Historical xlOil fromExcel ownership flag, v0.8.2"
[P01]: https://docs.python.org/3/library/ctypes.html "ctypes PyDLL versus CDLL GIL behavior"

The inline source keys are clickable. A source's presence does not upgrade an explicitly unmeasured outcome into a fact: the oracle cases in D remain the release gates for the pinned binaries and Excel builds.

