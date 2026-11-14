# The Excel object model: one model, every route

Owner direction 2026-11-13: one uniform, full COM object model, written in Rust and done right from the start.
Add-in work implements members of that model in priority order; it never builds a separate facade. No C++
(AGENTS.md, Language). This note is the design. It implements the pro-10 answer 14 plan (one descriptor, one
semantic owner, projections that only translate; `review-2026-11/pro/14-object-model-and-js-answer.md` 1.1,
3.1-3.5, 6) with the corrections of its harvest (`pro/harvest/14-harvest.md` findings 1 and 6), and the crate
SYNTHESIS.md planned (`calc-object-model`: "descriptor model, generated interfaces and adapters, Range
handles"). It widens bead dc-1nbp.5 (14.d), which asked for this note.

## 1. What was there (2026-11-13), and what replaces it

Four hand-written member tables, none generated, none shared:

| Table | Where | Replaced by |
|---|---|---|
| COM facade for add-ins: Excel IIDs, hand-filled vtables, a `_wcsicmp` DISPID chain, member semantics in C++ (Address, Union, Count, Rows/Columns, Workbook name) | `calc-com-host/src/projection.cpp` | the descriptor (section 3), the Rust COM projection (section 6), member semantics in profiles (section 5) |
| Office objects (COMAddIns, CommandBars, ribbon) | `calc-com-host/src/projection-office.h` | Office members of the descriptor; semantics in `calc-com-host` (Rust) |
| RTD client and update sink | `calc-com-host/src/rtd.cpp` | Rust (`windows` crate) in `calc-com-host` |
| VBA UDF objects (private tokens, DNA IIDs) and VBA event objects (typed slots) | `calc-vba/src/runtime/spreadsheet.rs`, `calc-vba/src/events/` | later, by the CalcVBA session, binding to the same descriptor and profiles (section 8). Not touched tonight |

What stays: the session's one event source with its observer fan-out (`calc-host/src/events.rs`
`WorkbookEventObserver`, 9159812c), the one EnableEvents switch, the session reference tracker for Range
identity, the session's reference binder (`HostState::resolve_reference_text`), the XLL C API
(`calc-xll-host`, not an object model).

## 2. Rules

1. **One descriptor.** Every interface fact (IID, DISPID, vtable slot, parameter list with `[lcid]` and
   `[out, retval]`, optional and default values) comes from `crates/calc-object-model/descriptor/*.json`,
   generated from the installed type libraries. No projection keeps a member table.
2. **One semantic owner per member.** A member is implemented once, in a profile, against Automation values
   and object identities. COM, VBA and JavaScript call the same implementation.
3. **Projections translate.** The COM projection lowers VARIANT/vtable/IDispatch calls to `Call` and back.
   It holds no workbook state and answers no member itself.
4. **Complete layouts or nothing.** An interface the projection advertises (answers QueryInterface for) has
   every vtable slot of the descriptor filled with a function of the right signature. Members without a
   handler answer the uniform refusal. An interface whose layout we cannot generate is not advertised
   (answer 14, 3.5).
5. **The same answer on every route.** A refusal is run-time error 1004 with the text "DNA Calc does not
   model `<Interface.Member>` yet (`<bead>`)": through the vtable `SetErrorInfo` + `0x800A03EC`; through
   `Invoke` `DISP_E_EXCEPTION` with that EXCEPINFO, or the same HRESULT when EXCEPINFO is null; and a typed
   refusal on the calling formula when the call happens inside a worksheet invocation (honesty rule 2).
   This closes 14.c (dc-1nbp.4) by construction.
6. **Excel's behaviour, measured.** Semantics follow Excel; where unmeasured (Value date/currency
   conversion is measured but not yet implemented; multi-area and mismatched-shape writes) the member
   refuses until dc-1nbp.6 (14.e) measures it. Range identity is measured (section 9).
7. **The engine does no more than Excel**, and OneCalc stays values-only (dc-svqh).

## 3. The descriptor

