# One VARIANT, one COM object: CalcVBA's Automation layout migration

*Completed in epic `dc-yqq0` (2026-11-22, 29 of 29 tasks).*

CalcVBA runs VBA projects outside Excel on Windows, Linux, macOS, and WebAssembly across three execution backends: the **VM3** interpreter, the **Cranelift JIT**, and **AOT** native binaries. When VBA code touches `VarPtr`, `CopyMemory`, `Declare`, COM Automation servers, Excel's object model, or native add-in exports, its runtime values and class instances are observed directly at binary ABI boundaries.

Over epic `dc-yqq0`, CalcVBA replaced its earlier internal value and wrapper layers with one uniform architecture: **every runtime `Variant` cell is the platform's real Automation `VARIANT`, and every VBA class instance is one COM object with no wrappers**. Both VM3 and the JIT share one runtime representation, one marshaller, and one descriptor model.

## Why the layout had to change

CalcVBA originally stored runtime values in a 16-byte cell (`vt`, reserved fields, and an 8-byte payload), kept COM and VBA user-defined types (UDTs) in boxed `RecordPayload` wrappers, laid out multi-dimensional arrays in C-order (last dimension fastest) with a private descriptor prefix, translated COM calls through intermediate `ComValue` / `DynamicValue` carriers and `i32` object handle maps, and backed `VarPtr`/`StrPtr`/`Declare` with copy-in/copy-out pin buffers.

Probing Excel 16.0 (x64, VBA7) showed why those abstractions diverged from VBA at interop boundaries:

- **64-bit `VARIANT` is 24 bytes, not 16.** `BRECORD { pvRecord, pRecInfo }` adds a second pointer field at offset 16 on 64-bit platforms (`16` bytes total on 32-bit and `wasm32`, with `BRECORD` at offsets 8 and 12).
- **VBA exposes its own cells and descriptors directly.** `VarPtr(v)` returns the address of the live `VARIANT`, `VarPtr(a(1)) - VarPtr(a(0))` equals `24` for `Variant()` and `4` for `Long()`, `StrPtr(s) - 4` holds `LenB(s)`, `ReDim Preserve` keeps the `SAFEARRAY*` descriptor address, and `ByRef Variant` parameters bind `VT_BYREF | vt` cells pointing straight into typed variables, array elements, and UDT fields.
- **Multi-dimensional arrays and UDTs follow column-major Automation storage.** Elements are stored **first dimension fastest** with `rgsabound` reversed, `fFeatures` flags match Excel (`&H80` dynamic typed, `&H180` String, `&H880` Variant, `&H440` `VT_DISPATCH` object/class, `&H12` fixed UDT, `&H92` fixed typed), and UDTs sit in plain Automation-aligned memory rather than `Variant` boxes.
- **A VBA class instance has one COM identity.** Passing a project object into `Scripting.Dictionary`, a .NET/C++ COM client, or Excel itself must preserve pointer identity (`Is`), answer `IDispatch` and `ITypeInfo`, support early-bound vtable slots, `Implements`, connection points, and `ISupportErrorInfo`, and defer `Class_Terminate` from a foreign-thread `Release` back to the owning thread.

The migration adopted two rules from the start: **no compatibility layer over the old 16-byte layout**, and **borrow, point, or adopt instead of converting**.

## What changed

### 1. One shared `calcvba-automation` crate and `VARIANT` cell

The platform ABI types moved into a dedicated portable crate, `calcvba-automation` (re-exported by `calcvba-runtime` so downstream hosts such as DNA Calc keep existing import paths):

- **ABI constants and COM vtables:** `VARIANT_SIZE` (`24` on 64-bit, `16` on 32-bit/`wasm32`), `VARIANT_ALIGN`, `VT_OFFSET`, `PAYLOAD_OFFSET`, `RECINFO_OFFSET`, `Guid`, `IUnknownVtbl`, `IDispatchVtbl`, `IRecordInfoVtbl` (all `extern "system"` taking `*const Guid` / `REFIID`), `DispParams`, and `ExcepInfo`. On Windows, compile-time assertions verify size and every field offset against `windows_sys::Win32::System::Variant::VARIANT`.
- **Typed `#[repr(C)]` union payload:** `VariantCore` and the owning `Variant` store pointers as native-width pointer union members (`bytes`, `pointer`, `BRECORD`) rather than exposed-provenance integer bytes. Addresses become integers only at `VarPtr`/`StrPtr`/`ObjPtr`.
- **Raw `VARTYPE` fidelity:** Cells store exact Automation tags: `VT_ARRAY | vt` (including unallocated typed arrays with a null `parray`), `VT_UNKNOWN` (`VarType` 13 / `vbDataObject`) distinct from `VT_DISPATCH` (`9`), `VT_ERROR` holding the SCODE (such as `CVErr(2042)` = `&H800A07FA`), `VT_INT` (`22`) and `VT_UINT` (`23`), scalar `VT_I1`/`VT_UI2`/`VT_UI4`/`VT_UI8` (`VarType` 16/18/19/21), and `VT_BYREF | vt` runtime cells.

