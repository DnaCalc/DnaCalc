# DNA Calc exposed API: three faces over one object-model core

Status: architecture and plan for epic `dc-17spk`, task `dc-17spk.1` (2026-11-23; reconciled with `3de337300`, `af2f22cb2`, `51e5993ee`, and `4094ef5c2`). Design document only; no code changes.

Companion documents:
- `docs/architecture/OBJECT_MODEL.md` — the one Rust object model, descriptor, profiles, and measured Excel COM behaviour.
- `docs/architecture/XLL_COM_HOSTING.md` — native `.xll` hosting, `MdCallBack12`, `XLCALL32.DLL`, and X0–X19 Excel traces.
- `docs/architecture/ADDIN_SURROGATE.md` — the per-session surrogate process boundary for native add-ins (Stages 3–4).
- `docs/architecture/CODE_EXECUTION_POLICY.md` — the shared `Off` / `Guarded` / `Unguarded` model, origins, and ten-capability vocabulary (`dc-8ovd`).
- `docs/architecture/LINUX_DESKTOP_SHELL.md` — Linux desktop shell options (`dnacalc serve`, WebKitGTK) and native add-in boundaries.

---

## 1. Architecture: three faces over one core

Owner direction (2026-11-22/23, `dc-17spk`): DNA Calc exposes one programmable Excel API through three external faces over a single implementation, plus one user-function registry shared by every add-in runtime:

1. **COM object model face** — dual vtables, `IDispatch`, `ITypeInfo`, connection points, Windows out-of-process activation (`GetObject`/ROT and `LocalServer32`, `dc-17spk.2`), Windows COM add-ins (`IDTExtensibility2` / `IRibbonExtensibility`, `dc-17spk.7`), Windows Automation add-ins (`Programmable` COM worksheet functions, lower priority, `dc-17spk.8`), and Linux in-process binary COM ABI.
2. **Excel C API command face** — `xlcXXX` command-equivalents and `xlSet` through `Excel4`/`Excel4v` (`XLOPER`) and `Excel12`/`Excel12v` (`XLOPER12`), distinct from the `xlfXXX` worksheet functions already served by `crates/calc-xll-host`.
3. **Office.js scripting face (two distinct interactions)** —
   - **Face 1: Office Add-ins runtime (`dc-17spk.5`)** — the add-in web page runs out of process from the calc core in a platform browser runtime (WebView2 on Windows, WebKitGTK on Linux, or an iframe/Worker beside the page in the browser build), using DNA Calc's own clean-room Office.js-compatible library (`Excel.run`, `RequestContext`, proxy objects, `load`/`sync` JSON batches) against `calc-host`'s descriptor member owners, with XML and unified JSON manifests, task panes, ribbon commands, events, dialogs, and the shared-runtime option.
   - **Face 2: JavaScript custom functions (`dc-17spk.6`)** — `functions.json` metadata + JS functions (sync, async `Promise`, streaming `setResult`, cancellation, invocation context) running in an embedded, compile-time swappable JS engine crate (`crates/calc-js-host`: QuickJS / Boa / V8 natively, browser Worker on `wasm32`) and registered as UDFs in the same `CallableTable` as XLL, VBA, and Automation add-ins.

**VBA host boundary (`dc-piw0.5`, blocks `dc-17spk`):** CalcVBA stays strictly host-neutral and has no Excel or Office knowledge (owner 2026-11-23, `3de337300`). `'Excel'` and `'Office'` are references declared in a workbook's VBA project; DNA Calc (`crates/calc-host-vba` and `crates/calc-vba`) supplies their type libraries and root objects from `calc-object-model` and routes calls to `calc_host::automation`, retiring the private tables in `crates/calc-vba/src/runtime/spreadsheet.rs` and synthetic IIDs in `crates/calc-vba/src/events/source.rs`. CalcVBA changes only where a generic host-supplied type-library hook is missing (never Excel names in CalcVBA).

### 1.1 Architectural rules

1. **One descriptor.** Every interface, member identity, ABI fact, and Guarded capability reach entry is described once under `crates/calc-object-model/descriptor/`. No face keeps a private semantic switch or member table.
2. **One semantic owner per member.** Workbook semantics live in `crates/calc-host/src/automation.rs` (`WorkbookAutomation`, backed by `HostSession::dispatch_from(CommandIngress::Automation, ...)` and `calc-engine`/`calc-doc-model`). Workbook-free application members live in `crates/calc-object-model/src/profile/values_only.rs` (`ValuesOnly`). Only COM-by-nature Office add-in/ribbon/CommandBar objects live in `crates/calc-com-host/src/office.rs`.
3. **One user-function registry.** XLL functions (`calc-xll-host`), VBA modules/`.xlam` add-ins (`calc-vba` / `calc-host-vba`), JavaScript custom functions (`calc-js-host`, `dc-17spk.6`), and Automation add-ins (`calc-com-host`, `dc-17spk.8`) all register through `calc_fml::CallableTable` (`crates/calc-fml/src/callable.rs`) implementing `calc_func::UserFunction` (`crates/calc-func/src/udf.rs`). One name-resolution, completion/signature-help, async-continuation, and Guarded admission path serves all four.
4. **Faces only translate.** Each face converts wire/ABI values, object identities, batches, events, and errors between its client protocol and `calc_object_model::{ObjectModel, Call, AutoValue, AutoError, ObjectRef, EventSource}`. No face holds a shadow workbook, a second evaluator, a second dependency graph, or a second event switch.
5. **Guarded reach read from the descriptor.** Every face enforces `CODE_EXECUTION_POLICY.md`'s three-level model (`Off`, `Guarded`, `Unguarded`) using per-member `reach` annotations read from the descriptor and audited through one sink.
6. **The engine does no more than Excel.** Every face follows measured Excel behaviour (COM: E1–E11, X19, Q0–Q22; C API: X0–X18 plus C1–C4 command probes; Office.js: J1–J5; Automation add-ins: measured before freezing). There is no `DnaExcel` dialect with invented semantics (`docs/architecture/review-2026-11/pro/harvest/14-harvest.md`).
7. **Dependency ceiling preserved.** `tools/xtask/architecture.json` rules remain intact: `calc-object-model` and `calc-host` stay `portable` (`wasm32-unknown-unknown` clean); `dna-onecalc`, `dna-onecalc-desktop`, `dna-formula-ui`, and `calc-addin-host` never depend on `calc-host` or `calc-engine`; `calc-platform-desktop` stays free of product dependencies; `dna-calc` (UI page/skins) never depends on `calc-host`, `calc-engine`, `calc-doc`, or `dna-calc-worker`; the planned `calc-js-host` crate depends only on `calc-value`, `calc-func`, and `calc-fml` (never `calc-host` or `calc-engine`).

### 1.2 Crate layering and seams

```mermaid
flowchart TD
    subgraph Clients["External and in-process API clients"]
        COM_IN["In-process COM: XLLs, COM add-ins (dc-17spk.7), Automation add-ins (dc-17spk.8), C/C++/.NET"]
        COM_OOP["Out-of-process COM (Windows, dc-17spk.2): PowerShell, Python, C#, VBScript"]
        CAPI_CLI["C API command callers (Windows XLLs; Win/Linux in-process C/Rust)"]
        OJS_ADDIN["Office Add-ins webview / iframe (dc-17spk.5): task pane, commands, shared runtime"]
        OJS_CF["JS Custom Functions (dc-17spk.6): functions.json + sync/async/streaming JS"]
    end

    subgraph Faces["API faces and UDF hosts (translate only)"]
        COM_FACE["calc-com-host (native-only): Win64 vtables/IDispatch/ROT/LocalServer/COMAddIns/Automation UDFs; Linux SysV COM ABI (planned)"]
        CAPI_FACE["C API command dispatcher (planned split): calc-xll-abi + portable command translator; Win XLL loader in calc-xll-host + calc-xll-shim"]
        OJS_FACE["Office.js add-in face (dc-17spk.5): clean-room JS client + manifest/webview host -> dna-calc-protocol -> dna-calc-worker Server"]
        JS_HOST["calc-js-host (dc-17spk.6, planned): swappable JsEngine trait (QuickJS / Boa / V8 / browser Worker)"]
    end

    subgraph Core["One object-model core and one UDF registry (portable, wasm32-clean)"]
        OM["calc-object-model: Descriptor + reach.json + AutoValue + ObjectRef + ObjectModel/EventSource + ValuesOnly"]
        UDF["calc-fml::CallableTable + calc-func::UserFunction (one registry: XLL + VBA + JS + Automation)"]
        HOST["calc-host (WorkbookAutomation): HostSession + CommandIngress::Automation + ReferenceTracker + WorkbookEventObserver fan-out"]
        ENG["calc-engine + calc-fml + calc-func + calc-doc-model"]
    end

    COM_IN --> COM_FACE
    COM_OOP --> COM_FACE
    CAPI_CLI --> CAPI_FACE
    OJS_ADDIN --> OJS_FACE
    OJS_CF --> JS_HOST
    OJS_CF -. "shared runtime" .-> OJS_FACE

    COM_FACE -->|"ObjectModel / EventSource"| OM
    COM_FACE -->|"Automation UDFs"| UDF
    CAPI_FACE -->|"ObjectModel / UserFunctionContext"| OM
    CAPI_FACE -->|"XLL UDFs"| UDF
    OJS_FACE -->|"AutomationBatch -> ObjectModel"| OM
    JS_HOST -->|"JS Custom Functions"| UDF
    OM -. "trait impl (DNA Calc)" .-> HOST
    HOST --> ENG
    UDF --> ENG
```

Why the `ObjectModel` (`crates/calc-object-model/src/model.rs:88-100`) and `CallableTable` (`crates/calc-fml/src/callable.rs:111`) seams are essential:
- `crates/calc-addin-host`, `crates/calc-com-host`, `crates/calc-xll-host`, and planned `crates/calc-js-host` sit below OneCalc's dependency ceiling (`tools/xtask/architecture.json:266-288`), so none of them may depend on `calc-host`, `calc-engine`, or `calc-doc`.
- DNA Calc (`apps/dna-calc-desktop/src/automation.rs`, `apps/dna-calc-worker/src/server.rs`, `apps/dnacalc`) constructs `WorkbookAutomation` (`crates/calc-host/src/automation.rs`) and binds it as `Rc<dyn ObjectModel>` into every active face, while publishing the merged `CallableTable` into `HostSession::set_user_functions` (`crates/calc-host/src/session.rs:427`).
- DNA Calc - Formula Explorer (`apps/dna-onecalc`, `apps/dna-onecalc-desktop`) binds `ValuesOnly` (`crates/calc-object-model/src/profile/values_only.rs`), so every workbook/range member returns Excel error `1004` `"needs a workbook: open in DNA Calc"` uniformly across every face, while still reusing `CallableTable` for XLL, VBA, and pure JS/Automation UDFs (`apps/dna-onecalc/src/session.rs:579`).

### 1.3 How one operation maps across all three faces

Every workbook action converges onto one `WorkbookAutomation` handler and one `HostSession::dispatch_from(CommandIngress::Automation, ...)` transaction:

| Workbook operation | COM face | C API command face | Office.js face (`dc-17spk.5`) | Single semantic owner |
|---|---|---|---|---|
| Write values to `A1:B2` | `Range.Value2 = arr` (1-based 2-D `SAFEARRAY(VARIANT)`) | `Excel12(xlSet, &res, 2, &ref, &multi)` (raw values, no formula parse) or `xlcFormula` | `range.values = [[1,2],[3,4]]; await ctx.sync()` | `WorkbookAutomation::put_value2` (`crates/calc-host/src/automation/mutation.rs:104-111`) -> `SkinIntent::Edit` |
| Enter a formula in `C1` | `Range.Formula = "=SUM(A1:B1)"` | `Excel12(xlcFormula, &res, 2, &str, &ref)` (or active selection if `ref` omitted) | `range.formulas = [["=SUM(A1:B1)"]]; await ctx.sync()` | `WorkbookAutomation::put_formula` (`crates/calc-host/src/automation/formula.rs:228-276`) -> `SkinIntent::EnterEntries` |
| Read `Value2` of `A1` | `Range.Value2` -> scalar `VT_R8` | `Excel12(xlCoerce, &res, 1, &ref)` -> `XL_NUM` | `range.load("values"); await ctx.sync()` -> `[[n]]` (always 2-D) | `WorkbookAutomation::read` (`crates/calc-host/src/automation.rs:792-848`) -> `calc_host::project` / `UserFunctionContext::values_of` |
| Format address | `Range.Address(True, True, xlA1)` -> `"$A$1"` | `Excel12(xlfReftext, &res, 2, &ref, &true)` | `range.load("address"); await ctx.sync()` -> `"Sheet1!A1"` | `calc_object_model::geometry::address` (`crates/calc-object-model/src/geometry.rs:107`) |
| Recalculate | `Application.Calculate` / `Worksheet.Calculate` / `Range.Calculate` | `xlcCalculateNow` / `xlcCalculateDocument` | `ctx.application.calculate("Recalculate")` / `sheet.calculate()` / `range.calculate()` | `WorkbookAutomation::calculate*` (`crates/calc-host/src/automation/mutation.rs:397-437`) -> `SkinIntent::Recalculate` / `Calculate` / `CalculateRange` |
| Clear contents | `Range.ClearContents` | `xlcClear(3)` on active selection | `range.clear("Contents"); await ctx.sync()` | `WorkbookAutomation::clear_contents` (`mutation.rs:249-262`) -> `CellEdit::Clear` |
| Create table | `sheet.ListObjects.Add(1, rng, , 1)` | No 1:1 XLM command | `sheet.tables.add("A1:C3", true)` | `WorkbookAutomation::tables_add` (`crates/calc-host/src/automation/report.rs:72`) -> `WorkbookCommand::Table` |
| Create chart / shape | `sheet.ChartObjects.Add(...)` / `sheet.Shapes.AddShape(...)` | `xlcCreateObject` (legacy; deferred) | `sheet.charts.add(...)` / `sheet.shapes.addGeometricShape(...)` | `WorkbookAutomation::chart_objects_add` / `shapes_add_shape` (`report_draw.rs`) -> `WorkbookCommand::Chart*` / `Shape` |
| Toggle events | `Application.EnableEvents = False` | No 1:1 `xlc*` command (COM only) | `ctx.runtime.enableEvents = false` (measured in J-series) | `WorkbookAutomation::put_events` (`automation.rs:879-889`) -> `HostSession::workbook_events_switch()` |

### 1.4 Handling non-1:1 operations

Not every API member is a 1:1 rename of a COM member. Three explicit categories keep the one-owner rule intact:

1. **Selection-implicit C API commands.** Many `xlc*` commands omit the target range and act on the active selection (`xlcClear`, `xlcFormula` with 1 argument, `xlcFormatNumber`, `xlcEditDelete`). The C API translator holds no selection state: it reads `Application.Selection` (once `Application.Selection` is registered on `WorkbookAutomation`) through `ObjectModel::invoke`, then dispatches the target COM descriptor member (`Range.ClearContents`, `Range.Formula`, etc.) on that `ObjectRef`.
2. **Face-native protocol services.** Operations that belong to a face's transport rather than the workbook model stay in that face crate and never enter `calc-host`:
   - COM: `IUnknown`, `IDispatch`, `ITypeInfo`, `ISupportErrorInfo`, `IConnectionPointContainer`, `COMAddIns`, `CommandBars`, `IDTExtensibility2`, `IRibbonExtensibility`, `IRtdServer`, Automation add-in `IDispatch` UDF marshalling, ROT/class-factory registration (`crates/calc-com-host`).
   - C API: `xlFree`, `xlStack`, `xlGetName`, `xlGetHwnd`, `xlGetInst`, `xlGetInstPtr`, `xlAbort`, `xlRunningOnCluster`, `xlAsyncReturn`, `xlEventRegister`, `xlfRegister`, `xlfUnregister`, `xlfRegisterId`, `xlfCall` (`crates/calc-xll-host`).
   - Office.js: `Excel.run`, `RequestContext`, `load`, `sync`, `trackedObjects`, `isNullObject`, `getItemOrNullObject` wrapper flag, JS Promise scheduling, manifest loading, `Office.context.ui.displayDialogAsync`, `CustomFunctions.associate`.
3. **Workbook operations present in C API or Office.js with no exact COM descriptor twin.** Examples: `xlSet` (writes raw values into a reference without Excel's formula/entry text coercion, unlike `Range.Value` Put), Office.js `Range.valuesAsJson` (rich typed cell values), or Office.js `Range.getUsedRange(valuesOnly)`. These still live in `calc-host` (as helper methods on `WorkbookAutomation` or `HostSession` commands, keyed by an explicit entry in `calc-object-model`'s cross-face mapping metadata so Guarded `reach` and coverage counts remain uniform). A face crate never calls `calc-doc-model` or `calc-engine` directly.

### 1.5 Cross-face summary matrix