- `tools/com-descriptor` (Rust, Windows) reads a type library through `LoadRegTypeLib`/`ITypeLib` and writes
  the listed types to JSON. Lists: `descriptor/<library>.interfaces.txt`. Libraries: Excel 1.9 (EXCEL.EXE
  16.0.20527), Office 2.8 (MSO.DLL), Add-in Designer 1.0 (MSADDNDR.OLB). The file version is recorded.
- For a dual interface the JSON holds the vtable view: every parameter in order, `slot = oVft / 8` (x64),
  `vtable_slots` counting IUnknown and IDispatch. A dispinterface has DISPIDs only and 7 slots.
- Names and ABI facts only, no documentation strings. These are the facts any client compiled against
  Excel's type library already carries (PIAs, Excel-DNA, xlOil); we record them for interoperability, as
  `projection.cpp` already did from the same library.
- Growth: to advertise a new interface (Font, Interior, ListObject, ...), add its name and regenerate.
- Coverage: each profile can list what it implements against what the descriptor holds
  (`Handlers::implemented`), so "how much of the object model works" is a count, not a claim.

## 4. Values, objects, identity

- `AutoValue` (calc-object-model): Empty, Null, Missing, Bool, I2, I4, R4, R8, Currency, Date, Text
  (UTF-16, `ExcelText`), Error (CVErr number), Object (`ObjectRef`), Foreign (a client's own IDispatch,
  held by the projection), Array (2-D, 1-based).
- `ObjectRef { interface, key }`: the default interface and what it denotes. Workbook objects name session
  identities and are resolved on every call. A Range is a `StoredReference` handle of the session tracker
  (harvest finding 1), released when the last wrapper is released (`ObjectModel::released`).
- The COM projection makes a fresh wrapper per acquisition; COM identity (`Is`) is wrapper identity until
  E1 measures Excel.

## 5. Dispatch and profiles

`ObjectModel` (trait): `application()`, `invoke(target, call)`, `released(target)`, `record_refusal(error)`.
`Handlers<C>`: a profile's member table keyed by descriptor identity; registration by `Interface.Member`
text panics on a name the descriptor lacks.

| Profile | Crate | Members (first set) |
|---|---|---|
| **values-only**: application members that need no workbook. DNA OneCalc uses it alone; DNA Calc's workbook profile falls back to it | `calc-object-model` (`profile::values_only`), portable, host services through a small `ApplicationHost` trait (Hwnd, Run, version) | Application: Application, Parent, Name, Version, Hwnd, Run, EnableEvents (host switch), Path, Workbooks/ActiveWorkbook/ActiveSheet as OneCalc answers them today; Caller, ThisCell, Range, Union refuse with Excel 1004 "needs a workbook: open in DNA Calc" (dc-svqh). WorksheetFunction later (its catalogue route is `UserFunctionContext::worksheet_function`) |
| **workbook**: DNA Calc's workbook members | `calc_host::automation` (calc-host), over the session | Application: Range, Union, Caller, ThisCell, ActiveWorkbook, ActiveSheet, Workbooks, EnableEvents (session switch), Calculate later; Workbooks: Count, Item; Workbook: Name, FullName, Path, Worksheets later; Worksheet: Name, Range later; Range: Value (refuses formatted dates/currency until E6), Value2, Address (A1 and R1C1, absolute flags, multi-area, external), Row, Column, Count, Rows, Columns, Worksheet, Application, Parent |
| **COM by nature** | `calc-com-host` | COMAddIns, COMAddIn (connect through IDTExtensibility2), CommandBars and controls (the bounded model), AutomationSecurity get, ribbon GetCustomUI |

Session access for the workbook profile (calc-host): one trait the desktop implements, with three contexts:
- **worksheet invocation** (an XLL or VBA UDF is running): reads go through that invocation's
  `UserFunctionContext` (`values_of`, `caller`, `retain_reference`, `resolve_reference`), so dependency
  and read rules stay the evaluator's;
- **event** (inside `WorkbookEventObserver::worksheet_change`/`after_calculate`): the borrowed
  `&mut HostSession` (so `Target.Value` works in a SheetChange handler);
