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