### 2. Real addresses, stable storage, and `VT_BYREF` cells

The pin registry, write-back tables, and `lookup_pointer` heuristics were deleted:

- **One address-of primitive:** Both VM3 and the JIT lower `VarPtr`, `StrPtr`, `ObjPtr`, and `ByRef` native arguments to `OxInst::AddrOf` backed by `calcvba_runtime::address`. VM3 temporaries (`Vec<Variant>`), globals, and boxed class fields (`Box<[Variant]>`, kept when set to `Empty`) remain address-stable across calls. `vbNullString` is a real null `BSTR` (`StrPtr` = `0`, passing `NULL` to `Declare`), and `StrPtr(s) - 4` carries `LenB(s)` on all platforms.
- **In-place `VT_BYREF` binding:** When a typed variable, typed array element, or UDT field (scalar, `String`, `Object`, `Variant`, or dynamic-array field) is passed `ByRef` to a `Variant` parameter, `OxInst::MakeRef` binds a `VT_BYREF | vt` (or `VT_BYREF | VT_ARRAY | vt`) cell pointing directly at that storage; class-typed object variables use a temporary with copy-back, matching Excel. Writes through the reference apply VBA's `Let` coercion (banker's rounding; errors `13`, `94`, and `6` leave the target unchanged).
- **Array locks and in-place `ReDim`:** Active `ByRef` element references and `For Each` loops take an `ArrayLock` (`cLocks` + 1) on every array along the path, raising runtime error `10` on `ReDim`, `Erase`, or overwrite while keeping locked arrays alive until the last unlock. `ReDim` and `ReDim Preserve` swap storage in place (`SafeArray::swap_storage`), preserving the `SAFEARRAY*` descriptor pointer when rank and element layout match.

### 3. Records, `SAFEARRAY`s, and `SAFEARRAY(VT_RECORD)`

| Construct | Representation after `dc-yqq0` | Key behavior |
|---|---|---|
| **VBA UDTs** | Plain Automation-aligned memory (`Object` = pointer, dynamic array = `SAFEARRAY*`, fixed arrays first-dimension-fastest) | Assigning a standard-module `Type` to or from a `Variant` is a compile error (17 measured Excel cases). |
| **COM `VT_RECORD`** | `BRECORD { pvRecord, pRecInfo }` (`CoTaskMem` + `IRecordInfo`) | Copies and clears via `VariantCopy`/`VariantClear` (`GetSize`, `RecordCopy`, `RecordClear`, `Release`). |
| **`SAFEARRAY`s** | `oleaut32` on Windows (`SafeArrayAllocDescriptorEx`/`Data`/`Destroy`); identical descriptors and 16-byte hidden prefixes (`vt` at `psa - 4`, `IID` at `psa - 16`) elsewhere | First-dimension-fastest storage, reversed `rgsabound`, class-typed arrays carrying their class/interface IID, Excel-exact `fFeatures`. |
| **COM `SAFEARRAY(VT_RECORD)`** | Nominal `VbaRecordIdentity` (source-case name + deterministic project/UDT GUID) + portable `IRecordInfo` (`FADF_RECORD`) | Bidirectional marshalling across early-bound slots, `IDispatch::Invoke`, `ITypeInfo`, and `Variant` cells, with `ByRef`/`retval` copy-back and pre-copy nominal mismatch rejection (`DISP_E_TYPEMISMATCH`). |

### 4. One COM marshaller, `Declare`, host aliasing, and lossless UTF-16