| Dimension | COM object model | C API commands (`xlc*` + `xlSet`) | Office.js Face 1: Add-in runtime (`dc-17spk.5`) | Office.js Face 2: JS custom functions (`dc-17spk.6`) |
|---|---|---|---|---|
| **Binding** | Dual vtables (generated x64) + `IDispatch` + `ITypeInfo`; `IDTExtensibility2` (`dc-17spk.7`); `Programmable` UDFs (`dc-17spk.8`) | `Excel12`/`Excel12v` (`XLOPER12`) & `Excel4`/`Excel4v` (`XLOPER`); `0x8000\|Cetab`, `xlSet`, `XL_INTL`, `XL_PROMPT` | Clean-room JS proxy `ObjectPath` recipes queued in `RequestContext`, flushed on `await context.sync()` | `functions.json` + JS module loaded in `calc-js-host` (`JsEngine` trait) and registered in `CallableTable` |
| **Values** | `VARIANT`, `BSTR`, 1-based 2-D `SAFEARRAY(VARIANT)`, `VT_DATE`, `VT_CY`, `VT_ERROR` SCODE | `XLOPER12` (UTF-16, 32-bit ref) / `XLOPER` (ANSI, 16-bit ref); `FP12`/`FP` | JSON/tagged `WireAutoValue` -> JS 2-D `any[][]`, `""` blanks, error strings or `CellValue` | Direct `CalcValue` <-> JS engine values (numbers, `-0.0`, booleans, strings, 2-D arrays, error codes) |
| **Identity** | Per-acquisition `Wrapper` (`Range`/`Window`); canonical `Application` (`runtime.rs:108-118`) | Transient `XlSRef12`/`XlRef12` + `SheetMap` `IDSHEET`; no retained object handles | Run-scoped host handle IDs; `trackedObjects` promotes `ReferenceTracker` leases | Invocation token `(workbook_incarnation, invocation_id, eligibility_token)` for async/streaming |
| **Threading** | Owner STA; wrong-thread `RPC_E_WRONG_THREAD`; `IMessageFilter` `RPC_E_CALL_REJECTED` in calc | Owner main lane for commands/`#`/normal; `$` pool gets `128`; `xlAsyncReturn` any thread | Separate browser/webview process; async JSON batching over bridge/Worker; never blocks calc | Embedded JS isolate next to calc engine (step/time/memory bounded) or browser Worker on `wasm32` |
| **Events** | Synchronous `IConnectionPoint` (`AppEvents`, `WorkbookEvents`, `DocEvents`, `ByRef Cancel`) | `xlEventRegister` (`CalculationEnded`/`Canceled`) + legacy `xlcOn*` hooks | Asynchronous `onChanged`/`onCalculated`/`onSelectionChanged` messages; no `ByRef Cancel` | Streaming `setResult` (RTD-style external update) + `invocation.onCanceled` |
| **Errors** | `HRESULT` + `IErrorInfo` + `EXCEPINFO` (`0x800A03EC` / `DISP_E_EXCEPTION`) | `CallbackReply` (`xlret*` status + `#VALUE!` write or `FALSE` bool) + frame refusal | Rejected `sync()` Promise with `OfficeExtension.Error` (`RichApi.ErrorCodes` + `debugInfo`) | `CustomFunctions.Error` -> Excel `#VALUE!`/`#NA`/`#NUM!`/`#NAME?`; timeout/OOM -> `#VALUE!` + diagnostic |
| **Guarded** | Native COM add-ins & Automation add-ins never load in `Guarded`; OOP COM & VBA checked via `reach` | Native XLLs never load in `Guarded`; `ExecuteExcel4Macro` / embedded C API checked via `reach` | **Runs in `Guarded`** inside webview/iframe sandbox; every batch op checked via `reach` + mediated fetch | **Runs in `Guarded`** inside bounded JS engine (CPU/memory limits, no fs/process, mediated fetch denied) |
| **Wasm** | No binary COM ABI (`calc-com-host` native-only); `calc-object-model` core runs in wasm | No native `XLOPER` ABI (`calc-xll-*` native-only); XLM text macro only if routed portably | **Natural browser face:** iframe/Worker beside page over `dna-calc-worker` (`postMessage`) | Uses **browser's own JS engine** in a dedicated Worker (`engine-browser` feature) |

---

## 2. Shared core extensions required across all three faces

The core already exists and is `portable`: `crates/calc-object-model` (`Descriptor`, `AutoValue`, `AutoError`, `ObjectRef`, `ObjectKey`, `ObjectModel`, `EventSource`, `ValuesOnly`), `crates/calc-fml/src/callable.rs` (`CallableTable`), and `crates/calc-host/src/automation.rs` (`WorkbookAutomation`, `AutomationHost`, `AutomationObserver`). Six shared additions keep all faces and UDF providers unified:

1. **Per-member Guarded reach in the descriptor (`crates/calc-object-model/descriptor/reach.json`).**
   - `crates/calc-object-model/descriptor/*.json` is regenerated on Windows by `tools/com-descriptor` (`crates/calc-object-model/src/descriptor.rs:6`: "never hand-edit the JSON"). Hand-editing `reach` into `excel.json` would be wiped on the next regeneration.
   - Instead, commit a small overlay file `crates/calc-object-model/descriptor/reach.json` keyed by `(interface, member, invoke_kind)` (plus an `"xlc"` section for XLM commands without a COM twin, and an `"officejs"` check verifying every mapped JS member inherits its descriptor member's reach). `Descriptor::get()` loads `reach.json` beside the four library JSONs and attaches `Member::reach: CapabilitySet` (using the ten capability names of `CODE_EXECUTION_POLICY.md` section 2.2: `fs.read`, `fs.write`, `process`, `env`, `settings`, `ffi`, `com.activate`, `ui.keys`, `network`, `project.write`).
   - A portable unit test in `crates/calc-object-model/tests/descriptor.rs` verifies every entry in `reach.json` resolves in the descriptor and every member listed in `CODE_EXECUTION_POLICY.md` section 2.2 (`Workbooks.Open`, `Workbook.SaveAs`, `Workbook.FollowHyperlink`, `Application.ExecuteExcel4Macro`, `Application.RegisterXLL`, `Application.Run`, `Application.VBE`, `Workbook.VBProject`, etc.) carries its declared capabilities.
   - Add `AutoError::PolicyDenied { capability: &'static str, subject: String, number: i32 }` to `crates/calc-object-model/src/model.rs:36` and a single `PolicyChecker` hook on `ApplicationHost` / `WorkbookAutomation::invoke` so every face checks reach in one place and logs to one audit sink before calling any handler.

2. **Multi-observer event fan-out in `HostSession` (`crates/calc-host/src/events.rs`).**
   - Today `WorkbookEventState` (`crates/calc-host/src/events.rs:91-100`) holds a single `owner: Option<Rc<dyn WorkbookEventHandler>>` (VBA) and a single `observer: Option<Rc<dyn WorkbookEventObserver>>` (COM `AutomationObserver`).
   - Replace `observer` with an ordered observer registry (`add_workbook_event_observer` / `remove_workbook_event_observer`, keyed by a token) and extend `WorkbookEventObserver` with `workbook_open`, `before_save` (for COM synchronous `WorkbookEvents.BeforeSave` with `Cancel`), and `selection_changed` (for Office.js `onSelectionChanged` and COM `SheetSelectionChange`).
   - All observers share `HostSession::workbook_events_switch()` (`Rc<Cell<bool>>`). Synchronous observers (VBA, then COM) finish before asynchronous observers (Office.js event queue) enqueue notifications.

3. **Canonical `Application` identity and multi-workbook readiness (`dc-6332.16`).**
   - `WorkbookAutomation::snapshot` (`crates/calc-host/src/automation.rs:84-92, 282-315`) currently tracks one active `HostSession` with a monotonic `workbook: u64` incarnation ID. Keep that single-workbook incarnation honest (never a fake `Book1` in DNA Calc), and prepare `ObjectKey::Workbook(u64)` for a future multi-workbook application registry once cross-workbook calculation ownership lands (`harvest/14-harvest.md` section 6).
   - Single-STA canonical `Application` identity in `calc-com-host` was **implemented in `4094ef5c2`**: `State::application_wrapper` (`crates/calc-com-host/src/projection.rs:394`) and `wrap()` (`crates/calc-com-host/src/runtime.rs:108-118`) reuse the live canonical `Application` `Wrapper` across `Application.Application`, `Application.Parent`, `Window.Application`, `Window.Parent.Application`, and ROT/class-factory acquisitions (releasing the redundant model lease). Second-STA comparison against Excel and no-Office acceptance remain in `dc-6332.16`.

4. **Truthful `OperationOutcome` for commit-then-fail writes.**
   - `WorkbookAutomation::mutate_pre` (`crates/calc-host/src/automation/mutation.rs:446-510`) currently converts a post-commit calculation failure or `HostProgress::Waiting` into a plain `Err(AutoError::not_modelled(...))`, discarding the committed `AuthoredRevision`.
   - Extend the internal outcome returned to callers (especially Office.js batch execution and C API diagnostics) to carry whether the authored edit committed (`revision`) separately from calculation/event faults (`14-object-model-and-js-answer.md` section 3.7), so callers never retry a committed mutation blindly.

5. **Host-neutral CalcVBA convergence (`dc-piw0.5`, prerequisite of `dc-17spk`).**
   - Per owner boundary (2026-11-23, `3de337300`), CalcVBA stays host-neutral with no Excel or Office knowledge. `'Excel'` and `'Office'` are workbook VBA project references whose type libraries and root objects are supplied by DNA Calc (`crates/calc-host-vba` / `crates/calc-vba`) from `calc-object-model` (IIDs, DISPIDs, slots, and the `reach.json` side table keyed by DISPID per `CODE_EXECUTION_POLICY.md` question 8) and routed to `WorkbookAutomation`, retiring the private tables in `crates/calc-vba/src/runtime/spreadsheet.rs` and synthetic IIDs in `crates/calc-vba/src/events/source.rs`. CalcVBA changes only where a generic host-supplied type-library hook is missing.

6. **One user-function registry (`calc_fml::CallableTable`) for all four UDF providers.**
   - Today `crates/calc-fml/src/callable.rs:111-118` (`CallableTable`), `crates/calc-func/src/udf.rs:238-357` (`UserFunction`, `UserFunctionInvocation`, `UserFunctionStep::{Suspended, Awaiting, Complete}`, `UserFunctionContext`), and `crates/calc-addin-host/src/host.rs:353-375` (`AddinHost::callables()`) already unify XLL UDFs and VBA module/`.xlam` UDFs into one generation-checked table published atomically to `HostSession::set_user_functions` (`crates/calc-host/src/session.rs:427`) and `apps/dna-onecalc/src/session.rs:579`.
   - Extend that exact same pipeline to absorb all four providers with zero parallel registries:
     - **XLL UDFs** (`crates/calc-xll-host`): `xlfRegister` registrations (`origin = "xll <file>"`), including `$` thread-safe parallelism (`max_parallel_calls`), `#` cached-reads, `X` async continuations (`UserFunctionStep::Awaiting`), and RTD (`read_topic`).
     - **VBA UDFs** (`crates/calc-vba`, `crates/calc-host-vba`): standard module and `.xlam` functions (`origin = "VBA module <name>"`), registered under both unqualified `Func` and qualified `Module.Func` keys.
     - **JavaScript custom functions (`crates/calc-js-host`, `dc-17spk.6`):** functions declared in `functions.json` (`origin = "JS add-in <id>"`), registered under Excel's namespace spelling (`NAMESPACE.FUNC`, matching `CallableTable`'s `(provider, name)` key), with `CallableDescriptor` populated from `functions.json` (`description`, `parameters`, `volatile`, `asynchronous`), sync calls via `UserFunction::call`, async `Promise` calls via `UserFunction::begin_call` / `UserFunctionStep::Awaiting`, and streaming `setResult` calls via admitted external topic updates (`UserFunctionContext::read_topic`).
     - **Automation add-in UDFs (`crates/calc-com-host`, `dc-17spk.8`, Windows, lower priority):** public `IDispatch` methods on `Programmable` COM classes (`origin = "COM Automation <ProgID>"`), registered under qualified `ProgID.Method` and unqualified `Method` per measured Excel precedence, invoking on the STA (`state_lane`) with `VARIANT` <-> `CalcValue` coercion.
   - **One precedence and Guarded admission rule:** Built-in Excel functions (`calc-catalogue`) and workbook-defined names keep Excel's precedence over add-in UDFs. `AddinHost::callables()` (on desktop) and `Server::set_user_functions` (on worker/browser) assemble the single `CallableTable` in deterministic provider order, filtering out native providers (XLL, Automation add-ins) in `Guarded` and all user code in `Off`, and publishing `CallableDescriptor` metadata so formula completion and signature help (`ONECALC_ADDINS.md`) show every active UDF uniformly.

---

## 3. Face 1: COM object model

### 3.1 Binding

- **Descriptor-generated vtables (dual interfaces):** `crates/calc-com-host/generate.rs` reads `crates/calc-object-model/descriptor/{excel,office,addin-designer,accessibility}.json` at build time and emits one static `RawVtable<N>` for every interface whose slots `7..vtable_slots` are completely described (`OBJECT_MODEL.md` rule 4; incomplete interfaces are never advertised via `QueryInterface` and stay `IDispatch`-only). Each slot is a monomorphized thunk `thunk_<shape>::<I, S>` forwarding to `decode_slot` (`crates/calc-com-host/src/dispatch.rs:206-271`).
  - Win64 x64 register classes (`generate.rs:5-11`): `R4` as `f32`, `R8` and `DATE` as `f64`, all other parameters (`BSTR`, `I2`, `I4`, `BOOL`, `CY`, pointers, by-value `VARIANT` passed by hidden pointer on Win64) as `usize`.
  - `[lcid]` parameters populate `Call::lcid`; `[out, retval]` pointers are zeroed before dispatch (`zero_out`) and written via `write_abi` on success (`dispatch.rs:226-262`).
  - Early-bound calls never silently retry through `IDispatch::Invoke` on failure.
- **`IDispatch` (dispinterfaces and late-bound clients):** Excel's type library declares 28 types in `excel.json` as dispatch-only (7 vtable slots), including `Range`, `Window`, `Windows`, `Areas`, `Names`, `Name`, `WorksheetFunction`, `AppEvents`, `WorkbookEvents`, and `DocEvents`.
  - `GetIDsOfNames` (`dispatch.rs:39-100`) resolves member names and parameter names case-insensitively across base chains from `Descriptor::get()`.
  - `Invoke` (`dispatch.rs:102-204`) and `arguments::bind` (`crates/calc-com-host/src/arguments.rs:14-52`) map reverse-order positional `rgvarg`, named DISPIDs, `DISPID_PROPERTYPUT` (`-3`), `VT_BYREF` (unwrapped via `VariantCopyInd` in `values.rs:202-207`), and omitted optionals (`VT_ERROR` / `DISP_E_PARAMNOTFOUND` -> `AutoValue::Missing`) into the same `Call`. Assigning to a getter-only property returns `DISP_E_BADPARAMCOUNT` (VBA error 450, measured in `dc-piw0` Q3; `dispatch.rs:132-138`).
  - `GetTypeInfo` (`crates/calc-com-host/src/typeinfo.rs`) builds an in-memory `ITypeLib` once per STA via `CreateTypeLib2(SYS_WIN64, "DnaCalcObjects.tlb")` with `TKIND_DISPATCH` `FUNCDESC`s and parameter names, so `TypeName(rng)` answers `"Range"` and late-bound introspection works without writing files or touching Excel's registry keys.

### 3.2 Identity and lifetime

- **Wrapper identity:** `Wrapper` (`crates/calc-com-host/src/runtime.rs:77-92`) is `#[repr(C)]` with primary vtable, `AtomicU32` refcount, `ObjectRef`, weak links to `State` and `ObjectModel`, `Arc<Apartment>`, and two inline tear-offs (`ISupportErrorInfo` and `IConnectionPointContainer`).
  - `Range` and `Window`: one fresh `Wrapper` per acquisition (matching Excel's measured E1 and X19 `COM_IDENTITY` results: two `Range("A2")` acquisitions are not `Is`, two `AccessibleObjectFromWindow` calls return distinct `IUnknown`s).
  - `Application`: **canonical on its STA** (`4094ef5c2`, `crates/calc-com-host/src/projection.rs:394` + `runtime.rs:108-118`). `State::application_wrapper` caches the live canonical `Application` `IDispatch` and releases the extra `ObjectModel` lease on re-acquisition, so `Application.Application`, `Application.Parent`, `Window.Application`, `Window.Parent.Application`, and ROT/class-factory acquisitions share one controlling `IUnknown`.
- **Acquisition accounting:** `ResultValues` (`projection.rs:46-73`) tracks every `ObjectRef` returned by `ObjectModel::invoke` (including inside nested `AutoArray`s). If conversion or `QueryInterface` fails, or a caller discards the result, `Drop` calls `ObjectModel::released` exactly once for every unwrapped object. `ArgumentScope` (`projection.rs:77-96`) releases borrowed foreign `IDispatch` arguments at call end without holding `RefCell` borrows during client `Release`.
- **Tracker release:** `destroy()` (`runtime.rs:213-221`) removes the wrapper from `State::wrappers` and calls `ObjectModel::released(&wrapper.target)`, which decrements `WorkbookAutomation::ranges` and releases the `ReferenceTracker` slot when the last lease drops (`crates/calc-host/src/automation.rs:904-918`).
- **Disconnection:** Dropping `Projection` closes add-ins, drains `Apartment::retired`, sets `apartment.connected = false`, and clears connections, foreign refs, and `application_wrapper` (`projection.rs:473-490`). Calls on any surviving wrapper return `CO_E_OBJNOTCONNECTED`.
- **Out-of-process client rundown:** When an external COM client crashes without calling `Release`, Windows COM's DCOM/LRPC stub manager pings the client and releases the server-side stub references (~6 minutes default DCOM rundown). On workbook close or application exit, DNA Calc should call `CoDisconnectObject` on active exported objects (in addition to `apartment.connected = false`) so shutdown never hangs waiting on dead external clients.

### 3.3 Out-of-process COM on Windows (`dc-17spk.2`)

Out-of-process COM exposes DNA Calc's STA projection to external Windows clients while upholding the hard constraint of `XLL_COM_HOSTING.md` section 8 (**never write, alter, or shadow Microsoft Excel's HKLM or HKCU ProgIDs, CLSIDs, TypeLibs, or interface keys**).