- **command** (an add-in command, a ribbon callback, a client call outside calculation): the session,
  borrowed for the call; busy is a typed refusal, never a deadlock.
Writes (Range.Value2 put, Calculate) go through the session's Automation ingress in the event and command
contexts only, as the VBA event route already does; inside a worksheet invocation they are Excel's 1004.
This needs a mutable session in those two contexts: the first access hooks (2026-11-14) lend
`&HostSession` only, so writes wait for a write route in `AutomationHost` (lane om-d's work), and the
hooks must also say which kind of invocation is running (XLL or VBA), because ThisCell differs.

Address formatting, Union geometry, Rows/Columns/Count are one portable module (`calc-object-model`), so the
CalcVBA session can drop its own copies later.

## 6. The COM projection (Rust, `calc-com-host`)

- **Wrapper**: `#[repr(C)]` object with a vtable pointer, an atomic reference count, the `ObjectRef` and a
  weak link to the projection. Release of the last reference calls `ObjectModel::released`.
- **Dispatch-only types**: Excel's library makes Range, Window, Windows, Areas, Names, Name,
  WorksheetFunction and the event interfaces dispinterfaces (7 slots); their vtable twins (IRange 260
  slots, IWindow, ... IAppEvents) are separate hidden types, not in the descriptor. The projection answers
  those objects as Excel's library declares them: IDispatch only. A twin is added to the descriptor and
  advertised only when a client is shown to ask for it.
- **QueryInterface**: IUnknown; IDispatch; the default interface's IID and the IIDs the descriptor gives the
  object (a coclass's interfaces, a base chain such as CommandBarPopup → CommandBarControl);
  ISupportErrorInfo; IConnectionPointContainer for objects with a source interface (Application: AppEvents).
- **Vtables, generated**: `build.rs` reads the descriptor JSON and, for each advertised interface, emits a
  static vtable whose entry `n` is a thunk monomorphized for (interface, slot `n`). On x64 the thunk's
  signature gives each parameter its register class from the descriptor (R4 as f32, R8 and DATE as f64,
  everything else one 64-bit integer slot: pointers, BSTR, I4, BOOL, VARIANT by address).
  The thunk collects the raw arguments and calls one decoder, which reads the slot's member from the
  descriptor, converts each argument by its declared type to `AutoValue`, calls `ObjectModel::invoke`,
  writes the `[out, retval]`, and maps errors by rule 5. Only x64 is built (`compile_error!` elsewhere).
- **IDispatch**: `GetIDsOfNames` from the descriptor (case-insensitive, parameter names for named
  arguments); `Invoke` decodes DISPPARAMS (reverse order, named arguments, DISPID_PROPERTYPUT, VT_BYREF,
  omitted optionals) into the same `Call`. `GetTypeInfo` serves type information built once from the
  descriptor with `CreateTypeLib2` (names and FUNCDESCs, so `TypeName` and late-bound introspection work).
- **Events**: connection points per source interface; `EventSource::raise` invokes subscribers with the
  descriptor's DISPIDs, snapshotting subscribers so a handler can Unadvise during delivery.
- **Activation**: WM_GETOBJECT with OBJID_NATIVEOM returns the Window object (`LresultFromObject`), as
  Excel-DNA's and xlOil's `AccessibleObjectFromWindow` expect. Nothing is registered; Excel's own
  registrations are never touched.
- **Threading**: the projection lives on the workbook window's STA; a call from another thread is
  `RPC_E_WRONG_THREAD`.
- **Foreign interfaces we call** (IDTExtensibility2, IRibbonExtensibility, IRtdServer) are declared with
  the `windows` crate's `#[interface]` and checked against the descriptor by a test (IID, slot order).
  The RTD update sink (IRTDUpdateEvent) is free-threaded and implemented with `#[implement]`.
- **Rust API** kept for callers (`calc-addin-host`, the two desktop apps): `Projection::new(hwnd, host)`
  with the profile (`ObjectModel`) supplied by the application, `get_object`, `verify_layout`,
  `close_addins`, `ribbons`, `run_early`, event raising; `RtdClient` unchanged.

## 7. The C++ goes

`projection.cpp`, `projection-office.h`, `rtd.cpp`, `rtd-interfaces.h`, `value.h` and the `cc` build are
deleted when the Rust projection passes the same tests: each `dna_com_test_*` and `dna_rtd_test_*` becomes a
Rust test driving the real vtables and IDispatch (Windows), plus the page acceptance of sol3's runbook
(`calcvba-udf` 20/20, X19 metadata and values, Excel-DNA and xlOil AppEvents).

## 8. Later routes

- **VBA (CalcVBA session)**: DNA Calc's VBA host gives CalcVBA the descriptor's type information for the
  "Excel" reference (Excel's IIDs, DISPIDs and slots, so code compiled against Excel binds early), and its
  host objects call the same profiles. CalcVBA's `ObjectHeader` typed slots carry the descriptor's members.
  The CalcVBA session owns that change; this note is its requirement.
- **JavaScript**: Office.js-shaped batches over the same `ObjectModel`, once J1-J5 measure Office.js
  (deferred; harvest 14 section 6).

## 9. Measured Excel behaviour these members follow (dc-1nbp.6, 2026-11-14)

From `crates/calc-xll-reference/traces/2026-11-14-om-probes/` (Excel 16.0.20527.20150, three fresh runs,
identical):
- **Range has no vtable**: a live Range refuses QueryInterface for IRange; every Range call is
  `IDispatch::Invoke`, including from early-bound PIA clients. Same for Areas. Only interfaces such as
  `_Application` are called through their vtables.
- **Identity**: two acquisitions of `Range("A2")` are not `Is` (different IUnknowns). One wrapper per
  acquisition is right.
- **Held Range**: follows an inserted row (`$A$3`, value 20); after its row or sheet is deleted every
  access raises 424 (`0x800A01A8`).
- **Value vs Value2**: Value of a date-formatted number is VT_DATE (same bits), Value2 VT_R8; a time-only
  format gives R8 for both. Value is VT_CY (4 decimals) only for the locale's Currency style; `$#,##0.00`,
  `[$$-409]` and accounting formats give R8. Out-of-range CY/date: DISP_E_EXCEPTION scode `0x8002000A`.
  Empty VT_EMPTY, `""` zero-length BSTR, `#N/A` VT_ERROR `0x800A07FA`, a block a 1-based 2-D array.
- **Errors**: `Application.Range("not a range")`: DISP_E_EXCEPTION with scode `0x800A03EC` and no
  description (vtable: `0x800A03EC`, no IErrorInfo). Union across sheets: 1004 "Union method of
  Application class failed" (`0x800A03EC` with a null EXCEPINFO).
- **Union**: areas keep argument order; Count counts overlaps (Union(B2:C3,F5,C3:D4).Count = 9);
  Rows.Count, Row and Column come from the first area; For Each visits overlapping cells twice.
- **Address**: each area formatted alike; External gives `[Book]Sheet!` once; R1C1 relative without
  RelativeTo is relative to A1.
- **Optional arguments**: Empty for RowAbsolute is False; Null and Error mean "use the default";
  `Range("A1", Empty/Null/Error)` is 1004; `Value(Null)` is a type mismatch (`0x80020005`).

## 10. Open items

- Value date/currency conversion (measured, section 9; implementation pending); multi-area and
  mismatched-shape writes (E11 assignment branches not yet run).
- Review findings of 2026-11-14 (independent read of the night's code), being fixed by lane om-e:
  discarded object results and client-object arguments must release their handles; Invoke positional
  arguments must fill slots 0..k-1; EXCEPINFO sets only one of wCode/scode; interface-typed results are
  checked against the declared interface; a refusal is recorded once; one handler per descriptor
  member (no second name switch inside a handler); one EnableEvents switch (the session's);
  AutomationSecurity in the values-only profile; foreign interface parameter types checked against
  the descriptor.
- x86 builds: not supported (DNA Calc ships x64).