- **One `VARIANT`-to-`VARIANT` COM client:** `ComValue`, `DynamicValue`, `ComInvokeValue`, and `i32` handle registries are gone. Both late-bound `IDispatch::Invoke` and early-bound vtable calls (`calcvba_com::automation::early` via `DispCallFunc`) borrow `ByVal` cells, pass `VT_BYREF` pointers into runtime storage, and adopt `[out, retval]` results by move. `For Each` iterates lazily via `IEnumVARIANT::Next(1, ...)`, and `WithEvents` advises one sink per variable covering all DISPIDs with synchronous delivery on both VM3 and the JIT (including `End` and unhandled-error aborts via `RunAbort`). Windows interpreter lanes now default to the OS `Scripting.Dictionary`.
- **Direct `Declare` and JIT parity:** VM3 and the JIT share `DeclarePlan` with cached libffi CIFs, direct storage pointers for `ByRef` scalars/arrays/string-free records, ANSI `BSTR` temporaries (`CP_ACP` on Windows, Windows-1252 elsewhere) converted back in place, and `AddressOf` callbacks in the JIT.
- **Zero-copy host `ByRef` aliasing and lossless UTF-16:** `calcvba-host` aliases `ByRef` arguments directly into caller storage (`Activation::alias_host`, `ByRefCell`). Library string paths (`InStr`, `InStrRev`, `InStrB`, `Format`, `StrConv`, `StrReverse`) and the error model (`Utf16String` in `ErrState`, `Fault`, `EXCEPINFO`, `IErrorInfo`) operate on UTF-16 code units without losing lone surrogates.

### 5. Project objects as real COM objects

Every VBA class instance (`CompatObjectBase`) now carries its COM interfaces inline on the object, and all project-object wrappers and maps (`RuntimeObjectDispatch`, `project_dispatch`, `WRAPPERS`, `DispatchObject`) have been deleted:

- **One identity, one descriptor, O(1) dispatch:** `QueryInterface` for `IUnknown`, `IDispatch`, and the class's default dual IID returns `&primary`, giving pointer-equal `Is` identity and a single atomic reference count across every interface. VM3, the JIT/`calcvba-rt-abi`, and native builds share one interned `RuntimeClassDescriptor` builder (`CVBD` format up to v5), eliminating name scans in favour of vtable slot, DISPID, and `RuntimeDispatchPlanCache` dispatch.
- **`ITypeInfo` and DISPID/vtable numbering matching Excel:** `IDispatch::GetTypeInfoCount` returns `1` and `GetTypeInfo(0)` serves a portable dual `ITypeInfo` (`TKIND_DISPATCH` + `TKIND_INTERFACE`, flags `0x1C2`) built from `RuntimeClassDescriptor`. Live `TestEventServer.DescribeDispatch` probes on Excel established the exact rules CalcVBA now follows:
  - **Public fields:** `0x4003_0000 + n` in source order (slotted first starting at vtable offset `56` / slot `7`; `Object` fields emit Get, `FUNCFLAG_FRESTRICTED` Let `0x1`, and Set; `Variant` fields emit Get, Let, and Set).
  - **Properties:** `0x6803_0000 + n` numbered first in reverse source order.
  - **Other procedures:** `0x6003_0000 + n` continuing forward in source order, with `Private`, `Friend`, and nonempty lifecycle methods (`Class_Initialize`, `Class_Terminate`) occupying hidden slots after the public dual surface.
  - **Default members:** `VB_UserMemId = 0` assigns DISPID `0` without advancing the ordinary counter.
- **Early-bound slot thunks (Windows x64):** `ClassDualVtbl` fills slots `>= 7` with native entry stubs (`4,096` 16-byte stubs with SEH unwind metadata into a shared dispatcher) supporting scalars, `Decimal`, UDTs, `SAFEARRAY`s (including nominal `SAFEARRAY(VT_RECORD)`), and `ParamArray`. Non-Win64 targets retain portable `IDispatch` and native slot stubs.
- **`Implements`, connection points, error info, and thread safety:** Implemented interfaces sit as extra vtable faces on `CompatObjectBase` whose `IUnknown` delegates to `&primary`. Classes with `Event` declarations expose `IConnectionPointContainer` / `IConnectionPoint` / `IEnumConnectionPoints` / `IEnumConnections` and deliver `RaiseEvent` synchronously on VM3 and JIT. `ISupportErrorInfo` / `IErrorInfo` (`RuntimeErrorInfo` and Win32 `SetErrorInfo`/`GetErrorInfo`) publish VBA faults—preserving lone surrogates—for both late-bound and early-bound calls. Off-thread `Invoke` returns `RPC_E_WRONG_THREAD`, and a final `Release` on a foreign thread enqueues onto an owner-scoped multi-producer queue (`OwnerTerminationChannel`) so `Class_Terminate` and field drops run on the owning thread.

