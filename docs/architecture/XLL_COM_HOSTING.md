# XLL and COM add-in hosting for DNA Calc (owner ruling 4.9)

Date 2026-10-31. Read-only research for the architecture review; connects to SYNTHESIS.md 1.3-1.6, 3, 4, 6.
Markers: **[V]** verified this session in source or Microsoft documentation (link or path given);
**[I]** inference; **[P]** must be measured in Excel on this machine before the dependent type freezes.

Owner direction (2026-11-01): full `.xll` compatibility, including Excel-DNA, xlw and xlOil out
of the box, and broad COM support are an early focus. Section 9 records the first X0 boundary
measurements; the remaining [P] items are still open. How the desktop OneCalc loads, lists and unloads
these add-ins and shows what they registered is in [ONECALC_ADDINS.md](ONECALC_ADDINS.md).

Sources read: Excel-DNA local clone `C:\Work\Excel-DNA\ExcelDna` (master f4847492, 2026-09-13; paths under
`Source/`, https://github.com/Excel-DNA/ExcelDna/tree/master/Source); xlOil https://github.com/cunnane/xloil (master);
xlw https://github.com/xlw/xlw (master; branch `2026-update` for Native AOT). Microsoft: xlfRegister (Form 1) https://learn.microsoft.com/en-us/office/client-developer/excel/xlfregister-form-1;
  async UDFs https://learn.microsoft.com/en-us/office/client-developer/excel/asynchronous-user-defined-functions;
  MTR https://learn.microsoft.com/en-us/office/client-developer/excel/multithreaded-recalculation-in-excel;
  memory https://learn.microsoft.com/en-us/office/client-developer/excel/memory-management-in-excel;
  events https://learn.microsoft.com/en-us/office/client-developer/excel/handling-events;
  creating XLLs https://learn.microsoft.com/en-us/previous-versions/office/bb687861(v=office.14).

## 1. What the three frameworks call

### 1.1 How each reaches Excel [V]

| | Excel-DNA | xlw 6 | xlOil |
|---|---|---|---|
| Callback route | `GetProcAddress(GetModuleHandle(NULL),"MdCallBack12")` (`ExcelDna.Loader/XlCallImpl.cs:84-88`); `SetExcel12EntryPt` fallback export | SDK-style `xlcall.cpp`, same lookup (`xlw/src/xlcall.cpp`) | SDK `XLCALL.CPP` (`external/Excel2013SDK/SRC`), same lookup |
| XLCALL32.DLL | `[DllImport] XLCallVer()` (`XlAddIn.cs:177`, version = result/256); `LoadLibrary` + `LPenHelper` (`ExcelDna.Host.Shared/LPenHelper.cpp:13-15`) | `LoadLibrary("XLCALL32.DLL")` must succeed or `xlAutoOpen` returns 0 (`xlw/src/XlfExcel.cpp`); an xlw add-in needs `Excel4`/`Excel4v` only where it makes legacy calls or registers legacy signatures (XLOPER12, Unicode and MTR since xlw 3.0, x64 since 5.0) | linked, probably not imported at run time [I] |
| Excel4 / XLOPER | no | no, unless the add-in itself calls Excel4 or registers legacy types (any xlw version) | no |
| xll exports | `xlAutoOpen/Close/Remove`, `xlAutoFree12`, `xlAddInManagerInfo12`, `SetExcel12EntryPt`, `DllRegisterServer/Unregister/GetClassObject/CanUnloadNow`, `SyncMacro`, `RegistrationInfo`, `CalculationCanceled/Ended` (`ExcelDna.Host.Shared/exports.def`) plus 10,000 stubs `f0..f9999` jumping through a thunk table (`ExcelDna/JmpExports64.asm`) | `xlAutoOpen/Close/Remove`, `xlwGenDoc`, one `xl<Name>` per function; no `xlAutoFree12` | `xlAutoOpen/Close/Free12`, `xlAddInManagerInfo12`, `xlAutoAdd/Remove`, `xlHandleCalculationCancelled`; core `xlOil.dll` exports `xloil_stub`, `xloRunInXLLContext`, `localFunctionEntryPoint` |
| UDF entry points | fixed pool `fN`, registered with the **full xll path** (`XlRegistration.cs:146-171`) | static exports | **one export `xloil_stub` rewritten in xlOil.dll's export table before each `xlfRegister`**, module given as bare file name (`src/xlOil-Dynamic/DynamicRegistration.cpp`) |
| Runtime | .NET Framework via `CLRCreateInstance`/`ICorRuntimeHost`, AppDomain per add-in (`ExcelDna/ExcelDnaLoader.cpp:229-256`); .NET 6+ via `hostfxr_initialize_for_runtime_config` + `load_assembly_and_get_function_pointer` (`ExcelDna.Host/host.cpp:98-210`); Native AOT (`ExcelDna.Host.NativeAOT/host.cpp`) | C++; xlwDotNet is C++/CLI (Ijwhost) or Native AOT | C++ core, delay-loaded `xlOil.dll`, plugins by `LoadLibrary`, embedded CPython |

### 1.2 Callback table (difficulty for DNA Calc: L low, M medium, H high)

| Callback | Used by | Purpose | Diff. |
|---|---|---|---|
| `xlfRegister` (149) Form 1 | all | register UDFs/commands; returns register ID, defines a hidden name evaluating to it [V doc] | M: type strings, export lookup at call time, hidden names, use counts |
| `xlfUnregister` (201), `xlfSetName` (88) | all | unregister; remove wizard name via fake `"I"` registration of `xlAutoRemove`/`SetExcel12EntryPt` then `xlfSetName` (`XlRegistration.cs:217-231`; xlOil `FuncRegistry.cpp`) | L |
| `xlFree`, `xlCoerce`, `xlSet` | all / Excel-DNA, xlOil | free Excel memory (must accept any XLOPER12, xlOil frees every result); coerce refs; write ranges from commands | L / M (needs read view) / M (needs command transaction) |
| `xlfCaller` (89), `xlSheetId`, `xlSheetNm` | all | caller cell/area, sheet ids and names | M: caller per `EvaluationLocus` |
| `xlfGetWorkspace` (2, 37) | all | version string "16.0"; international settings (xlw) | L |
| `xlGetName`, `xlGetHwnd`, `xlGetInstPtr` | all / xlOil, Excel-DNA | xll path; XLMAIN window; Excel hInstance | L, but needs "current XLL" context |
| `xlEventRegister` (Canceled, Ended) | Excel-DNA, xlOil | async life cycle; not raised on programmatic recalc [V doc] | M |
| `xlAsyncReturn` | Excel-DNA (native async), xlOil | complete an `X` call, from any thread [V doc] | M-H |
| `xlfRtd` (379) | Excel-DNA (`ExcelAsyncUtil.Run/Observe`, `XlCall.RTD`), xlOil, xlwDotNet | RTD topic subscription from inside a UDF | H (COM RTD client, topic life cycle) |
| `xlUDF`, `xlfEvaluate`, `xlfCall` | Excel-DNA (`RegistrationInfo.cs:73-74`, `ExcelIntegration.cs:140`), xlOil (IntelliSense refresh) | call another add-in's function by name; evaluate a name to its register ID | M |
| `xlcRun`, `xlcOnKey`, `xlcMessage`, `xlcAlert`, `xlcNew`, `xlcWorkbookInsert`, `xlcFileClose`, `xlcEcho` | Excel-DNA | macro run; shortcuts; status bar; getting Application with no workbook open (`Excel.cs:329-358`) | M (commands edit the model) |
| `xlfGetBar`, `xlfAddMenu`, `xlfAddCommand`, `xlfDeleteMenu/Command`, `xlfGetTool` | Excel-DNA (`MenuManager.cs`), xlw | legacy menus | L (map to command model or accept and record) |
| `xlfFilter(1,1)`, `xlfIndirect`, `xlfGetCell`, `xlfIsformula`, `xlcFormula`, `xlcFormulaArray` | xlOil | dynamic-array probe; address parse; formula read/write | M |
| `xlAbort`, `xlStack` | xlOil, xlw | Esc check; xlOil detects "outside XLL context" by expecting `xlretInvXlfn` from `xlStack` | L, but exact return codes matter [P] |
| `xlfVolatile`, `xlRunningOnCluster` | Excel-DNA | volatility toggle; cluster probe | L |
| `LPenHelper(xlGetFmlaInfo)` (XLCALL32 export) | Excel-DNA | edit-mode check before `Application.Run`; failure is treated as "editing", so queued macros never run (`ExcelSynchronizationContext.cs:513-531`) | L, required |
| xlw `XlfServices` (`xlfGetFormula`, `xlfReftext`, `xlfTextref`, `xlfAbsref`, `xlfFormulaConvert`, `xlfInput`, `xlfDialogBox`, `xlcSelect`...) | xlw, only if add-in code calls them | services | per function, by Ftab number |

Not used by any of the three: `xlAutoRegister12` (doc: "only supported in Excel 2007" [V]), `xlcOnTime`, `xlDefineBinaryName`.
User code in all three can call **any** function number (`XlCall.Excel`, `xloil.call`), so the long tail is the
Ftab/Cetab table: `xlf` numbers are Ftab indices and `xlc` numbers Cetab indices + 0x8000 [I, consistent with
`xlfRegister`=149, `xlfCaller`=89]. A worksheet function called through `Excel12(xlfSum, ...)` should route by
Ftab index through `calc-func-catalog` (SYNTHESIS 1.8) to the same kernel.

### 1.3 Registration type strings and XLOPER12 [V]

- Codes used: Excel-DNA `Q U E D% K% L X` (`ExcelDna.Loader/XlTypes.cs:186-192`), return `Q`/`>`; suffixes `! # $ &`
  (`XlRegistration.cs:342-357`; `$` never with `#`; `&` never with `X`), commands with `>B` (SyncMacro, macro type 2).
  xlw: return `Q`, args `Q B K% U`, suffixes `! $ & #`, commands `A`. xlOil: `U Q K% X`, return `U K% A >` or digit
  (in-place), suffixes `! # $`. These inventories are examples taken from the frameworks' code, not closed ABIs:
  Excel-DNA also has exception-safe numeric/array paths and commands with return values, and xlOil documents in-place
  FPArray returns; capture each add-in's emitted registration strings. The suffixes are not one exclusive choice:
  `!` can accompany an execution class (measured `Q!#`, `>EX!`; our parser accepts `!` with `#` or `$`).
- Excel rules: `#` and `$` exclusive; `#` and `&` exclusive too (macro-equivalent functions cannot be cluster-safe;
  `Q#&`/`QU#&` rejection, its status and output are unmeasured, so the host does not invent a code; `Q$&` is a permitted
  combination); `X` with `>` marks async; async and cluster-safe exclusive (`&` is not a synonym of `$`); a synchronous
  `>` returns through its first argument, so `>` is not only an async marker; up to 255 arguments [V xlfRegister page].
  Thread-safe async (`>...X$`) is a valid registration: refusing it is this host's limitation (`invoke.rs`), not an
  Excel rule (the parser accepts `>EX$`).
- `#` with an `R`/`U` parameter is not volatile as a property of the registration: in the measured profile it is a
  call-start volatility event when an actual reference arrives in an `R`/`U` position (VOLATILITY.md, answer 13 A.3 #2).
  The parser's static `volatile |= macro_equivalent && any R/U` (`signature.rs`) is to be replaced (dc-6332.11).
- Types exchanged: Num, Str (counted UTF-16), Bool, Err, Multi, Missing, Nil, Ref (multi-area), SRef, Int, BigData
  (`xlGetInstPtr`, async handle), FP12. `xlbitDLLFree` returns: Excel copies, then calls `xlAutoFree12` of **the module
  named in `xlfRegister`** (xlOil: `xlOil.dll`, not the .xll) [V doc + xlOil]. xlw returns pool memory with no free bit,
  reset on the next top-level entry on that thread: copy the result before the next call into that module [V].

## 2. How an xll loads, and what DNA Calc must provide

Excel: `LoadLibrary` of the .xll, then `xlAutoOpen` on the main thread inside an XLL call context; Excel12 entry
points are resolved by name from the host exe [V xlcall.cpp]. For an **unmodified** xll DNA Calc must provide:

1. **`MdCallBack12` exported from the main exe image** (`int __stdcall(int xlfn, int count, LPXLOPER12*, LPXLOPER12)`),
   since every framework we traced calls `GetModuleHandle(NULL)` (another binary may import a shim function or use an
   older API route; check its route with the tracer). Rust: `#[no_mangle] extern "system"` plus `/EXPORT`. [V/I]
2. **An `XLCALL32.DLL` shim** beside the exe: `XLCallVer` returning 12*256,
   `LPenHelper` (edit state; `xlModeReady` when not editing), `Excel4`/`Excel4v` (refuse with `xlretFailed` until legacy
   XLOPER support exists), `Excel12`/`Excel12v` forwarding. [V needs; layout I] The application directory does not
   unconditionally win DLL resolution: an already loaded module of that name, redirection and SxS come first, and the
   host loads XLLs with `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`, while an XLL that loads
   XLCALL32 itself uses the normal search order. The hardening is to load the shim by full path before the first XLL
   and check the bound path; the system's Excel files are never replaced.
3. **Process identity**: version resource `FileMajorPart` >= 15 (Excel-DNA takes pre-15 paths otherwise,
   `Excel.cs:620-641`; cluster-safe needs >= 14, `LoaderHelpers.cs:41-67`); `xlfGetWorkspace(2)` = "16.0". Excel-DNA
   treats a process not named EXCEL or ET as an HPC cluster host unless `EXCELDNA_ASSUME_EXCEL_PROCESS=1`
   (`LoaderHelpers.cs:66-86`, `Host.Shared/utils.cpp:228-243`); that switch only exists since commit 51833b4a
   (2026-07-06), so older built add-ins see cluster mode in `dna-calc.exe` (effects found: cluster-safe flag, load
   errors suppressed; full effect [P]). The public classic loader's cluster path also uses the default AppDomain and
   does not unload it [public code, unmeasured], so a renamed executable is not equivalent to Excel for older binaries.
4. **Bitness**: x64 loads only x64 xlls. All three ship both builds; 32-bit DNA Calc is an owner decision (sec. 7).
5. **A "current XLL" context per thread**: `xlGetName` (16393, the current XLL's path; not the XLM `xlfGetName`, 107),
   `xlEventRegister` and the rest carry no module argument, so the host tracks which module it called (xlAutoOpen,
   UDF, command, event, `Application.Run`) [I]. `xlfRegister` does name a module, but the originating XLL is still
   needed for module-relative operations and ownership. Calls
   outside any context must fail with Excel's exact codes: Excel-DNA probes with `xlGetName` (`Excel.cs:288-306`),
   xlOil with `xlStack` [P].
6. **Registration semantics**: resolve `GetProcAddress` **at `xlfRegister` time** and keep the pointer per register ID
   (xlOil rewrites the export between registrations); accept full path (Excel-DNA) or bare name of an already loaded
   module (xlOil); hidden application-scope name per function so `xlfEvaluate("Name")` and `xlUDF` work.
7. **No hostile process mitigations**: the CLR JIT, xlOil's `PAGE_EXECUTE_READ` thunks and export patching need
   dynamic code and writable images: no Arbitrary Code Guard, no Microsoft-signed-only image policy. What is needed
   depends on the framework (a Native AOT-only profile has different needs); check the actual image and thunk
   behaviour rather than disabling unrelated mitigations globally. COM and OLE
   initialised as STA on the main thread before `xlAutoOpen` [I].
8. **CLR hosting needs nothing from DNA Calc** beyond the above: the xll hosts its own runtime. The conflict is the
   selection and configuration of one hosted CoreCLR per process; .NET Framework coexists with it under its own
   side-by-side rules. DNA Calc itself must not load a .NET runtime, but another add-in or COM component can select
   one first [I].
9. **Lifecycle routes are distinct**: `xlAutoAdd`/`xlAutoRemove` are Add-in Manager actions, not part of every unload.
   On UNREGISTER Excel enters only `xlAutoClose` (all X2, X3 and X4/X5 Excel traces); the host's unload and close call
   only `xlAutoClose` since dc-6332.10 (they called `xlAutoRemove` first before). Excel-DNA's managed
   `IExcelAddIn.AutoClose` is a separate contract from native `xlAutoClose`.

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

## 3. COM object model: what add-ins touch, what comes first

- Late bound (Excel-DNA core; `dynamic` user code): `IDispatch` with Excel's names and DISPIDs.
- **Early bound against Excel's own type library**: xlOil `#import libid:00020813...` and calls vtables; Excel-DNA user
  code via the PIA or ExcelDna.Interop QIs for Excel IIDs such as `_Application`; xlOil's AppEvents sink switches on
  Excel's DISPIDs (SheetChange 0x61c, WorkbookOpen 0x61f, AfterCalculate 0xa34, ...). [V xlOil; PIA I]
- **Consequence for SYNTHESIS decision 9**: a projection with DNA's own IIDs fails every early-bound add-in at QI. For
  the Windows add-in surface the facade must answer Excel's IIDs with Excel's vtable order and DISPIDs; members not
  implemented return a fixed HRESULT and count as refusals. Decision 9's "complete contract" reads as complete
  layout and identity with honest per-member refusal. DNA's own LIBID can remain for DNA-only extensions.
- First tier (startup of the three frameworks): Window (`Application`, `Parent`, protected-view `Workbook`),
  Application (`Run`, `Version`, `Hwnd`, `AutomationSecurity`, `COMAddIns`, `Workbooks`, `ActiveWorkbook`,
  `ActiveSheet`), COMAddIns/COMAddIn (`Update`, `Item`, `Connect`), AppEvents connection point.
- Second tier (common at run time, xlOil's list plus Excel-DNA practice [I]): Range (`Value`, `Value2`, `Formula`,
  `Formula2`, `FormulaArray`, `Address`, `Cells`, `Rows`, `Columns`, `Areas`, `Count`, `Offset`, `Resize`,
  `SpecialCells`, `Clear`, `HasFormula`, `Worksheet`), Worksheet(s) (`Name`, `Range`, `Cells`, `UsedRange`, `Shapes`),
  Workbook(s) (`Name`, `FullName`, `Path`, `Open`, `Add`, `Save`, `SaveAs`, `Close`, `Names`), Names (`Add`, `Item`),
  Application (`Selection`, `ActiveCell`, `Calculate*`, `Calculation`, `ScreenUpdating`, `EnableEvents`,
  `DisplayAlerts`, `StatusBar`, `Caller`, `Evaluate`, `WorksheetFunction`, `Intersect`, `Union`, `Quit`), Windows.
- Third tier: ribbon XML (customUI 2009/07) mapped onto the command model (SYNTHESIS 1.5; a ribbon is a command
  source and a skin projection, never UI logic in the host), `IRibbonControl` callbacks through `IDispatch::Invoke`,
  task panes (ActiveX host in a native child window), CommandBars (legacy menus, Excel-DNA edit-mode fallback),
  VBIDE (`VBProject.VBComponents.CodeModule`, xlOil workbook-local functions; needs CalcVBA).
- Event argument order: xlOil reads AppEvents `rgvarg[0]` as the first declared parameter, the reverse of the usual
  convention: measure Excel's order [P].

## 4. Persistence of XLL functions

- xlsx: XLL function calls are written with the `_xll.` prefix (`=_xll.MyFunc(A1)`): Microsoft KB 2405796
  (https://support.microsoft.com/kb/2405796) on `_xll` in client formulas; XLParser's grammar accepts `_xll.`
  (https://github.com/PerfectXL/XLParser/blob/master/src/XLParser/ExcelFormulaGrammar.cs); DNA Calc's xlsb reader
  already maps add-in SupBook names to `_xll.` (`calc-doc/src/xlsb/formula.rs:732-739`). [V for the prefix]
- xlsb/BIFF: PtgNameX into an add-in SupBook's external names (same file). [V in code]
- On open: with the add-in loaded the name binds to the registered function; without it the cell keeps its cached
  value until recalculated, then `#NAME?`; an add-in loaded after the workbook does not recalculate those cells by
  itself (known Excel-DNA support question). Display form, the edit round trip and rebinding rule [P].
- VBA UDFs: workbook-local procedures are written by plain name; procedures from an .xlam are external references
  (`[1]!Func`, path-qualified when the add-in is missing) [I, P]. XLL and VBA names therefore resolve through
  different routes: `_xll.` names through the application callable table, VBA names through the workbook's project
  and its external-link table.
- RTD topics are cached in the xlsx volatile-dependencies part; Excel-DNA's ProgIDs are per session, so cached
  topics never reconnect to the same ProgID [I].
- Design: the one prefix reader/printer (SYNTHESIS 1.8) owns `_xll.`; the binder produces
  `Callable::Xll { name }` or an unresolved external name that keeps its text and evaluates to `#NAME?`; the saver
  writes `_xll.` for both. Registering a function rebinds through `BindingProof` probes of the callable table, the
  same mechanism as names (SYNTHESIS 1.2); whether Excel recalculates rebound cells is [P].

## 5. Threading and the scheduler

Excel's rules [V MTR page]: one main thread runs xlAutoOpen, XLL and VBA commands, VBA UDFs, XLL functions not marked
`$`, macro-sheet-equivalent `#` functions, COM add-in calls, CF formulas and **functions inside name definitions**;
other threads run only thread-safe cells. Thread-safe XLL functions may call `xlCoerce` (not on uncalculated cells),
`xlFree`, `xlStack`, `xlSheetId`, `xlSheetNm`, `xlAbort`, `xlGetInst`, `xlGetHwnd`, binary names; XLM information
functions fail `xlretFailed`, `xlfSetName`/unsafe `xlUDF`/unsafe `xlfEvaluate` fail `xlretNotThreadSafe`; no worksheet
function may call commands. A non-`#` function that coerces an uncalculated cell gets `xlretUncalced` and **is called
again** once the cell is calculated; a `#` function reads the old value [V xlfRegister page]. Async: only
`xlAsyncReturn` may be called from non-calculation threads during recalculation; arguments must be deep-copied [V].

Implications for DNA Calc (connects to SYNTHESIS 1.3-1.4):
- **One application main lane**: VBA's owner lane, the XLL main thread, the COM STA and the thread owning `XLMAIN` must
  be the same OS thread (Excel-DNA caches `_mainNativeThreadId` and enumerates that thread's windows). It runs non-`$`
  XLL UDFs, `#` UDFs, name-definition evaluation that calls them, commands, RTD server calls, events, ribbon callbacks.
- `$` XLL UDFs run on engine pool threads; each pool thread carries the per-thread XLL context and the permitted
  callback set. Return values are copied before the next call into the module on that thread (xlw's pool).
- **Uncalced retry is Excel semantics**: a non-`#` UDF that reads an uncalculated cell through `xlCoerce` aborts and is
  re-invoked later. Under SYNTHESIS 1.3 that is a **new `InvocationId`** in the same pass, not a scheduler retry, and
  its side effects can happen twice, as in Excel. It discovers a dependency at run time; the engine records it as a
  dynamic edge for the cell.
- **Read policies** map directly: `#` functions and `xlCoerce` in commands use `CachedMacroObservation`; non-`#`
  functions use `RegisteredWorksheetCallback`, whose answer for an uncalculated cell is `xlretUncalced`. Visible-first
  ordering changes which cells are uncalculated when a UDF reads them, so the observation frontier (decision 13)
  covers XLL reads as well as VBA reads.
- **Async** (`X`): the synchronous part runs on its lane; the handle is an `xltypeBigData` wrapping
  `(InvocationId, EvaluationTarget)`; `xlAsyncReturn` from any thread becomes `ExternalOperation::AsyncReturn` and is
  rejected if the formula moved, was overwritten or undone (already in SYNTHESIS 1.3). The calculation command waits
  for outstanding handles before `CalculationEnded`; Esc raises `CalculationCanceled` then `CalculationEnded`;
  neither is raised for programmatic recalculation [V doc]. Late returns after cancel [P].
- **RTD** is `ExternalOperation::RtdUpdate`: `UpdateNotify` (any thread) -> main lane calls `RefreshData` subject to
  the throttle interval -> topics dirty their caller cells. Each evaluation of a cell re-declares its topics; after
  the command, topics no longer declared are disconnected. Excel-DNA's `ExcelAsyncUtil.Run/Observe` depends on exactly
  this (disconnect when the formula stops calling `xlfRtd`) [V code comments `ExcelRtdObserver.cs:229-419`, timing P].
- Reentry: a UDF calling `xlUDF`, `xlfEvaluate` or COM while the main lane waits on calculation is the reentry case of
  SYNTHESIS 1.4; requests go through the invocation's frame, never the general inbox. COM calls from `$` threads
  marshal to the STA and need the same rule or they deadlock [I].

## 6. Proposed architecture insertion

### 6.1 Crates (extends SYNTHESIS section 3)

| Crate | Owns | Depends on |
|---|---|---|
| `calc-xll-abi` (native-only) | `#[repr(C)]` XLOPER12, FP12, XLREF12, XLMREF12 written from the documented layout (no SDK header in the repo); return codes; type-string parser to `XllSignature`; XLOPER12 <-> calc-value conversion with a per-invocation arena | calc-value, calc-func-catalog (Ftab/Cetab numbers) |
| `calc-xll-host` (native-only, Windows) | module loading and lifetime; registration table (register ID, use count, hidden name, entry pointer, free module); `MdCallBack12` dispatcher; per-thread call context and permission table; ownership of `xlbitDLLFree`/`xlbitXLFree`; async handle table; event registry; `xlfRtd` bridge; `XLCALL32.DLL` shim as a small cdylib | xll-abi, calc-fml (`CallableTable`, `FunctionContext`), object-model contracts |
| `calc-com-host` (native-only, Windows) | Excel-IID projection of `calc-object-model` (generated, SYNTHESIS 1.6); `XLMAIN/XLDESK/EXCEL7` windows with `WM_GETOBJECT`; COMAddIns and `IDTExtensibility2`; ribbon and CTP hosts; RTD client; AppEvents/WorkbookEvents connection points; `Application.Run` dispatch to XLL and VBA commands | object-model, xll-host (Run), windows |
| `dna-calc-desktop` (new app) | Win32 main thread (STA, message loop), shell (WebView2 hosting the Leptos UI is the cheapest path [I]), composition of worker, xll-host, com-host, vba | as today's app plus the above |

Rules: calc-fml never sees an XLOPER; calc-engine never depends on calc-xll-host; the app composes. Since the XLL host
sits on calc-fml and a host-services trait, **OneCalc on Windows can host XLL UDFs without the engine** (keeps the S8
honesty check; small first target); its host services refuse `xlSet`, commands and uncalculated reads, typed.

### 6.2 Contracts

- `CallableEntry::External { id, generation, signature: CallableSignature, lane: MainLane | AnyLane, source:
  Xll { module, register_id } }`; `CallableSignature` carries per-parameter kind (value, value-or-reference,
  number array, in-place, async handle), return kind, volatile, macro-equivalent, thread-safe, cluster-safe, async.
  Facts stay separate properties (SYNTHESIS 1.8 policy rule).
- calc-fml invokes through `FunctionContext.callable_invoker` with calc-value arguments plus reference descriptors
  for `U`/`R`; the invoker (xll-host) marshals into the arena, calls the entry on the declared lane, copies the result,
  calls `xlAutoFree12` of the free module, returns calc-value. Reference arguments stay references until the XLL
  coerces them through the invocation's read view.
- C API callbacks reach the engine only through `InvocationFrame` (SYNTHESIS 1.6): `xlfCaller` answers from
  `EvaluationLocus`; `xlCoerce` reads via `ReadRequest` with the function's policy; `xlSet`, `xlcFormula` and other
  commands produce a `PreparedTransaction` through a `CommandEnvelope` (only in command context); `xlfEvaluate` and
  `xlUDF` evaluate as nested invocations.
- Registration events change the callable table at an execution boundary (code+state lease rule of 1.6);
  `xlfUnregister` decrements use count; module unload waits for outstanding async handles and RTD servers.

### 6.3 Shimming strategy

1. **General shims first**, each backed by an Excel probe and a test: exe export, XLCALL32 shim, version resource and
   `xlfGetWorkspace` values, window hierarchy and class names, HKCU COM registration as written by the frameworks,
   Excel IIDs/DISPIDs, exact return codes outside context, `Application.Run` return values.
2. **Add-in-specific shims** only when a general one cannot work, as data keyed by module name and version, each
   with a counter and a reason: e.g. Excel-DNA IntelliSense (UI Automation of Excel's formula bar: decline cleanly),
   xlOil workbook-local functions (VBIDE module writing: route to a native registration once CalcVBA exists).
3. Never change a user's Trust Center or Office registry settings; read Office keys only where a framework writes
   them for this load (COM add-in keys). Whether DNA Calc reads Excel's `OPENn` add-in list is an owner decision.

### 6.4 Staged plan

- **X0 probes** (Excel here, AGENTS.md allows add-ins): build a tracing XLL that logs every callback, argument and
  return code, load Excel-DNA, xlw and xlOil samples behind it in real Excel and keep the traces as the oracle for
  DNA Calc's dispatcher. Plus the [P] items above: out-of-context codes, uncalced re-call count, `Application.Run`
  return of a `>B` command, AppEvents argument order, `_xll.` display and rebinding, RTD disconnect timing, async
  after cancel, `xlfCaller` in names, CF and arrays.
- **X1 shell**: `dna-calc-desktop` main thread, windows, exe export, XLCALL32 shim; our own fixture XLL in Rust
  (`Q`, `B`, `$`, `!`, `#`, `X`, `xlbitDLLFree`) on OneCalc first, then DNA Calc.
- **X2 Excel-DNA hello world**: `=SayHello("x")` under .NET 8, .NET Framework 4.8 and Native AOT; `xlAutoOpen`,
  `fN` registration, `xlAutoFree12`, `XLCallVer`, `LPenHelper`, `xlEventRegister`, Application via
  `AccessibleObjectFromWindow`, `QueueAsMacro` via `Application.Run`, `xlAutoClose`/reload.
- **X3 xlw**: generated C++ sample (`Q B K% U`, `$`, menus via `xlfAddMenu` mapped or recorded), then xlwDotNet.
- **X4 xlOil**: core UDFs, then xlOil_Python: export-table rewrite, early-bound COM via Excel IIDs, AppEvents,
  `xloRunInXLLContext`, `CoRegisterClassObject` servers.
- **X5 concurrency and async**: `$` on the pool with the permission table; native async; RTD client; Excel-DNA
  `ExcelAsyncUtil.Run/Observe`; cancellation and events. **X6 UI**: COM add-in host, ribbon into commands, task panes.
- **X7 later**: Excel4/XLOPER (add-ins of any framework that call Excel4 or register legacy types, old commercial XLLs), 32-bit build, cluster connector, PyXLL.

### 6.5 Test list

1. Exe export resolves through `GetModuleHandle(NULL)`; `XLCallVer`/256 = 12; `LPenHelper` returns 0 and ready mode.
2. Register/unregister/re-register: register ID, hidden name evaluating to it, use count, wizard-name removal; the
   xlOil pattern (one export name, patched between registrations) calls two different entries.
3. Each type code round trip (`A B E J Q U K% D% C% L` and digit in-place) including Missing, Nil, Multi with blanks
   as Nil, multi-area Ref, 32,767-character strings, 1,048,576-row Multi.
4. `xlbitDLLFree` calls `xlAutoFree12` of the registering module once per result, on the calling thread [P]; `xlFree`
   on every result shape, twice; xlw pool results copied before the next call.
5. Callback permission table per context (function, `#`, `$`, command, async thread, outside): codes equal Excel's trace.
6. Uncalced: a non-`#` UDF reading a later cell is re-invoked after it; a `#` UDF sees the old value; counts match Excel.
7. Volatile `!`, `#`+`U` default volatility, `xlfVolatile(FALSE)`; `$` functions concurrent on the pool, non-`$`
   only on the main lane (thread-id fixture).
9. Async: results after overwrite/undo/reload rejected; Esc raises Canceled then Ended; no events on programmatic recalc.
10. RTD: connect, update throttle, disconnect after the formula stops calling `xlfRtd`, server terminate on unload.
11. `Application.Run` of an XLL command with C API access and Excel's return value; `QueueAsMacro` drains.
12. COM: `AccessibleObjectFromWindow` on `EXCEL7`; QI for `_Application`, `Window`, `Range` IIDs; DISPIDs; AppEvents order.
13. COM add-in load via HKCU keys and `COMAddIns.Update/Item/Connect`; ribbon commands in the command model; callbacks.
14. Persistence: `_xll.` round trip (xlsx, xlsb); open without the add-in keeps text and cache; later load rebinds as Excel.
15. Unload: `xlAutoClose`, outstanding handles, `FreeLibrary` after the last use; X2-X4 samples run unmodified vs X0 traces.

## 7. Decisions for the owner

| # | Decision | Recommendation |
|---|---|---|
| A1 | Native Windows desktop process with a Win32/STA main thread as the XLL and COM host (no native shell exists today) | Yes; WebView2 shell over the existing worker first |
| A2 | Excel IIDs, DISPIDs and vtable order in the Windows COM projection (revises decision 9) | Yes for the add-in surface; own LIBID only for DNA-only extensions; layout taken from observing Excel's type library, licence position to confirm |
| A3 | Process identity: executable name and version resource | Version 16.x now; set `EXCELDNA_ASSUME_EXCEL_PROCESS` in the process environment; naming the exe EXCEL.EXE only after measuring what older Excel-DNA builds lose without it |
| A4 | 32-bit build | Later (X7), only for 32-bit-only XLLs |
| A5 | Legacy XLOPER/Excel4 | Later (X7); refuse with `xlretFailed` and count |
| A6 | Read Excel's own add-in list (`OPENn`) and COM add-in keys | Read COM keys written for the current load; do not share Excel's `OPENn` list; DNA Calc keeps its own |
| A7 | One main lane for VBA, XLL, COM and windows | Yes (sec. 5); it fixes where `#` and name-definition UDFs run |

## 8. Owner note (2026-10-31): COM activation stays an open design area

Intercepting or virtualising COM activation inside the DNA Calc process is to be investigated, but
the compatibility design is not chosen yet (BRIEFING 4.10). Candidates to evaluate, none adopted:
per-process class objects (CoRegisterClassObject), registration-free COM through activation-context
manifests, a process-local Running Object Table entry for GetObject, and hooking activation calls.
Hard constraint: nothing may change or shadow the machine's COM registrations for Excel (ProgIDs,
CLSIDs, type libraries, interfaces) in HKCU or HKLM; Excel must keep working exactly as before.

## 9. X0 measurements (2026-11-01, dc-snz.16)

The source, runner, verifier and small text captures are in
[`crates/calc-xll-reference`](../../crates/calc-xll-reference/README.md). This is a native x64
debugger observing real Excel, not a DNA Calc dispatcher or a second calculation verdict path.
It loads the source-built fixture with `Application.RegisterXLL` and observes the original
Excel-DNA NuGet XLL and loader. Excel files, its COM registrations and security settings are
unchanged. Temporary breakpoints are confined to the newly created Excel process and restored
before detach. No disassembly was used.

These observations apply to Excel **16.0.20527.20088 x64**, Windows, en-ZA, and
**ExcelDna.AddIn 1.9.0**, fixture target **net48**, CLR **v4.0.30319**. The captures retain binary and source hashes.
QPC times include debugger pauses and thread suspension; they do not establish normal latency
or an unperturbed calculation schedule. A separate untraced run checks worksheet outcomes.
The retained run has 2,524 records, 1,207 paired boundary/COM calls and 117 C API callbacks;
all calls have matching returns, and detach reports zero restore failures or outstanding calls.

### 9.1 Registration and execution

The observed type strings settle the fixture registrations, rather than every signature a
framework can produce:

| Fixture | Type string |
|---|---|
| `X0.Hello(string)` | `QD%` |
| `X0.ThreadSafe(double)` | `QE$` |
| `X0.Reference(object, AllowReference=true, IsMacroType=true)` | `QU#` |
| `X0.Caller()` (macro type) | `Q#` |
| `X0.Big()` (mixed array) | `Q` |
| `X0.Async(double, ExcelAsyncHandle)` | `>EX` |
| Excel-DNA `SyncMacro_<guid>` | `>B`, macro type 2 |
| Native `X0.NativeOwned()` | `Q` |

The original Excel-DNA binary exports 10,000 numbered `fN` stubs and no `xlAutoRegister12`.
The tracer observes `xlAutoOpen` and discovers the actual `fN` entries from `xlfRegister`
arguments. `$` calls occur on calculation worker threads; reference/caller and ordinary calls
occur on the load thread in this fixture (64 `$` invocations across 16 calculation threads).
Two native async completions call `xlAsyncReturn` (16400)
from background threads and receive return code 0. This does not test cancel, overwrite or a
late handle. `XLCallVer` returns 3072 (0xC00), not a 16.x Office build number.

`ExcelReference` coercion uses `xlCoerce` (16386). The reference descriptors and sheet IDs are
retained in the trace, including Excel's SRef-to-Ref steps. `X0.Caller` identifies B3 as zero-based
rectangle `[2,2,1,1]`. The queued macro and explicit command contexts receive return code 0
from `xlfCaller` with an `xltypeErr` value of 23 (`#REF!`), not a cell caller.

On a ThreadPool thread outside any calculation or command, this Excel-DNA fixture calls
`xlGetName` (16393), `xlStack` (16385), and `xlfCaller` (89): each returns **32 (`xlretFailed`)**.
Excel-DNA then attempts `xlFree` (16384), which also returns 32 there. This narrows the
out-of-context [P] in section 2: do not assume `xlretInvXlfn` for this measured route. It does
not complete the full permission table for all thread and function contexts.

`QueueAsMacro` executes through `Application.Run` of the registered `SyncMacro_<guid>`;
`LPenHelper` is also observed. The queued body has C API access and `xlGetName` identifies its
own Excel-DNA XLL even after a second native XLL has loaded. Application.Run of the fixture's
zero-argument commands returns Boolean TRUE in the controller. The two observed Run calls of
`SyncMacro_<guid>` return `VT_R8` (5), value 0; this settles that `>B` command's Run return
for this build, separately from the fixture commands' Boolean return.

### 9.2 Ownership, COM and persistence

The 1-by-4096 mixed-array return is observed with type 64, without DLLFree. Source expectations
about allocation thresholds are not substituted for that binary observation. A separate small
native fixture explicitly returns an allocated counted string with `xlbitDLLFree`; Excel calls
`xlAutoFree12` once, with the exact returned pointer and on the same native thread.
This measures the single-thread ownership case, not every shape or a worker-thread free.

Application lookup is observed as `AccessibleObjectFromWindow(OBJID_NATIVEOM, IID_IDispatch)`,
then Window.Application through IDispatch (DISPID 148 / 0x94). The fixture reads Version (392 /
0x188) and Hwnd (1950 / 0x79E); Excel-DNA's macro queue uses Run (259 / 0x103). COM traffic from
the external controller is also retained, so its Ready, calculation and workbook calls must not
all be attributed to the add-in. The managed fixture log identifies its own explicit accesses.

**AppEvents argument order [measured]:** FindConnectionPoint requests
`00024413-0000-0000-C000-000000000046`; Advise installs the fixture's sink. SheetChange arrives
at IDispatch::Invoke as DISPID **1564 (0x61C)** with `wFlags=3`, two `rgvarg` entries and named
DISPIDs **[0,1]**. `rgvarg[0]` is the worksheet (Name returns `X0`), and `rgvarg[1]` is the range
(Address returns `$A$1`). This settles section 3's ordering question for this event and Excel
build: preserve the named-argument mapping rather than blindly reversing the array. The sink
also observes SheetCalculate (1563 / 0x61B) and AfterCalculate (2612 / 0xA34). AppEvents
AfterCalculate is distinct from the C API CalculationEnded event; these observations do not
change the C API event claim in section 5.

Excel saves these registered UDF formulas into xlsx with `_xll.` prefixes, including nested
`SUM(_xll.X0.Big())`. The saved package is inspected for the recorded formula text and caches.
No missing-add-in reopen, rebinding or xlsb result is claimed.

`ExecuteExcel4Macro("UNREGISTER(<path>)")` returns TRUE and calls native `xlAutoClose`.
For Excel-DNA this is not proof of managed AutoClose, AppEvents Unadvise, CLR unload or
FreeLibrary: Add-in Manager removal has a different route. Full removal/reload stays [P].

### 9.3 What remains open

The automatic Excel-DNA ribbon/COM-add-in helper is not run because its
`AutomationSecurityOverride` can change Application.AutomationSecurity, prohibited in this
session. Ribbon activation and DllGetClassObject therefore remain unmeasured. RTD-based async,
.NET 8/Native AOT, cancellation/late returns, uncalculated-cell retry, the complete permission
table, early-bound COM vtables, legacy Excel4 and x86 also remain open. xlw and xlOil were not
built or loaded; the reference README records the upstream source-build requirements. These
captures establish X0 observations, not compatibility of a DNA Calc host that does not yet exist.

## 10. X1 native host (2026-11-01, dc-snz.17)

`calc-xll-abi` now supplies XLOPER12/FP12 layouts, registration type parsing and
value arenas. `calc-xll-host` loads XLLs, retains registration-time addresses and
the separate originating and free modules, dispatches C API callbacks and copies
results before releasing their storage. Failed loads restore the previous names
and use counts. The caller-owned `UserFunctions` registry connects it to CalcFml.
The existing Ftab table moved from CalcDoc to CalcFml so both consumers share it.

The minimal `dna-calc-desktop` executable creates an STA and hidden Win32 window
hierarchy, exports MdCallBack12, carries version 16.x and calculates through the
ordinary OneCalc controller. `calc-xll-shim` builds its XLCALL32 companion. There
is no native dependency in the browser application closures.

The [same-binary fixture comparison](../../crates/calc-xll-reference/traces/2026-11-01-x1/summary.json)
records 21 agreeing formulas, including `=MyAdd(1,2)` = 3, and the same ordered
127 callback numbers and return codes in Excel 16.0.20527.20088 and DNA Calc.
Exact-pointer/same-thread DLLFree ownership and two addresses registered through
a rewritten export are checked. Native process tests cover 10,000 extra names,
helper-module ownership, failed-load rollback and refusals that survive handlers.

`Excel12(xlfCall)` and `Excel12(xlfRegisterId)` (dc-gh22) run from an .xll in both hosts. The
[comparison of 2026-11-08](../../crates/calc-xll-reference/traces/2026-11-08-x1-call/summary.json)
extends the X1 cases to 289 formulas (Excel 16.0.20527.20150, same fixture binary): every case
agrees, with the same ordered 599 callback numbers and return codes. Measured rules, all in
`fixture/x1-cases.json`: both need a macro-equivalent (`#`) registration or xlAutoOpen (a plain
worksheet function gets return code 2 and #VALUE! in the result); zero CALL arguments or a wrong
REGISTER.ID count return code 4 and #VALUE!; every other failure is #VALUE! (or #NUM! for an
out-of-range integer) with return code 0. REGISTER.ID returns the first live registration of the
module's export (module name case-insensitive, with or without `.dll`; procedure name case-sensitive)
or registers it when a third argument is present; a text third argument must be valid type text.
CALL by module, procedure and type text registers an unregistered export too; one UNREGISTER ends
it. More arguments than the type text declares is #VALUE!, fewer are zero. Integers round half up
(unsigned types take the absolute value first); strings use the ANSI code page with `?` for
unmappable characters. Not measured, so refused: types `O`, `P`, `R`.

This establishes X1's own fixture, not Excel-DNA or general XLL compatibility.
Synchronous entries stay on the main lane; async waits for one completion.
COM objects, command integration, worker scheduling and complete async lifetime
behavior remain open. The raw xlAsyncReturn result operand differs from this
Excel build; the documented Boolean is returned by DNA Calc, while callback
codes and published values match. See the reference folder's discrepancy note.

## 11. X2 Excel-DNA host (2026-11-01, dc-snz.18)

The [retained X2 evidence](../../crates/calc-xll-reference/traces/2026-11-01-x2/)
runs original ExcelDna.AddIn 1.9.0 packages under net48 and net8.0-windows.
Each runtime matches six Excel-DNA UDF cases and the native DLLFree control
(7/7). Original NativeAOT 1.10.0-preview5 on net10.0-windows matches hello (1/1).
Separate controls without the debugger agree. Source and binary hashes are
retained; publication records identify code captured before its commit existed.

The host keeps its executable name, carries version 16.x and sets
`EXCELDNA_ASSUME_EXCEL_PROCESS=1` before creating threads. A separate original
1.10.0-preview6 loader/PIA fixture confirms process identity, Framework AppDomain
isolation and ExcelDna.Interop 15.0.1 calls through Excel's Application IID and
vtable. Version, Hwnd and Run work. Window is a seven-slot dispatch interface;
Application's dual interface has 474 slots on the measured Excel build.
No COM activation mechanism is adopted; no Excel registrations or HKLM settings
are written. Application comes from the host's own EXCEL7 window.

The complete original 10,000-export pool registers; f0 and f9999 execute their
different delegates in Excel and the host. Empty-type commands read the native
Boolean return (TRUE for these Framework wrappers, FALSE for these net8 wrappers).
SyncMacro's `>B` returns R8 zero. QueueAsMacro executes on the main STA through
Application.Run, with its own XLL context after another XLL loads. Caller and
reference descriptors/coercion, sheet names/IDs and outside-call code 32 follow
the X0 observations. Thread-safe UDFs run on calculation workers; the evaluator's
reader remains on its owner thread. This does not establish overlapping formula
execution. Unimplemented worker mutations/registered calls are typed refusals.

Two raw async differences remain visible: xlAsyncReturn's result operand is
type zero in Excel and the documented Boolean in the host; net8's incoming
opaque handle cbData also differs. Excel-DNA's marshaler reads hData only on
that entry path. Defined values and ordered nested UDF callbacks agree; this
is not a claim that all raw bytes agree. Native async publishes its result from
a background thread; cancellation, overwrite and late-return integration remain X5.

Framework and net8 remove/close/reload work. The Native AOT preview aborts when
its still-loaded runtime installs an import resolver a second time. The host
refuses repeat packed-runtime initialization across hosts and XLL aliases;
dc-snz.19 owns safe reinitialization. The local AppEvents connection point supports
subscription/removal only. Delivered host events, RTD, the complete permissions
table, general COM objects, Excel4 and x86 remain open. The merged callable-table
integration keeps the existing 1 MB LAMBDA stack test by retaining block cursor
storage on the heap, without changing the test's limit or assertions.

The corpus remains 18,812/20,030 matched (93.9191%), with 1,131 failures
(1,124 mismatches, six refusals and one not run), 73 external-link cells and
14 circular-rule cells. All cell verdict records are unchanged. The structural
run passes layering, OS imports, native wasm refusals, provenance and dependency
ceilings; its token scan flags the merged VBA adapter's external HAL type name
(dc-hsa5.4.1). This is a partial sweep: browser, app-smoke and acceptance were
skipped, and the failed scan is retained.

Final integration `053481ad` retains the heap fix while adopting the shared
reference-lifetime API. XLL scalar argument reads remain dependencies, and `#`
callback reads select the cached policy; worker requests preserve that service
distinction. Nested registered calls that switch policies are typed refusals.
All original runtime/control comparisons were repeated on the combined source;
Windows checks pass 1,529 tests and every corpus cell record remains unchanged.
Distinct live/cached native forwarding is source-reviewed without a dedicated
two-reader regression. The final evidence is under the X2 `integrated` folder.

## 12. X3 xlw and xlOil host (2026-11-01, dc-snz.20)

The [X3 evidence](../../crates/calc-xll-reference/traces/2026-11-01-x3/) uses the
original xlw 23.10.12-rc generator/library and xlOil 0.22.1 native/Python binaries.
The unchanged xlw sample, a separate scalar/matrix/reference fixture, xlOil Utils
and SQL functions, and three Python functions produce 27 agreeing formula cases
and 28 matching native UDF invocations. Separate controls agree without debugger
instrumentation. The comparison checks ordered UDF callbacks and 17 exact-pointer,
same-thread frees through their owning core/plugin modules before reuse.

The host reads packed XLOPER12 operands and UTF-16 strings without assuming native
alignment, supplies the actual executable instance handle, and obtains regional
settings from Windows NLS with English interface/function syntax. Other locales
remain unverified. Lifecycle FILTER calls use the existing CalcFunc path.
COM adds the observed Window.Parent workbook, its application/name/path properties,
and the Workbooks collection. This is one Book1 object, not a general workbook
object model. No Excel registration or security setting is changed.

Initialization callback numbers and codes agree in order. Raw FILTER element types,
the xlEventRegister result payload and Python registration order differ and remain
in the summaries. Teardown actions differ between the Excel runner and host and
are counted separately. An unprotected xlOil reload crashes; the exact measured
loader now returns a typed refusal after teardown, across host instances in the
same process. That refusal is an intentional divergence from Excel (Excel crashes),
not an equality result, and it counts as a failure. Safe reload remains dc-snz.21; Native AOT remains dc-snz.19.
Delivered events, RTD, cancellation, workbook concurrency, UI/ribbon behavior,
general COM, xlwDotNet, Excel4 and x86 have not been established by these cases.

## 13. X4 worksheet interfaces and AppEvents (dc-snz.22)

The Windows projection now exposes the measured `_Worksheet` IID and its complete
158-slot layout, plus Excel's dispatch-only Range interface for single-cell event
targets. Application.ActiveWorkbook/ActiveSheet, Workbook.ActiveSheet and worksheet
Name use the measured slots. Range identity, parent, local/qualified address and bounds are available;
qualified names use the shared reference printer, including apostrophe escaping;
general Range value/formula editing remains unimplemented and returns a failing HRESULT.
No type library, Excel COM identity or security setting is registered or changed.

AppEvents subscriptions belong to the source across connection-point lookups. The
desktop composition delivers SheetChange after committed OneCalc edits and
AfterCalculate after calculation. Named arguments preserve Excel's measured
declaration order. A native regression checks fan-out, EnableEvents suppression,
reentrant Unadvise, cross-point cookies and disconnected retained Range objects.
The fixture running with the original xlOil Python plugin reads these properties and retains
an event range across edits. Its event callback enters `xloRunInXLLContext` through
Application.Run; hidden macro-equivalent registrations use the ordinary native
invocation path on the STA. Async Application.Run remains a typed refusal.

The reference runner now measures close/open in Excel as well as the host. Original
xlOil 0.22.1 also crashes in Excel after UNREGISTER followed by RegisterXLL, with an
access violation retained by the debugger. The source destroys its COM connector
on close without recreating it on open. Safe runtime reinitialization remains
dc-snz.21; the existing measured-runtime refusal is retained and is not a pass.
Python COM references and completed futures in this fixture are explicitly released
before normal teardown. The unreleased-future failure is kept outside the repository.
The built-in Python import runs asynchronously and temporarily suppresses Python
events. The reference composition waits for a fixture AfterCalculate acknowledgment
after that import, before measuring edits. Captures that raced startup or teardown
remain failed observations. Startup context commands are bounded before the first
SheetChange and their exact built-in registrations are checked separately.
These cases do not establish general COM, arbitrary Python teardown, RTD or safe reload.

## 14. X5 native async owner route (dc-snz.23)

The desktop `--engine` composition calculates through CalcHost. Native async entries
can opt into its cooperative callable route: one native entry produces an opaque
request, the existing activation parks, and return messages are resolved to the
complete owner-issued P07 token. Only CalcHost.complete_async admits the value;
resuming consumes that checked return without running the native entry again.
The normal OneCalc controller retains its explicit synchronous completion path.
Module removal refuses while an admitted native handle remains outstanding.
Dropping an activation invalidates its handle; request numbers are never reused.
An entered canceled call retains its module lease until a late return attempt is
observed. A task that never returns keeps removal refused and its code resident
until process exit. Normal teardown still asks the add-in to drain its own work.
Thread-safe native async entries and asynchronous Application.Run remain refusals.
Synchronous `$` entries can also use an owning cooperative activation. Only owned
arguments, callback requests and results cross the pool boundary; evaluator reads
are serviced on the owner. Two workbook commands can interleave these activations.
The native barrier fixture establishes overlapping entry before either call
returns, while a single workbook still services a suspended cell first.
Queued calls retain both originating and executable-module leases. Dropping a
pool activation cancels and disconnects callback replies before releasing its
owner. Final host drop retains a pool with outstanding work instead of joining a
hung task. Unsupported ABI failures remain typed refusals through IFERROR.
An overwrite case checks P07 token revocation, the delayed native return code256,
and the replacement cell/dependent values. The controlled RTD client and callback
permission observations below extend this route; general coverage remains open
under dc-snz.23.

## 15. RTD numeric inputs on P07 (dc-snz.23)

The native COM client implements the measured Excel IRTDUpdateEvent IID and calls
IRtdServer on the owning STA. UpdateNotify only sets an atomic notice; the owner
polls RefreshData and copies scalar values. Initial and refreshed inputs pass
through CalcHost.update_rtd with the exact callable generation, subscription and
monotonic sequence. The desktop controller explicitly requests another manual
calculation after admitting an input. Existing subscriptions remain declared reads
when an add-in returns a cached value. A formula replacement retires its topics
after the edit succeeds. RTD never completes a parked native async invocation.

The client only reads ProgID mappings and registers no class. Its callback owns
independent notification state, including after disconnection. Close runs before
XLL teardown; public module removal refuses while RTD servers remain. A failed
termination, including cleanup after a failed start, keeps the failure and native
server/module leases until process exit, so repeating close cannot convert it to
success. An ordinary failed start with successful cleanup releases those leases.

The existing native ownership fixture has an optional controlled numeric RTD
server and callback-permission functions. That test server creates unique private
HKCU DnaCalc.X5 classes, rolls back failed setup, and removes its own keys on close.
It never changes Excel registrations. Numeric controls produce55 and dependent110
from admitted values1 and42; the final certificate retains sequence2. Replacing
the formula produces99/dependent198 and disconnects the subscription. Excel uses
IDispatch for this server; the host uses its early-bound interface. The traced
comparison checks ServerStart, ConnectData, UpdateNotify, RefreshData,
DisconnectData and ServerTerminate with their observed payloads, plus independent
controls. Excel's native UDF returns1 then42; the host first returns temporary
#N/A before admitting its first input, then1 and42. This difference remains in the
evidence. These are controlled numeric inputs, not general RTD compatibility.

The original qualification refused empty RTD values. Original xlOil Python uses
empty RTD signals and a separate Python result cache; a numeric coincidence of55
without a retained RTD read was a failed qualification. The client now admits an
empty value with its sequence through the same P07 route. The C API projects that
admitted value as numeric zero, as observed in Excel, while the certificate keeps
the empty input. Heartbeat age follows the last UpdateNotify or heartbeat, with
monotonic notification timestamps across provider threads. Arrays, remote servers,
nontext topics, general signaling-cache providers and arbitrary teardown are open.

## 16. X4/X5 measured limits, 2026-11-01

Evidence and exact source bindings are retained under
`crates/calc-xll-reference/traces/2026-11-01-x4-x5`. The original xlOil Python
fixture agrees on all 12 formula cases and the defined operands and callbacks of
13 measured native invocations. Raw undefined xlFree result operands still differ.
Both hosts deliver13 SheetChange calls with the measured argument types and order,
but nested COM property calls differ at three event positions. The strict
X4 verifier therefore fails. Matching outputs do not establish matching event
traffic, and dc-snz.22 remains open.

The callback matrix measures13 requests under each of normal, macro-equivalent
`#`, and thread-safe `$` registrations. Excel's registration flags, native
arguments and main/pool thread placement are checked explicitly. The host
calculates matching results for37 of39 requests. General EVALUATE in normal and
macro-equivalent calls remains a typed refusal under dc-snz.2, so both requests
fail. The measured `$` EVALUATE and MESSAGE denials return128; the host preserves
these ordinary return codes rather than treating a handled denial as an
unsupported ABI. This is evidence for these exact requests and contexts, not a
general assertion about every expression or callback payload. Macro MESSAGE
updates the owning logical message state; UI painting is unmeasured.

The native barrier test establishes overlapping pool entries from two workbook
commands. A single workbook still services its suspended cell first. Native async
tests exercise one engine activation, dependent publication, overwrite revocation
and a rejected late return through the application composition. Broader concurrent
workbook scheduling, cancellation cases and provider teardown remain open under
dc-snz.23. Safe xlOil reload remains open under dc-snz.21.

## 17. Corrections to the X4/X5 observations

The X4 tracer now discovers the worksheet and Range arguments of every
SheetChange. The former global discovery flag missed later event objects; xlOil
also uses the worksheet pointer without another QueryInterface. The verifier
requires Worksheet.Name and both local and qualified Range.Address reads in each
expected event, so equal incomplete traces cannot pass. RTD factory observations
retain the actual requested IID, returned object and enclosing xlfRtd callback.
Unrelated COM factories are not presumed to be RTD servers.

EVALUATE can evaluate a constant expression through the current CalcFml compiler
and block runner, including its ordinary array and LAMBDA operations. It uses the
owning formula's services. References, defined names, registered calls, volatility
and escaping callable values remain typed refusals; a native add-in cannot hide
that refusal by discarding the callback result. The measured normal and `#`
requests evaluate `1+1`; the measured `$` denial remains128. The native fixture
retains the result type and exact double bits for comparison with Excel. Nested
retained kernel calls live on the heap so the existing one-megabyte recursion test
continues to exercise the full evaluator depth after the shared helper changes.

The corrected X4 comparison checks all 12 cases and13 SheetChange deliveries,
including Worksheet.Name and both addresses in every event. The permission matrix
now matches all39 requests, including result2 and its exact double bits for the
normal and `#` EVALUATE calls. Original xlOil empty signals produce55/dependent110
with a sequence3 P07 certificate, followed by replacement99/dependent198 and topic
retirement. The strict RTD protocol comparison still fails: Excel makes11 client
calls, including three Heartbeats, and the host makes eight. The other eight
method occurrences and their payloads match. A traced completion deadline failure
is retained beside a separate successful capture of the same executable.

## 18. Within-workbook scalar pool calls (dc-snz.23)

A callable opts into a fixed maximum of independent scalar activations when it
registers. State lanes, cached reads, semantic reevaluation and reentry keep the
default single activation. Synchronous `$` XLL adapters publish their pool's
capacity. Only explicitly legacy scalar cells with eligible calls may share a
segment. Dynamic spill formulas, legacy array producers and volatile plans remain
ordered; iteration and data-table calculation retain their existing scheduling.

The scheduler checks static prerequisites and shares service by age. A suspended
pure prefix reserves capacity until its cell completes, so a later prefix cannot
start another call beyond the limit. All fallback admissions check that limit.
An additional prerequisite discovered inside a concurrent callback returns a
typed dc-snz.23 refusal instead of consuming another slot or becoming a circular
result. Native async continues to use its separate P07 completion route.

The reference fixture selects legacy form and Manual calculation in both hosts.
Excel also evaluates these formulas on its owner thread during entry. Its extra
entry calls remain part of the strict comparison. The fixture resets its existing
barrier after entry and before Calculate, in both hosts, to measure actual worker
overlap independently. An overlap witness is not a full lifecycle compatibility
pass; differing native invocation or owned-result counts remain failures.

## 19. R7 pool, RTD and bounded native reload observations

The new captures in `crates/calc-xll-reference/traces/2026-11-03-xll7`
supersede the strict RTD and pool failures recorded in sections 17 and 18. The
earlier failed captures remain unchanged. Formula entry now uses an explicit
single-cell selection through the existing engine. Its native calls run on the
owner even for a `$` function; a later Manual Calculate uses the worker pool.
The full pool comparison matches all 15 ordered native calls and two owned frees,
including arguments, results, nested callbacks, thread placement and module/pointer
ownership. It retains the two-worker overlap witness. Asynchronous native formula
entry remains a typed refusal.

The tracer now maintains thread IDs from debugger thread events instead of taking
a machine-wide thread snapshot at every breakpoint. The original signaling RTD
case has eight ordered client calls in both hosts, with equal payloads. The earlier
11-versus-eight failure is retained. An explicit 22-second idle extension exposed
an orphan transport: the host kept its native topic after the generator completed.
Completed synchronous invocations now reconcile their actual RTD declarations,
disconnecting an unused transport while retaining the final P07 input certificate
until formula replacement. Both hosts disconnect the completed generator before
the idle interval, with no Heartbeat. A persistent numeric topic instead receives
exactly one Heartbeat in that interval: seven client calls in each host.

The numeric fixture also calls its idempotent ServerTerminate body from its final
Release destructor. The verifier retains that body and requires a same-object,
same-thread, zero-reference Release from the client, following successful external
termination. It does not invent a second client call. A copied trace changed to
report one remaining reference is rejected. General topic reuse after completion,
arrays, remote RTD, spill concurrency and broader cancellation remain open under
dc-snz.23; a completed transport cannot reconnect before its logical input retires.

The original xlOil 0.22.1 native Utils/SQL profile can now reopen in the DNA Calc
process, with both the same host object and a replacement host object. All12
formula outcomes and all 13 ordered native calls agree with Excel's complete first
load and with the host's second load. Owned returns are freed by the same module,
pointer and thread before the next native entry. The trace establishes actual
unmapping and remapping of the XLL, core and both plugins, rather than reuse of a
partly torn-down runtime. Old callable snapshots return #NAME? after removal.
The original Excel UNREGISTER/RegisterXLL controls still fail with RPC loss and a
traced second-chance access violation. Their failed reload and debugger diagnostics
remain failures. Security is read as unchanged before teardown; after the process
crashes it cannot be read again.

The repair is a compatibility adjustment for exact hashes in
`fixture/x3-upstream.json`, informed by xlOil 6aff3a4f
`src/xlOil-Loader/LoaderEntryPoint.cpp`, `src/xlOil/Loaders/AddinLoader.cpp`,
`src/xlOil-COM/API/ExcelThread.cpp`, `src/xlOil-XLL/LogWindow.cpp` and
`src/xlOilHelpers/Settings.cpp`. No framework binary is patched. A pre-load
qualification rejects settings overrides, a user settings file, a renamed loader
and an already resident core. It requires the native ini and both plugin hashes;
release-time checks can only withdraw that qualification. After successful
teardown and release of every originating XLL lease, the host validates the two
process-local window classes and their procedures' exact owning core. A live
message window prevents any log-UI mutation. Unknown core UI or a foreign owner
also prevents cleanup. Only then does it remove the abandoned log window/classes
and balance the measured loader's orphaned PE delay-load reference. Reopening
requires recorded successful cleanup and confirmation that the core really left
the process. A native test checks preservation of live and unknown window contexts.

This follows the documented
[PE delay-import layout](https://learn.microsoft.com/windows/win32/debug/pe-format),
[FreeLibrary reference count](https://learn.microsoft.com/windows/win32/api/libloaderapi/nf-libloaderapi-freelibrary)
and [UnregisterClass ownership/lifetime rules](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-unregisterclassw).
These window classes are private to the host process; no Excel COM registration,
HKLM key, Trust Center or macro setting is written. Python and settings-override
reload controls retain explicit failures. A renamed-loader control crashed and
remains unqualified. Arbitrary plugins, Python teardown and general safe reload
remain open under dc-snz.21.

## 20. Excel-DNA scalar and private COM support (dc-snz.2)

Native typed arguments now use the existing coercion and text-rendering path.
An incoming worksheet error in a numeric native argument becomes #VALUE!, as
measured independently with the same unchanged native fixture in Excel. REFTEXT
uses the shared ADDRESS implementation for a single rectangular reference;
unsupported reference shapes retain a typed refusal.

The existing Application projection now exposes a bounded COMAddIns and
CommandBars model. COM discovery reads only private HKCU classes whose in-process
server is an XLL already owned by this host. Activation uses the real
IDTExtensibility2 interface, and ribbon inspection calls IRibbonExtensibility
GetCustomUI on the owner STA. The returned XML is retained by the application.
This does not render a ribbon or implement arbitrary button actions. Unsupported
Office members return an error. No Excel registration or security setting is
written by the host. The reference runner requires a nonadministrator token and
an unchanged AutomationSecurity value of 1 before the original Excel-DNA helper
can register its temporary private classes.

COM providers may reenter while connecting, querying a ribbon or disconnecting.
Stable object references, generation checks and a shutdown flag protect those
transitions. Failed or incomplete teardown retains the provider and module leases
rather than unloading callable native code. Native tests exercise vector growth,
self-disconnection in QueryInterface and GetCustomUI, retained disconnected
controls and calls from the wrong thread.

Reference comparisons retain teardown separately: Excel's UNREGISTER(path) and
the host's Remove+Close are different operations. xlFree has no defined output
operand. Identity normalization of its input now follows the callback's operand
address and recorded value, rather than treating any matching number as a
registration ID. Raw pointer values and every callback remain in the capture.

## 21. R8 reference comparisons and RTD ownership (dc-snz.2, dc-snz.23)

Clean captures at 5ae95bf6 compare the unchanged Registration sample on net462
and net6.0-windows, local runtime-test binaries with an explicit net8/noncollectible
configuration, the built net10 Native AOT runtime tests, and the net6 ribbon
sample. Their 114 formula cases match in both traced and untraced controls.
All 110 ordered native entries and 606 pre-teardown C callbacks match after the
recorded identity normalization. The ribbon comparison includes seven helper
property calls and exact XML. Teardown remains different and separately counted.
The rebuilt original xlw sample and the existing xlw generated type fixture each
match ten cases. These are bounded comparisons, not whole-framework qualification.

The default local runtime-test configuration fails in both Excel and the host;
its failure is retained. The net8 configuration changes only the external DNA
configuration and uses unchanged rebuilt binaries. The local Excel-DNA checkout
was already dirty; source hashes and build inputs are retained separately from
its HEAD. The shared scalar cases use Framework's "12.3" formatting; the recorded
net6 variant expects "12.2", as observed in both hosts. No framework binary is
patched, and no upstream source tree was edited for these comparisons.

RTD transport declarations now reconcile over all completed invocations in the
cell's producing certificate. One native call cannot disconnect a sibling's
topic. Records selected by a reused certificate stay available; partial or aborted
invocations cannot keep a transport alive. Replacing a formula retires its old
logical inputs after the edit succeeds and before formula-entry evaluation.
A disconnected transport can reconnect while its logical input remains leased:
the logical subscription and increasing input sequence survive. Each connection
gets a fresh native transport ID. Queued updates from a known retired transport
cannot reach the new connection; an unknown transport ID still fails validation.
The native client test covers that distinction with another topic still active.

Native asynchronous formula entry uses the existing CalcHost P07 token route and
owner message pump. Its completion resumes the producing activation and its
dependents. Manual Calculate is a separate invocation. Reference cases wait for
the assigned formula's completion before requesting that calculation. Ordinary
OneCalc calls without an admitted continuation retain a typed refusal.
The broader RTD verifier checks ordered formula occurrences, including edits that
keep the formula, exact admitted input sequences and producing certificates.
It retains every native/client call and reports unequal traffic as a strict
comparison failure even when final values match. Arrays, remote RTD, ribbon
painting, button actions and general COM remain outside the scalar observations.

## 22. Spill computation, modern entry and array RTD (dc-snz.23)

Independent checked native calls can compute spilled results concurrently. Their
completed output buffers release native pool capacity while placement keeps the
admitted reservation order. A placement delay is not a formula read and creates
no dependency edge. The ordered placement rule applies to the initial parallel
wave; repair waves retain their existing sequential order. Preferred producers
share service by age, so a parked preferred output cannot starve another producer.
Portable HostSession tests force later completion first, check competing and
disjoint ranges, and make two later preferred producers finish before an earlier
reservation starts. They require the capacity limit and no provider reentry.
The native application test uses two DLL-owned 2x2 outputs, checks all eight
published cells, actual worker overlap, and three same-thread frees.

The reference composition can request FormulaEntry independently of whether the
authored formula uses legacy Formula or modern Formula2 form. The explicit
CalcHost command owns its P07 continuation. An Excel Formula2 control observed
one native async invocation before Manual Calculate, as with legacy assignment.
Both forms have application regression coverage; an ordinary calculation remains
a separate command. Configurable independent anchors and spill probes extend the
existing reference runner rather than adding another evaluator or fixture path.

A controlled COM RTD provider returns a VARIANT SAFEARRAY with four numeric
values from ConnectData and RefreshData. Excel 16.0.20527 exposes those values to
xlfRtd as #N/A, without a spill. The host now makes the same conversion for this
variant type while retaining the admitted input and sequence. Other SAFEARRAY
element types and remote servers remain unqualified.

The native async callback's raw result remains different: Excel leaves its result
operand at type zero, while the host supplies the Boolean described by
[Microsoft's xlAsyncReturn reference](https://learn.microsoft.com/en-us/office/client-developer/excel/xlasyncreturn).
The argument values, return code and admitted P07 publication can agree despite
that difference. The verifier retains it and returns a failed strict comparison;
it does not count the raw result as equal. Broader managed RTD traffic likewise
remains a failure whenever its complete ordered client/native calls differ.

The spill comparison keeps owner calls in their observed order. Only the two
proven overlapping worker invocations are compared by their distinct numeric
arguments. Each packet contains its outer call, exactly one same-thread nested
call and its same-pointer/thread/module free. Every native entry belongs to an
ordered owner call or one of those packets; extra entries fail verification.
The raw cross-worker entry order remains recorded, including its difference.

COM RTD observation now records caller stack, return address, entry address and
all non-this arguments for the measured method ABIs, including zero-argument
termination and notification. A watched thunk that forwards or restarts with exactly the
same frame and arguments is an explicit forwarding event for the original call;
a new frame remains a separate invocation. Older traces lacking that machine
proof retain their original strict failures.

## 23. Owned Excel session jobs (dc-snz.24)

The Excel side of the existing XLL reference runner now submits to the lane host in
`tools/excel-session`. Capture and witness workers receive that same owned application.
The queue groups waiting jobs by environment and puts fresh-required lifecycle jobs last.
Receipts retain process identity, launch attempts, script/result hashes and the recorded
baseline before and after each job. Settings and workbooks are reset; add-in inventories
are checked rather than changed. A failed reset refuses the job and recycles its process.
Every fiftieth ordinary observation job is repeated in a fresh process to measure leakage.
Both outputs survive, including differences. A deadline can terminate only the recorded owned
Excel and its host. Interrupted jobs remain failures after recovery. No Excel COM registration,
Trust Center or macro policy is changed. Timing workers retain their existing separate route.

The baseline checks cover recorded application settings, workbooks, registered functions and
add-ins; they cannot prove absence of hidden framework state. Jobs requiring a new process
must declare that condition explicitly. This changes measurement orchestration, not the RTD
lifecycle or native async compatibility claims in sections 21 and 22.

Capture and timing workers now preserve the existing AutomationSecurity value, as witness
workers already do. The host always checks for drift without assigning that setting, including
when restoring the recorded baseline. Raw capture records retain Formula and Formula2 separately
beside the saved package's formula metadata.

## 24. Native async result type (dc-snz.23)

The existing native fixture now records the result operand's type after xlAsyncReturn.
Separate default and numeric-sentinel conditions use the same fixture binary; marker files
retained in each capture select the condition and a 50 ms delay. Excel 16.0.20527.20088
returns 32 zero bytes for both a zeroed operand and one initially containing numeric 13.25.
The host now clears the whole operand on accepted completion instead of supplying Boolean TRUE.
Its completion value and the existing P07 admission, overwrite and late-return rules remain
independent of that output operand. A regression exercises the exported callback route.

The reference case extends the earlier entry/calculation cases with a real native getter for
the observed output type. Earlier capture files and their strict failures remain unchanged.
The verifier still compares every raw callback result and keeps any difference as a failure.
This bounded result does not settle the managed RTD lifecycle traffic still open in dc-snz.23.

For native and extended RTD captures made by the session host, the verifier accepts
`--excel-session <retained-ledger>`. It checks the exact PID/start-time tuple, pre-activation
inventory, successful fresh-job receipt, retained host and runner hashes, security baseline
and capture interval. The runner's raw inventory includes the already-owned session PID;
that metadata stays unchanged. Without the explicit ledger, the original ownership check
still refuses that PID. Value, callback and ordered RTD comparisons are unchanged.

## 25. BigData count layout (dc-snz.23)

The XLOPER12 BigData declaration uses `long cbData`, which is signed 32-bit on
Windows, including x64. The ABI and tracer previously read 64 bits, including
the following four padding bytes. Sources: the published
[XLOPER12 declaration](https://xloil.readthedocs.io/en/v0.9.2/doxygen/_xl_call_slim_8h_source.html)
and [Windows data types](https://learn.microsoft.com/windows/win32/winprog/windows-data-types).

The ABI now declares that field as i32. New traces retain all 32 operand bytes
and identify the four bytes following cbData separately. Before comparing the
declared fields, the verifier checks the decoded pointer, signed count and type
against the retained bytes. Padding stays in the raw comparison and diagnostics;
it is not treated as part of the count. Tests change each declared field and
check rejection of inconsistent raw storage. Earlier captures keep their original
decoding and are not rewritten. Raw identity normalization also masks the duplicate
pointer bytes after validating their binding; it leaves all other bytes exact.

Round 13 repeats both default and numeric-sentinel conditions with four captures
each (Excel and DNA Calc, traced and control). The three formula cases, dependents,
declared native operands, callback return codes and P07 certificates pass in both
conditions. Every observed BigData count is zero. The xlAsyncReturn output is
32 zero bytes in both hosts; raw input padding and unused union bytes still differ
and remain visible in the comparison. These are not part of the declared count.
Evidence is in `C:/Work/DnaCalc/codex-runs/excel-results-native13`.

A fresh original eight-case managed RTD run has matching values, dependents and
P07 certificates in all four captures, but still fails ordered client and native
traffic comparison. The first client difference is a RefreshData signal of 1 in
Excel versus -2 in DNA Calc; topic replacement and server termination order also
differ. This does not qualify the managed RTD lifecycle. Raw captures and the
failed strict summary are in `C:/Work/DnaCalc/codex-runs/excel-results-rtd13`.

A separate control changes the explicit provider delay from 50 to 2000 ms in
the five delay-bearing cases, retains their expected values and proofs, and ends
with the same replacement constant (six cases). Its first RefreshData signal
matches, but replacement order still differs: Excel connects the next topic
before disconnecting the old topic; the host disconnects and terminates first.
All six values and dependents agree in the four captures. The strict client
comparison matches 4 positions and fails 28; native calls match 0 and fail 24.
The original eight-case comparison matches 4 client positions and fails 41;
native calls match 0 and fail 30. Longer delay therefore narrows the observed
timing difference without settling lifecycle compatibility.

## 26. Replacement handoff and round 14 measurements (dc-snz.23)

Formula replacement now separates logical retirement from native disconnection.
After a successful edit, old logical inputs and queued updates retire immediately.
The native transport stays only until that caller's replacement attempt finishes,
so a new topic can connect before the old transport disconnects. A constant
replacement also completes the disconnect; another caller retains its own lease.
Native-client tests observe connect-old, connect-next, disconnect-old, and reject
old queued inputs. The application composition finishes the handoff after explicit
formula-entry evaluation as well as ordinary calculation.

This is a partial implementation. Fresh round 14 managed-provider captures still
show an earlier disconnection before the next connection. All eight original
values and six separate delayed-condition values, dependents and P07 certificates
agree in Excel and DNA Calc, traced and control. The unchanged strict verifier
reports client positions matched4/failed38 and native matched0/failed30 for the
original condition; delayed client matched4/failed29 and native matched0/failed23.
Source inspection points to completed cached calls consuming an admitted RTD read
without making xlfRtd again, followed by declaration reconciliation disconnecting
the transport. The follow-up must preserve the specific producing dependency
without retaining skipped or aborted calls indiscriminately. Initial temporary
#N/A, refresh timing and caller-book spelling also remain visible. Evidence and
all failed comparisons: `C:/Work/DnaCalc/codex-runs/excel-results-rtd14`.

A later certificate-only renewal attempt kept the native connection long enough
for the next topic to connect first. The unchanged full verifier then caught a
regression in both conditions: replacing the completed shared-topic formula
returned the expected text but recorded zero RTD inputs for the main cell and
its shared dependent, each of which requires one. Keeping the native connection
also kept the framework's completed-result cache; after logical retirement the
cached call made no xlfRtd callback to declare its new dependency. That
certificate-only renewal was reverted. The bounded begin/finish handoff and its
existing tests remain; the application replacement-order problem stays open.
A future correction must renew the specific call's dependency through P07
admission after the edit. It cannot publish an unadmitted cached result.

The failed attempt is preserved in
`C:/Work/DnaCalc/codex-runs/excel-results-rtd14-cert`, including the full verifier
rejection and raw captures. A failed traced Excel job and its separate fresh
retry are both retained. The failed job has no after-baseline security reading;
successful jobs preserve the recorded value and no route assigns it. The
27-workbook numerical corpus has identical input identities, inclusive counts
and all 62 cell-report payloads before and after that attempt; it does not
exercise this managed shared-cache failure.

The same round records a bounded template comparison for dc-bctq in
`C:/Work/DnaCalc/codex-runs/excel-results-templates14`. Excel created .xltx/.xltm
inputs and ran six Add/default-Open/editable-Open save/reopen conditions. The
actual application Workbook and ordinary-save route produced four files:
.xlsx/.xlsm from ordinary Open and .xltx/.xltm from template editing. Excel accepted
all four in eight further open/save/reopen conditions. The evidence checks the
selected numeric/formula cells, number format and defined name, correct workbook
content types, copied styles/theme parts and exact VBA binary preservation.
DNA Calc uses Book1 names for ordinary Open; Excel uses the template base name
plus a counter. Other macro-fixture cells, startup mode inheritance, volatile
startup, default templates, insertion, dialogs, legacy .xlt and wider preservation
remain unqualified by these observations. Every session preserved AutomationSecurity.

## 27. Managed RTD replacement reads and ordering (dc-snz.23)

Replacing a formula in the measured managed RTD cases now connects the next native
topic before disconnecting the old one. Completed shared calls retain one admitted
RTD read for each of the main cell and its dependent after an edit. The unchanged
full verifier accepts all eight original and six delayed values, dependents and
exact P07 producing cuts in the traced and untraced runs. The ordered
ServerStart/ConnectData/DisconnectData/ServerTerminate method, server and topic
identities agree in all 16 original and 12 delayed rows.

The native adapter records the inputs of each completed call together with its
provider, caller and coerced arguments. Numeric argument matching preserves bits,
including signed zero inside arrays. When that call returns from its native cache
without another xlfRtd callback, the adapter reads its specific input through the
ordinary UserFunctionContext. An edit retires the old logical key and queued
updates. A still-live native connection can transfer to a fresh logical key whose
first sequence is 1. Its last observed native signal enters the ordinary owner
admission queue; the cached output remains temporary #N/A until that read is
admitted. The bridge stores signals and dependency identities, never UDF answers.

Unused transport cleanup waits for an idle owner poll. Queued signals protect
their sources until delivery, and polling during active calculation preserves
the native cache until the consumer can execute. Ordinary application-command completion
also waits for observed input queues to drain. A main-cell-only application
control retains all 16 outcomes and the required positive P07 reads without a
dependent supplying another evaluation turn. That control has no separate Excel
reference and establishes application behavior, not standalone protocol parity.

The complete strict comparison remains failed. Original client positions match 10
and fail 16; native invocations match 0 and fail 26. The delayed client comparison
matches all 20 calls, while native invocations match 0 and fail 22. Every payload
and differing native call stays in the full summaries. These results qualify the
tested sequential scalar replacement behavior; broader compatibility remains open.
Cached reference recovery and expired handoffs return dc-snz.23 refusals. Mixed
live/cached topics within one native call, nested handoffs, all ABI narrowing
forms and RTD formula-entry settling remain unqualified.

The source changes are 083178c0 and cb8b9503, integrated through 9604bc0f. Evidence is
in C:/Work/DnaCalc/codex-runs/excel-results-rtd15. The first experimental captures
remain unqualified: source provenance was refused, and fresh logical keys used
an inherited sequence. Their raw records and failed verification remain unchanged.
The numerical corpus before and after 083178c0 has the same 36 workbook identities,
counts and all 80 cell-report payloads: 29,335 pass and 1,366 fail of 30,701 (95.55% pass). The later
merged corpus includes 43 workbooks and 35,048 formulas: 33,085 pass and 1,963 fail
(1,270 mismatched, 672 refused, 7 not run and 14 circular-held; 94.40% pass). Incoming function
changes and seven new workbooks are separate from the native work; no numerical
gain is attributed to RTD. The session scripts never assign AutomationSecurity.
The 45 successful jobs verify 1 before/after; the two failed jobs lack after-baseline
readings. Only private test RTD classes are registered; all captured private names
and CLSIDs are removed, with five preexisting names unchanged. The ownership
audit finds all 33 owned Excel identities gone.

## 28. Boundary batch x13: xlFree, failed-callback writes, xlGetInst, xlStack, auto-free (dc-6332.10)

Answer 13's D.3, the D.22 rows 16385/16391/16403 and a sentinel pass over the X5 permission
matrix, run as fixture probes (`calc-xll-fixture/src/fixture/boundary.rs`, opt-in marker
`boundary-tests.enable`) through one untraced Excel control per run in the lane's owned session
on a private desktop (Excel 16.0.20527.20150, x64, en-ZA, MTR on, 16 threads), and one OneCalc
host process per case group. The cases and Excel's answers are
`crates/calc-xll-reference/fixture/x13-boundary-cases.json`; runner, guard files and
summaries are in `crates/calc-xll-reference/traces/2026-11-10-x13-boundary`. The probes fill
each result operand with 0xA5 bytes and report status, thread and what the callback left there.

Measured in Excel:

- **Failed callbacks write #VALUE!.** Every failed callback in a frame (statuses 2, 32 and 128,
  normal, `#`, `$` and xlAutoOpen) left xltypeErr 15 in the result operand; none left it
  untouched. CALL and REGISTER.ID already did this in the host (`call.rs`).
- **xlFree changes the caller's operand.** For Str and Multi results the pointer word becomes
  null; for every kind (Str, Multi, Num, SRef, Ref) the type becomes xltypeMissing (0x80) and the
  other value bytes stay. A second xlFree of the same operand returns 0.
- **No XLFree bit on callback results.** The raw type of xlGetName, xlfCaller, xlCoerce to Str
  and to Multi, xlSheetId, SUM and EVALUATE results carried no flag bit (answer 13 A.3 #3: do not
  set it universally; the prompt's R3 premise is not what Excel does).
- **xlGetInst (16391) fails with 32** in normal, `#`, `$` and xlAutoOpen contexts (A.3 #10).
- **xlStack (16385) is not capped at 64 KiB** (A.3 #11 does not hold here): it answered the
  caller's own headroom less 87,119 bytes on the main thread, a worker and in xlAutoOpen.
- **xlGetInstPtr (16403)** is BigData with the main image handle and count 0 in every context;
  **xlGetHwnd (16392)** is this process's XLMAIN window.
- **Commands from worksheet functions**: ECHO (0x8000|141) answers 2 from a normal function,
  0 and FALSE from `#`, 128 from `$`. MESSAGE from `#` answers 0 and FALSE.
- **xlUDF from `$`** answers 32; **SET.NAME** answers 2 from normal and `$`, 0 from `#`;
  **xlEventRegister from `$`** succeeds (0, with an Int payload that differs per process).
- **Auto-free**: a DLLFree number result gets xlAutoFree12 on the main thread; xlFree inside it
  returns 0; Excel terminated (RPC server died) when xlAutoFree12 called xlStack, xlGetName,
  xlfCaller and SUM (one trial, not repeated; which of the four is unknown).
- **Retained callback storage**: an xlGetName result kept past the return stays readable in the
  next entry and frees with 0, on the main thread and on a worker.
- `$` functions ran on a worker thread at formula entry in this run.
- Excel keeps one registration per module and procedure: three `xlfRegister` calls of one export
  with type texts `QB`, `QB#` and `QB$` under three names made all three names run as `QB$` (the
  probes therefore use one export per registration). The host keeps three registrations; that is
  a registration-identity difference for dc-6332.13.

Host changes (dc-6332.10): callbacks answer through one `CallbackReply` (status, result write,
refusal) applied in `dispatch`; failed 2/32/128 callbacks write #VALUE!; the measured denials
above return Excel's status without refusing the formula (`measured_denial`, with its table);
xlFree nulls the pointer and sets Missing; xlGetInst answers 32; xlStack keeps Excel's reserve;
MESSAGE from `#` answers FALSE; xlAutoFree12 runs after the worksheet frame is left, in a
free-only frame that serves xlFree and refuses anything else (`dc-6332.13`, since Excel
crashes there); callback allocations live in one registry shared by the owner and its workers,
so a worker's retained callback result is no longer a refusal. Still refused: commands and
xlUDF with a cached-read change from `#`, xlEventRegister and RTD from `$` (events are dc-6332.13).

Comparison (pass = matched Excel; fail = mismatched + refused + not run; `verify.py
--boundary-host`, which also reports the C.1 manifest, the four counts and an Excel repeat):
before the host changes 36 of 68 (30 of 52 ordered boundary cases, 6 of 16 lifecycle and
ownership cases), after 65 of 68 (49 of 52 and 16 of 16; the three failures are the refusals
above); the open-time xlGetInst case 0 of 1, then 1 of 1. A second Excel run agreed on 67 of 68
(the xlEventRegister payload differs per process). The existing framework sets through the host
route (`run.ps1 -HostExecutable`, values checked against Excel's recorded answers: X1 289,
Excel-DNA net48 7, net8 7, AOT 1, X2Pool 2, xlw 5 and 5, xlOil Utils/SQL 12, xlOil Python 5)
pass on the changed host.

## 29. Operand batch x14: xlCoerce masks, UTF-16 units, input mutation (dc-6332.10)

Answer 13's D.4 (as a baseline) and D.5, with the same fixture, runner and session as
section 28: `fixture/x14-operand-cases.json` (145 cases with Excel's answers), evidence in
`crates/calc-xll-reference/traces/2026-11-10-x13-boundary/x14`. A second Excel run agreed on
all 145.

Measured in Excel 16.0.20527.20150:

- **xlCoerce's type operand is a mask.** A value whose kind the mask allows comes back
  unchanged (TRUE with Num|Str|Bool|Err|Multi stays TRUE, "12" with Num|Str stays "12"); an
  array is kept when Multi is allowed, else its top-left element is coerced; a reference is kept
  when Ref is allowed and read otherwise; otherwise Num is preferred (TRUE with Num|Str gives 1).
- Conversions: text to Num/Int parses (" 12 " and "1e3" convert; "a" and "" fail); 2.5 to Int
  gives 2; text to Bool fails except TRUE/FALSE; any non-error to Err gives #VALUE! with status 0;
  a scalar or an error to Multi gives a 1x1 array; a value to Ref fails; a blank cell reads as 0,
  "", FALSE, Int 0 or a 1x1 Nil array.
- **Failures**: status 32 with #VALUE! in the result operand for a failed conversion, and with
  the source's own error (#N/A) when an error value cannot convert. A two-area reference reads as
  #VALUE! (32 for value targets; Ref keeps both areas).
- **Text**: UTF-16 code units arrive unchanged in Q and D% arguments (a surrogate pair stays two
  units; LEFT(UNICHAR(128512),1) keeps both); returned text keeps an embedded NUL (LEN 3,
  UNICODE 0) and a lone surrogate (LEN 3; UNICODE of it is #VALUE!).
- **Input mutation**: an add-in that swaps the rows of its own Q array input changes only its
  copy: `LET(a,{1,2;3,4},MySwapRows(a)&"|"&INDEX(a,1,1))` gives "3|1".
- **Nested origin**: xlGetName before and after a nested xlUDF from a `#` function agree.
- **Nil inside an expression**: `MySwapRows(A1:B2)&"|"&A1` (the swapped top-left is a blank
  cell's Nil) gives "|7" in Excel; DNA Calc's engine gives "0|7", because a native Nil or
  Missing result becomes 0 before it reaches the expression (`invoke.rs` `normalize_return`).
  Recorded, not changed here.

Host change: `coerce_callback` (callbacks.rs) implements the mask rules above and answers
measured failures with status 32 and the measured error, without refusing the formula; Missing,
Nil, SRef and flow targets and non-numeric type operands remain typed refusals. OneCalc before
(the 31478370 build) matched 61 of 145, after 128 of 145; the 17 failures are OneCalc's
MissingInput refusals for cells other than its one input A1 (its scratch-session design).
