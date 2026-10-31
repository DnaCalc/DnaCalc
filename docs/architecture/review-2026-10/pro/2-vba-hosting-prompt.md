# Question for GPT-6 Pro: how Office hosts VBA, and how DNA Calc should host CalcVBA

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet. CalcVBA (repository
OxVba) is our own independent VBA engine with an interpreter and a JIT (Cranelift), running on
Windows, Linux and in the browser (wasm, interpreter only). The owner's direction (Appendix A):
CalcVBA contains nothing spreadsheet-specific; it offers a host interface (load code, enumerate
public module procedures with signatures, give the host callable entry points); DNA Calc owns a glue
library that registers worksheet-function shims and builds and registers its own COM type library
(Application, Range, WorksheetFunction) that VBA code uses as it would Excel's. Early binding is the
design centre: the worksheet-to-VBA call must be an early-bound, typed, fast call into JIT-compiled
code, and VBA-to-host calls must be early-bound COM (vtable, type-library driven) in both runtimes;
late binding is not a starting point. Module procedures are not COM objects; Excel's glue to VBA
probably has a small non-COM step in registration and COM-like dispatch otherwise, and we want to
understand and mirror that relationship, no more. Windows: full compatibility; Linux and web: a
subset (no general COM references on Linux, no direct file I/O on the web).

Clean room: public documentation (MS-VBAL, MS-OVBA, COM/Automation and type-library documentation,
Office VBA documentation), published literature and black-box behaviour of Excel through public
interfaces only; no disassembly or inspection of Office binaries.

Appendix B is our engine team's own stock-take after a first session; Appendix C the audit of the
current coupling; Appendix D the agreed target section of the architecture synthesis; Appendix E
what we measured in Excel about VBA UDF recalculation.

Questions:
1. **How Office hosts VBA**, as far as public documentation and well-established public knowledge
   go: the host/VBA boundary (project storage, the VBE, references and type libraries, how the host
   object model is exposed: Application as an appobject, global members, the host's type library);
   how Excel discovers worksheet-callable procedures (public functions in standard modules, Option
   Private Module, Application.MacroOptions, Private functions callable from cells in some cases);
   how a cell call reaches VBA (argument coercion from cells/ranges, ByRef behaviour, Range objects
   for untyped parameters, Application.Caller, Application.Volatile, error mapping to #VALUE!, what
   happens on unhandled errors, recalculation and threading rules: VBA UDFs on the main thread,
   multi-threaded recalculation interplay). Separate documented facts from inference, and propose
   black-box experiments for the inferences.
2. **The hosting interface CalcVBA should expose**, in host-neutral terms: types and lifetimes for
   loaded projects, procedure descriptors and signatures, bound callable handles with generations
   (reload without calling unloaded code), typed argument/return marshalling without per-call
   Variant boxing, error/exception reporting, budgets/cancellation for runaway code, threading
   affinity, and how a host supplies its object library for early binding (type library
   description, vtable layout, dual interfaces, events). How should this look so the same contract
   works with real COM on Windows and with a portable COM-ABI-compatible object model on Linux and
   wasm (where the interpreter must call host objects too)?
3. **The DNA Calc glue**: how to register module procedures as worksheet functions so that calls
   are resolved once at bind time and dispatched as typed calls; what metadata (volatility,
   thread-safety, side effects, argument demand such as Range vs value) the glue must derive or
   declare, and how; how DNA Calc's object library should be generated (one descriptor model
   generating a .tlb on Windows and portable vtables elsewhere) and whether reusing Excel's type
   library GUIDs/layouts (observed through public COM introspection of a running Excel) is wise.
4. **Critique** Appendices B-D: what is wrong, missing or will not perform; the riskiest
   assumption; the first three concrete steps you would take.

Be concrete and technical; cite documentation where you rely on it.

---

# Appendix A: owner principles (briefing extract)

4.5 **CalcVBA and DNA Calc.** CalcVBA is a separate repo and engine with nothing
spreadsheet-specific (no "UDF" concept). It exposes the right kind of external interface, as Office
VBA presumably does: load code, enumerate public module procedures and their signatures, give the
host callable entry points. DNA Calc has a glue library that registers shims for those as worksheet
functions, and builds and registers its own COM type library (Application, Range,
WorksheetFunction) that VBA code uses as it would Excel's. Owner, verbatim in substance:
- the UDF call path must be an **early-bound, high-performance call** from CalcFml/CalcFunc
  invocation through to JIT-compiled code; do not settle for "getting the right number across";