## How it was verified

1. **Excel 16.0 (x64, build `16.0.20527.20196`) measurements:**
   - **Layout oracle (`variant-layout.vba`):** `83` cases covering strides, cell bytes, `SAFEARRAY` headers, `CopyMemory`, `VarPtr`/`StrPtr` writes, UDT layouts, `VT_BYREF` coercions, and `Type`-to-Variant compile errors; VM3 and JIT moved from `81` known VM3 differences at step 1 to **0 known differences**.
   - **COM datatype oracle:** **`15 of 15`** matching Excel across scalar subtypes (`VT_I1`, `VT_UI2`, `VT_UI4`, `VT_UI8`, `VT_INT`, `VT_UINT`) and variant conversions.
   - **`ITypeInfo` & DISPID oracle (`DescribeDispatch`):** **`26` Excel comparisons** across `Widget` and `W2`–`W8` matched on both `TKIND_DISPATCH` and `TKIND_INTERFACE` halves on VM3 and JIT.
   - **Events, identity, slot thunks, and in-process Excel hosting:** `events8`, `events8end`, `events8tes`, `objid4`, and the 25-line `global10` probe via both VBA and XLL loaders matched Excel on VM3 and JIT. Three foreign-assignment cases where Excel 20527 accepted foreign records, foreign record arrays, and signed-byte arrays were recorded as WitnessSet rows while preserving CalcVBA's current refusal tests (`T6b`).
2. **Miri strict provenance:**
   - `calcvba-automation` and `calcvba-runtime` (including `variant_strict_provenance`, `automation_record`, `Implements`/connection points/`IErrorInfo`, multi-threaded `object_owner` + `class_dispatch`, `28/28` runtime and `45/45` `calcvba-com` checks in the wrapper-deletion pass) ran clean under Miri with **0 undefined behavior and 0 integer-to-pointer warnings**.
3. **Portable VM3, JIT, and cross-target suites:**
   - End-to-end portable VM3 + JIT tests (`object_identity_portable`, `declare_cells_unix`, `err_object_value`, `typed_incoming_byref`) verify identity, between-runs session invocation, foreign-thread release, `GetTypeInfo`, `Implements`, connection points, `IErrorInfo`, and nominal `SAFEARRAY(VT_RECORD)` marshalling on Linux and Windows.
   - Cross-builds verified across Linux x86-64, `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`, and `wasm32-unknown-unknown`.
4. **Windows x64 live COM and native builds:**
   - Full Windows workspace verification reached **`4,123` tests passing** (`comhost_inproc` on VM3 + JIT with `RaiseEvent` re-entry and `Self()` identity, `Win64 Declare` **`9/9`**, `Win32 COM` **`155/155`**, `object_identity` **`11/11`** with live `OxVba.TestEventServer` COM round-trips and nominal `SAFEARRAY(VT_RECORD)` vtable/`IDispatch` tests, native EXE **`5/5`**, and runtime **`101/101`** with `CVBD v5` native PE templates refreshed).

## What it enables

- **Early-bound and dual COM callers:** External COM clients (.NET, C++, Excel, or another VBA host) can bind directly to CalcVBA class vtables on Win64, inspect `ITypeInfo`, invoke `Implements` interfaces, subscribe to `WithEvents` connection points, pass `Decimal`, UDTs, `SAFEARRAY(VT_RECORD)`, and `ParamArray`, and receive `IErrorInfo` faults without proxy wrappers.
- **Native exports and in-process COM servers:** Compiled AOT DLL/EXE exports and `calcvba-comhost` in-process servers return live project instances directly as themselves, sharing one `RuntimeClassDescriptor` and `MemberInvoker` contract across VM3, JIT, and AOT.
- **Add-ins and Excel hosting:** Because CalcVBA cells and objects already match the Automation ABI, host objects such as Excel's `Application`, `Workbook`, and `Range` cross in-process into CalcVBA—and CalcVBA objects cross back—with zero conversion layer, enabling VBA add-ins (`.xlam`), workbook macros (`.xlsm`), and document-module event handling in DNA Calc and Formula Explorer.
- **Cross-platform consistency:** One `Variant` and `SAFEARRAY` model plus portable `IRecordInfo`/`ITypeInfo`/`IDispatch` keeps semantics identical across Windows, Linux, macOS, and `wasm32`.