**Already landed in `4094ef5c2` (cross-built with `cargo-xwin`, queued for native Windows acceptance in `docs/plan/windows-queue.md#dc-17spk2-desktop-automation`):**
1. **DNA-owned activation identity (`crates/calc-com-host/src/automation.rs:19`):** `APPLICATION_CLSID = {B363DB86-B412-4C60-91F2-9D538F75BB65}` (`DnaCalc.Application`), completely independent of Excel's `{00024500-...}` coclass.
2. **ROT and `LocalServer32` class-factory publication (`automation.rs:78-149`):** `LocalAutomation::publish(projection)` calls `RegisterActiveObject(&app, &APPLICATION_CLSID, ACTIVEOBJECT_WEAK, ...)` and `CoRegisterClassObject(&APPLICATION_CLSID, &factory, CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE)`. `LocalAutomation::set_workbook(id, path)` creates a file moniker (`CreateFileMoniker`) for absolute local-drive workbook paths and registers the live `ActiveWorkbook` wrapper in the ROT (`ROTFLAGS_REGISTRATIONKEEPSALIVE`), revoking the prior cookie on open/save and revoking all cookies on `Drop`.
3. **Desktop CLI triggers (`apps/dna-calc-desktop/src/main.rs:75-81`, `app.rs:105-106, 349-360, 450-467`):** `dna-calc-desktop.exe --automation` (or COM-supplied `-Embedding` after opt-in per-user `HKCU\Software\Classes` registration of `DnaCalc.Application\CLSID`, `CLSID\{...}\LocalServer32`, and `ProgID`) publishes `LocalAutomation` once the real workbook session replaces the provisional startup session.
4. **Late-bound cross-process events (`crates/calc-com-host/src/events.rs:130-137`):** `FindConnectionPoint` accepts `IID_IDispatch` on `Application` in addition to `AppEvents`'s dispinterface IID, allowing external late-bound clients on machines without Office to receive `AppEvents` (`SheetChange`, `SheetCalculate`, `AfterCalculate`) through the system `IDispatch` proxy.

**Remaining design requirements to complete `dc-17spk.2`:**
1. **Native Windows acceptance run (`docs/plan/windows-queue.md#dc-17spk2-desktop-automation`):** Execute warm (`GetActiveObject` + workbook moniker) and cold (`CoCreateInstance` / `New-Object -ComObject DnaCalc.Application`) tests (`external_automation_client`, `external_automation_revoked`) on a clean non-elevated Windows profile without Office.
2. **Early-bound dual-interface marshalling without shadowing Excel's keys:**
   - Late-bound (`IDispatch`) works across apartments/processes on every Windows machine today via `oleaut32.dll`'s built-in `IDispatch` proxy/stub.
   - Early-bound cross-process calls on dual interfaces (`_Application`, `_Workbook`, `_Worksheet`, etc.) require `PSOAInterface` (`{00020424-...}`) type-library marshalling (or a dedicated proxy/stub). On a machine **with Excel installed**, Excel's `HKLM\Software\Classes\Interface\{Excel-IID}` keys already exist and DNA Calc must **never** overwrite or shadow them. On a machine **without Office** (`dc-6332.16`), `typeinfo.rs` currently emits `TKIND_DISPATCH` only — completing dual-interface marshalling requires a verified vtable TypeLib (`TKIND_INTERFACE | TYPEFLAG_FDUAL | TYPEFLAG_FOLEAUTOMATION`) or custom proxy arrangement that installs no keys conflicting with Excel (checked against `HKLM` before any opt-in `HKCU` write, and fully reversible).
3. **STA `IMessageFilter` during calculation:** Register a Rust `IMessageFilter` via `CoRegisterMessageFilter` on DNA Calc's STA. While `HostSession` is inside a non-reentrant calculation slice or edit commit (and the call is `CALLTYPE_TOPLEVEL`, not `CALLTYPE_NESTED` from a synchronous event sink), `HandleInComingCall` returns `SERVERCALL_REJECTED` so the remote client receives `RPC_E_CALL_REJECTED` (`0x80010001`), matching Excel's measured X19 `COM_STA2` result (`OBJECT_MODEL.md` section 9).
4. **Clean shutdown & opt-in registration helper:** Call `CoDisconnectObject` on exported objects at shutdown; decide headless `--embedding` lifetime (`Factory::LockServer` in `automation.rs:44-47` currently keeps visible-window lifetime only — add lock-count exit when started hidden by COM unless `Application.Visible = True` is set); optionally add `dna-calc-desktop --register-com` / `--unregister-com` writing and removing only the `HKCU\Software\Classes` DNA Calc keys.
5. **Alignment with Add-in Surrogate (`ADDIN_SURROGATE.md` Stage 4):** Once Stage 4 moves `HostSession`, `WorkbookAutomation`, and the COM STA into the surrogate process (`ADDIN_SURROGATE.md` sections 2–3), `LocalAutomation` lives in the surrogate process beside `HostSession`, while committed edits flow through the exact same durable controller commit point (`ADDIN_SURROGATE.md` section 7).

### 3.4 In-process COM on Linux (and macOS later)

Today `crates/calc-com-host/src/lib.rs:83-112` compiles all COM runtime modules under `#[cfg(windows)]`. On Linux, two distinct in-process layers apply:

1. **Portable Rust `ObjectModel` API (works on Linux today).** `calc-object-model` and `calc_host::automation::WorkbookAutomation` compile and pass all 626-member coverage and report tests on Linux. Rust embedders and CalcVBA (`dc-piw0.5`) call `ObjectModel::invoke(&target, &call)` directly with zero OS COM dependency.
2. **In-process binary COM ABI on Linux (`x86_64` SysV ABI, new work).** For native C/C++/Python/.NET in-process callers on Linux that expect binary `IUnknown`/`IDispatch`/dual-vtable pointers:
   - Extract the OS-independent ABI logic (wrapper refcounting, `ObjectRef` table, `arguments::bind`, descriptor vtable generation, `IConnectionPoint` dispatch) from Win32-only APIs (`LresultFromObject`, `CreateTypeLib2`, `Win32_System_Registry`, `LocalAutomation`, `RtdClient`).
   - Provide a Linux `#[repr(C)]` ABI layer defining binary-compatible `GUID`, `HRESULT`, `VARIANT` (24 bytes), `BSTR` (length-prefixed UTF-16 buffer with a Rust-owned allocator replacing `oleaut32::SysAllocString`/`SysFreeString`), `SAFEARRAY`, `DISPPARAMS`, `EXCEPINFO`, `IUnknown`, `IDispatch`, and `IConnectionPointContainer`.
   - **Calling-convention divergence in `generate.rs`:** Win64 `extern "system"` passes 4 arguments in `rcx/rdx/r8/r9` or `xmm0..xmm3` with 32-byte shadow space and passes >8-byte structs (like by-value `VARIANT` in `Application.Run`) via hidden caller-allocated pointer; Linux x86_64 SysV `extern "C"` passes up to 6 integer/pointer arguments in `rdi/rsi/rdx/rcx/r8/r9`, up to 8 float arguments in `xmm0..xmm7` **independently** of integer register positions, and passes 24-byte `VARIANT` by value on the stack (`MEMORY` class). The Linux thunk generator in `generate.rs` must emit genuine SysV parameter types (including by-value `VARIANT`, not `usize`) rather than reusing the Win64 `usize`-register erasure.
   - **Explicit Linux exclusions:** No `WM_GETOBJECT`, no ROT, no `LocalServer32`, no Win32 `CreateTypeLib2`, and no Windows COM add-in / Automation add-in / RTD DLL loading (`COMAddIns`, Automation add-ins, and `RtdClient` stay Windows-only typed refusals).

### 3.5 Threading, events, errors, and Guarded reach (COM)

- **Threading:** Owner STA thread checked via `Apartment::check()` (`runtime.rs:56-65`); wrong-thread direct calls return `RPC_E_WRONG_THREAD`; cross-thread `Release` defers `destroy()` to `Apartment::retired` (`runtime.rs:200-209`); marshaled calls during calculation return `RPC_E_CALL_REJECTED` via `IMessageFilter`.
- **Events:** `IConnectionPointContainer` / `IConnectionPoint` (`crates/calc-com-host/src/events.rs`) snapshots sinks before delivery and invokes with declared-order named DISPIDs `[0, 1, ...]` (measured X0).
  - Gaps to close: add `WorkbookEvents` (`BeforeSave`, `Open`, `SheetChange`, `SheetCalculate`) and `DocEvents` (`Change`, `Calculate`) routing in `AutomationObserver`, plus `VT_BYREF | VT_BOOL` `Cancel` copyback in `State::raise` (`events.rs:147-211`) so a COM `BeforeSave` sink can cancel a save synchronously (`harvest/14-harvest.md` E5).
- **Errors:** Uniform Rule 5 lowering in `runtime.rs:357-391` (`SetErrorInfo` + `0x800A03EC` on vtable / null `EXCEPINFO`; `DISP_E_EXCEPTION` + `EXCEPINFO` with `wCode=0`, `scode=0x800A03EC` when `EXCEPINFO` is non-null; `CO_E_OBJNOTCONNECTED` on `Disconnected`; `PolicyDenied` -> `0x800A0046` error 70 or `0x800A01AD` error 429). `dispatch.rs:199, 267` calls `state.record(&e)` once so worksheet UDFs cannot swallow `NotModelled` refusals.
- **Guarded reach:** Native COM add-ins (`dc-17spk.7`) and Automation add-ins (`dc-17spk.8`) never load in `Guarded` (`CODE_EXECUTION_POLICY.md` section 1). In `Unguarded`, descriptor-member reach is still audited. Out-of-process COM clients (`dc-17spk.2`) run in their own OS process, so DNA Calc enforces the application's automation/macro policy before admitting `Application.Run`, `ExecuteExcel4Macro`, `RegisterXLL`, or external file/network members, and audits every reaching member.

### 3.6 COM Add-ins (`IDTExtensibility2`, `dc-17spk.7`, P1, Windows)

Excel COM add-ins are in-process DLLs or out-of-process EXEs implementing `IDTExtensibility2` (`{B65AD801-ABAF-11D0-BB8B-00A0C90F2744}`) and optionally `IRibbonExtensibility` (`{000C0396-0000-0000-C000-000000000046}`).

**What already exists in `crates/calc-com-host`:**
- Foreign interface definitions verified slot-by-slot against `addin-designer.json` and `office.json` in `crates/calc-com-host/src/interfaces.rs:12-29, 63-87` (`IDTExtensibility2` slots 7..=11, `IRibbonExtensibility` slot 7).
- `crates/calc-com-host/src/office.rs` implements **49** COM-owned Office handlers (`Application.COMAddIns`, `Application.CommandBars`, `COMAddIns`, `COMAddIn` including `Object` reentrancy preparation in `Office::prepare`, `_CommandBars`, `CommandBar`, `CommandBarControls`, `CommandBarPopup`, `_CommandBarButton`), `Office::refresh()` (`office.rs:789-893`), `Office::connect()` (`office.rs:701-788`), `Office::close()` (`office.rs:894-917`), and `Office::ribbons()` (`office.rs:918-980`, surfaced via `AddinHost::ribbons()` at `crates/calc-addin-host/src/host.rs:442-444`). `IDTExtensibility2` connection/disconnection was also exercised in CalcVBA's `wrapped_com_server_smoke` and `office.rs`'s native unit tests (`office.rs:1031-1354`).