- the DNA Calc object library exposed to VBA needs **full early-binding COM through to JIT code**
  where appropriate and compatible; late binding is not a start to improve later, experience says
  that builds the wrong system; if early-bound COM is not fully baked in both interpreter and JIT,
  that is the priority before any late-bound path from DNA Calc;
- module procedures are not COM objects, but their exposure should be as close to COM as
  possible; Excel's VBA glue likely has a small non-COM step in registration and COM-like dispatch
  otherwise; understand and mirror it, do not overdo our own;
- both interfaces may be needed; Windows gets full compatibility; Linux and web a subset (no
  general COM references on Linux, no direct file I/O on the web).
Open: whether "no code cross-dependencies" forbids a Cargo dependency on a neutral CalcVBA hosting
API crate (so the boundary is a binary interface: C/COM ABI, wasm component), or allows the glue
crate alone to depend on it. Propose an answer with reasons.


# Appendix B: OxVba docs/spec/HOSTING_AND_EARLY_BINDING.md (HEAD)

# Hosting and early binding

2026-10-31. Owner direction; current code and bounded evidence, not full Windows parity.
Authority: `CHARTER.md`, `OXVBA_SYSTEM_CONTRACT_V1.md` HOST-SESSION-001,
COMP-BIND-001, COM-CLIENT-001 and JIT-CORE-001. Names remain OxVba; master only.

## Engine boundary

CalcVBA is an independent VBA engine. Worksheet functions, calculation dependencies,
volatility, cell errors, Range, Application and WorksheetFunction belong to DNA Calc's
glue/object library. DNA Calc publishes that library as COM type information and supplies
its objects to the engine. The engine must not contain an Excel-shaped substitute.
Windows targets full VBA/COM compatibility; Linux excludes general COM references;
web hosts expose a declared subset with no direct filesystem I/O.

## Current stock-take

| Capability | Interpreter (VM3) | JIT |
|---|---|---|
| Typelib import | Shared symbol/provider loader resolves registered/file-backed libraries, coclasses, dual partner interfaces, IIDs, member kinds, slots, bounds, parameter/wire types and defaults. Selection/provenance completeness remains open. | Same compiler metadata reaches OxIR. No separate JIT importer. |
| Native early calls before this change | `ComCallEarly` retained metadata in OxIR but passed only token/name through HAL; runtime rediscovered metadata and PreferVtable could fall back to Invoke. | Same token/name reduction through the project-member helper; no preserved-descriptor native contract. |
| Native early calls in this change | Descriptor reaches `ComHal::invoke_early_bound`; native x64 dual calls use declared IID/slot/signature, QI and shared vtable marshalling. No implicit Invoke retry. | Compiled call embeds an immutable member descriptor retained by CompiledImage and uses that same HAL/marshaller contract. |
| Dual/vtable signatures | Existing marshaller covers Automation scalars, strings, Variant, several object/array/record and ByRef shapes. New end-to-end proof is bounded to in-process scalar/string/Variant methods/properties, private ByRef temporaries and errors. | Same native marshaller and bounded proof; broad signature/proxy parity is still open. |
| Implements | Project-class interface routing and descriptors exist; bounded VM3-backed COM serving emits dual vtables. Imported-interface completeness and general serving identity remain open. | Internal project interface/class routing exists. General JIT-backed COM serving/generated export vtables are missing. |
| Events | Internal WithEvents/RaiseEvent plus native connection-point metadata/subscription/queued delivery. General synchronous native ByRef event writeback and safe same-VM reentry remain open. | Internal events execute; native event delivery/reentry/serving are not parity-complete. |
| Host objects | Native retained IDispatch roots can be supplied through ComHal; the host owns its library and activation. Existing HostInjected providers also exist. | Same host boundary; typed host calls use the preserved descriptor. |

Code anchors: `oxvba-com/src/windows_typelib_loader.rs`, `typelib.rs`,
`windows_invoke.rs`, `windows_vtable.rs`; `oxvba-symbol/src/provider.rs`;
`oxvba-bind`; `oxvba-oxir/src/com.rs`; VM3 `dispatch_early_com_method`;
JIT `emit_project_member_call_to_slot` and `invoke_foreign_member_to_slot_for_jit`.
Detailed evidence: [early_scalar.md](../evidence/programs/ideal-2026-07/windows-x64/WIN-4/early_scalar.md).

Object/Variant receivers and CallByName remain genuinely dynamic. A pure dispinterface
uses an explicit compiler-bound IID/DISPID/accessor Invoke plan, with no runtime name or
type-info lookup; it has no custom slots. This is separately tested on both backends.
Dual members never retry through that path. Missing/unsafe dual signatures fail before
entry. Custom IUnknown-only interfaces, general inherited/proxy transport, ByRef
aliasing/error-edge and named-argument breadth, nominal interface arrays, callbacks and JIT-backed COM exports remain open.
Portable registered objects can explicitly implement `PortableDispatch::invoke_early_bound`;
the default refuses typed calls. Legacy synthetic token arithmetic is not an early-binding target.
Unregistered headless projections still synthesize object/member numbers for dynamic
calls; replace that legacy path with explicit host services under `bd-59co.2.10.7`.
The JIT still uses Variant-backed execution frames/helpers; typed primary procedure entries
and a public, lifetime-safe native host-call ABI remain missing. Native transport alone
does not establish the required worksheet-to-JIT invocation cost or performance parity.

## What public Office contracts establish

[MS-VBAL projects](https://learn.microsoft.com/en-us/openspecs/microsoft_general_purpose_programming_languages/ms-vbal/4cd406c7-1ade-4522-8d7b-933183059ac7)
defines source/library/host projects, ordered references and host-introduced public entities.
[MS-OVBA](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/575462ba-bf67-4190-9fac-c275523c75fc)
defines project storage, references and modules; it is not an executable hosting ABI.
[Automation dual interfaces](https://learn.microsoft.com/en-us/windows/win32/com/dual-interfaces)
permit either direct vtable calls or Invoke. [TYPEKIND](https://learn.microsoft.com/en-us/windows/win32/api/oaidl/ne-oaidl-typekind)
distinguishes static module functions/data from interfaces and coclasses.
[ITypeInfo::AddressOfMember](https://learn.microsoft.com/en-us/windows/win32/api/oaidl/nf-oaidl-itypeinfo-addressofmember)
provides a COM type-information pattern for static callable addresses whose lifetime
is tied to the type description; it does not require inventing an instance receiver.
These contracts do not prove that Excel wraps each standard-module procedure in a COM
object, or reveal Excel's internal registration/call path. We must not invent that path.

Excel owns workbook/macro discovery, formula lookup, dependencies and argument/result
adaptation around VBA. Its documented [Application.Run](https://learn.microsoft.com/en-us/office/vba/api/excel.application.run)
accepts positional Variant arguments and returns the procedure result; it is a dynamic
host facility, not a required fast engine entry or a compile check.
The public [xlUDF C API](https://learn.microsoft.com/en-us/office/client-developer/excel/xludf)
also invokes VBA functions through Excel's function-reference/name adapter. Neither
API documents Excel's internal call ABI or proves per-procedure COM wrapping.
[Option Private Module](https://learn.microsoft.com/en-us/office/vba/language/reference/user-interface-help/option-private-statement)
restricts cross-project references while retaining within-project visibility.

Black-box observations on this machine use source injection into an owned workbook,
VBE Debug Compile command 578 before execution, and an armed PID-scoped UIA guardian.
No Office binary inspection, disassembly, security/Trust Center or registry changes.
The retained [probe](../evidence/programs/ideal-2026-07/windows-x64/WIN-4/hosting-public.bas)
observes same-workbook cell calls to Public and Private Functions; a Sub yields cell `0`
and Run yields Empty. Long parameters round `2.5` to `2`, accept text `"3"`, and reject
`"bad"` as error 2015 (#VALUE!). CVErr(2042) remains error 2042. A referenced cell arrives
as Range (TypeName), while VarType reflects its default numeric value (5); a literal is Double.
An explicit unhandled Err.Raise produced a captured runtime dialog and aborted the
instrumented run, so ordinary worksheet exception mapping is not established by that case.
The Option Private probe retains those within-workbook results. Public/Private Functions
also return 2/73 through explicitly workbook-qualified formulas in another workbook;
that host lookup is distinct from VBA project-reference visibility. Broader coercions,
class-module entry and ordinary unhandled-error mapping remain open observations.
The formulas were calculated while RunProbe was active; idle recalculation and
function-discovery UI visibility were not tested.

## Required host-neutral interface

1. Load code/projects plus ordered references and host-provided type libraries/root objects;
   verify the artifact and report compile diagnostics before publishing any call handles.
2. Enumerate public module procedures with stable project/module/procedure identity, kind
   (Function/Sub/accessor), visibility/Option Private, exact return/parameter types,
   ByVal/ByRef, Optional/default/Missing, ParamArray, array/UDT/interface identity and ABI.
   The host chooses which procedures its UI or formula language exposes.
3. Bind once to an opaque callable owned by a project session. Invoke the selected VM3
   procedure or typed JIT primary entry directly; retain code and state for the handle's
   lifetime. No per-call name search, source wrapper, COM-object fabrication or backend retry.
4. Define a versioned COM-compatible value/ownership/error convention and typed native
   entry signatures. Keep the checked universal Variant thunk as an explicit dynamic adapter.
   Standard-module procedures have no invented receiver, IUnknown identity or connection point.
5. Retain/reset/dispose sessions with globals, Static, Err and objects; define reload
   invalidation, thread/apartment ownership, budgets and reentry. Host errors and CVErr values
   stay distinct. Hosts own scheduling, formula conversion and registration lifetimes.

This mirrors COM's typed metadata, stable callable identity, HRESULT/error and ownership
discipline without pretending module procedures are COM objects. Excel's private glue ABI
is unknown; reproducing undocumented internals is unnecessary.

## oxvba-embed and UDF_EMBEDDING_V1 migration

`oxvba-embed` currently owns a useful function catalogue, persistent VM3/JIT sessions and
immutable-code cache, but also `Value::Range`, grid limits, SpreadsheetContext, a fake Excel
typelib and Application/Range/WorksheetFunction callbacks. Those are architectural debt.
Move all spreadsheet types/providers/readers/callbacks and formula error conversion into
DNA Calc's glue library. Replace the narrowed Function-only catalogue and name lookup with
the neutral procedure/signature/bound-callable API above, preserving the existing shared
session/compiler owners. Expose exact VBA/OLE values rather than a spreadsheet value enum.
Use DNA Calc's real dual typelib/native objects on Windows and an explicitly typed portable
host subset elsewhere. Do not build another late-bound spreadsheet object model here.

`UDF_EMBEDDING_V1.md` is a legacy consumer contract pending that extraction, not engine
architecture. Its scheduling/Range/Excel-error claims must move with the glue; portable
engine loading, metadata, session/callable lifetime and ABI belong in this document and
the shared hosting contract. Typed JIT entries remain CORE-7; cache/reentry remain CORE-8/CORE-5;
native COM residuals remain WIN-4. Broader work stays in-progress with explicit delivery beads.

# Appendix C: risk-vba.md (audit of the coupling)

# Risk lens: VBA coupling and binding model

Read-only audit, 2026-10-31. DnaCalc at `5a9348b` (C:\Work\DnaCalc\DnaCalc-review). OxVba read at
the consumed pin `c7ce1349` via `git show` (HEAD is `b05b8ef1`, one commit later; working tree
not read because another agent is active there). Paths below are relative to each repo.

Owner model, for reference: CalcVBA is host-neutral (no UDF term, no Excel types) and exposes
load / enumerate / callable entry. DNA Calc's glue registers shims and builds its own type library
(Application, Range, WorksheetFunction). Early binding both ways: formula -> JIT code by a fast
typed call; VBA -> host by vtable through a type library, in interpreter and JIT.

## Verdict

The current integration is a late-bound, name-keyed, `Value`-enum bridge whose spreadsheet
object model lives inside OxVba. Both directions contradict the owner model. The DnaCalc side
is small (about 1,100 lines) and cheap to unwind. The OxVba side is the problem: the only
embedding API (`oxvba-embed`) is spreadsheet-specific, and the backends lower even the
"early-bound" IR instruction to by-name dynamic dispatch. Every further dc-snz.5 /
bd-59co.2.10.6 increment (more Range members, Caller, WorksheetFunction methods) is being
added to the wrong repo on the wrong mechanism.

## Ranked findings (most building on the wrong shape first)

### 1. The Excel/DNA Calc object model is implemented inside OxVba (wrong repo, wrong owner)
- `crates/oxvba-embed/src/spreadsheet.rs` (635 lines) implements Application, Range and
  WorksheetFunction as a `ComHal` (`:416-557`): `Application.Caller`, `.Volatile`,
  `.WorksheetFunction` (`:486-511`), Range `Value2/Count/Row/Column/Address/Rows/Columns/Cells/Item`
  (`:292-367`), Excel's grid limits 1,048,576 x 16,384 (`:41,45`), error 1004 semantics
  (`:340,519-522`), and "Excel materializes an object-valued UDF result through its Range value"
  (`:275`).
- A hardcoded type-library resolver answers both `"DnaCalc"` and `"Excel"` with this surface
  (`:559-634`); `load_inner` injects `DnaCalc.Application` and a reference named `Excel`
  (`crates/oxvba-embed/src/lib.rs:127-152`). ProgID `DnaCalc.Application` and `DnaCalc.ReadOnly`
  are string literals in OxVba (`spreadsheet.rs:420,431,628`).
- Spreadsheet vocabulary is in the public API: `Value` enum doc "Spreadsheet values",
  `Value::Range`, `Value::Error(i32)` "Excel error code" (`lib.rs:20-38`), `Module::load_spreadsheet`
  (`:109`), `SpreadsheetContext` (`spreadsheet.rs:77-86`), `Error::HostRefused` "spreadsheet host
  refused" (`lib.rs:75-76`), project name `"UdfProject"` (`:119,512,559`).
- The spec is `docs/spec/UDF_EMBEDDING_V1.md` ("UDF value-call contract v1"), which tells the
  consumer to "register the admitted functions in the caller-owned calc-func registry / calc-fml
  resolver" and names `dc-snz.3`, `dc-snz.5`, OneCalc, CalcFunc evaluator callbacks.
- This contradicts OxVba's own charter: out of scope "Office application object model
  implementation (host-provided)" (`CHARTER.md:49`) and "Spreadsheet engine" (`:47`). It also
  reverses the May 2026 boundary contract (`docs/evidence/host_callable/NEUTRAL_DESCRIPTOR_MODEL.md:14,22`:
  "UDF-named options, shims, bridges ... are removal targets"; runtime must not own "a worksheet-UDF
  concept; Excel-specific caller semantics"). That neutral `VbaHost` facade
  (`crates/oxvba-host/src/vba_host.rs`, `invoke_callable_typed`) was deleted with the legacy stack
  (commit `2dbaec43`) and `oxvba-embed` (first commit `0113e7ee`, 7 commits) replaced it with a
  UDF-shaped API. The neutral design existed and was lost.
- Remaining spreadsheet traces elsewhere in OxVba core are test fixtures only
  (`oxvba-runtime/src/call_frame.rs:345`, Excel typelib tests); the coupling is concentrated in
  `oxvba-embed`, which keeps removal tractable.

### 2. VBA -> host calls are late-bound by string in both backends, even for "early" IR
- OxIR has the right instruction: `OxInst::ComCallEarly { method: ComMethodRef, ... }` carries a
  typed type-library descriptor (`crates/oxvba-oxir/src/inst.rs:230-254`), distinct from
  `ComCallLate` (`:255-262`).
- VM3 lowers it to a dynamic request: `TokenNamed { token: dispid, name: member_name.clone() }`
  through `dispatch_com_method` (`crates/oxvba-vm3/src/lib.rs:2175-2203`), which allocates a
  `DynamicCallRequest` with a `Vec<DynamicCallArg>` of boxed Variants and calls
  `host.com().dispatch_invoke_dynamic_variant_with_writebacks` (`:4161-4188`).
- The JIT lowers `ComCallEarly` and `ComCallLate` to the same generic helper
  `project_member_get_slot`, passing a name pointer and a token
  (`crates/oxvba-jit/src/lowering.rs:497-540,3307-3375`); the helper builds the same heap
  `DynamicCallRequest` with `name.to_string()` and calls the same `ComHal` dynamic entry
  (`crates/oxvba-jit/src/helpers.rs:4359-4383`). No compiled vtable call exists in the JIT.
- The spreadsheet host then ignores the token: `TokenNamed { name, .. }` -> `to_ascii_uppercase()`
  -> string `match` (`spreadsheet.rs:467-474,486-527`). Object handles are resolved through a
  `Mutex<Vec<Object>>` lookup (`:196-208`). Its synthetic type library declares
  `vtable_slot: None`, `is_dual: false`, all-Variant parameters, and overlapping tokens
  (Application 1-3, Range 1-8) (`:572-615`). The "type library" is decorative.
- The only real vtable path is the Windows x86_64 standard HAL: a runtime, per-call libffi
  slot call under a `PreferVtable` policy with IDispatch fallback (`crates/oxvba-com/src/windows_vtable.rs:1-22`,
  `windows_invoke.rs:2954-2990`, `crates/oxvba-hal/src/adapters/standard/com.rs:660-670`). It
  is reached only after the same dynamic request has been built, needs a live Windows COM
  object, and does not exist for an in-process Rust host object or on Linux/web. Non-Windows
  COM is an extended profile (`README.md:162`, `CHARTER.md:44`).
- `ComHal` (`crates/oxvba-hal/src/traits.rs:360-500`) is the only host-object extension point,
  and it is dynamic by construction. No host-supplied vtable/interface ABI exists.
- Correctly late: members called on a `Variant`/`Object` receiver are late-bound in Excel too.
  The gap is typed receivers (`r As Range`, `Application.`), which Excel binds early.

### 3. Formula -> VBA is a name-keyed, triple-converted call into a Variant-frame JIT
Path for `=Add2(A1,2)` (native, JIT selected), per call:
1. Compile: `Instruction::UserCall { name: String, argc }` (`crates/calc-fml/src/eval/mod.rs:235`,
   emitted at `compile.rs:1015-1022`). Plans keep names by design ("Plans retain names rather
   than engine objects", `docs/plan/VBA_UDFS.md:19`).
2. `Run::user_call`: `stack.split_off` allocates the argument Vec (`run.rs:266-267`);
   `functions.contains(name)` uppercases (alloc) and probes a BTreeMap (`:270`, `udf.rs:71-73`).
3. `UserFunctions::call_with_context`: uppercases again and probes the BTreeMap again
   (`udf.rs:86`), then a `dyn UserFunction` call.
4. calc-vba `call_with_context`: builds `ContextBridge`, maps each `CalcValue` to
   `oxvba_embed::Value` into a new Vec; text becomes a `String` from UTF-16 (`calc-vba/src/lib.rs:135-146,160`).
5. oxvba-embed: scoped TLS installs the context (`spreadsheet.rs:118-129`); the function is
   found by linear scan with `eq_ignore_ascii_case` on bare and qualified names (`lib.rs:392-400`);
   each arg becomes a `Variant` (`to_variant`, `:583-617`), is coerced (`coerce_argument`,
   `:526-553`) and pushed to another Vec (`:419-444`).
6. JIT entry: `invoke_in_session` -> `execute_in_session` allocates per-call `globals` and
   `program_images` Vecs and installs a proc-invoker bridge (`crates/oxvba-jit/src/runtime.rs:136-244`);
   `invoke_func` allocates a heap frame of Variant locals and clones the args into it
   (`:396-399`, `new_jit_frame` `:648-659`). There is no typed native entry point to call.
7. Return: `Variant` -> `SpreadsheetHost::result` -> `from_variant` -> `Value` (`lib.rs:499-502`)
   -> `from_vba` -> `CalcValue` (text re-encoded to UTF-16 twice, `calc-vba/src/lib.rs:200-204`).
   Errors are flattened to `String` at each layer (`.map_err(|e| e.to_string())`, `:127,146`)
   and mapped to `#VALUE!` (`run.rs:286-294`).

Count: 4 name lookups (compile `contains`; runtime `contains`; registry `get`; embed linear
scan), 2 uppercase allocations, 3 representation hops each way (CalcValue -> Value -> Variant ->
frame slot clone), and roughly 8-10 heap allocations for a two-Double call before any VBA code
runs. Inferred from reading; no benchmark exists. Zero of these are early-bound.

VBA -> host on `Range.Value2` inside that call adds: dynamic request (step 2 above) ->
uppercase match -> object-table lookup -> TLS context -> `ContextBridge::read_range` ->
`Sheets::reference` (linear sheet list, `calc-vba/src/lib.rs:443-459`) -> `values_of` ->
`CalcValue` -> `to_vba` -> `Value` -> `to_variant`. Callback `WorksheetFunction.Sum` resolves
by name again through `calc_func::catalogue().resolve_surface` (`run.rs:1900-1903`).

### 4. Host object identity has no lifetime and caps at 4,096 per loaded module
`SpreadsheetHost.objects` only grows (`spreadsheet.rs:147,177-189`); `MAX_OBJECTS = 4096`
(`:151`), then every new Range/Caller/Rows/Columns identity is refused. The host lives as long
as the session (`crates/oxvba-embed/src/cache.rs:20-29`). One module whose UDF reads
`Application.Caller` from more than 4,096 distinct cells would start refusing (inferred from
code, not tested). Identity is deduplicated by coordinates (`VBA_UDFS.md:33`), not reference
counted. With COM-style host objects (IUnknown refcount) this table disappears.

### 5. DnaCalc dependency surface (small, contained)
- Only `crates/calc-vba/Cargo.toml:15,18` depends on OxVba: `oxvba-embed` by git rev
  `c7ce1349`, `jit` feature on non-wasm. `Cargo.lock:1535-1718` pulls 17 OxVba crates.
- `apps/dna-onecalc/Cargo.toml:26` depends on calc-vba; `src/session.rs:338-348` calls
  `calc_vba::load_module`; also `surface.rs:469`, `fixture.rs:275,354`, `examples/vba.rs`.
- calc-engine, calc-host and calc-doc have no UDF/VBA wiring at all (grep clean). Workbook
  VBA hosting has not started, so nothing in the engine has to be unwound.
- calc-func `UserFunction`/`UserFunctionContext`/`UserFunctions` (`crates/calc-func/src/udf.rs:11-110`)
  and calc-fml `UserCallContext` (`run.rs:1862-1930`) are spreadsheet-side and engine-neutral
  in naming. Their shape is value-slice in, `Result<CalcValue, String>` out, name-keyed
  registry; that suits a late-bound adapter, not a typed shim. The refusal owner `"dc-snz.5"`
  is hardcoded in calc-fml (`run.rs:1879`).
- If "no code cross-dependencies" means no Cargo edge at all, the git pin itself must become
  a binary/ABI boundary. If it means only the glue crate depends on the neutral hosting API,
  today's single edge is already the right topology; only the contents are wrong. Worth an
  owner ruling.

## What is reusable
- `ComCallEarly` with typed `ComMethodRef` descriptors in OxIR (`inst.rs:230-254`) and the
  pluggable `oxvba_symbol::TypeLibResolver` at bind time (`embed/lib.rs:148-153`): a
  host-supplied type library can already reach the compiler. The missing parts are backend
  lowering and a host-object ABI.
- Windows type-library loader with dual/PSOA classification and the libffi vtable marshaller
  (`oxvba-com/src/windows_vtable.rs`, `windows_typelib_loader.rs:4763-4774`); `oxvba-comhost`
  dual-interface tier for exporting VBA classes.
- Procedure catalogue metadata (`Function`/`Parameter`, `embed/lib.rs:46-61,175-217`) is
  host-neutral apart from naming; optional/ParamArray/default folding (`:419-452`) is reusable.
- Behavioral evidence is mechanism-independent: Excel run-3 observations
  (`crates/calc-vba/tests/fixtures/excel-recalculation/observations.json`, 17 stages), volatility
  rules (`VBA_UDFS.md:64-100`), the interpreter/JIT parity tests. Keep these as the oracle.
- Per-call budget, state retention across calls, code cache keyed by target/ABI.

## What must be unwound to reach the owner model
OxVba (larger):
1. Delete `oxvba-embed/src/spreadsheet.rs`, `Value::Range`, `load_spreadsheet`,
   `SpreadsheetContext`, `HostRefused`, `"UdfProject"`, the `Excel`/`DnaCalc` resolver, and
   rename/replace `UDF_EMBEDDING_V1.md` with a neutral hosting contract (load project,
   enumerate public procedures and signatures, obtain a callable handle, register host type
   libraries and host object implementations). Restore the May neutral-contract boundary.
2. Define a host-object ABI that works without Windows COM: host-registered interfaces with
   vtable slots (Rust `extern "C"` tables), so a DnaCalc Range is reachable on Linux/web.
   Windows can additionally accept real COM objects.
3. Lower `ComCallEarly` to a direct slot call: JIT emits load-vtable / call-indirect with
   typed arguments; VM3 calls a resolved slot without building a name-carrying request.
   Keep `ComCallLate` for Variant/Object receivers.
4. Give the JIT typed native entry points per public procedure (scalar args in registers,
   no per-call frame/globals Vec setup) and hand the host a function pointer plus signature.

DnaCalc (smaller, about 1,100 lines: calc-vba 551, udf.rs 171, run.rs user-call path ~120,
OneCalc vba tests 353):
5. calc-vba becomes the glue: it owns the type library and the Application/Range/
   WorksheetFunction implementations (moved from OxVba), and generates typed shims.
6. Replace `Instruction::UserCall { name }` with a resolved handle (slot into a
   generation-stamped registry) so evaluation does no name lookup; reload bumps the generation
   instead of relying on names. This reverses the "plans retain names" decision in VBA_UDFS.md.
7. Replace `UserFunction::call(&[CalcValue]) -> Result<_, String>` with a typed shim entry and
   structured errors; keep a slow path for Variant/Range parameters.

## Risk if unchanged
- Each new Range member or WorksheetFunction method grows a string switch in OxVba that has
  to be moved and rewritten later; every test written against `oxvba_embed::Value` and
  `SpreadsheetContext` is mechanism-coupled.
- Performance ceiling: the formula -> VBA path cannot approach a direct call while plans hold
  names and the JIT takes a Variant heap frame; billion-node recalculation with UDFs is out of
  reach on this shape.
- The JIT gives no binding advantage on host calls today: its host calls are identical in
  shape to the interpreter's, so "native prefers JIT" buys only VBA-internal speed.

# Appendix D: architecture synthesis, section 1.7

### 1.7 CalcVBA hosting and early binding (owner 4.5)

- **State at OxVba 0ad2ad28** (corrects round 1, which read c7ce1349; Opus(r2), Fable(r2), owner note):
  `ComCallEarly` now carries the compiler's descriptor (IID, slot, wire signature) to
  `ComHal::invoke_early_bound` in VM3 (`oxvba-vm3/src/lib.rs:2181-2192, 4258-4300`) and the JIT
  (`oxvba-jit/src/helpers.rs:4397-4404`); native x64 dual members go to vtables with no Invoke retry;
  `docs/spec/HOSTING_AND_EARLY_BINDING.md` is the hosting contract. Remaining: per call a `Vec<DynamicCallArg>`
  of Variants and a `HashMap` (`lib.rs:4264-4287`), libffi for Windows vtables (`windows_vtable.rs:1-10`), a JIT
  helper call, Variant-backed JIT frames (CORE-7). The commit also adds 186 lines to `spreadsheet.rs`.
- Target: `calcvba-host-api` = OxVba's host-neutral interface, **with a typed-slot entry from the start**:
  calls into DNA's in-process `#[repr(C)]` objects with typed arguments, no Variant vector, no libffi, emitted
  as direct calls by the JIT; typed JIT primary entries for procedures; formula -> procedure through
  `CallableTable` slot + generation and a typed shim per signature (no intermediate `Value`).
- **Object library** in `calc-object-model`: descriptors -> generated vtables on every platform; UDF-time
  backend (reads, refuses writes, inside OneCalc's ceiling) and macro-time backend above calc-host. `Range`
  handles are value descriptors (`SheetId`, area) resolved against the active invocation, refusing when none
  is active, so a Range kept in module state outlives its call safely; allocation-free per call; no 4,096 cap
  (Opus(r2), Gemini(r2)). Windows: windows-core implements interfaces but does not generate `.tlb`; the
  pipeline is descriptor -> IDL/MIDL or `CreateTypeLib2`, with HRESULT/retval, optional arguments, BSTR and
  SAFEARRAY ownership, apartments and reentry specified (GPT(r2)).
- **Reload**: new generation, code leases, lazy rebind, state restarts, ordered against effectful units (1.3).
- **Composition**: calc-* libraries depend only on `calcvba-host-api`; each app binary (`dna-calc-worker`,
  `dna-onecalc`, native shells) has one composition edge to an OxVba runtime facade that supplies the provider
  factory (native: linked VM3 + JIT; browser: interpreter, wasm emission later), with ABI version negotiation
  and allocator ownership stated (Opus(r2), GPT(r2)).

# Appendix E: DNA Calc docs/plan/VBA_UDFS.md (measurements of Excel UDF recalculation)

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