**Exact gaps in `office.rs` to support standalone Excel COM add-ins (`dc-17spk.7`):**
1. **Registry discovery (`HKCU` + `HKLM`, `LoadBehavior`, `FriendlyName`):** Today `office.rs:795-797, 842-860` scans only `HKCU\Software\Microsoft\Office\Excel\Addins` and resolves `CLSID`/`InprocServer32` only in `HKCU`, ignoring `LoadBehavior` and `FriendlyName`. Extend discovery to enumerate both `HKCU` and `HKLM` `Software\Microsoft\Office\Excel\Addins\<ProgID>` (with `HKCU` overriding `HKLM` for the same ProgID), read `LoadBehavior` (`DWORD`: bit `1` = connected, bit `2` = load at startup -> `3` = load on startup; `8` = demand; `16` = first-time), `FriendlyName`, and `Description`, and resolve `CLSID` through `HKCR` (`HKCU\Software\Classes` then `HKLM\Software\Classes`).
2. **Standalone DLL and EXE activation:** Today `office.rs:751` and `office.rs:861` reject any add-in whose DLL is not already loaded by `XllHost` (`!state.host.contains_loaded_module(&data.module)` — built for Excel-DNA's XLL-registered COM add-in pattern). Lift that restriction for standalone COM add-ins explicitly allowed by policy (and CLI/config override for tests), supporting both `InprocServer32` (`CLSCTX_INPROC_SERVER`, with `ModuleLease` teardown pin on failure) and `LocalServer32` (`CLSCTX_LOCAL_SERVER` out-of-process EXE add-ins, which do not use `LoadLibraryExW`).
3. **Complete `IDTExtensibility2` lifecycle:** Today `office.rs:774` always passes `ext_ConnectMode = 0` (`ext_cm_AfterStartup`) and calls `OnConnection` -> `OnAddInsUpdate`, never `OnStartupComplete`. Startup-loaded add-ins (`LoadBehavior & 2 != 0`) must receive `ext_cm_Startup = 1` in `OnConnection(Application, 1, COMAddIn, &custom)`, followed by `OnAddInsUpdate` and `OnStartupComplete` once the desktop window and workbook session are live, and `OnBeginShutdown` + `OnDisconnection(ext_dm_HostShutdown = 0, &custom)` on close (with `ext_dm_UserClosed = 1` on `COMAddIn.Connect = False`).
4. **Ribbon XML callbacks and UI integration:** Wire `IRibbonExtensibility::GetCustomUI("Microsoft.Excel.Workbook")` XML returned by `AddinHost::ribbons()` into DNA Calc's desktop shell UI, dispatching ribbon callbacks (`onAction`, `getLabel`, `getEnabled`, `getVisible`, `onLoad`) via `IDispatch::Invoke` on the add-in's `IDTExtensibility2` object with descriptor-backed `IRibbonControl` and `IRibbonUI` (`Invalidate` / `InvalidateControl`) wrappers. Keep `CommandBars` minimal (already bounded in `office.rs`); defer `ICustomTaskPaneConsumer` to a later step.
5. **Trust and platform:** Never loaded in `Off` or `Guarded` (`CODE_EXECUTION_POLICY.md` section 1), regardless of `LoadBehavior`. Windows x64 only; on Linux, COM add-ins are reported as Windows-only with a typed refusal. Verify with both Excel-DNA's COM add-in path and a standalone C# or C++ `IDTExtensibility2` sample add-in.

### 3.7 Automation Add-ins (`dc-17spk.8`, P3, lower priority, Windows)

Excel Automation add-ins are COM classes registered with a `CLSID\{...}\Programmable` subkey and enabled in Excel's Add-ins dialog (`HKCU\Software\Microsoft\Office\<ver>\Excel\Options`, `OPEN`/`OPEN1`/… string values of the form `/A "ProgID"`). Their public COM methods become worksheet functions (`=ProgID.Method(a, b)` or unqualified `=Method(a, b)`).

- **Discovery and activation:** Enumerate configured Automation add-in ProgIDs (from user options or explicit CLI/config test list — never modifying Excel's registry keys), verify `CLSID\{...}\Programmable`, instantiate via `CoCreateInstance` on the STA (or keep lazy per-session STA instance), and inspect the default `IDispatch` interface's `ITypeInfo` (`GetTypeAttr` / `GetFuncDesc`) to enumerate public `INVOKE_FUNC` / `INVOKE_PROPERTYGET` methods (filtering out `IUnknown`/`IDispatch` and .NET `Object` base methods per Excel's rules).
- **One user-function registry (`CallableTable`):** Each discovered method registers as a `calc_func::UserFunction` adapter in `calc_fml::CallableTable` (merged via `AddinHost::callables()`), keyed under both qualified `(Some(progid), method)` and unqualified `(None, method)` per measured Excel shadowing rules, with `CallableDescriptor` populated from `ITypeInfo` parameter names and docstrings (`origin = "COM Automation <ProgID>"`).
- **Execution and coercion:** Calls run on the owning STA (`UserFunction::state_lane`), converting `CalcValue` arguments to `VARIANT`s via `crates/calc-com-host/src/values.rs` (including missing optionals, 1-based 2-D `SAFEARRAY`s, and `Range` object vs dereferenced value passing) and lowering `VARIANT` / `EXCEPINFO` / `HRESULT` back to `CalcValue` or Excel worksheet errors.
- **Measure Excel first:** Because Automation add-in edge cases (argument coercion, `Range` parameters, `Missing`/`Empty`, COM error -> `#VALUE!`/`#NA` mapping, `IDTExtensibility2` coexistence, volatility/recalc rules, name collisions with built-ins and XLL/VBA functions) have not yet been captured in this repository, `dc-17spk.8` begins with a small Windows Excel probe sample before freezing adapter behaviour.
- **Trust and platform:** Never loaded in `Off` or `Guarded`; Windows x64 only (typed refusal on Linux); explicitly lower priority than COM add-ins (`dc-17spk.7`).

---

## 4. Face 2: Excel C API command face (`xlc*` and `xlSet`)

Excel's C API (`Excel4`/`Excel4v` and `Excel12`/`Excel12v`) has three function-number namespaces (`crates/calc-xll-abi/src/lib.rs:40-45`):
- `xlfXXX` (`0x0000..=0x0FFF`): worksheet and XLM information functions — **already served** for 478 Ftab functions + `614` (`FILTER`) + 12 explicit callbacks in `crates/calc-xll-host/src/callbacks.rs`.
- `XL_SPECIAL` (`0x4000`, `16384..=16403`): host services — 13 served today, **excluding `xlSet` (`16387`)**, which is a command-context cell-write operation belonging to this command face.
- `XL_COMMAND` (`0x8000 | Cetab`, `xlcXXX`): Excel 4 macro commands — **only 1 (`XLC_MESSAGE = 32890`) is handled today**, and it only sets an in-memory status string without reaching `calc-host`.

### 4.1 Binding: `Excel12`/`Excel12v`, `Excel4`/`Excel4v`, flags, and arguments

- **Excel 12 entry points (Unicode, large grid):**
  - Windows XLLs call `MdCallBack12(xlfn, count, opers, result)` exported from `dna-calc-desktop.exe` / `dna-onecalc-desktop.exe` (`apps/dna-calc-desktop/src/main.rs:27-34`) or `Excel12`/`Excel12v` in companion `XLCALL32.DLL` (`platform/calc-xll-shim/src/bridge.rs:36-63`, `variadic.c:6-15`).
  - Operands are 32-byte `XlOper12` (`crates/calc-xll-abi/src/lib.rs:137-140`): UTF-16 length-prefixed strings (up to 32,767 code units), 32-bit `XlRef12` (`row_first..row_last` in `0..1_048_576`, `col_first..col_last` in `0..16_384`), `XlMRef12` multi-area references, `XlSRef12` active-sheet references, `Fp12` arrays, `XlBigData`.
- **Excel 4 entry points (legacy ANSI, small grid):**
  - Legacy XLLs and C callers invoke `Excel4` (`__cdecl` variadic) and `Excel4v` (`__stdcall`) in `XLCALL32.DLL` (today stubbed to return `32` in `platform/calc-xll-shim/src/variadic.c:17-22`).
  - Add `XlOper` (legacy `XLOPER`), `XlRef` (`u16` rows `0..65_536`, `u8` cols `0..256`), `XlMRef`, `XlSRef`, `Fp`, and ANSI byte-counted strings (max 255 bytes, code-page conversion matching `calc-xll-host/src/call.rs:56-117` on Windows / Windows-1252 table on Linux, measured against Excel) to `crates/calc-xll-abi`.
  - `Excel4`/`Excel4v` lowers legacy `XLOPER` inputs to `CalcValue` / `AutoValue`, calls the **exact same** command/function dispatcher as `Excel12`, and narrows the result back to `XLOPER` (returning `xlretFailed` or truncating/refusing per measured Excel rules when a string exceeds 255 bytes or a reference exceeds 65,536 rows / 256 columns — measured in C1–C4 probes before freezing).
- **Flag bits on `xlfn` (never silently stripped, `crates/calc-xll-abi/src/lib.rs:40-45`):**
  - `XL_COMMAND` (`0x8000`): selects the `xlc*` command table (Cetab index = `xlfn & 0x0FFF`).
  - `XL_INTL` (`0x2000`): forces US-English formula syntax, A1/R1C1 spellings, and locale-independent command argument parsing (matching `Range.Formula` rather than `Range.FormulaLocal`). Without `XL_INTL`, locale-dependent syntax must follow the session's UI/locale rules or refuse explicitly if non-en-US parsing is unmodelled.
  - `XL_PROMPT` (`0x1000`): requests Excel's interactive command dialog (e.g., `xlcFormula | xlPrompt`, `xlcFileSaveAs | xlPrompt`). In headless/non-interactive execution or where no dialog bridge exists, `XL_PROMPT` MUST return a typed `NotModelled` refusal — never silently strip `XL_PROMPT` and mutate the workbook.
- **Argument rules:**
  - Positional `0..=255`; omitted trailing arguments and `XL_MISSING` (`0x80`) map to `AutoValue::Missing`; blank cells (`XL_NIL`, `0x100`) map to `AutoValue::Empty`.
  - Reference arguments (`XL_SREF` on active sheet, `XL_REF` with `IDSHEET` decoded via `SheetMap`) convert to `ReferenceLike`, are retained for the duration of the command via `ReferenceTracker` (producing an `ObjectRef` of interface `Range`), passed to the descriptor handler, and released before returning.
  - `xlc*` commands with an optional target reference that the caller omits (e.g., `xlcFormula("=A1+1")`, `xlcClear(3)`) resolve `Application.Selection` through `ObjectModel` first, then invoke the descriptor member on that `Range`.

### 4.2 Mapping `xlc*` commands and `xlSet` onto `ObjectModel` and `calc-host`

A declarative command table (to be inventoried completely in `dc-17spk.3` -> `docs/architecture/C_API_COMMANDS.md`) maps each `xlc*` Cetab number to its owning object-model interface/member (or explicit host service):

| Command group | Representative C API numbers | Target `ObjectModel` / `calc-host` owner | Translation notes |
|---|---|---|---|
| **Direct value write** | `xlSet` (16387) | `Range.Value2` Put (`WorkbookAutomation::put_value2`) | Command-context only; writes literal/array values directly without formula entry parsing (measure in C1 whether `xlSet` fires `Change`, respects calc mode, and handles text strings starting with `=`). |
| **Formulas & entry** | `xlcFormula`, `xlcFormulaArray`, `xlcFormulaFill`, `xlcClear` | `Range.Formula` Put (`formula.rs:228`), `Range.FormulaArray` Put, `Range.ClearContents` (`mutation.rs:249`) | `xlcFormula(text, [ref])` defaults `ref` to `Application.Selection`; respects `XL_INTL`. |
| **Calculation** | `xlcCalculateNow`, `xlcCalculateDocument`, `xlcCalculation` | `_Application.Calculate`, `_Worksheet.Calculate` (`mutation.rs:397-429`), `_Application.Calculation` Put | Maps directly to existing `SkinIntent::Recalculate` / `Calculate` / `SetCalcMode`. |
| **Selection & navigation** | `xlcSelect`, `xlcActivate`, `xlcWorkbookActivate`, `xlcWorkbookSelect` | `Range.Select`, `_Worksheet.Activate`, `_Workbook.Activate` | Updates `HostSession` selection (`SkinIntent::SetSelection`) through `WorkbookAutomation`. |
| **Names** | `xlcDefineName`, `xlcDeleteName` | `Names.Add`, `Name.Delete` (extend `navigation.rs` / `calc-host/src/names.rs`) | Distinct from `xlfSetName` (88), which sets/deletes XLL/macro-sheet names (`callbacks.rs:883-899`). |
| **Sheets & structure** | `xlcWorkbookInsert`, `xlcWorkbookDelete`, `xlcWorkbookName`, `xlcInsert`, `xlcEditDelete` | `Sheets.Add`, `_Worksheet.Delete`, `_Worksheet.Name` Put, `Range.Insert`, `Range.Delete` | Structural edits update `ReferenceTracker` at commit (`calc-host/src/session.rs`) automatically. |
| **Workbooks & files** | `xlcNew`, `xlcOpen`, `xlcSave`, `xlcSaveAs`, `xlcFileClose` | `Workbooks.Add`, `Workbooks.Open`, `_Workbook.Save`, `_Workbook.SaveAs`, `_Workbook.Close` | Carries `fs.read` / `fs.write` / `network` Guarded reach from the descriptor. |
| **Formatting & layout** | `xlcFormatNumber`, `xlcFormatFont`, `xlcColumnWidth`, `xlcRowHeight`, `xlcBorder`, `xlcPatterns` | `Range.NumberFormat`, `Font.*`, `Range.ColumnWidth`, `Range.RowHeight`, `Borders.*`, `Interior.*` | Translates XLM positional flags into `calc-host` format/axis commands via `WorkbookAutomation`. |
| **Macro execution** | `xlcRun`, `xlcOnKey`, `xlcOnTime`, `xlcOnSheet`, `xlcOnRecalc` | `_Application.Run` (`values_only.rs:329`), `_Application.OnKey` / `OnTime`, `HostSession` events | `xlcRun` routes through `Application.Run`; `xlcOn*` hooks attach to the single event/scheduler owner. |
| **UI / controller feedback** | `xlcEcho` (`0x8000 \| 141`), `xlcMessage` (`0x8000 \| 122`), `xlcAlert` | `_Application.ScreenUpdating`, `_Application.StatusBar`, controller dialog bridge | Under `ADDIN_SURROGATE.md` section 5, `xlcAlert` crosses as a `parent`-scoped controller request; headless mode refuses `xlcAlert` typed. |

*(Only the three command numbers verified in the repository today — `XL_SET = 16387`, `XLC_MESSAGE = 0x8000 | 122 = 32890` in `crates/calc-xll-abi/src/lib.rs:58, 63`, and `ECHO = 0x8000 | 141 = 32909` in `crates/calc-xll-host/src/callbacks.rs:622, 1785` — are numbered here. Exact Cetab numbers and argument lists for every `xlcXXX` constant in `xlcall.h` are inventoried in `dc-17spk.3`, `docs/architecture/C_API_COMMANDS.md`.)*

### 4.3 Identity and lifetime

- **No persistent object wrappers:** C API references are value descriptors (`XlSRef12` or `XlMRef12` + `IDSHEET`).
- **Sheet handles (`SheetMap`, `crates/calc-xll-abi/src/value.rs:10-23`):** Generational `SheetId`s map to opaque non-zero `usize` handles. `xlSheetId` encodes, `xlSheetNm` formats `[Book]Sheet`, and deleted sheets fail resolution rather than aliasing a new sheet.
- **Callback result storage:** Host-allocated `XL_STR`/`XL_MULTI`/`XL_REF` callback results live in `State::allocations` (`callbacks.rs:543-552`) until `xlFree` releases them (nulling the pointer word and setting `xltype = XL_MISSING` as measured in x13, `callbacks.rs:567-576`).
- **Temporary `ObjectModel` acquisitions:** When a C API command translates an `XLOPER12` reference into an `ObjectKey::Range(StoredReference)` to invoke `WorkbookAutomation`, an RAII scope calls `ObjectModel::released` before `Excel12`/`Excel4` returns, so commands never leak `ReferenceTracker` slots.
- **Return memory ownership:** Add-in returns with `xlbitDLLFree` invoke `xlAutoFree12` in a `FrameKind::FreeOnly` frame on the calling thread (`callbacks.rs:61-79, 366-375`).

### 4.4 Threading, calling contexts, and Linux in-process split

| Entry context | Thread | `xlc*` commands & `xlSet` | Evidence / code |
|---|---|---|---|
| **Normal worksheet UDF** | Main lane (STA) | `2` (`xlretInvXlfn`) + `#VALUE!` (measured denial) | `callbacks.rs:639` (x13) |
| **Thread-safe (`$`) UDF** | Worker pool (1–4 threads, `worker.rs`) | `128` (`xlretNotThreadSafe`) + `#VALUE!` (measured denial) | `callbacks.rs:638` (x13) |
| **Macro-equivalent (`#`) UDF** | Main lane (STA) | Non-mutating subset per Excel (`xlcMessage`/`xlcEcho` -> `0` + `FALSE`); mutating commands refused/denied as measured | `callbacks.rs:707-714, 754-769` (x13) |
| **Registered command (`macro_type == 2`, `Application.Run`)** | Main lane (STA) | **Admitted** via `ObjectModel` -> `WorkbookAutomation` (`with_session_mut`, `CommandIngress::Automation`) | `native.rs:1058-1092`, `desktop/src/automation.rs:109-136` |
| **Lifecycle (`xlAutoOpen`/`xlAutoClose`)** | Main lane (STA) | Admitted subset (`xlcMessage` returns `TRUE`; verify workbook-edit commands during open in C-API probes) | `callbacks.rs:768` (X3) |
| **In-process C API command host (Win/Linux embedder)** | Main lane (STA) | **Admitted** inside an explicit command frame bound to `Rc<dyn ObjectModel>` | New portable command frame |
| **`xlAutoFree12` (`FreeOnly`)** | Calling thread | Refused (`32` + `#VALUE!` + `dc-6332.13` refusal; Excel crashes there) | `callbacks.rs:366-375` (x13) |
| **Background async thread** | Any thread | Only `xlAsyncReturn` (16400); all other calls return `32` (`xlretFailed`) | `callbacks.rs:357-364` (X0) |

**Enabling Linux (and macOS later) in-process C API commands without inventing a `.so` XLL plugin ABI:**
- Today `crates/calc-xll-abi` already compiles and passes all layout tests on Linux (it refuses only `wasm32`). Only `crates/calc-xll-host` compiles all runtime modules strictly under `#[cfg(windows)]` because Win32 DLL loading (`LoadLibraryExW`, `GetProcAddress`, `dbghelp`, `user32` in `native.rs`) lives in the same crate as `callbacks.rs`.
- Extract the OS-independent C API command & value dispatcher (a non-Windows module in `calc-xll-host` or sibling crate) holding an `Rc<dyn ObjectModel>`, `SheetMap`, `Arena`, callback allocation table, and command-frame guard, with zero `kernel32`/`user32`/`libffi` dependency.
- On Windows, `calc-xll-host`'s `MdCallBack12` and `calc-xll-shim`'s `Excel4`/`Excel4v`/`Excel12`/`Excel12v` route `XL_COMMAND` and `XL_SET` into that shared command dispatcher.
- On Linux, in-process C/C++/Rust embedders (and `dnacalc` / Linux tests) enter the exact same `Excel12`/`Excel12v`/`Excel4`/`Excel4v` command dispatcher over `WorkbookAutomation` using SysV `extern "C"`. Loading unmodified Windows `.xll` PE DLLs remains a typed platform refusal on Linux (`LINUX_DESKTOP_SHELL.md`: we do not invent a non-Excel `.so` XLL plugin format).

### 4.5 Events, errors, and Guarded reach (C API)

- **Events:**
  - `xlEventRegister` (`1` = `CalculationEnded`, `2` = `CalculationCanceled`, `callbacks.rs:851-867`) registers native callbacks per module; firing hooks into `HostSession`'s interactive recalculation lifecycle (and **not** on programmatic `Application.Calculate`, matching Microsoft's documented rule in `XLL_COM_HOSTING.md` section 5).
  - Legacy XLM event hooks (`xlcOnSheet`, `xlcOnRecalc`, `xlcOnEntry`, `xlcOnData`) register observers on `HostSession`'s single observer fan-out and dispatch their named macro via `Application.Run`. `xlcOnKey` and `xlcOnTime` require UI/timer controller hooks and stay typed refusals until their own beads land.
- **Errors:**
  - Every callback exits through `CallbackReply::apply` (`callbacks.rs:135-160`).
  - `AutoError` -> C API mapping:
    - `NotModelled { what, owner }` -> `xlretInvXlfn` (2) or `xlretFailed` (32) + `#VALUE!` in `*result` + `refuse(frame, ...)` (Honesty Rule 2).
    - `Status(DISP_E_BADPARAMCOUNT)` -> `xlretInvCount` (4); `Status(DISP_E_TYPEMISMATCH)` -> `xlretInvXloper` (8) or `xlretFailed` (32) as measured.
    - `Excel { .. }` (runtime failure of an implemented `xlc*` command) -> `xlretSuccess` (0) with `xltypeBool(FALSE)` or `xlretFailed` (32) per Excel's measured per-command answer.
    - `PolicyDenied` -> `xlretFailed` (32) + `#VALUE!` + audit record.
    - `Disconnected` -> `xlretFailed` (32) + `#VALUE!`.
  - Measured Excel denials (`measured_denial`, `callbacks.rs:629-644`) return Excel's exact status and write `#VALUE!` with `refusal: None`.
- **Guarded reach:**
  - Native `.xll` DLLs never load in `Guarded` (`CODE_EXECUTION_POLICY.md` section 1).
  - When C API commands run under `Guarded` — specifically through VBA's `Application.ExecuteExcel4Macro` or a Guarded in-process embedder — every `xlc*` command checks the mapped descriptor member's `reach` (plus `reach.json`'s `"xlc"` overlay for commands like `xlcExec` -> `process`, `xlcOpen` -> `fs.read` + `network`, `xlcSaveAs` -> `fs.write`, `xlcSendKeys` -> `ui.keys`, `xlcRegister` -> `ffi`) through the same `PolicyChecker` and audit sink.

---

## 5. Face 3: Office.js scripting face (two interactions)

Office.js splits cleanly into two complementary runtimes (`dc-17spk.5` and `dc-17spk.6`) that share `calc-object-model`'s semantics and Excel's data model without forcing a heavy JS engine into every host mode:

- **Interaction 1 — Office Add-ins runtime (`dc-17spk.5`):** Task panes, ribbon commands, dialogs, and events run in a separate browser runtime out of process from the calc core (WebView2 on Windows, WebKitGTK on Linux, or an iframe/Worker beside the page in the browser build). No embedded JS engine is required in the calc process.
- **Interaction 2 — Embedded JavaScript custom functions (`dc-17spk.6`):** Cell-callable JS custom functions (`functions.json` + JS) run next to the calc engine behind a compile-time swappable `JsEngine` trait (`crates/calc-js-host`: QuickJS, Boa, or V8 natively; browser Worker on `wasm32`) and register into `calc_fml::CallableTable`. When an add-in manifest requests a **shared runtime**, its custom functions can instead execute inside the Interaction 1 webview so task pane and custom functions share JS globals.

### 5.1 Interaction 1: Office Add-ins runtime — binding, clean-room library, and manifests (`dc-17spk.5`)

- **Clean-room MIT client library (licence rule):** DNA Calc implements its own Office.js-compatible client library (`apps/dna-calc/web/office-js/`). **Do not ship or bundle Microsoft's `office.js`** unless its licence is explicitly checked and cleared; our clean-room library speaks our JSON batch protocol directly while matching Office.js's public API surface (`Office.onReady`, `Excel.run`, `RequestContext`, `OfficeExtension.Error`, `Office.context.ui.displayDialogAsync`, `Office.actions.associate`).
- **Manifest loading and hosting:** Load both classic Office Add-in XML manifests (`OfficeApp` `<TaskPaneApp>` / `<VersionOverrides>`) and unified Microsoft 365 JSON manifests (`manifest.json`), extracting:
  - Task pane entry URLs (hosted in a dedicated WebView2 child view on Windows, WebKitGTK panel on Linux, or sandboxed `<iframe>` in browser wasm — cross-platform by construction).
  - Ribbon command actions (`FunctionFile` / action commands bound via `Office.actions.associate`, wired into the app ribbon).
  - Dialog requests (`Office.context.ui.displayDialogAsync` -> child webview/modal with `messageParent` / `messageChild`).
  - Shared-runtime declaration (routing `CustomFunctions` calls into the active task-pane webview when requested).
- **Proxy objects, `load(...)`, and `context.sync()`:**
  - `await Excel.run(async (context) => { ... })` binds a `RequestContext` to the active workbook incarnation (never silently rebinding if the user switches workbooks during an `await`).
  - Calling `context.workbook.worksheets.getItem("Sheet1").getRange("A1:B2")` performs **zero** host calls, building a declarative `ObjectPath` proxy (`Root(Workbook) -> Property("Worksheets") -> Method("Item", ["Sheet1"]) -> Method("Range", ["A1:B2"])`).
  - `proxy.load("values, formulas, address")` appends a `Load` op to `context._queue`. Reading an unloaded property throws `OfficeExtension.Error` (`code: "PropertyNotLoaded"`).
  - Property puts and method calls append `SetProperty` / `InvokeMethod` ops in exact statement order.
  - `await context.sync()` sends `context._queue` as **one** JSON batch message to the host, hydrates every `Load` snapshot at its exact ordinal position in the batch (so a `load` before a write + `calculate` + second `load` preserves both snapshots), and resolves the Promise.
  - `Excel.run` performs one final `sync()` if queued ops remain when the user callback resolves; if the callback rejects, only unsubmitted queued ops are discarded.
  - Null-object methods (`getItemOrNullObject`, `getUsedRangeOrNullObject`) populate `isNullObject = true` instead of failing the batch with `ItemNotFound`.
- **Representation translation (Office.js <-> `AutoValue`):**

| Concept | Office.js surface | `AutoValue` / `calc-host` core |
|---|---|---|
| Single-cell `range.values` / `formulas` | Always 2-D `[[v]]` (`1×1` nested JS array) | Scalar `AutoValue` for 1×1, `AutoArray` for multi-cell |
| Blank cell in `range.values` | Empty string `""` (distinguished from `""` text via `valueTypes` / `valuesAsJson`) | `AutoValue::Empty` vs `AutoValue::Text("")` |
| Worksheet error in `range.values` | Error spelling string (`"#N/A"`, `"#DIV/0!"`) or structured `CellValue` in `valuesAsJson` | `AutoValue::Error(2042)` / `CalcValue::error` |
| `rowIndex` / `columnIndex` | 0-based integers (`A1` is `(0, 0)`) | 1-based `Range.Row` / `Range.Column` (`A1` is `(1, 1)`) |
| `range.address` | Sheet-qualified A1 without `$` (`"Sheet1!A1:B2"`, quoted if sheet has spaces) | `geometry::address` (`row_absolute: false, column_absolute: false` + sheet qualifier) |
| `range.values` dates/numbers | Serial IEEE-754 `number` (identical to `Value2`, not JS `Date`) | `AutoValue::R8(f64)` |
| Member names | camelCase (`worksheets`, `getRange`, `clear`, `calculate`) | Descriptor PascalCase (`Worksheets`, `Range`, `ClearContents`, `Calculate`) via `OFFICE_JS_MAP.md` (`dc-17spk.4`) |

- **No `DnaExcel` dialect (`harvest/14-harvest.md`):** Batch ordering, partial-failure hydration, `trackedObjects` reuse across runs, concurrent `sync()` rejection, and async `CustomFunctions` eligibility follow Excel's measured Office.js behaviour from experiments J1–J5 (`dc-1nbp.30`).

### 5.2 Batch wire protocol over `dna-calc-protocol` and `dna-calc-worker`

Office.js Face 1 reuses the existing worker transport (`apps/dna-calc-protocol/src/lib.rs`, `apps/dna-calc-worker/src/server.rs`, `apps/dna-calc-worker/src/worker.rs`, `apps/dna-calc/web/desktop-bridge.js`) rather than introducing a second RPC stack:

- Add `Request::AutomationBatch(AutomationBatchRequest)` and `Request::AutomationRelease(AutomationReleaseRequest)` to `apps/dna-calc-model/src/projection.rs` (re-exported by `dna-calc-protocol`), returning `Reply::AutomationBatch(AutomationBatchReply)`.
- **Lossless internal wire values:** `AutomationBatch` encodes values in a tagged `WireAutoValue` format preserving `Empty`, `Null`, `Missing`, `Bool`, `I4`, `R8` (with exact IEEE-754 bits for `-0.0`), `Text` (UTF-16 code units if unpaired surrogates occur), `Error(CVErr)`, `Object(HandleId)`, and 2-D `Array`. Only the JS client adapter converts `WireAutoValue` to public Office.js `any[][]`, keeping capture/verdict records lossless (`14-object-model-and-js-answer.md` 3.10).
- **Batch execution on `Server` (`apps/dna-calc-worker/src/server.rs`):**
  1. `Server` owns one `Rc<WorkbookAutomation>` bound to its `Workbook`/`HostSession` (sharing `HostSession::workbook_events_switch()`, `ReferenceTracker`, and `WorkbookEventObserver` fan-out).
  2. Each `BatchOp` (`ResolvePath`, `Invoke`, `Load`, `Track`, `Untrack`, `SubscribeEvent`, `UnsubscribeEvent`) executes sequentially on the owner lane:
     - Check workbook incarnation and Guarded `reach` policy before invoking `WorkbookAutomation::invoke`.
     - Capture `Load` snapshots at their exact ordinal position in the batch.
     - On error, **stop at the first failing op**: do not run the suffix, do not roll back earlier committed ops, record the failing op ordinal, `OperationOutcome` (whether that op itself committed an authored edit), and mapped error.
  3. Flush `Server`'s normal `HostDelta` tile updates so the grid UI reflects every committed edit immediately.
- **No automatic replay on lost transport:** Each batch carries a unique `batch_id` (matching `Server::commands` `CommandJournal` in `server.rs:93`). Disconnection reports unknown outcome and never replays mutations automatically.

### 5.3 Identity and lifetime (Office.js Face 1)

- **Host handle table (`AutomationHandles` on `Server`):** Maps opaque `HandleId(u64)` -> `(context_id, ObjectRef, tracked: bool)`. Each `ObjectRef` owns one `ReferenceTracker` lease inside `WorkbookAutomation::ranges` (`crates/calc-host/src/automation.rs:104`).
- **Untracked handles:** Released automatically when `Excel.run` completes (`final_release: true` on the final `sync()` or explicit `Request::AutomationRelease`). Loaded JS properties remain plain JS values; further `sync()` calls on released handles return `InvalidObjectPath`.
- **Tracked handles (`context.trackedObjects.add(proxy)` / `remove(proxy)`):** `Track` / `Untrack` are ordered batch ops. Tracked handles keep their `ReferenceTracker` slots alive across `Excel.run` boundaries and structural edits (`ReferenceTracker::apply` in `crates/calc-fml/src/reference/tracked.rs:58-85`), until `Untrack` or workbook close (which tombstones the incarnation and releases all handles).
- **Wrapper equality:** Two `sheet.getRange("A1")` proxies are distinct JS objects (`a !== b`) with independent loaded property caches.

### 5.4 Threading, events, errors, and Guarded reach (Office.js Face 1)

- **Threading:** Never blocking (no `Atomics.wait`). In browser `wasm32`, `Worker.postMessage` talks to `dna-calc-worker` (`worker.rs:84-161`), which advances calculation slices cooperatively via `setTimeout(0)`. On desktop (Windows WebView2, Linux WebKitGTK / `dnacalc serve`), the add-in webview sends the exact same JSON batches to the native `Server` over the platform bridge / authenticated loopback WebSocket. Between two `sync()` calls, `HostSession` is not locked; each `sync()` re-validates incarnation, handle liveness, and Guarded policy.
- **Events:** `worksheet.onChanged`, `worksheet.onCalculated`, `workbook.onSelectionChanged`, and `table.onChanged` attach an `AutomationEventObserver` to `HostSession`'s observer fan-out. Delivery is **always asynchronous** (`Event::AutomationEvent`), emitted after synchronous VBA and COM handlers finish. Synchronous `ByRef Cancel` does not exist in Office.js.
- **Errors:** Client-side errors (`PropertyNotLoaded`, concurrent `sync()` if J4 confirms refusal) fail locally as `OfficeExtension.Error`. Host-side errors reject `sync()` with `OfficeExtension.Error` (`code` mapped from `AutoError`: `NotModelled` -> `"ApiNotFound"` / `"GeneralException"` carrying owner bead + refusal metadata so test harnesses count refusals even under `try/catch`; `Excel` -> `RichApi.ErrorCodes`; `Disconnected` -> `"InvalidObjectPath"`; `PolicyDenied` -> `"AccessDenied"`) plus `debugInfo` (`errorLocation`, `statement`, `surroundingStatements`). Partial failure clears the remaining unexecuted queue.
- **Guarded reach (`CODE_EXECUTION_POLICY.md` section 1):** **Office.js Face 1 runs in `Guarded`**. Under `Off`, add-ins do not load. Under `Guarded`, every `BatchOp` checks its mapped descriptor member's `reach` from `descriptor/reach.json` via the shared `PolicyChecker` and logs to the audit sink, allowing pure workbook/range/table/chart/shape/calculation calls while blocking `fs.*`/`network`/`process`/`ffi`/`project.write` with `"AccessDenied"`; network `fetch` from the add-in webview routes through host mediation (refused in `Guarded` until an allow-list exists). Under `Unguarded`, allowed bridges run with audit logging.

### 5.5 Interaction 2: Embedded JS custom functions behind a swappable engine (`crates/calc-js-host`, `dc-17spk.6`)

Cell-callable Excel JavaScript custom functions (`functions.json` metadata + JS implementation) must evaluate next to the calc engine — including in desktop/CLI/headless runs where no task-pane webview is open, and in Formula Explorer (OneCalc).

- **New crate placement & dependency ceiling:** `crates/calc-js-host` depends only on `calc-value`, `calc-func`, and `calc-fml` (plus optional JS engine crates behind Cargo features). It **never** depends on `calc-host`, `calc-engine`, `calc-doc`, or `calc-platform-desktop`, so both DNA Calc (`AddinHost` / `Server`) and Formula Explorer (`OneCalc`) can load JS custom functions without breaking `tools/xtask/architecture.json:266-288`.
- **Compile-time swappable `JsEngine` interface:**

```rust
// Conceptual seam in crates/calc-js-host/src/engine.rs (no code written in dc-17spk.1)
pub trait JsEngine {
    fn load_module(&mut self, id: &str, source: &str) -> Result<(), JsHostError>;
    fn register_functions(&mut self, metadata: &FunctionsMetadata) -> Result<Vec<JsFunctionSpec>, JsHostError>;
    fn invoke_sync(&mut self, id: JsFuncId, args: &[CalcValue], ctx: &JsInvocationCtx, limits: ExecLimits) -> Result<CalcValue, JsHostError>;
    fn invoke_async(&mut self, id: JsFuncId, args: &[CalcValue], ctx: &JsInvocationCtx, token: AsyncToken, limits: ExecLimits) -> Result<JsAsyncStart, JsHostError>;
    fn start_stream(&mut self, id: JsFuncId, args: &[CalcValue], ctx: &JsInvocationCtx, token: StreamToken) -> Result<(), JsHostError>;
    fn cancel(&mut self, token: InvocationToken);
    fn drain_jobs(&mut self, budget: ExecLimits) -> Result<Vec<JsCompletion>, JsHostError>;
    fn reset(&mut self);
}
```

- **Compile-time Cargo feature backends:**
  - `engine-quickjs` (`rquickjs`): small footprint, fast startup, ES2023, native Linux & Windows — **first native backend to implement** (check licence/vendoring first).
  - `engine-boa` (`boa_engine`): pure Rust, zero C/C++ build dependencies — candidate portable backend.
  - `engine-v8` (`rusty_v8` / `deno_core`): highest JS throughput, heaviest binary/build — optional high-speed native backend.
  - `engine-browser` (`wasm32-unknown-unknown`): delegates to the **browser's own JS engine** inside a dedicated Web Worker (`postMessage`), adding zero JS interpreter bytes to the wasm bundle.
- **One shared conformance suite across every engine:** Every feature backend passes the exact same test suite covering:
  - `functions.json` parsing (namespace, ID, parameter names/types/dimensionality/optional/repeating, `volatile`, `stream`, `cancelable`, `requiresAddress`, `requiresParameterAddresses`).
  - Value marshalling (`f64` including `-0.0`, `NaN`/`Infinity` -> `#NUM!`, booleans, UTF-16/UTF-8 strings, 2-D arrays, blank cells, `CustomFunctions.Error` -> Excel `#VALUE!`/`#NA`/`#NUM!`/`#NAME?`/`#DIV/0!`/`#REF!`/`#CALC!`).
  - Synchronous return, `Promise` async completion mapped to `UserFunctionStep::Awaiting { request }` (calc-engine P07 async continuation with `(workbook_incarnation, invocation_id, eligibility_token)` discarding stale completions after input edit or workbook close), and `CustomFunctions.StreamingInvocation` (`setResult` feeding admitted external updates like RTD, plus `onCanceled` firing when a formula is edited/deleted or workbook closes).
  - Hard execution limits: wall/instruction timeout and heap memory ceiling cleanly aborting runaway JS and returning `#VALUE!` + diagnostic without leaking or poisoning subsequent calls.
  - `CallableTable` integration: functions appear in `CallableTable` alongside XLL, VBA, and Automation UDFs, expose `CallableDescriptor` for completion/signature help, and obey `Off` / `Guarded` / `Unguarded` (in `Guarded`, pure JS runs inside CPU/memory limits with no filesystem/process access and no unmediated network `fetch`; nested `Excel.run` mutations inside a custom function are refused unless J5 measures an exception).
- **Shared-runtime interop with Face 1 (`dc-17spk.5`):** When an add-in manifest declares a shared runtime, `calc-js-host`'s `UserFunction` adapter routes invocations over Face 1's webview bridge rather than a separate isolate, while still registering into the exact same `CallableTable` and sharing the exact same P07 eligibility tokens.

---

## 6. Platforms and WebAssembly scope

### 6.1 Platform matrix

| Platform / target | Face 1: COM object model (`dc-17spk.2/.7/.8`) | Face 2: C API commands (`xlc*` / `xlSet`) | Face 3: Office.js (`dc-17spk.5/.6`) | Formula Explorer (OneCalc) |
|---|---|---|---|---|
| **Windows x64 (in-process)** | **Full native COM** (`calc-com-host`: vtables, `IDispatch`, `CreateTypeLib2`, `IConnectionPoint`, `WM_GETOBJECT`), **COM add-ins** (`IDTExtensibility2`, `dc-17spk.7`), **Automation add-ins** (`dc-17spk.8`, P3), RTD | **Full native C API** (`MdCallBack12` + `XLCALL32.DLL`: `Excel12`/`Excel12v`, `Excel4`/`Excel4v` planned, unmodified x64 XLLs + in-process callers) | **Face 1 (`dc-17spk.5`):** add-in page in WebView2 -> JSON batches to native `Server`; **Face 2 (`dc-17spk.6`):** `calc-js-host` (QuickJS/Boa/V8) in `CallableTable` | `ValuesOnly` profile (`1004` on workbook/range); XLL, VBA, and pure JS (`calc-js-host`) / Automation UDFs via `CallableTable` |
| **Windows x64 (out-of-process)** | **ROT + `LocalServer32`** (`dc-17spk.2`): `LocalAutomation` (`DnaCalc.Application`, workbook file monikers, canonical `Application`, `IID_IDispatch` `AppEvents`) landed in `4094ef5c2`; Windows acceptance, dual marshalling & `IMessageFilter` open | Not applicable (Excel's C API is strictly in-process) | Add-in webview is inherently out of process from calc core (WebView2) | No out-of-process workbook COM server |
| **Windows x86 (32-bit)** | Out of scope (`compile_error!` in `calc-com-host` / `calc-xll-host`; DNA Calc ships x64) | Out of scope (`X7` deferred) | No x86 desktop exe | Out of scope |
| **Linux x86_64 (in-process)** | **Portable Rust `ObjectModel`** (today) + **in-process SysV binary COM ABI** (planned); **COM add-ins & Automation add-ins stay Windows-only typed refusals** | **In-process C API command dispatcher** (`Excel12`/`Excel12v`/`Excel4`/`Excel4v` SysV `extern "C"` over `WorkbookAutomation`, planned); Windows `.xll` DLL loading stays refused (`LINUX_DESKTOP_SHELL.md`) | **Face 1 (`dc-17spk.5`):** WebKitGTK panel or browser via `dnacalc serve` WebSocket; **Face 2 (`dc-17spk.6`):** `calc-js-host` (QuickJS/Boa/V8) in `CallableTable` | `ValuesOnly`, portable Rust/VBA + `calc-js-host` UDFs |
| **macOS (later; deferred)** | Same architecture as Linux (portable `ObjectModel` + SysV/AAPCS64 in-process ABI; no Win32 COM/ROT/COM add-ins); compilation and testing deferred | Same architecture as Linux (in-process C API dispatcher; no Windows `.xll` DLLs); compilation and testing deferred | `dnacalc serve` / future `WKWebView` + `calc-js-host`; compilation and testing deferred | `ValuesOnly`; deferred |
| **Browser (`wasm32-unknown-unknown`)** | **No binary COM ABI** (`calc-com-host` refuses wasm32). `calc-object-model` + `WorkbookAutomation` + CalcVBA compile and run natively in wasm32 | **No native `XLOPER` ABI** (`calc-xll-*` refuse wasm32). XLM text (`ExecuteExcel4Macro`) only if routed portably | **Natural browser face:** Face 1 (`dc-17spk.5`) iframe/Worker -> `postMessage` to wasm `dna-calc-worker`; Face 2 (`dc-17spk.6`) `engine-browser` Worker using browser's own JS engine | Browser OneCalc stays strictly `ValuesOnly` (`dna-onecalc` has no `calc-host`/`calc-engine`) |

### 6.2 Why Office.js is the natural browser (wasm32) face

In the browser (`wasm32-unknown-unknown`), there is no Win32 STA, no `WM_GETOBJECT`, no ROT, no `LoadLibrary`, and no native `XLOPER12` pointer ABI (`calc-com-host`, `calc-xll-abi`, `calc-xll-host`, `calc-xll-shim`, and `calc-addin-host` all enforce `#[cfg(target_arch = "wasm32")] compile_error!`). By contrast, the entire calculation, UDF-registry, and object-model stack below the faces is **already `portable` and tested in headless Chrome and Firefox**:

- `crates/calc-object-model` (`portability = "portable"`)
- `crates/calc-fml` (`CallableTable`) and `crates/calc-func` (`UserFunction`) (`portability = "portable"`)
- `crates/calc-host` including `WorkbookAutomation` (`portability = "portable"`)
- `crates/calc-vba` and `crates/calc-host-vba` (`portability = "portable"`)
- `apps/dna-calc-model`, `apps/dna-calc-protocol`, and `apps/dna-calc-worker` (`portability = "portable"`)

Because the browser UI already talks to `dna-calc-worker` asynchronously over `Worker.postMessage` (`apps/dna-calc-worker/src/worker.rs:84-161`), and both desktop shells (`dna-calc-desktop` via `chrome.webview` and `dnacalc serve` via loopback WebSocket) tunnel that exact same `Worker` protocol through `apps/dna-calc/web/desktop-bridge.js`, building Office.js Face 1 over `Request::AutomationBatch` on `dna-calc-protocol` gives DNA Calc **one identical Office Add-ins protocol across browser wasm, Windows WebView2, and Linux WebKitGTK/`dnacalc serve`**, while Office.js Face 2 (`calc-js-host`) uses the browser's own JS engine via a Worker on `wasm32` and a compile-time swappable engine natively.

---

## 7. What exists today (measured 2026-11-23) and the gaps

### 7.1 Object-model descriptor and profile coverage

Measured on Linux (`dna-firehorse`, commit `591fe3e1c`; unchanged in `4094ef5c2`) by running:
- `cargo test -p calc-object-model values_only_descriptor_coverage_lists_every_registered_member -- --nocapture`
- `cargo test -p calc-host --test calc-host-integration workbook_descriptor_coverage_lists_own_and_inherited_members -- --nocapture`

Descriptor inventory (`crates/calc-object-model/descriptor/*.json`, generated by `tools/com-descriptor`):
- `excel.json`: Excel 1.9 (`EXCEL.EXE 16.0.20527.20150`), 40 types (8 dual, 28 dispinterface, 4 coclass), 2,916 raw members.
- `office.json`: Office 2.8 (`MSO.DLL 16.0.20527.20112`), 21 types (19 dual, 2 coclass), 364 raw members.
- `addin-designer.json`: AddInDesignerObjects 1.0 (`MSADDNDR.OLB 16.0.20527.20112`), 1 dual interface, 5 raw members.
- `accessibility.json`: Accessibility 1.1 (`oleacc.dll 7.2.26100.9549`), 1 dual interface, 21 raw members.
- Total across all 57 complete advertised layouts (with base-interface members included): **3,545** `(interface, DISPID, invoke_kind)` members.

Registered handler counts (including conditional refusals and values-only workbook refusals; workbook counts include values-only fallback with overrides counted once):

| Interface group | Descriptor members | `ValuesOnly` handlers | `WorkbookAutomation` handlers | Change vs. `OBJECT_MODEL.md` §10 (2026-11-14) |
|---|---:|---:|---:|---|
| `_Application` | 467 | 18 | 21 | unchanged |
| `Workbooks` | 19 | 5 | 5 | unchanged |
| `_Workbook` | 282 | 6 | 9 | unchanged |
| `_Worksheet` | 151 | 3 | 10 | unchanged |
| `Sheets` | 22 | 0 | 5 | unchanged |
| `Range` | 253 | 9 | **26** | +3 (`Formula` Get/Put, `Text` Get; `formula.rs`) |
| `Areas` | 7 | 0 | 5 | unchanged |
| `Names` / `Name` | 8 / 36 | 0 / 0 | 5 / 5 | unchanged |
| `Window` | 107 | 3 | 3 | unchanged |
| `WorksheetFunction` | 420 | 420 | 420 | unchanged (402/417 method names resolve in catalogue) |
| `ListObjects` / `ListObject` / `ListColumns` / `ListColumn` / `ListRows` | 9 / 59 / 8 / 16 / 9 | 0 | 4 / **15** / 3 / 2 / 2 | +4 on `ListObject` (`Delete`, `Unlist`, `TableStyle` Get/Put) |
| `ChartObjects` / `ChartObject` / `_Chart` / `ChartTitle` | 54 / 49 / 208 / 48 | 0 | 4 / 11 / **11** / 2 | +2 on `_Chart` (`PlotBy` Get/Put) |
| `SeriesCollection` / `Series` | 11 / 113 | 0 | 3 / **7** | +4 on `Series` (`Values` Get/Put, `XValues` Get/Put) |
| `Shapes` / `Shape` / `TextFrame2` / `TextRange2` | 29 / 109 / 35 / 44 | 0 | 8 / 23 / 1 / 2 | unchanged |
| `FillFormat` / `LineFormat` / `ColorFormat` / `ConnectorFormat` | 48 / 33 / 14 / 15 | 0 | 1 / 3 / 2 / 8 | unchanged |
| Zero-handler interfaces (`Windows`, `AppEvents`, `WorkbookEvents`, `DocEvents`, `Axes`, `Axis`, `AxisTitle`, `Legend`, `ListRow`, `ShapeRange`, `IRtdServer`, `IRTDUpdateEvent`, `IAccessible`, Office/Add-in Designer) | 964 | 0 | 0 | unchanged |
| **All complete advertised layouts** | **3,545** | **464** | **626** | **+13** (was 613 on 2026-11-14) |

Outside those portable profile counts, `crates/calc-com-host/src/office.rs` registers **49** COM-by-nature handlers (2 on `_Application`: `COMAddIns`, `CommandBars`; 47 across `COMAddIns`, `COMAddIn`, `_CommandBars`, `CommandBar`, `CommandBarControls`, `CommandBarPopup`, `_CommandBarButton`). Note also that `apps/dna-calc-desktop/src/automation.rs:109-136` now wires `AutomationHost::with_session_mut` for both `SessionAccess::Server` and `SessionAccess::Session`, closing the command-write desktop gap noted in `OBJECT_MODEL.md` section 11.

### 7.2 Face 1 (COM, COM add-ins, Automation add-ins) today and gaps

- **Exists today:**
  - Complete Win64 x64 Rust COM projection (`crates/calc-com-host`), zero C++: `build.rs`/`generate.rs` vtables, `IDispatch` (`dispatch.rs`, `arguments.rs`), in-memory `CreateTypeLib2` (`typeinfo.rs`), `ISupportErrorInfo`, `IConnectionPointContainer` (`events.rs`, `AppEvents` `SheetChange`/`SheetCalculate`/`AfterCalculate` plus `IID_IDispatch` sink acceptance), `WM_GETOBJECT`/`OBJID_NATIVEOM` (`platform/calc-platform-desktop/src/shell.rs`), `RtdClient` (`rtd.rs`, `rtd_native.rs`).
  - Canonical single-STA `Application` identity (`4094ef5c2`, `projection.rs:394`, `runtime.rs:108-118`).
  - Desktop out-of-process COM foundation (`4094ef5c2`, `crates/calc-com-host/src/automation.rs`, `apps/dna-calc-desktop/src/{main,app}.rs`): `APPLICATION_CLSID = {B363DB86-B412-4C60-91F2-9D538F75BB65}` (`DnaCalc.Application`), `LocalAutomation::publish` (ROT `ACTIVEOBJECT_WEAK` + `CoRegisterClassObject` `CLSCTX_LOCAL_SERVER`), `LocalAutomation::set_workbook` (file monikers for local-drive paths), `--automation` / `-Embedding` CLI triggers, and queued Windows verification (`docs/plan/windows-queue.md#dc-17spk2-desktop-automation`).
  - XLL-coupled `IDTExtensibility2` / `IRibbonExtensibility` / `COMAddIns` / `CommandBars` (`interfaces.rs:12-29`, `office.rs:701-980`, 49 Office handlers, `AddinHost::ribbons()`).
  - Bound to DNA Calc's real `HostSession` via `apps/dna-calc-desktop/src/automation.rs` (both `--app` and `--engine`).
- **Gaps:**
  1. **Out-of-process COM completion (`dc-17spk.2`, `dc-6332.16`):** native Windows warm/cold verification run; no-Office dual-interface vtable marshalling without shadowing Excel's `Interface\{IID}` keys; `IMessageFilter` (`RPC_E_CALL_REJECTED` during calculation); `CoDisconnectObject` on shutdown; `--embedding` hidden lock-count lifetime (`LockServer`); optional `--register-com`/`--unregister-com` helper; second-STA comparison against Excel (`dc-6332.16`).
  2. **Standalone COM add-ins (`dc-17spk.7`):** `office.rs:751, 795, 842, 861` currently reads `HKCU` only, ignores `LoadBehavior`/`FriendlyName`, rejects non-XLL DLLs via `contains_loaded_module`, lacks `LocalServer32` EXE add-ins, never passes `ext_cm_Startup` or calls `OnStartupComplete`, does not yet connect `GetCustomUI` ribbon XML callbacks to DNA Calc's UI, and has no Linux typed-refusal listing.
  3. **Automation add-ins (`dc-17spk.8`, lower priority):** zero implementation today (no `Programmable` / `OPENn /A "ProgID"` loader, no `ITypeInfo` -> `CallableTable` registration, no Excel measurement baseline).
  4. **Incomplete COM events:** `WorkbookEvents` and `DocEvents` are not raised by `AutomationObserver`; `State::raise` (`events.rs:147-211`) lacks `VT_BYREF | VT_BOOL` copyback for `BeforeSave(Cancel)`.
  5. **No Linux in-process binary COM ABI:** all runtime modules in `crates/calc-com-host/src/lib.rs:83-112` are `#[cfg(windows)]`; `generate.rs` only handles Win64 register classes.
  6. **CalcVBA host binding still open (`dc-piw0.5`):** `crates/calc-vba/src/runtime/spreadsheet.rs` and `crates/calc-vba/src/events/source.rs` are not yet retired in favour of `calc-object-model` + `WorkbookAutomation`.
  7. **Descriptor `reach` absent (`dc-8ovd`):** no `reach.json` overlay in `crates/calc-object-model/descriptor/`, no `AutoError::PolicyDenied` in `model.rs`, no Guarded member-reach check in `WorkbookAutomation::invoke`.
  8. **Open workbook-member gaps (`OBJECT_MODEL.md` §11, `dc-1nbp.6`, `dc-piw0.11`):** `Range.Formula2`, `Application.Evaluate`, `Application.Selection`/`ActiveCell`, `Range.Select`/`Insert`/`Delete`, `Workbooks.Open`/`Add`, `Workbook.Save`/`SaveAs`/`Close`, sheet-local `Names`, `SAFEARRAY` ranks/element types beyond 2-D `VT_VARIANT`, and multi-workbook registry.

### 7.3 Face 2 (C API commands) today and gaps

- **Exists today:**
  - `crates/calc-xll-abi`: `XLOPER12`/`FP12`/`XLREF12`/`XLMREF12`/`XLSREF12` layouts (builds on Linux and Windows, refuses wasm32), `XllSignature` parser (parses both legacy `P`/`R`/`C`/`D`/`F`/`G`/`K`/`O` and Excel 12 codes), `Arena`, `SheetMap`.
  - `crates/calc-xll-host` (`#[cfg(windows)]` only):
    - Worksheet functions (`xlf*`): **478** Ftab entries via `calc_catalogue::binary_function` (`crates/calc-catalogue/src/file_function_ids.rs`) + `614` (`FILTER`, `invoke.rs:687-691`) + 12 explicit callbacks (`xlfRegister`, `xlfCall`, `xlfRegisterId`, `xlfUnregister`, `xlfSetName` delete-only, `xlfCaller`, `xlfEvaluate` registered names/constants, `xlfReftext`, `xlfGetWorkspace` items 2 & 37, `xlfVolatile`, `xlfRtd`, `xlUDF`).
    - `XL_SPECIAL`: **13** callbacks served (`xlFree`, `xlStack`, `xlCoerce`, `xlSheetId`, `xlSheetNm`, `xlAbort`, `xlGetInst` measured denial 32, `xlGetHwnd`, `xlGetName`, `xlAsyncReturn`, `xlEventRegister`, `xlRunningOnCluster`, `xlGetInstPtr`).
    - **`XL_COMMAND` (`xlc*`) and `xlSet`:** **1** `xlc*` command handled (`XLC_MESSAGE = 32890`, `callbacks.rs:754-769`, setting `State::status_message` only). `XL_SET` (`16387`) is declared in `crates/calc-xll-abi/src/lib.rs:63` but **not handled in `callbacks.rs`**. Every other `xlc*` command is refused (`callbacks.rs:707-714, 1047-1053`) or hits `measured_denial` from worksheet functions.
  - `platform/calc-xll-shim`: `Excel12`/`Excel12v`/`XLCallVer`/`LPenHelper` work on Windows; **`Excel4` and `Excel4v` are stubs returning `32` (`xlretFailed`)** (`platform/calc-xll-shim/src/variadic.c:17-22`).
- **Gaps:**
  1. **No `xlc*` inventory** (`dc-17spk.3`): `xlcall.h` commands are not yet enumerated and grouped by object-model owner.
  2. **Zero `ObjectModel` wiring in `calc-xll-host`:** `XllHost` holds no `Rc<dyn ObjectModel>`, so no `xlc*` command or `xlSet` can dispatch through `WorkbookAutomation`.
  3. **Zero `Excel4`/`Excel4v` / legacy `XLOPER` support:** no `XlOper`/`XlRef`/`XlMRef`/`XlSRef`/`Fp` structs in `calc-xll-abi`, no ANSI narrowing/widening converter, no `Excel4`/`Excel4v` dispatcher.
  4. **No Linux in-process C API command dispatcher:** `crates/calc-xll-host/src/lib.rs:82-89` compiles only a failing stub on non-Windows because Win32 DLL loading and callback dispatch are coupled.
  5. **`XL_INTL` / `XL_PROMPT` handling:** flags are preserved in `calc-xll-abi`, not yet decoded for command dispatch.
  6. **No `Application.ExecuteExcel4Macro`:** XLM text macros cannot invoke the same command table yet.
  7. **`xlEventRegister` firing:** registered callbacks in `State::events` (`callbacks.rs:864`) are not yet fired by `HostSession` on interactive calculation end/cancel.

### 7.4 Face 3 (Office.js — Add-in runtime & JS custom functions) today and gaps

- **Exists today:**
  - **Zero Office.js runtime code and zero `calc-js-host` code** in `crates/`, `apps/`, or `platform/` (verified by `git grep`: 0 hits for `Office.js`, `Excel.run`, `RequestContext`, `CustomFunctions`, `calc-js-host`).
  - Portable core, UDF registry, and transport foundations exist: `calc-object-model` (`464` values-only handlers), `calc-host` (`WorkbookAutomation`, `626` handlers), `calc-fml::CallableTable` + `calc-func::UserFunction` (`UserFunctionStep::Awaiting` + `read_topic`), `dna-calc-protocol`, `dna-calc-worker` (`apps/dna-calc-worker/src/worker.rs`), `apps/dna-calc/web/desktop-bridge.js`, `apps/dna-calc-desktop/src/app.rs`, and `apps/dnacalc/src/serve.rs` all compile and test across Linux, Windows, and `wasm32`.
- **Gaps:**
  1. **No Office.js API inventory (`dc-17spk.4`):** Excel JS API objects, members, events, and requirement sets are not yet mapped to `calc-object-model` descriptor members.
  2. **No Excel J1–J5 probes (`dc-1nbp.30`):** requires an owner-approved sideload route that does not modify HKCU registry or Trust Center settings.
  3. **No `AutomationBatch` protocol or handle/tracked-lease table** in `dna-calc-model`/`dna-calc-protocol`/`dna-calc-worker`.
  4. **No Office Add-ins runtime (`dc-17spk.5`):** no clean-room Office.js-compatible JS client library, no XML/unified-JSON manifest loader, no WebView2/WebKitGTK/iframe task-pane + ribbon-command + dialog host, no async Office.js event bridge, no mediated `fetch` or shared-runtime hook.
  5. **No embedded JS custom-functions host (`dc-17spk.6`):** no `crates/calc-js-host`, no `JsEngine` trait, no QuickJS/Boa/V8/browser-Worker backends, no `functions.json` loader into `CallableTable`, no shared engine conformance suite.

---

## 8. Follow-up epics and beads (in dependency order)

### 8.1 Existing open beads in `.beads/issues.jsonl`

All existing prerequisites and children of `dc-17spk` are reused directly rather than duplicated:

| Existing bead | Status | Role in this programme |
|---|---|---|
| `dc-piw0.5` | `in_progress` (P1) | Blocks `dc-17spk`: DNA Calc supplies `Excel`/`Office` references from `calc-object-model` + `calc-host` to host-neutral CalcVBA, retiring `calc-vba/src/runtime/spreadsheet.rs`. |
| `dc-6332.16` | `in_progress` (P3) | Second-STA Excel comparison and no-Office Windows COM acceptance (single-STA canonical `Application` landed in `4094ef5c2`). |
| `dc-8ovd` | `open` (P1) | Cross-runtime Code Execution Policy (`CODE_EXECUTION_POLICY.md`): Guarded origins, capability vocabulary, and member reach. |
| `dc-1nbp.6` | `open` (P2) | Excel object-model probes E1–E11 (`Range.Value` conversions, `Formula2`, `Evaluate`, multi-area puts). |
| `dc-1nbp.30` | `in_progress` (P3) | Office.js review: owner-approved J1–J5 sideload route, manifest/task-pane/shared-runtime decisions. |
| `dc-17spk.2` | `open` (P2) | Child of `dc-17spk`: Out-of-process COM on Windows (`4094ef5c2` landed ROT/class-factory/canonical `Application`/`IID_IDispatch` `AppEvents`; Windows acceptance, dual marshalling, `IMessageFilter` remain). |
| `dc-17spk.3` | `open` (P2) | Child of `dc-17spk`: C API command inventory (`docs/architecture/C_API_COMMANDS.md`). |
| `dc-17spk.4` | `open` (P2) | Child of `dc-17spk`: Office.js inventory (`docs/architecture/OFFICE_JS_MAP.md`). |
| `dc-17spk.5` | `open` (P2) | Child of `dc-17spk`: Office.js face 1 — Office Add-ins runtime (task pane, ribbon commands, events, dialogs, manifests, clean-room JS library, shared-runtime option). |
| `dc-17spk.6` | `open` (P2) | Child of `dc-17spk`: Office.js face 2 — `calc-js-host` custom functions in compile-time swappable JS engine (QuickJS/Boa/V8/browser Worker) registered in `CallableTable`. |
| `dc-17spk.7` | `open` (P1) | Child of `dc-17spk`: COM add-ins (`IDTExtensibility2` + `IRibbonExtensibility` on Windows; Guarded refusal; Linux typed refusal). |
| `dc-17spk.8` | `open` (P3) | Child of `dc-17spk`: Automation add-ins (`Programmable` COM UDFs in `CallableTable` on Windows; lower priority; measure Excel first). |

### 8.2 Dependency order

```mermaid
flowchart TD
    P_VBA["dc-piw0.5 (existing): DNA Calc supplies Excel/Office to CalcVBA"]
    P_ID["dc-6332.16 (existing): Second-STA + no-Office COM acceptance"]
    P_POL["dc-8ovd (existing): Code execution policy"]
    P_OJS["dc-1nbp.30 (existing): Office.js review + J1-J5 route"]

    A1["AF-1: Descriptor reach.json + PolicyDenied + Guarded invoke"]
    A2["AF-2: HostSession observer fan-out + WorkbookEvents/DocEvents/BeforeSave Cancel"]
    A3["AF-3: Core workbook members (Selection, Formula2, Insert/Delete, Names.Add, Save/Open)"]
    A4["AF-4: Unified CallableTable provider contract (XLL + VBA + JS + Automation)"]

    B1["dc-17spk.2 (existing): Finish Windows OOP COM (acceptance, dual marshal, IMessageFilter)"]
    B2["dc-17spk.7 (existing): Windows COM add-ins (IDTExtensibility2 + IRibbonExtensibility)"]
    B3["dc-17spk.8 (existing, P3): Windows Automation add-ins (Programmable COM UDFs)"]
    B4["AF-5: Linux in-process binary COM ABI (SysV x86_64 vtables + IDispatch)"]

    C0["dc-17spk.3 (existing): C API xlc* inventory (C_API_COMMANDS.md)"]
    C1["AF-6: Excel C API command probes (C1-C4: xlSet, xlcFormula, Excel4, flags)"]
    C2["AF-7: Split portable C API command dispatcher from Win32 XLL loader"]
    C3["AF-8: Implement xlSet + Tier-1 xlc* commands over WorkbookAutomation"]
    C4["AF-9: Legacy Excel4/Excel4v (XLOPER) translation into shared dispatcher"]
    C5["AF-10: Application.ExecuteExcel4Macro over the C API command table"]

    D0["dc-17spk.4 (existing): Office.js inventory (OFFICE_JS_MAP.md)"]
    D1["AF-11: Excel J1-J5 Office.js probes (owner-approved sideload)"]
    D2["AF-12: AutomationBatch protocol + handle/tracked-lease table in worker"]
    D3["dc-17spk.5 (existing): Office.js face 1 (clean-room library, manifests, webview runtime)"]
    D4["dc-17spk.6 (existing): Office.js face 2 (calc-js-host swappable JS engine + conformance)"]

    E1["AF-13: Cross-face & cross-UDF parity suite (COM, C API, Office.js, VBA, 4 UDF providers)"]
    E2["AF-14: macOS in-process build & testing (deferred)"]

    P_POL --> A1
    A1 --> A3
    A1 --> A4

    A1 --> B1
    A2 --> B1
    P_ID --> B1
    A1 --> B2
    A2 --> B2
    A4 --> B3
    B2 --> B3
    A1 --> B4
    A2 --> B4

    C0 --> C1
    C0 --> C2
    A1 --> C2
    A3 --> C3
    C1 --> C3
    C2 --> C3
    C1 --> C4
    C2 --> C4
    C3 --> C5
    A1 --> C5

    D0 --> D2
    P_OJS --> D1
    A1 --> D2
    A2 --> D2
    A3 --> D2
    A1 --> D3
    D1 --> D3
    D2 --> D3
    A4 --> D4
    D1 --> D4

    P_VBA --> E1
    B1 --> E1
    B2 --> E1
    B4 --> E1
    C3 --> E1
    D3 --> E1
    D4 --> E1
    B4 --> E2
    C2 --> E2
    D3 --> E2
    D4 --> E2
```

### 8.3 Bead plan and acceptance tests (`AF-1`..`AF-14` as `dc-17spk.10`..`dc-17spk.23` + existing `dc-17spk.2`..`.8`)

#### Wave 0 — Shared core foundations (prerequisite to all faces and UDF providers)

- **AF-1 (`dc-17spk.10`) — Descriptor `reach.json`, `AutoError::PolicyDenied`, and one Guarded check in `ObjectModel::invoke`** (Priority: P1; depends on: `dc-8ovd`)
  - **Scope:** Add `crates/calc-object-model/descriptor/reach.json` (keyed by `Interface.Member (InvokeKind)` plus `"xlc"` section), load into `Member::reach` in `crates/calc-object-model/src/descriptor.rs`, add `AutoError::PolicyDenied` in `model.rs`, and wire one `PolicyChecker` + audit call inside `WorkbookAutomation::invoke` (`crates/calc-host/src/automation.rs`) and `ValuesOnly::invoke`. Keep `tools/com-descriptor` regeneration lossless.
  - **Platform:** Portable (Linux, Windows, wasm32).
  - **Acceptance tests:**
    1. `descriptor_reach_overlay_resolves_every_entry` (`crates/calc-object-model/tests/descriptor.rs`, portable + wasm): every key in `reach.json` exists in `Descriptor::get()`, and `Workbooks.Open`, `Workbook.SaveAs`, `Application.ExecuteExcel4Macro`, `Application.RegisterXLL`, `Application.Run`, `Application.VBE` carry their `CODE_EXECUTION_POLICY.md` §2.2 capabilities.
    2. `guarded_policy_refuses_and_audits_reaching_members_once` (`crates/calc-host/tests/automation.rs`, portable + wasm): calling a reaching member under `Guarded` returns `AutoError::PolicyDenied` and logs one audit record; calling pure `Range.Value2` / `Calculate` succeeds with zero denials; calling under `Unguarded` succeeds and logs one allowed audit record.

- **AF-2 (`dc-17spk.11`) — `HostSession` observer fan-out, `WorkbookEvents`/`DocEvents`, and synchronous `ByRef Cancel`** (Priority: P1; depends on: none)
  - **Scope:** Replace single `observer` slot in `crates/calc-host/src/events.rs:93` with ordered multi-observer registration (`add_workbook_event_observer` / `remove_workbook_event_observer`), add `workbook_open`, `before_save`, and `selection_changed` callbacks to `WorkbookEventObserver`, wire `WorkbookEvents` and `DocEvents` in `AutomationObserver` (`crates/calc-host/src/automation.rs:939-1013`), and add `VT_BYREF | VT_BOOL` copyback in `crates/calc-com-host/src/events.rs:147-211`.
  - **Platform:** Portable core (Linux/Windows/wasm32) + Windows COM event test.
  - **Acceptance tests:**
    1. `multiple_observers_receive_ordered_events_under_one_enable_events_switch` (`crates/calc-host/tests/automation.rs`, portable + wasm): two registered observers (standing in for COM and Office.js) receive `WorksheetChange`, `WorksheetCalculate`, `AfterCalculate`, and `SelectionChanged` in registration order after the VBA handler, and both suppress when `EnableEvents = false`.
    2. `com_before_save_sink_cancels_save_via_byref_bool` (`crates/calc-com-host/src/tests.rs`, Windows): a COM `WorkbookEvents` sink setting `*Cancel = VARIANT_TRUE` cancels `HostSession` save before file replacement, matching E5.

- **AF-3 (`dc-17spk.12`) — Core workbook automation members required by all three faces** (Priority: P2; depends on: `AF-1`, `dc-1nbp.6`)
  - **Scope:** Add missing shared Tier-1 workbook members once in `crates/calc-host/src/automation/` (never in a face crate): `Application.Selection`, `Application.ActiveCell`, `Application.Calculation` (Get/Put), `Application.ScreenUpdating` (Get/Put), `Range.Select`, `Range.Formula2` (Get/Put, per E7), `Range.FormulaArray` (Get/Put), `Range.NumberFormat` (Get/Put), `Range.Insert`, `Range.Delete`, `Worksheet.UsedRange`, `Worksheet.Activate`, `Sheets.Add`, `Worksheet.Delete`, `Names.Add`, `Name.Delete`, `Workbook.Save`/`SaveAs`/`Close`, `Workbooks.Add`/`Open`, and truthful `OperationOutcome` reporting in `mutation.rs`.
  - **Platform:** Portable (Linux, Windows, wasm32).
  - **Acceptance tests:**
    1. Portable round-trip tests in `crates/calc-host/tests/automation.rs` (running natively and in wasm32) verifying each added member against `HostSession`, including `ReferenceTracker` survival across `Range.Insert`/`Delete`, `Formula` vs `Formula2` spill/intersection (E7), and `OperationOutcome` preserving committed revision on post-commit recalc failure.
    2. `workbook_descriptor_coverage_lists_own_and_inherited_members` reports the exact new total over `3,545` with zero regressions on existing 626 handlers.

- **AF-4 (`dc-17spk.13`) — Unified `CallableTable` provider contract across XLL, VBA, JS, and Automation UDFs** (Priority: P2; depends on: `AF-1`)
  - **Scope:** Formalize provider-tagged registration, qualified vs unqualified lookup precedence (`ProgID.Method`, `NAMESPACE.FUNC`, `Module.Func`), `CallableDescriptor` origin/metadata propagation to formula completion and signature help, async/streaming cancellation tokens tied to `(workbook_incarnation, invocation_id, eligibility_token)`, and `Off` / `Guarded` / `Unguarded` provider filtering in `crates/calc-fml/src/callable.rs` and `crates/calc-addin-host/src/host.rs`.
  - **Platform:** Portable core (`calc-fml` / `calc-host`) + native `calc-addin-host`.
  - **Acceptance tests:**
    1. Portable tests in `crates/calc-fml` verifying all four provider kinds register into one `CallableTable`, obey built-in > workbook-name > provider precedence, publish `CallableDescriptor` signatures, invalidate handles on `replace()`, and filter native providers cleanly under `Guarded` and all providers under `Off`.

#### Wave 1 — COM face (Windows OOP COM, COM add-ins, Automation add-ins, Linux in-process COM)

- **Existing bead `dc-17spk.2` — Finish out-of-process COM on Windows (`GetObject`/ROT, `LocalServer32`, `IMessageFilter`, dual marshalling)** (Priority: P2; depends on: `dc-6332.16`, `AF-1`, `AF-2`)
  - **Scope:** Building on `4094ef5c2` (`crates/calc-com-host/src/automation.rs`, canonical `Application`, `IID_IDispatch` `AppEvents`, `--automation`/`-Embedding`), complete: (a) native Windows warm + cold verification per `docs/plan/windows-queue.md#dc-17spk2-desktop-automation`; (b) verified dual-interface vtable marshalling on no-Office machines without touching or shadowing Excel's `Interface\{IID}` keys; (c) STA `IMessageFilter` returning `SERVERCALL_REJECTED` (`RPC_E_CALL_REJECTED`) during calculation; (d) `CoDisconnectObject` on shutdown and `--embedding` hidden `LockServer` lifetime; (e) optional `--register-com`/`--unregister-com` HKCU helper.
  - **Platform:** Windows x64 (cross-built on Linux with `cargo-xwin`, executed on Windows).
  - **Acceptance tests:**
    1. Warm (`GetActiveObject("DnaCalc.Application")` + workbook file moniker) and cold (`CoCreateInstance` / `New-Object -ComObject DnaCalc.Application`) Win64 tests pass on a clean non-elevated Windows profile without Office, verifying identical controlling `IUnknown`, `Range` read/write, `AppEvents.SheetChange`, clean revocation on exit, and zero Excel registry diff.
    2. Separate-STA/process call during an active calculation returns `RPC_E_CALL_REJECTED` (`0x80010001`).
    3. Early-bound dual-interface cross-process call (`_Application::get_Version`, `_Application::get_Range`) succeeds on both Excel-present and no-Office Windows profiles.

- **Existing bead `dc-17spk.7` — COM add-ins (`IDTExtensibility2` + `IRibbonExtensibility` on Windows)** (Priority: P1; depends on: `AF-1`, `AF-2`)
  - **Scope:** Extend `crates/calc-com-host/src/office.rs` and `crates/calc-addin-host/src/host.rs`: discover `HKCU` and `HKLM` `Software\Microsoft\Office\Excel\Addins\<ProgID>` (`LoadBehavior`, `FriendlyName`, `Description`); support standalone `InprocServer32` DLLs and `LocalServer32` EXEs (lifting `contains_loaded_module` for policy-allowed standalone COM add-ins); drive full `OnConnection(ext_cm_Startup)` -> `OnAddInsUpdate` -> `OnStartupComplete` and `OnBeginShutdown` -> `OnDisconnection` lifecycle; wire `IRibbonExtensibility::GetCustomUI` XML and ribbon control callbacks (`onAction`, `getLabel`, `getEnabled`, `IRibbonControl`, `IRibbonUI::Invalidate`) into DNA Calc's desktop UI; keep `CommandBars` minimal (`ICustomTaskPaneConsumer` deferred). Refuse loading in `Guarded`; report Windows-only typed refusal on Linux.
  - **Platform:** Windows x64 (plus Linux typed-refusal test).
  - **Acceptance tests:**
    1. Standalone C++ or C# `IDTExtensibility2` + `IRibbonExtensibility` sample add-in and Excel-DNA COM add-in load on Windows into `dna-calc-desktop` (`Unguarded`), receive DNA Calc's COM `Application`, mutate a `Range`, supply custom ribbon XML whose button `onAction` callback mutates the workbook, and shut down cleanly through `OnBeginShutdown`/`OnDisconnection` with zero leaked `ReferenceTracker` slots.
    2. Under `Guarded` and `Off`, no COM add-in loads (verified by load count = 0 and audit record).
    3. On Linux, requesting COM add-in load returns a typed Windows-only refusal.

- **Existing bead `dc-17spk.8` — Automation add-ins (`Programmable` COM worksheet functions on Windows, lower priority)** (Priority: P3; depends on: `AF-4`, `dc-17spk.7`)
  - **Scope:** First measure Excel's behaviour on a small Windows sample Automation add-in (argument coercion, `Range` vs value args, missing/optional params, COM errors -> worksheet errors, volatility/recalc, shadowing). Then implement `Programmable` / `OPENn /A "ProgID"` discovery and `ITypeInfo` -> `CallableTable` registration in `crates/calc-com-host` + `crates/calc-addin-host` (`=ProgID.Method(...)` and unqualified `=Method(...)`). Never load in `Guarded`; typed refusal on Linux.
  - **Platform:** Windows x64 (plus Linux typed-refusal test).
  - **Acceptance tests:**
    1. Committed Excel measurement trace for the sample Automation add-in (argument coercion, errors, recalc).
    2. Windows test loads the sample Automation add-in in `dna-calc-desktop` (`Unguarded`), evaluates `=ProgID.Method(...)` and `=Method(...)` in cells through `CallableTable`, matches Excel's measured answers bit for bit, refuses loading in `Guarded`, and returns a typed Windows-only refusal on Linux.

- **AF-5 (`dc-17spk.14`) — Linux in-process binary COM ABI projection** (Priority: P2; depends on: `AF-1`, `AF-2`)
  - **Scope:** Factor OS-independent wrapper/dispatch/event machinery out of Win32 modules in `crates/calc-com-host`, add a Linux SysV x86_64 `#[repr(C)]` `VARIANT`/`BSTR`/`SAFEARRAY`/`IUnknown`/`IDispatch`/`IConnectionPointContainer` ABI layer (no `oleaut32`, no `WM_GETOBJECT`/ROT/`LocalServer`, no Windows COM/Automation add-ins), and extend `crates/calc-com-host/generate.rs` to emit SysV-ABI-correct vtable thunks (6 GPRs + 8 XMM registers and stack-passed by-value `VARIANT`s).
  - **Platform:** Linux x86_64 (`cargo xtask check` native).
  - **Acceptance tests:**
    1. A compiled C fixture (`gcc` test binary or `cdylib` test on Linux) receives raw `_Application*` and `IDispatch*` from `calc-com-host` against a real `WorkbookAutomation` + `HostSession`, calls early-bound `_Application::get_Version`, `_Application::get_Range`, `_Application::Run` (31 by-value `VARIANT` args), and late-bound `Range.Value2` get/put via `IDispatch::Invoke`, receives `AppEvents.SheetChange` on a C `IDispatch` sink, and verifies `0` leaked wrappers/tracker slots after `Release`.
    2. `calc-com-host` still `compile_error!`s on `wasm32-unknown-unknown` and passes `cargo xwin check` on Windows x64.

#### Wave 2 — C API command face (`xlc*`, `xlSet`, `Excel4`/`Excel4v`, Linux in-process)

- **Existing bead `dc-17spk.3` — C API command inventory (`docs/architecture/C_API_COMMANDS.md`)** (Priority: P2; depends on: none)
  - **Scope:** Enumerate every `xlcXXX` command from `xlcall.h` (number, name, arguments, XLM equivalent, today's status in `calc-xll-host`), group by object-model owner, and identify the Tier-1 implementation set. No code.
  - **Acceptance tests:** `docs/architecture/C_API_COMMANDS.md` lists 100% of `xlcall.h` `xlc*` constants with verified counts matching `crates/calc-xll-abi` and `crates/calc-xll-host/src/callbacks.rs`.

- **AF-6 (`dc-17spk.15`) — Excel C API command probes (batch C1–C4)** (Priority: P2; depends on: `dc-17spk.3`)
  - **Scope:** Extend `crates/calc-xll-fixture` and `crates/calc-xll-reference` on Windows Excel 16.0.20527 to measure: (C1) `xlSet` semantics (formula strings starting with `=`, automatic vs manual calc, `Change` event firing, multi-area/shape mismatch, return code); (C2) Tier-1 `xlc*` return codes, `XL_INTL` and `XL_PROMPT` behaviour, and `#`-UDF vs command-context legality; (C3) legacy `Excel4`/`Excel4v` (`XLOPER` ANSI code page, >255-char string truncation/failure, >65,536-row/256-col ref failure); (C4) `Application.ExecuteExcel4Macro` return values, error codes, and `xlc*` reach.
  - **Platform:** Windows x64 + Excel (queued in `docs/plan/windows-queue.md`).
  - **Acceptance tests:** Trace files and JSON summaries committed under `crates/calc-xll-reference/traces/` with two agreeing fresh Excel trials per batch and zero Excel registry/Trust Center drift.

- **AF-7 (`dc-17spk.16`) — Split portable C API command dispatcher from Win32 XLL DLL loader** (Priority: P2; depends on: `dc-17spk.3`, `AF-1`)
  - **Scope:** Separate the OS-independent C API callback/command dispatcher (`XLOPER12` decoding, `SheetMap`, `Arena`, `CallbackReply`, `State::allocations`, `xlFree`, `xlCoerce`, `xlSheetId`/`xlSheetNm`, `ObjectModel` command frame, Guarded reach check) from Win32 `LoadLibraryExW`/`user32`/`dbghelp` XLL loading in `crates/calc-xll-host`. Export an in-process `Excel12`/`Excel12v` command entry usable on both Windows and Linux without loading `.xll` DLLs, while keeping `calc-xll-host`'s existing X1–X19 Windows XLL behaviour unchanged.
  - **Platform:** Linux x86_64 and Windows x64 (`native-only`, refuses `wasm32`).
  - **Acceptance tests:**
    1. Linux native tests in `crates/calc-xll-host` enter a command frame bound to `WorkbookAutomation`, invoke `xlSheetId`, `xlSheetNm`, `xlCoerce`, `xlFree`, `xlcMessage`, and `xlfSum`, and verify zero leaked `State::allocations` and exact `CallbackReply` statuses.
    2. Windows `cargo xtask app xll-accept onecalc` (350/350) and X1/x13/x14/x17/x18 traces remain unchanged.

- **AF-8 (`dc-17spk.17`) — Implement `xlSet` and Tier-1 `xlc*` commands over `WorkbookAutomation`** (Priority: P2; depends on: `AF-3`, `AF-6`, `AF-7`)
  - **Scope:** Bind `Rc<dyn ObjectModel>` into the C API command dispatcher (`apps/dna-calc-desktop/src/automation.rs` on Windows; in-process command host on Linux). Implement `xlSet` (16387) and Tier-1 `xlc*` commands (`xlcFormula`, `xlcFormulaArray`, `xlcSelect`, `xlcCalculateNow`, `xlcCalculateDocument`, `xlcEcho`, `xlcClear`, `xlcDefineName`, `xlcDeleteName`, `xlcWorkbookInsert`, `xlcInsert`, `xlcEditDelete`, `xlcFormatNumber`, `xlcRun`, `xlcAlert` refusal/controller bridge, `XL_INTL` / `XL_PROMPT` handling) strictly by translating to `ObjectModel::invoke` / `WorkbookAutomation`, plus wiring `xlEventRegister` firing on interactive calc end/cancel.
  - **Platform:** Linux x86_64 and Windows x64.
  - **Acceptance tests:**
    1. Linux in-process C/Rust test drives `Excel12v` with `xlSet`, `xlcSelect`, `xlcFormula`, `xlcCalculateNow`, `xlcDefineName`, and `xlcClear` against a real `HostSession`, verifying calculated values, `HostDelta` publications, `WorkbookEventObserver` events, `XL_PROMPT` refusal, and Guarded reach denial.
    2. Windows `calc-xll-fixture` C1–C2 command suite matches the AF-6 Excel captures 100% on `dna-calc-desktop --engine` and `--app`.

- **AF-9 (`dc-17spk.18`) — Legacy `Excel4`/`Excel4v` (`XLOPER`) ABI translation** (Priority: P3; depends on: `AF-6`, `AF-7`)
  - **Scope:** Add `XlOper`, `XlRef`, `XlMRef`, `XlSRef`, `Fp`, and ANSI counted-string conversion to `crates/calc-xll-abi`; replace the `return 32` stubs in `platform/calc-xll-shim/src/variadic.c:17-22` with `Excel4`/`Excel4v` forwarding to the shared C API dispatcher (and expose `Excel4`/`Excel4v` on Linux in-process). Support legacy `P`/`R`/`C`/`D`/`K` type codes in `calc-xll-host/src/invoke.rs` where measured in C3.
  - **Platform:** Linux x86_64 and Windows x64 (`native-only`).
  - **Acceptance tests:**
    1. Layout/offset unit tests for `XlOper`/`XlRef`/`XlMRef`/`XlSRef`/`Fp` in `crates/calc-xll-abi` on Linux and Windows.
    2. Linux and Windows C tests invoking `Excel4` and `Excel4v` for `xlSet`, `xlcFormula`, `xlCoerce`, `xlFree`, and `xlfSum`, plus >255-byte string and >65,536-row boundary cases matching AF-6 C3 Excel captures.

- **AF-10 (`dc-17spk.19`) — `Application.ExecuteExcel4Macro` over the C API command table** (Priority: P3; depends on: `AF-1`, `AF-8`)
  - **Scope:** Implement `_Application.ExecuteExcel4Macro` in `crates/calc-host/src/automation/` by parsing the XLM macro expression via `calc-fml`, checking the `xlc*`/`xlf*` command's `reach` under Guarded policy, and dispatching into the same command/function core (portable, including Linux and wasm32).
  - **Platform:** Portable (Linux, Windows, wasm32).
  - **Acceptance tests:**
    1. Portable tests covering `ExecuteExcel4Macro("S!R1C2")`, `ExecuteExcel4Macro("1+1")`, `ExecuteExcel4Macro("SET.VALUE(...)")`, and Guarded refusal of `ExecuteExcel4Macro("EXEC(\"cmd\")")` with audit record, matching AF-6 C4 Excel answers.

#### Wave 3 — Office.js face (inventory, J1–J5 probes, batch protocol, Face 1 add-in runtime, Face 2 swappable JS custom functions)

- **Existing bead `dc-17spk.4` — Office.js inventory (`docs/architecture/OFFICE_JS_MAP.md`)** (Priority: P2; depends on: `dc-1nbp.30`)
  - **Scope:** Inventory Excel JavaScript API objects, members, events, and requirement sets (`ExcelApi 1.1..1.18`), map each to its owning `calc-object-model` descriptor member (or mark none), and record `WorkbookAutomation`'s current coverage. No code.
  - **Acceptance tests:** `docs/architecture/OFFICE_JS_MAP.md` lists every Tier-1/Tier-2 Office.js object and member with exact mapped descriptor `(Interface, Member, InvokeKind)` or explicit `none`, and live handler counts matching `WorkbookAutomation::coverage()`.

- **AF-11 (`dc-17spk.20`) — Excel J1–J5 Office.js measurements** (Priority: P2; depends on: `dc-1nbp.30` owner sideload approval)
  - **Scope:** Run experiments J1–J5 from `docs/architecture/review-2026-11/pro/14-object-model-and-js-answer.md` section 5 against real Excel 16.0.20527 via an owner-approved sideload route (zero Trust Center or prohibited registry mutation): (J1) read/write/calculate batch snapshot timing; (J2) partial-failure hydration and suffix skip; (J3) `trackedObjects` across structural edits and workbook close; (J4) ordinary `await`, final `Excel.run` sync, and concurrent `sync()`; (J5) async/streaming `CustomFunctions` eligibility and nested `Excel.run` write prohibition.
  - **Platform:** Windows x64 + Excel (queued in `docs/plan/windows-queue.md`).
  - **Acceptance tests:** Raw J1–J5 traces and verified outcome tables committed under `crates/calc-xll-reference/traces/` (or `docs/research/`) with three agreeing trials per scenario.

- **AF-12 (`dc-17spk.21`) — `AutomationBatch` protocol, host handle table, and `Event::AutomationEvent` in `dna-calc-protocol` / `dna-calc-worker`** (Priority: P2; depends on: `dc-17spk.4`, `AF-1`, `AF-2`, `AF-3`)
  - **Scope:** Add `Request::AutomationBatch`, `Request::AutomationRelease`, `Reply::AutomationBatch`, `Event::AutomationEvent`, and lossless `WireAutoValue` to `apps/dna-calc-model/src/projection.rs` and `apps/dna-calc-protocol`. Bind `Rc<WorkbookAutomation>` and `AutomationHandles` (run-scoped + `trackedObjects` leases) inside `apps/dna-calc-worker/src/server.rs`, executing each batch op sequentially on `WorkbookAutomation` with Guarded reach checking, stop-on-first-failure receipts, and `HostDelta` publication.
  - **Platform:** Portable (Linux, Windows, and `wasm32-unknown-unknown`).
  - **Acceptance tests:**
    1. Portable unit/integration tests in `apps/dna-calc-worker` (native + headless Chrome/Firefox wasm) executing the mixed read/write/calculate batch (`before.values == [[2]]`, `after.values == [[3]]`), partial-failure stop without rollback, `trackedObjects` survival across row insertion and tombstone on workbook close, Guarded `PolicyDenied` -> `"AccessDenied"`, and `-0.0`/error/empty wire bit fidelity.
    2. `cargo xtask sweep` dependency-ceiling check confirms `dna-calc` (UI) and `dna-onecalc` retain zero forbidden dependencies.

- **Existing bead `dc-17spk.5` — Office.js face 1: Office Add-ins runtime (clean-room library, manifests, webview host, task pane, commands, events, dialogs)** (Priority: P2; depends on: `AF-1`, `AF-11`, `AF-12`)
  - **Scope:** Implement DNA Calc's own clean-room MIT Office.js-compatible client library (`apps/dna-calc/web/office-js/`, never redistributing Microsoft's `office.js` without licence clearance): `Office.onReady`, `Excel.run`, `RequestContext`, proxy classes, `load`/`sync`, `trackedObjects`, `isNullObject`, `OfficeExtension.Error`, async events (`onChanged`, `onCalculated`, `onSelectionChanged`), `Office.context.ui.displayDialogAsync`, and `Office.actions.associate`. Implement XML and unified JSON manifest loading, out-of-process add-in page hosting (WebView2 on Windows, WebKitGTK / `dnacalc serve` on Linux, iframe/Worker in browser wasm), ribbon command wiring, Guarded mediation (mediated `fetch`, `Off`/`Guarded`/`Unguarded`, audit bar), and the shared-runtime hook for custom functions.
  - **Platform:** Browser wasm (Chrome, Firefox), Windows WebView2 (`dna-calc-desktop`), Linux (WebKitGTK / `dnacalc serve`).
  - **Acceptance tests:**
    1. Browser WebDriver suite (headless Chrome and Firefox against the wasm worker) and Linux `dnacalc serve` suite pass all J1–J4 parity cases, `PropertyNotLoaded`, `isNullObject`, `NotModelled` refusal propagation, async event delivery/removal + `EnableEvents = false`, and verify exactly one `AutomationBatch` message per `context.sync()` (zero per-cell RPCs).
    2. Sample Office Add-in (XML manifest and unified JSON manifest) loads its task pane, triggers a ribbon command, opens a dialog, and mutates cells in `Guarded` mode while unallowed `fetch` and `Workbooks.Open` fail with `AccessDenied` and increment the audit log; `Off` prevents loading.
    3. Windows WebView2 smoke run passes the same add-in suite against `dna-calc-desktop --app`.

- **Existing bead `dc-17spk.6` — Office.js face 2: Embedded JS custom functions behind a swappable engine (`crates/calc-js-host`)** (Priority: P2; depends on: `AF-4`, `AF-11`)
  - **Scope:** Create `crates/calc-js-host` (depending only on `calc-value`, `calc-func`, `calc-fml` — never `calc-host`/`calc-engine`) with the compile-time swappable `JsEngine` trait, `functions.json` metadata loader -> `CallableDescriptor`, sync/async (`Promise` -> `UserFunctionStep::Awaiting` with P07 eligibility tokens)/streaming (`setResult` + `onCanceled`) invocation adapters, CPU timeout and heap memory limits, and Cargo feature backends: `engine-quickjs` (`rquickjs`, first native backend), `engine-boa` (pure Rust), `engine-v8` (`rusty_v8`/`deno_core`), and `engine-browser` (`wasm32` Web Worker). Wire into `CallableTable` beside XLL, VBA, and Automation UDFs under `Off`/`Guarded`/`Unguarded`.
  - **Platform:** Linux x86_64, Windows x64, and `wasm32-unknown-unknown` (`engine-browser`).
  - **Acceptance tests:**
    1. Shared `calc-js-host` conformance suite runs identically and passes on every enabled engine feature (`engine-quickjs` first, then `engine-boa`, `engine-v8`, and headless Chrome/Firefox `engine-browser`): scalar/2-D array/`-0.0`/error marshalling, sync UDF, async `Promise` UDF with obsolete-completion discard, streaming `setResult` + `onCanceled`, infinite-loop timeout abort, memory-limit abort, and `CallableTable` + signature-help metadata.
    2. End-to-end workbook and OneCalc tests evaluate JS custom functions alongside XLL and VBA UDFs in `Guarded` mode (pure JS succeeds; `Off` blocks loading; nested `Excel.run` mutation refused).
    3. `cargo xtask sweep` confirms `calc-js-host` respects `tools/xtask/architecture.json` and OneCalc's dependency ceiling.

#### Wave 4 — Cross-face parity and deferred platforms

- **AF-13 (`dc-17spk.22`) — Cross-face and cross-UDF differential parity harness** (Priority: P2; depends on: `dc-piw0.5`, `dc-17spk.2`, `dc-17spk.5`, `dc-17spk.6`, `dc-17spk.7`, `AF-5`, `AF-8`)
  - **Scope:** One shared scenario suite (cell values, formulas, `Calculate`, `ClearContents`, `Names`, `Insert`/`Delete` with held references, tables/charts/shapes where supported, `EnableEvents`, Guarded reach denials, and save/reopen) executed against the exact same starting `.xlsx` through (1) COM (Windows in-proc, out-of-proc, and COM add-in; Linux in-proc), (2) C API `Excel12`/`Excel4` (Windows & Linux), (3) Office.js Face 1 (wasm browser, Windows WebView2, Linux WebKitGTK/`dnacalc serve`), and (4) VBA (`calc-host-vba`), plus one shared UDF suite calling equivalent functions across XLL, VBA, JS (`calc-js-host`), and Automation add-ins through `CallableTable`.
  - **Platform:** Linux x86_64, Windows x64, Browser wasm.
  - **Acceptance tests:** Every face produces bit-identical calculated cell values, identical authored workbook XML/parts on save, identical `ReferenceTracker` survival/tombstone outcomes, and identical Guarded audit records; zero face-specific workbook code exists outside `calc-host`.

- **AF-14 (`dc-17spk.23`) — macOS in-process build and verification (deferred)** (Priority: P3; depends on: `AF-5`, `AF-7`, `dc-17spk.5`, `dc-17spk.6`)
  - **Scope:** Compile and test the three in-process faces and `calc-js-host` on macOS (`aarch64-apple-darwin` / `x86_64-apple-darwin`), adding AAPCS64 vtable thunk generation in `calc-com-host/generate.rs`, in-process `Excel12`/`Excel4` C API command tests, `calc-js-host` conformance tests, and Office.js via `dnacalc serve` (or `WKWebView` when a macOS shell exists).
  - **Platform:** macOS (explicitly deferred per `dc-17spk`).
  - **Acceptance tests:** `cargo xtask check` green on macOS; AF-13 cross-face parity suite passes for all three in-process faces on macOS.
