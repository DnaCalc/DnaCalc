# Harvest 13: hosting Excel add-ins (GPT-6 Pro answer, read against 1ece3d0f)

Reader: Opus, dc-6332.1, 2026-11-10. Read in full: `pro/13-addin-hosting-answer.md` (1,532 lines),
`pro/13-addin-hosting-prompt.md`, `PRO9_PROGRAMME.md`, beads dc-6332.10-.13 and dc-snz, and the code
named below. Nothing was built, run or changed except this file.

**Pinned code is current code for this topic.** `git diff --stat 1ece3d0f HEAD` over `crates/calc-xll-abi`,
`calc-xll-host`, `calc-xll-fixture`, `calc-xll-reference`, `calc-com-host`, `calc-addin-host` and `apps/` is
empty; the six crate commits since the pin touch only `calc-func` and `calc-value`. Every `file:line` below is
valid at both 1ece3d0f and HEAD.

**What the answer is.** It says (line 5, line 1445) that it never read the repository: every statement about our
code comes from the prompt's appendices. So its code claims are claims about the prompt's description, and where
the prompt simplified, the answer inherits the simplification. Section 2 checks each claim against the code
itself. The answer's C++ and C# were not compiled by its author (line 570, 1445); section 3 reads them.

Bead keys: **.10** = dc-6332.10 (steps 0-1), **.11** = dc-6332.11 (step 2), **.12** = dc-6332.12 (steps 3-4),
**.13** = dc-6332.13 (deferred steps 5-11). New bead proposals are in section 6 and are named N1-N6.

---

## 1. Disposition of every section

| Section | Content | Disposition |
|---|---|---|
| A.1 | Scope: modern x64 UDF hosting vs full C API vs broad COM; keep page score, ordered-boundary score, lifecycle score and feature matrix apart | **.10** (feeds C.1's four counts). No code. |
| A.2 | Per-framework table (Excel-DNA classic/.NET 6-8/AOT, xlw, xlwDotNet, xlOil core/plugins/Python/local) | No action as a work item; it is the reference for the D.x framework lanes. One factual point to keep: xlOil's served docs are 0.19.0, our binary is 0.22.1 (line 41), so `macro=True` default must be read from the binary. |
| A.3 | 18 corrections to Appendix D | **.10**: edit `docs/architecture/XLL_COM_HOSTING.md:62-104` (the source of Appendix D) and `:28-29`, `:271` (xlw 5). Item-by-item check in 2.3. |
| A.4 | S1-S32 status table with consequence and likelihood | Reference table. Rows split by step: S1, S2, S7 -> .10; S3 (64 part), S23 -> .11; S15, S22, S28 (Run), S31 -> .12; S4-S6, S10-S14, S16-S21, S24-S27, S29, S30 -> .13. Row check in 2.1. |
| A.5 | R1-R16 dispositions | R1, R4 -> .11; R3, R7 (state lifetime part) -> .10; R8, R11, R15 -> .12; R2, R5, R6, R9, R10, R12, R13, R14 -> .13; R16 no action (coverage statement). Check in 2.2. |
| B.1 | Typed read outcome, caller descriptor, invocation identity, three result channels, host-only `CallbackReply` | `CallbackReply`/`CallbackWrite` -> **.10** (step 0). `ReferenceRead::Pending` -> **.11**. `CallerDescriptor` (CSE, spill, macro) -> **.12** for the COM Caller use, rest **.13**. |
| B.2 | x64 layout table, type values, ABI-argument plan, registration as a state machine | Layout rows -> **.10** (they agree with our crate; add the missing assertions, 2.4). ABI-argument plan (O%, digit, `>`, F%/G%) -> **.13** step 7. Registration state machine -> **.13** step 6/10. |
| B.3 | Ownership table and six concrete changes | Changes 1-4 -> **.10** step 1 (xlFree mutation, FreeOnly frame, persistent allocation registry, no owner-arena pointer in worker replies). Change 5 (teardown while copying) -> **.10**. Change 6 (binary names) -> **.13**. |
| B.4 | Frame stack, permission table per context, table-driven precedence, refusal vs modelled denial | Refusal classification audit -> **.10** (real defect, 2.5 item 1). Permission table rows for cell/`#`/`$`/command/lifecycle/free-only/after-64 -> .10 (free-only) and .11 (after-64); event row -> .13. |
| B.5 | Exact status 64 sequence (7 steps), reevaluation policy, no promised entry total, native effects vs evaluation events, nested/async races | **.11** core. Async race part (handle valid before entry, staged completion) -> .13 step 5. |
| B.6 | Volatility as ordered events; `VolatilityDefaults { explicit_bang, reference_sensitive_macro }` | **.11** for the XLL adapter part; the engine reduction is already **dc-hsa5.20.2** (open, stream-linux). See N2. |
| B.7 | Threading: `$` may run on main; keep workers but stop the empty per-call host; reentry; never hold borrows across foreign code; constrained pump; thread-count profile | Per-call State lifetime -> **.10**. Constrained pump witness -> .10 (D.21 first part). Conformance thread-count setting and nested same-module reentry -> **.13**. |
| B.8 | Process isolation: session/runtime-group surrogate, boundary table, timeout is not termination, lease insufficiency | **.13** step 11 plus N4 (design note now, as the answer and dc-6332.13 both say). The lease point (late return is not quiescence) -> .13 with D.21 lease test. |
| B.9 | COM: schema-generated layouts, COM0-COM5 member order, OneCalc via narrow session view, no shadowing, both activation paths, no-Office lane | COM0-COM2 -> **.12** (narrowed for OneCalc, see 5.3). COM3-COM5 -> **.13**. No-Office lane -> N5. |
| B.10 | Incremental publication instead of quiet interval; activation reasons; split persistence identities; rebinding; accommodation records | Quiet interval (`apps/dna-onecalc-desktop/src/app.rs:362-410`) -> **.13** step 10, but cheap; see 2.5 item 7. Accommodation record format -> no action now (R13 is already three separate mechanisms). |
| C.1 | Comparison manifest; exact vs normalised table; four counts; Excel-to-Excel repeat before a partial-order comparator | **.10**, proposed as its own bead N1 (it is verifier tooling, not ABI work). |
| C.2 | Delivery order steps 0-11 | Steps 0-1 **.10**, step 2 **.11**, steps 3-4 **.12**, steps 5-11 **.13**. Step 2 is too large for one bead: split per N2. |
| C.3 | Return-code table 0,1,2,4,8,16,32,64,128,256,512 and write rules | Constants -> **.10** step 0. 64/1 behaviour -> .11. 512 -> no action (no cluster connector). |
| C.4 | Smallest command model, `CommandServices`, xlSet fill algorithm | Transport minimum (Run, SyncMacro zero, edit-aware LPenHelper) -> **.12**. Work minimum (xlSet, FORMULA, menus) -> **.13**, DNA Calc only: OneCalc offers no commands (5.3). |
| C.5 | RTD one topic key, ConnectData byref, no provisional value; native async state machine | RTD -> **.12** step 4. Async state machine -> **.13** step 5. |
| C.6 | Registration/name records, callable generations, `Deactivate` vs `UnloadNativeWhenQuiescent` | **.13** step 10, except the xlAutoRemove-on-unload divergence, which is measured already and cheap: **.10** (2.5 item 2). |
| D.0 | Fixture conventions, logging, sentinel `A5`, foundation inputs | **.10**. Port to our fixture conventions, not a new header (3.0). |
| D.1 | Status 64, cached `#` reads, pre-entry readiness | **.11** |
| D.2 | Sticky unwind, nested 64 via 255, DLLFree after 64 | **.11** |
| D.3 | Raw XLFree bits, xlFree pointer mutation, DLLFree/TLS returns, retained callback allocation | **.10** |
| D.4 | xlCoerce mask matrix and reference identity | **.10** runs it as a baseline capture; fixes are step 6, **.13** |
| D.5 | Counted strings, embedded NUL, integer types, input aliasing | **.10** (strings, aliasing); in-place parts belong to D.17, **.13** |
| D.6 | Dynamic `#` default volatility and ordered requests | **.11** |
| D.7 | Native async immediate/held/stale/batched | **.13** step 5 |
| D.8 | XLL calculation events, xlAbort | **.13** step 5 (see 2.4 on xlEventRegister and .11's text) |
| D.9 | Permission matrix with sentinels and precedence | Sentinel/precedence baseline over the existing 39-case matrix -> **.10**; the wider matrix -> **.13** step 6 |
| D.10 | Nested calls, origin restoration, general EVALUATE | **.13** step 6 (origin restoration already exists, 2.5 item 9) |
| D.11 | Caller shapes, CSE, spill, active sheet | **.12** (COM Caller and single cell); CSE/spill/name callers **.13** |
| D.12 | Registration forms, hidden names, use counts | **.13** step 6 |
| D.13 | Lifecycle routes, partial startup, ManagerInfo | **.13** step 10, except the UNREGISTER-route check (already traced) -> .10 |
| D.14 | RTD initial values, sharing, signal traffic | **.12** |
| D.15 | COM PIA/dynamic/identity/no-Office | **.12** (PIA and dynamic for the three OneCalc failures); no-Office and second-apartment -> N5 |
| D.16 | QueueAsMacro, edit mode, first real commands | Transport -> **.12**; PermissionWrites and xlSet fill -> **.13** |
| D.17 | Modern in-place and O% expansion | **.13** step 7 |
| D.18 | Legacy Excel4/XLOPER/xlAutoFree | **.13** step 8 |
| D.19 | Runtime coexistence, process identity, logical reload | **.13** step 10; links dc-snz.19, dc-snz.21 |
| D.20 | Saved `_xll.` names, missing providers, workbook-local Python | **.13** step 10 |
| D.21 | Reentry, owner pump, lease, GIL, crash containment | Owner-proxy pump witness -> **.10**; lease, GIL and crash parts -> **.13** step 11 |
| D.22 | Cheap probes: 16385, 16391/16403, 16392, number flags, LPenHelper, worksheet ABI, macro info, menus, binary names | 16385, 16391/16403, 16392, number flags -> **.10**; LPenHelper -> .12; macro info, menus, binary names -> .13 |
| D.23 | First batch: D.1, D.2, D.3, D.6, ED D.14, D.16 transport, then D.15 | Agrees with the bead split once D.3 is moved first (it is the cheapest and decides step 1). Order in section 7. |
| Source register | 41 Microsoft pages, 11 Excel-DNA, 2 xlw, 6 xlOil, 1 Python | No action as work. Use the links as citations when editing XLL_COM_HOSTING.md. Caveats the answer states itself: O05/O06 are xlOil 0.8.2, the xlOil docs are 0.19.0, ED links are current master, not the 1.9.0 we test. All are tier "public documentation / public framework code" under the prompt's rules; nothing here is Microsoft source. |

---

## 2. Claims about our code and data, checked at 1ece3d0f

### 2.1 The S table (A.4)

The "supplied host status" column reproduces the prompt. Checked against the code:

| ID | Answer's host status | Code | Verdict |
|---|---|---|---|
| S1 xlFree | pointer-key removal, outside frame 32 | `callbacks.rs:474-479` reads a copy of each operand (`read_unaligned`) and calls `free`, which removes the map entry (`:430-434`); the caller's operand is never written. No frame -> 32 (`:186-190`). | Agrees. The missing pointer write is real. |
| S2 xlStack | real stack headroom | `callbacks.rs:562-574`: `current - low`, clamped to `i32::MAX`, published as Int (`:387-391`). No 64 KiB cap. | Agrees. Note `crates/calc-xll-reference/DISCREPANCIES.md:12` (DISC-005) still says xlStack "refuses": stale. |
| S3 xlCoerce | partial, en-US, no XLFree bit | `callbacks.rs:582-605`, `coerce` at `:870-923`: target compared by equality, not as a mask (`:871-879`); text->number in a fixed en-US environment (`:884-895`); Int target truncates (`:896-898`); a pending read fails with 32 (`:594-599`). | Agrees, with one correction to D.4: a target of `xltypeRef` does **not** return an SRef untouched. `from_oper` turns the SRef into a `CalcValue` reference and `publish` re-encodes it as `xltypeRef` with a sheet id and a new table (`:591-593`, `:414-427`). So the predicted "wrong base kind" does not occur; whether Excel also converts is still for D.4. |
| S4 xlSet | absent | No arm; falls to `other` -> `worksheet_name(16387)` fails -> 2 plus a typed refusal (`:849-855`). | Agrees. |
| S5 | impl | `callbacks.rs:276-342` | Agrees. |
| S6 xlAbort | always false | `:556-561` | Agrees. |
| S7 xlGetInst / xlGetInstPtr | old absent, ptr impl | 16403 at `:206-222` (BigData, module handle, count 0). 16391 has no arm: returns 2 **and records a typed refusal** (`:849-855`), which poisons the calling formula. | Agrees on status. The refusal side effect matters: if Excel x64 fails 16391 (answer A.3.10), our cell becomes a refusal where Excel's cell shows whatever the add-in returns. Fix in .10 after the D.22 witness. |
| S8 xlGetHwnd | impl | `:619-622`, `:367-377` (HWND truncated to i32) | Agrees. |
| S9 xlGetName | current module | `:550-555` uses `frame.module.path`, i.e. the module named in `xlfRegister` (the code module), not `frame.origin` (the XLL whose open routine registered it). | Agrees on "current module", but the answer does not know which one we report. For xlOil (code module `xlOil.dll`, origin the .xll) this is a live question; add to D.10. |
| S10 | absent | no arms | Agrees. |
| S11 xlGetFmlaInfo | LPenHelper only | shim `platform/calc-xll-shim/src/bridge.rs:23-31` sets mode 0; Excel12 route 16398 -> 2 + refusal | Agrees. |
| S12 xlAsyncReturn | token, 256, zeroed result | `callbacks.rs:436-467` | Agrees. Batched form: a Multi handle fails the BigData test (`:441-443`) -> 256. |
| S13 xlEventRegister | stored, never raised, `$` 128 | stored in `State.events` keyed `(origin handle, event)` (`:647-663`, `native.rs:759`); read nowhere except module removal (`native.rs:897-899`); `$` -> 128 (`callbacks.rs:489-494`); success publishes `xltypeInt 1` (`:387-391`, `:662`). | Agrees. The result payload is a recorded Excel difference already (`XLL_COM_HOSTING.md:538-540`). |
| S14 | false | `:623-626` | Agrees. |
| S15 xlfCaller | 1x1 SRef, `#REF!` in command | `:393-413`, `:575-581` | Agrees. |
| S16 xlfRegister | measured Form 1 subset; four-argument minimum | `:979-1075`; `< 4` -> 4 (`:980-982`); from a worksheet function -> 2 (`:983-988`); macro type defaults to 1 (`:1008`); empty type text accepted only for macro type 2 (`:1012-1017`). | Agrees: Form 2 (module only) and omitted-type auto-registration both fail with 4 today. |
| S17 xlfUnregister | ID use counts only | `:664-678`; a text (module form) argument -> 32 | Agrees. |
| S18 xlfSetName | clear only | `:679-695`; a definition -> 2 + refusal | Agrees. |
| S19 | CALL subset | `call.rs` | Agrees (not re-checked in depth). |
| S20 xlUDF | "denies policy changes; blanket same-module refusal blocks composition" | `:758-805`: refuses command targets (`:780-785`) and a change of cached-read mode (`:786-792`); `$` -> 128 (`:501-508`). It calls the child adapter directly (`:793-804`), so the evaluator's state-lane reentry refusal (`calc-fml/src/eval/run.rs:760-772`) is **not** on this route. | **Partly wrong.** There is no same-module refusal on the xlUDF route. The lane refusal exists for nested user calls made through the evaluator. |
| S21 xlfEvaluate | constant only, hidden IDs, `$` 128 | `:741-757`; `$` -> 128 in the preamble (`:199-201`) | Agrees. |
| S22 xlfRtd | values work, traffic fails | `:806-846`; remote server refused; **non-text topics refused** (`:814-818`) | Agrees, and adds a gap the answer did not name: F.4 measured that the number 1 and the text "1" are one topic, but our xlfRtd route refuses a numeric topic outright. D.14's `=RtdProbe(1)` will be a refusal in the host. -> .12. |
| S23 xlfVolatile | permission measured; engine sticky OR | preamble `:195-198` (2 from any non-`#` signature, before the `$` check); `#` in a cell -> `set_volatile` (`:627-645`); lifecycle/command frames have no context, so 0 and no effect. Engine: `calc-engine/src/recalc.rs:2694-2702`, `2880-2890`, `4842-4846`, `5657-5661` keep `volatile || built_in`. UDF requests themselves are last-wins (`calc-fml/src/eval/run.rs:4249-4253`). | Agrees. The "sticky OR" is specifically the OR of the plan's syntactic token with the UDF's last request. |
| S24 GET.WORKSPACE | items 2, 37 | `:606-618`; other items -> 2 + refusal | Agrees. |
| S25 REFTEXT | single area | `:696-740`; also `require(2)`, so the one-argument form returns 4 | Agrees, plus the one-argument gap. -> .13. |
| S26 MESSAGE | `$` 128, ordinary 2 | preamble `:199-204` | Agrees. |
| S27 | catalogue route; 614 FILTER | `:849-865`, `invoke.rs:596-607` | Agrees. |
| S28 commands | denials from cells, absent elsewhere | from a cell: `:480-488` -> 2 **plus a refusal**; from command or open routine: no arm -> 2 + refusal | Agrees on codes. The refusal on the measured cell denial is the classification defect of 2.5 item 1. |
| S29 | absent | no arms | Agrees. |
| S30 Excel4 | 32 | `platform/calc-xll-shim/src/variadic.c` (Excel4/Excel4v return 32) | Agrees. |
| S31 LPenHelper | always ready | `bridge.rs:23-31` | Agrees. |
| S32 XLCallVer | 3072 | `bridge.rs:3-6` | Agrees. |

### 2.2 The R items (A.5)

| ID | Answer | Check | Verdict |
|---|---|---|---|
| R1 | confirmed structural gap; typed not-ready reads | `UserCallContext::values_of` stores the fault and returns `Err(String)` (`calc-fml/src/eval/run.rs:4287-4325`); xlCoerce maps it to 32 (`callbacks.rs:594-599`) and `refuse()` keeps the MissingInput fault (`get_or_insert`, `run.rs:4262-4267`). `XllFunction` has no `reevaluation_policy` (`native.rs:1261-1331`). The engine route exists: `calc-engine/src/recalc.rs:5993-6003`, exercised by the `LateRead` double in `calc-host/tests/round_trip.rs:8921-8960`. The pending/invalid distinction exists in the reader: `pending_producer` (`run.rs:2341`, used at `:4270-4278`). | Agrees. The seam work is smaller than the answer assumes: the reader already knows "pending". |
| R2 | confirmed, with origin rule | as S13 | Agrees. |
| R3 | premise unproved; real defect is xlFree mutation | `publish` never sets XLFree (`callbacks.rs:414-427`); returned XLFree is honoured (`invoke.rs:362-364`) | Agrees with the answer: our prompt's R3 premise ("Excel's documented contract is that it sets that bit") is the doubtful part. Measure raw bits (D.3) before touching flags. |
| R4 | confirmed | `signature.rs:169` (`volatile |= macro_equivalent && any R/U`), applied at call start for every call (`invoke.rs:129-133`) | Agrees. The call-start event already exists; only its condition is static. |
| R5 | ABI gap confirmed, xlw claim corrected | `invoke.rs:21-53` (no cif for C D F G K O P R), `:138-146` (in-place results refused) | Agrees. |
| R6 | containment gap | `catch_unwind` only (`callbacks.rs:169`, `worker.rs:166`) | Agrees. |
| R7 | four threads legal as a profile; per-call State a lifetime risk | `worker.rs:30-33` (1..4), `:174-193` (empty private State per call), `:217-220` (a retained callback allocation turns into the refusal "worker retained callback storage" and the arena is then dropped with the State) | Agrees, and the lifetime risk is concrete: an add-in that keeps a callback string past return (legal until it calls xlFree) gets a refusal, and any later read of that string reads freed memory. |
| R8 | caller must be a tagged descriptor | `UserFunctionContext::caller() -> Option<CellKey>` (`calc-func/src/udf.rs:260`) | Agrees. |
| R9 | mask, locale, errors | as S3 | Agrees. |
| R10 | mixed | `State.names` is one case-folded map (`native.rs:754`) | Agrees. |
| R11 | feature gap, not queue failure | Run works (`native.rs:987-1021`) | Agrees. |
| R12 | route-dependent | none of `xlAddInManagerInfo12`, `xlAutoAdd`, `xlAutoRegister12` is referenced in any crate | Agrees. |
| R13 | three policies | already separate: xlOil hash profile (`native.rs:111`), AOT runtime reservation (`native.rs:350`), env switch (`calc-addin-host/src/lib.rs:40`) | Agrees; nothing to split. |
| R14 | split by contract | as S7, S10 | Agrees. |
| R15 | needs Caller/Range semantics, not all of Excel | `calc-com-host/src/projection.cpp`; README says no type information (`README.md:18`) | Agrees; see 5.3 for the OneCalc limit. |
| R16 | coverage statement | - | No action. |

### 2.3 The A.3 corrections

| # | Correction | Verdict and where it applies |
|---|---|---|
| 1 | type inventories are examples; "one of `! # $`" wrong | Correct: `XLL_COM_HOSTING.md:67` says it; our parser already accepts `!` with `#` or `$` (`signature.rs:73-88`, test `"QE!"` fixture, measured `Q!#`). Edit the doc. |
| 2 | `#` + declared reference is not unconditional volatility | Correct (R4). Also `XLL_COM_HOSTING.md:68` ("`#` + R/U arguments makes the function volatile by default") needs "when a reference is actually passed". |
| 3 | do not set XLFree universally; xlFree nulls the pointer | Direction agrees with code (2.2 R3). The "nulls the pointer" fact is cited to Microsoft's xlFree page [M03]; I could not open it here. D.3 is the witness. |
| 4 | "every framework calls the executable export" too categorical | Correct for `XLL_COM_HOSTING.md:81`. Our own fixtures do call it directly (`fixture.rs` xlAutoOpen; `native/owned.cpp:46`). Reword to "the frameworks we traced". |
| 5 | app directory does not always win | Correct for `:82`. The host never loads the shim itself; XLLs are loaded with `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | DEFAULT_DIRS` (`native.rs:438-441`, flags `0x1100`), and an XLL that loads XLCALL32 itself uses the normal order. Cheap hardening: the desktop exe loads its own XLCALL32.DLL by full path before the first XLL and asserts the bound path. -> .10. |
| 6 | old Excel-DNA cluster path uses the default AppDomain | Plausible from public loader code; not checkable here. Add to `:87-90` as "[public-code, unmeasured]"; D.19. |
| 7 | xlfRegister has a module argument; xlGetName 16393 vs xlfGetName 107 | Correct. `:92` groups xlfRegister with calls that "carry no module argument". The prompt's own Appendix C also wrote `xlfGetName` where it meant `xlGetName`. Edit the doc. |
| 8 | one .NET runtime per process too broad | Correct for `:102-103`: one hosted CoreCLR, plus side-by-side .NET Framework. |
| 9 | dynamic-code needs are framework dependent | Fine; `:99-101` reword. No code. |
| 10 | xlGetInst expected to fail on x64 | Cited [M15]; plausible. Our 2 + refusal (S7) must change to Excel's measured status without a refusal. D.22. |
| 11 | xlStack 64 KiB cap | Cited [M17]; I cannot confirm the cap from memory. Our value is uncapped (S2). D.22 decides. |
| 12 | xlAutoAdd/xlAutoRemove are manager actions | **Correct and already measured against us**: on UNREGISTER, Excel entered only `xlAutoClose` (`crates/calc-xll-reference/traces/2026-11-01-x2/aot/excel-traced/boundary.jsonl:46`), the host entered `xlAutoRemove` (with its re-registrations) and then `xlAutoClose` (`.../aot/dna-traced/boundary.jsonl:43-87`). Cause: `native.rs:1188` calls both on every unload and close. See 2.5 item 2. |
| 13 | calculation events have an origin rule | Matches our own design note (`XLL_COM_HOSTING.md:44`, "not raised on programmatic recalc [V doc]"). No change; D.8. |
| 14 | thread-safe async is valid | Correct: refusal is ours (`invoke.rs:71-75`, `native.rs:1301-1306`), not the parser's (`signature.rs:155-157` accepts `>EX$`). Say so in the doc. |
| 15 | "xlw 5 cannot run" wrong as family claim | Correct for `XLL_COM_HOSTING.md:28-29`, `:271` and our prompt's R5. |
| 16 | COM minima feature dependent; probe Window.Parent class | Fine. |
| 17 | refusing a crashing reload is an intentional divergence, not parity | Correct, and it matches our honesty rule (refused counts as fail). Our prompt's A.6 ("the refusal is parity, not a bug") overstated it. Reword in `ONECALC_ADDINS.md`/`XLL_COM_HOSTING.md:541-543` where it is described. |
| 18 | validate `#&` | Correct: `signature.rs:158-160` checks only `#$`. Measure `Q#&`/`QU#&` in D.12 before rejecting. -> .13 (registration), the measurement can ride the .10 batch. |

### 2.4 The named topics

- **16384 operand mutation.** Host: never writes the caller's operand (`callbacks.rs:474-479`). The answer's fix (null the pointer field, keep other bytes) is right if D.3 confirms it. Our tracer cannot show it today: `native/trace.cpp:549-556` records MdCallBack12 arguments at entry only, and the return record (`:637-639`) has only the result operand. Either add `args_after` for 16384 in the tracer (preferred, so the comparison sees it) or rely on the fixture's own post-free log (D.3 does both).
- **16385 xlStack.** Uncapped headroom (`callbacks.rs:562-574`). Cheap to change once D.22 gives Excel's number on this machine (in ordinary, `#`, `$`, command and outside frame).
- **16391 xlGetInst.** 2 + typed refusal (`:849-855`). Needs an explicit arm returning Excel's measured status and output with **no** refusal.
- **Status 64.** No constant (`calc-xll-abi/src/lib.rs:26-32` has 0,2,4,8,32,128,256; also missing 1, 16, 512). No reevaluation policy. Engine support exists (2.2 R1). One trap for the implementer: `calc-fml/src/callable.rs:285-293` forces `parallel = 1` for any callable whose `reevaluation_policy()` is `Some`, and `:160-164` excludes it from early execution. Giving `$` XLL functions `XllUncalcedUnwind` would therefore silently remove the measured pool concurrency (F.3 two-worker overlap witness, F.5 concurrent pool calls). Step 2 must change those two predicates to treat `XllUncalcedUnwind` separately from `ProvisionalArguments`, with the P03 test proving the unwind still works in a parallel segment.
- **Static `#` volatility.** `signature.rs:169`; consumers of `signature.volatile`: `invoke.rs:129-133` (call-start event), `native.rs:738` (descriptor shown as "volatile" in help), `native.rs:949` (early execution refused), `native.rs:964-967` (pool capacity), `signature.rs:194` (test). Splitting the flag touches all five.
- **xlEventRegister.** As S13. Raising events is step 5 (D.8). dc-6332.11's text says it "includes xlEventRegister and xlbitXLFree where the batch shows Excel's order"; D.1/D.2/D.6 do not exercise events, so that clause should move to .13. What .11 (or .10) can do cheaply is the recorded result-payload difference (`XLL_COM_HOSTING.md:538-540`): read Excel's result operand from the existing X3 traces and publish the same.
- **xlbitXLFree.** Host never sets it on callback results; honours it on returns (`invoke.rs:362-364`). The answer is right that the prompt's R3 premise is the weak part. D.3 measures raw bits per callback before any wrapper. -> .10.
- **Stale VOLATILITY.md.** `docs/architecture/VOLATILITY.md:101` ("DNA's `callbacks.rs` accepts it everywhere") and `:158-159` ("(DNA answers it everywhere)") are stale: `callbacks.rs:195-198` returns 2 outside `#`. Also `:154` cites `recalc.rs:2613-2619` and `5343-5346`; the OR now sits at `recalc.rs:2694-2702`, `2880-2890`, `4842-4846`, `5657-5661` (four sites, not two). -> .10, doc only.

### 2.5 Further code facts found while checking (not in the answer, or only hinted)

1. **Measured denials poison the formula.** `dispatch` treats the preamble denials (237/186 from non-`#`, MESSAGE, EVALUATE from `$`) as modelled, without `refuse()` (`callbacks.rs:192-205`). But the denials inside `run()` (xlc from a cell -> 2, `:480-488`; 16401/379/255/257/149/201/88/186 from `$` -> 128, `:489-516`) go through `refuse()` (`:243-246`), which sets a not-modelled fault on the formula. Where these are measured Excel rules (S28's cell denial is called "measured" in the prompt), the add-in sees the right code but the cell becomes a refusal instead of the add-in's own result. B.4's last paragraph asks for exactly this audit. -> .10, check each against `fixture/x5-permission-cases.json`.
2. **xlAutoRemove on every unload** (`native.rs:1188`), contradicted by the traces cited in 2.3 #12. One-line change (call only `xlAutoClose` on the UNREGISTER/unload route; keep `xlAutoRemove` for a future manager-removal route). Re-run the X2 AOT and net48 lifecycle comparisons. -> .10 (measured, cheap), though it is formally step 10.
3. **xlAutoFree12 runs inside the worksheet frame.** `convert_return` is called at `invoke.rs:276` while the guard from `:252` is alive (dropped at `:280`), so callbacks made from xlAutoFree12 get full worksheet permissions. B.3 change 2 (FreeOnly frame). -> .10.
4. **Worker callback storage** (2.2 R7). -> .10.
5. **Input buffers are already isolated.** Every call marshals from `CalcValue` into a per-call `Arena` (`invoke.rs:153`, `calc-xll-abi/src/value.rs:113-191`); nothing engine-owned is shared with native code. Step 1's "isolated mutable input buffers" needs a test (an export that swaps two rows of a Multi input), not new code.
6. **Frame restoration is already a stack.** `enter()` saves the previous TLS pointer and `FrameGuard::drop` restores it (`callbacks.rs:42-53`). D.10's prediction "a single unstacked pointer makes parent-after report the child" should not occur; what is missing is a frame *kind* (worksheet, command, lifecycle, free-only, event, after-64 latch).
7. **Load completion by quiet interval** is in the app, not the host: `apps/dna-onecalc-desktop/src/app.rs:362-363` (800 ms quiet, 60 s limit), `:396-410`; it also calls `after_calculate()` while settling, which raises COM AfterCalculate during a load. B.10's incremental publication fits OneCalc easily because the table is already republished per version. -> .13 step 10.
8. **Synchronous `>` and digit returns are half built.** `convert_return` has the `>`-with-pointer-first-argument path and the digit path (`invoke.rs:372-402`) but `start_inline` refuses both before the call (`:138-146`). Step 7 can start from that code.
9. **xlUDF children** run with the parent's context and are not recorded as evaluator invocations (`callbacks.rs:793-804`), so their entries are not in `UserFunctionEffects.invocations`. Relevant to D.2's nested-64 case and to D.10. -> .11 note, .13 work.
10. **Our fixture shares the host's ABI crate.** `calc-xll-fixture` depends on `calc-xll-abi` (`Cargo.toml`), so a layout assertion inside it checks the host against itself. The answer's warning (D.0, line 627) applies to us: the Excel trace (H.1) is the only independent layout witness. The two C++ fixtures (`native/owned.cpp`, `P03/native-probe.cpp`) declare their own layouts and are independent.

### 2.6 Claims about the prompt's data

All agree with the prompt and its sources: the 350/353 page score dominated by 289 own-fixture cases (`ONECALC_ADDINS.md`, G.2); F.4's Excel-DNA RTD numbers (10/16 client, 0/26 native, 20/0-of-22 delayed); the 16-thread Excel `$` witness (F.1) against our cap of 4; the `>B` SyncMacro VT_R8 0 result; the 32-zero-byte xlAsyncReturn output; the AppEvents argument order. Two disagreements:

- D.7 predicts Excel-DNA's managed native-async registration as `>BX$`/`>BX`. The prompt's own X0 measurement (F.1) shows Excel-DNA 1.9.0 registering `Async(double, handle)` as `>EX`. The answer does say "capture the actual registration"; expect `E`, not `B`.
- The prompt itself is stale in one place the answer could not see: A.1 says OneCalc may not depend on calc-doc. `tools/xtask/architecture.json:80-87` (owner ruling 2026-11-07) allows calc-doc; only calc-engine and calc-host are forbidden.

---

## 3. The proposed experiments

### 3.0 Common reading of the answer's fixture code

The answer's C++ will not compile against any of our fixtures as written. Common reasons:

- `#include "fixture_owned_abi.hpp"` does not exist. Our native fixtures each declare their own layout: `crates/calc-xll-reference/native/owned.cpp` (`struct Oper { value.number/string/integer; type }`) and `crates/calc-engine/tests/fixtures/excel-calc-probes/P03/native-probe.cpp` (`struct Operand { value.number/text/integer/single/reference/array/binary; type }`). The answer uses the SDK spellings `XLOPER12`, `xltype`, `val.num`, `val.w`, `val.xbool`, `val.str`, `val.array`, `val.sref.ref.rwFirst`, `FP12::array`, `xltypeNum`, `xlbitDLLFree`. A header that recreates those names field for field comes close to copying XLCALL.H's text, which the source rules forbid. Port the bodies onto P03's `Operand`, `call()` and `log()` helpers (they already match the answer's `Call()` and `Mark()/Trace()`), or onto the Rust fixture, instead of adding an alias header (extend, never duplicate).
- `Excel12v(fn, result, count, args)` is not linked by our fixtures; they resolve `MdCallBack12` from the main image (`owned.cpp:46`; P03 `call()`), argument order `(fn, count, args, result)`. Only the answer's `Call()` wrapper needs changing. This route works in both Excel and the host.
- Helpers are declared but not defined: `Mark`, `Trace`, `BuildFixtureCase`, `TrialReleaseEvent`, `RetainThreadUntilExit`, `FixtureBarrierBeforeCallback`, `NeverSignaledTrialEvent`, `ControlledProgId`, `TrialStartupSucceeds`, `BigDataFromBytes/Handle/SignedCount`, `RecordFourBytes`, `Trace4`, `Oper4`, `Fp4`. P03's `log()` (fixed record array, `GetTickCount`) and the Rust fixture's overlap barrier (`OVERLAP`, `OVERLAP_READY`) cover most of them.
- Build: our fixtures use `cl /std:c++17 /EHsc /W4 /LD` (`build.ps1:27`); unused parameters (D.21 `CallbackNumber`) give C4100 warnings, not errors.
- Where an experiment needs a second `xlAutoFree12` with a different allocator, use a separate module (the Rust fixture and `owned.cpp` both export one already).
- Turning on extra registrations already has a convention in the Rust fixture: marker files beside the XLL (`worker-tests.enable`, `async-tests.enable`) and `DNA_XLL_*` variables (`fixture.rs:301-330`). Case files go beside the existing ones in `crates/calc-xll-reference/fixture/` (x1-x12 naming).

### 3.1 Per experiment

| D | Fixture it needs | Code reading | What it measures | Bead |
|---|---|---|---|---|
| D.0 | Port of the helpers to P03 `native-probe.cpp` (or a new section of the Rust fixture); tracer `args_after` for 16384 | `Call()`'s `const_cast<Op**>(args.begin())` is valid; `Text` uses `wcslen` on literals only (fine, as stated); `ReturnNumber` returns thread-local storage (fine; xlw-like). Foundation inputs need a workbook with blank, other-sheet and two-area references: P03's workbook generator can produce them. | Raw descriptor bytes with A5 sentinels; layout witness from Excel's side | .10 |
| D.1 | P03 native probe already has `BU`, `BU$`, `BU#` (P03 README); add `QU`, `QU#`, `QU$`, `QQ`, `QUQ`, `BB` Late. Excel-DNA `ED_TRY_READ`/`ED_THROW_READ` in `fixture/Probe.cs` (X0) | Correct. `XlCall.TryExcel(int, out object, params object[])` and `XlCallException.xlReturn` are Excel-DNA's public names; check against the pinned 1.9.0 package. | 64 vs 32 at the callback, entry count, retry thread, `#` cached value, value-parameter readiness before entry | .11 |
| D.2 | Separate small C++ module (its own `xlAutoFree12` with `delete`); parent `QU` calling 255 | Correct; `new Op` + `delete` in the same module is consistent. Note the host never records 255 children as evaluator invocations (2.5 item 9). | Whether Excel calls xlAutoFree12 for a result returned after 64; outer status of 255 when the child gets 64; prefix effects kept | .11 |
| D.3 | P03 probe or Rust fixture (`QU#`, `Q`); a DLLFree string module (extend `owned.cpp`, which already returns a DLLFree counted string); a TLS-pool module | `RawOwned` frees the same operand twice: harmless in both Excel (if the pointer is nulled) and the host (second free misses the map). `ReturnHostOwned` returns thread-local storage with XLFree set: valid. | Raw XLFree bits per callback; pointer nulled or not after xlFree; DLLFree pointer/thread order under two overlapping `$` calls; retained callback allocation across entries (expect the host's "worker retained callback storage" refusal, `worker.rs:217-220`) | .10 |
| D.4 | Same probe module; `QUQ#`, `QUJ#`, ordinary and `$` variants; a decimal-comma locale run | Correct. Our host compares the target by equality, so every combined mask returns 32 (`callbacks.rs:871-879`). The SRef->Ref prediction is moot (2.1 S3). | Mask semantics, locale parsing, Int rounding vs our truncation (`:896-898`), Missing/Nil/Err sources | .10 (capture), fixes .13 |
| D.5 | Rust fixture already has `QQ` echo (`MyEcho`), `C%`/`D%`/`F%`/`G%` exports (`MyCatW`, `MyUpperW` ...); add a producer for special operands, an embedded-NUL string and a row-swapping Multi mutator | `CountedEcho` returns the borrowed input pointer: legal as a test; the host copies before dropping the arena. `CountedText` (`QD%`) correct. | UTF-16 unit preservation, embedded NUL, isolated surrogates, Int/Nil distinctions through CalcValue, input aliasing | .10 |
| D.6 | `QUB#`, `QUB!#` exports; reuse the E-bank (VOLATILITY probes, `excel-calc-probes/volatility/`) formulas | Correct. | Call-start `#` default only when a reference is passed; ordered last-wins; interaction with `!`; `ca` bits after save | .11 |
| D.7 | Native async exports `>EX`, `>EX$`; external release event; Excel-DNA `ExcelAsyncHandle` | `NativeHeld` copies the handle by value into `Job`: fine. `ExcelAsyncHandle.SetResult` returning `bool`: check the 1.9.0 API. Expected registration is `>EX`, not `>BX` (2.6). The host refuses `>EX$` and answers 256 to any batched (Multi) handle (`callbacks.rs:441-443`). | Immediate completion during entry, Esc while held, duplicate, batch atomicity, completion then 64 | .13 |
| D.8 | `CalcEnded`/`CalcCanceled` exports registered with 16401 in the open routine; a looping `xlAbort` function | Correct; event numbers 1/2 match our host's accepted set (`callbacks.rs:652-654`). | Which triggers raise XLL events, order, permissions inside handlers; xlAbort retain/clear | .13 |
| D.9 | Rust fixture's `MyXlf`/`MyXlfRaw` (arbitrary function number and operands, `fixture.rs:954-1002`) already do this for worksheet-built operands; add `BuildFixtureCase` for Int/Ref operands built inside the frame | Correct apart from the undefined builder. | Status, output untouched vs written (sentinel), denial precedence per context | .10 (sentinel pass over the existing 39 cases), .13 (rest) |
| D.10 | `QQ`/`QQ#` parent and `QQ$` child, a child in a second module, a split origin/code-module registration | Correct. | xlGetName before/after nested 255 (expect parent: frames restore), module vs origin reported, general EVALUATE forms | .13 |
| D.11 | `Q`, `Q#`, `Q$` CallerRaw; VBA `CallerShape` (add to `fixture/vba/DnaExamples.bas`) | Correct. | Caller SRef vs CSE/spill/name/command forms; COM Caller Count for CSE | .12 (single cell, COM), .13 (rest) |
| D.12 | Command that registers `RegTarget` 7-operand vectors | `RegisterAlias` returns sentinel bytes when 16393 fails; the caller must check the type before using it as an ID. Otherwise correct; order of operands matches Form 1. | Duplicate IDs, use counts vs hidden names, omitted name/type, macro type 0, `#&`, `$&`, ordinals | .13 |
| D.13 | Two builds (startup ok / fail), lifecycle exports, ManagerInfo | Correct. Our host already rolls back on a 0 return (`native.rs:854-871`); compare. | Export order per route; partial-startup leftovers; manager vs UNREGISTER | .13 (the UNREGISTER part is already traced: 2.3 #12) |
| D.14 | Controlled RTD server in `owned.cpp` (`DNA_X5_RTD`, `X5Native64.xll`) with ConnectData 17 then 23; the original Excel-DNA RTD corpus (`fixture/x8-exceldna-rtd*.json`) | `RtdProbe` sets XLFree on a numeric result (no-op; harmless). `=RtdProbe(1)` will be refused by our host (non-text topic, `callbacks.rs:814-818`). | Initial value without a provisional `#N/A` (`invoke.rs:303-308` invents one), topic identity, signal and disconnect order, native entry counts | .12 |
| D.15 | X0/X2 Excel-DNA projects with `ExcelDna.Interop` | `app.Caller[Type.Missing]` and `app.Range["A1:B2", Type.Missing]` compile in C# 4+ for PIA indexed properties. The `dynamic` method needs `Microsoft.CSharp` referenced in the net48 project. `ErrorWrapper` will not match: Excel errors in `Value2` arrive as boxed `Int32` (e.g. -2146826246), so that branch is dead but harmless. | Early-bound slot vs dispatch traffic for Version/Hwnd/Caller/Range.Value2; QI identity; cross-apartment marshalling; no-Office machine | .12 (PIA/dynamic), N5 (identity, apartments, no-Office) |
| D.16 | `fixture/Probe.cs` queue methods; C++ `PermCell`/`PermCommand` | Correct; passing NULL results is valid. `PermCommand`'s SRef B1 uses zero-based column 1: right. | Queue body count and timing vs edit mode, `>B` double zero, retry; then cell vs command write permissions and xlSet fill | .12 (transport), .13 (writes) |
| D.17 | In-place exports | Correct prototypes: O% expands to `INT32* rows, INT32* cols, double*`; F%/G% returning a pointer is ignored on x64. `InplaceFp` uses SDK field `array` (ours `values`). Our parser accepts all of these (`signature.rs`); the host refuses them before entry (`invoke.rs:138-146`, `:21-53`). | Which buffer is the result; row-major order; argument expansion | .13 |
| D.18 | Legacy fixture with its own XLOPER layout; Excel4v | Correct in shape. Our shim's Excel4v returns 32 for everything. | Legacy layout, Excel4 statuses, xlAutoFree vs xlAutoFree12 | .13 |
| D.19 | `fixture/Identity.cs` (X2Identity) extended | Correct C#. | AppDomain/default-domain, runtime selection, logical reload | .13 (with dc-snz.19, dc-snz.21) |
| D.20 | `SavedCounter` export; xlOil Python local modules | Correct. | `_xll.` persistence, missing provider, late load in Manual mode, local VBA stubs | .13 |
| D.21 | Rust fixture barrier (`MyOverlap`); Python companion via `ctypes.PyDLL`; fault probe | Correct. `RaiseException(EXCEPTION_ACCESS_VIOLATION, EXCEPTION_NONCONTINUABLE, ...)` is fine for a disposable process. | Owner pump under a pending worker callback; lease after last completion; GIL deadlock; containment | .10 (pump witness only), .13 (rest) |
| D.22 | `MyXlf` arbitrary-call export covers 16385, 16391, 16403, 16392, the 0x2000 flag; a command for binary names | Correct. Note our dispatcher does not strip function-number flag bits: `0x2000|SUM` falls to the unknown-callback arm (2 + refusal). | Excel's xlStack value, xlGetInst status, handle relations, flag-bit handling | .10 (first four), .12 (LPenHelper), .13 (rest) |
| D.23 | - | - | Batch order | section 7 |

---

## 4. Work lists for steps 0-2

For one Opus implementer on Windows, when local Rust work is allowed. Read AGENTS.md first. Every list ends
with the light check and a comparison run (`xtask app xll-accept onecalc` and the X1/X2/X3 traced comparisons
through `crates/calc-xll-reference/run.ps1`/`verify.py`); known-good traces must stay equal.

### Step 0 (dc-6332.10, part 1): keep the boundary exact

1. `crates/calc-xll-abi/src/lib.rs`
   - Add return codes `ABORT = 1`, `STACK_OVERFLOW = 16`, `UNCALCED = 64`, `NOT_CLUSTER_SAFE = 512`; type `XL_FLOW = 0x20`;
     function numbers used as magic numbers in the host (`XL_SET 16387`, `XL_SHEET_ID 16388`, `XL_SHEET_NM 16389`,
     `XL_GET_INST 16391`, `XL_GET_HWND 16392`, `XL_GET_FMLA_INFO 16398`, `XL_EVENT_REGISTER 16401`,
     `XL_RUNNING_ON_CLUSTER 16402`, `XL_GET_INST_PTR 16403`, `XLF_VOLATILE 237`, `XLF_GET_WORKSPACE 186`,
     `XLF_REFTEXT 146`, `XLF_RTD 379`, `XLC_MESSAGE 32890`) and the number flags `0x8000`, `0x4000`, `0x2000`, `0x1000`.
   - Extend `documented_offsets_and_sizes` (`:166-188`): `XlArray12.columns` at 12, `Fp12.columns` at 4, `XlSRef12` size,
     `XlMRef12` stride 16, `XL_BIG_DATA & XL_STR != 0` composite (so no code tests `& XL_STR` for strings).
2. `crates/calc-xll-host/src/callbacks.rs`
   - Replace the magic numbers with the constants.
   - Introduce a host-internal `CallbackReply { status: i32, write: Unchanged | ZeroAll32 | Operand(XlOper12 + allocation) , refusal: Option<String> }`;
     make `dispatch` (`:163-251`) apply it in one place. Keep `async_return` writing `ZeroAll32`.
   - Refusal audit (2.5 item 1): mark each early return in `run()` (`:480-516`) as modelled denial (no `refuse()`) or
     unsupported capability (keep `refuse()`), using `crates/calc-xll-reference/fixture/x5-permission-cases.json` and the
     F.3 matrix. Write the decision per function number in a comment table at the top of `run()`.
   - Snapshot-before-conversion: in `convert_return` (`invoke.rs:339-366`) keep the raw 32 bytes and flags in the
     host diagnostics before `from_oper`.
3. `crates/calc-xll-reference/native/trace.cpp`: on MdCallBack12 return (`:637-639`) also record the argument operands
   for 16384 (and any callback whose operands are caller-writable) as `args_after`; teach `verify.py` to compare them.
4. Docs, no code: `docs/architecture/XLL_COM_HOSTING.md:62-104`, `:28-29`, `:271` with the A.3 corrections (2.3);
   `docs/architecture/VOLATILITY.md:101`, `:154`, `:158-159` (2.4); `crates/calc-xll-reference/DISCREPANCIES.md:12`
   (DISC-005 xlStack is implemented, not refused); `crates/calc-com-host/README.md:21` ("does not deliver workbook events"
   predates X4's delivered AppEvents: check and correct).
5. C.1 manifest and four counts: in `verify.py` and the summary JSON (see N1 if split off).
6. Excel batch (one owned session, through the existing `run.ps1`): D.0 foundation operands, D.3, D.22 rows 16385/16391/16403/16392/flag bit,
   D.9 sentinel pass over the 39 permission cases.

### Step 1 (dc-6332.10, part 2): ownership and persistent call context

1. `crates/calc-xll-host/src/callbacks.rs:474-479`: after removing the allocation, null the pointer field of the caller's
   operand with an unaligned write of the field only (`string`, `array.values`, `reference.table`), keeping all other
   bytes, **only if** D.3 shows Excel does so. Decide also from D.3 whether operands we do not own are touched.
2. Frame kinds: add `kind: FrameKind { Worksheet, Command, Lifecycle, FreeOnly }` to `Frame` (`:16-40`); derive it where
   frames are built (`invoke.rs:242-251`, `native.rs:848`, `:1190`). In `dispatch`, a `FreeOnly` frame allows only 16384;
   other numbers return the status D.3's auto-free probe shows.
3. `crates/calc-xll-host/src/invoke.rs:347-361`: call `xlAutoFree12` inside a nested `FreeOnly` frame (origin and module of
   the registration), not under the worksheet guard.
4. `crates/calc-xll-host/src/worker.rs:174-221` and `native.rs:760`, `:777-780`: move callback allocations out of the
   per-call `State` into a registry shared by owner and workers (`Arc<Mutex<..>>`, keyed by pointer plus a generation,
   owner module, kind). Remove the "worker retained callback storage" refusal once D.3 shows Excel's lifetime; release at
   xlFree or module removal (`native.rs:900-902`). Callback operands for a worker are built on the worker from owned values
   (B.3 change 4): check `PoolCall::service` (`worker.rs:256-305`) never sends an owner-arena pointer.
5. `crates/calc-xll-host/src/native.rs:1188`: unload/close call `xlAutoClose` only (2.3 #12, trace evidence). Keep the
   `xlAutoRemove` call for a later explicit manager-removal route. Re-run the X2 AOT/net48 and X3 lifecycle comparisons.
6. 16385 (`callbacks.rs:562-574`) and 16391 (new arm, no refusal) to the D.22 values.
7. Shim binding (A.3 #5): `apps/dna-onecalc-desktop` and `apps/dna-calc-desktop` load their own `XLCALL32.DLL` by full path
   before the first XLL and record the bound path in diagnostics.
8. Tests (Rust, native-only): xlFree twice on one operand; xlAutoFree12 calling a non-free callback gets the FreeOnly status;
   a `$` export that keeps a callback string across two entries (new Rust fixture export, marker-file group); a Multi
   input mutated by the add-in leaves the engine value unchanged (2.5 item 5); xlGetName after a nested 255 reports the parent.
9. Excel batch: D.5 strings/aliasing, D.21 owner-pump witness (two `$` readers through the owner proxy, `MyOverlap` barrier).

### Step 2 (dc-6332.11): status 64 and invocation-time volatility

1. `crates/calc-func/src/udf.rs`: add a typed read outcome to `UserFunctionContext`, for example
   `fn read_reference(&mut self, r: &ReferenceLike) -> Result<ReferenceRead, String>` with
   `ReferenceRead::{Value(CalcValue), Pending}` and a default implementation over `values_of` (so VBA and doubles keep working).
2. `crates/calc-fml/src/eval/run.rs:4287-4325`: implement it in `UserCallContext` with `self.run.reader.pending_producer(cell)`
   (as `argument_values_of` already does at `:4270-4278`), without storing a not-modelled fault for `Pending`.
3. `crates/calc-xll-host/src/worker.rs`: carry the outcome through `Request::Read` and `Reader::values_of` (`:66-73`, `:92-98`, `:269-275`).
4. `crates/calc-xll-host/src/callbacks.rs:589-600`: on `Pending`, call `context.request_reevaluation(XllUncalcedUnwind)`,
   set an after-64 latch on the frame (from then on only 16384 is served; others return the status D.2's boundary trial shows),
   return 64 with the result left as D.1 shows, and do not call `refuse()`.
5. `crates/calc-xll-host/src/invoke.rs:266-279`: when the latch is set, do not convert the returned operand; apply the D.2
   discard cleanup (xlAutoFree12 or not).
6. `crates/calc-xll-host/src/native.rs:1261-1331`: `reevaluation_policy()` returns `Some(XllUncalcedUnwind)` for
   non-`#` synchronous registrations (async after D.7).
7. `crates/calc-fml/src/callable.rs:160-164`, `:285-293`: keep pool parallelism and early execution for `XllUncalcedUnwind`
   (they are refused today for any reevaluation policy). Prove with the P03 `LateRead` test (`calc-host/tests/round_trip.rs:8921`)
   run in a parallel segment and the existing overlap witness.
8. Volatility, adapter side: split `XllSignature.volatile` (`calc-xll-abi/src/signature.rs:57`, `:169`) into
   `explicit_bang` and `reference_sensitive_macro`; at `invoke.rs:129-133` emit `set_volatile(true)` for `!`, and for `#`
   only when a U/R slot actually receives a reference (check during the marshalling loop at `:161-235`). Update
   `native.rs:738`, `:949`, `:964-967` and `signature.rs:194`.
9. Volatility, engine side: the `volatile || built_in` OR at `calc-engine/src/recalc.rs:2694-2702`, `2880-2890`,
   `4842-4846`, `5657-5661` is owned by **dc-hsa5.20.2**; do not duplicate it here (N2).
10. Excel batch: D.1 (on P03's workbooks first), D.2, D.6, with the C.1 manifest and a repeated Excel control.

---

## 5. Where the answer is wrong, overstated, or against recorded direction

### 5.1 Wrong or overstated about our code (it could not read it)

- S20 "blanket same-module refusal" on xlUDF: not on that route (2.1).
- D.4's SRef-unchanged prediction: the host already converts to Ref (2.1 S3).
- D.10's unstacked-pointer prediction: frames already restore (2.5 item 6).
- B.3 "isolated mutable input buffers" as new work: already per-call copies (2.5 item 5).
- Step 2 says "ordinary/`$` XLL reevaluation policy" without noting that our callable table drops parallelism for any
  reevaluation policy (2.4); done naively, it would lose a measured behaviour.
- B.2's layout table is labelled [recollection]; it agrees with ours line for line, so it adds nothing beyond the missing assertions.

### 5.2 Wrong or doubtful about Excel or frameworks

- D.7's expected Excel-DNA async registration `>BX`; measured `>EX` (2.6).
- A.3 #11 (xlStack 64 KiB cap) and #10 (xlGetInst fails) are cited, not measured; treat as hypotheses for D.22.
- D.15's `ErrorWrapper` branch will never fire for `Value2` errors.
- The answer is careful elsewhere: it does not invent unknown Excel outcomes, and it repeatedly says which are unmeasured
  (D.23's last paragraph is a good list of them).

### 5.3 Against recorded owner direction

- **Commands in OneCalc.** C.4 has OneCalc provide `CommandServices` with "bounded scratch edits", and D.16's writes run
  in the host. `docs/architecture/ONECALC_ADDINS.md:51` (trust rule 4): commands, ribbons, menus and other add-in features
  are not offered in OneCalc. The command *transport* that UDFs rely on (Excel-DNA QueueAsMacro, `Application.Run` of
  SyncMacro, xlOil `xloRunInXLLContext`) stays; xlSet, FORMULA, menus and ON.KEY are DNA Calc work (dc-snz phase 2).
- **Object model in OneCalc.** COM2 lists `Worksheet.Range/Cells`, `Range.Value2`, `Union/Intersect`, `ActiveCell`, `Selection`.
  Owner ruling (`ONECALC_ADDINS.md:122`): OneCalc carries no full Excel object model, and in OneCalc `Range.Value` reads are
  typed refusals (`ONECALC_ADDINS.md:156`, `docs/plan/VBA_UDFS.md:218`). For the three acceptance failures, OneCalc needs
  `Application.Caller` and `Application.Range` returning Range objects with type identity, `Address`, `Row`, `Column`,
  `Count`; not `Value2`. The decision of what these calls return belongs to dc-snz.5's open question; the orchestrator can
  decide it (owner delegates decisions, 2026-11-08). The answer's own line 420 ("only expose values where the scratch
  session actually has values") is compatible with this narrowing.
- **OneCalc's engine.** The answer correctly keeps OneCalc engine-free (A.1, B.9). It has no conflict with the calc-doc
  allowance; its surrogate (B.8) for OneCalc hosts add-ins only, which the dependency ceiling allows.
- **CalcVBA early binding.** B.9 and D.15 test our COM server through late binding as well as PIA and `#import`. That is a
  test of how add-ins reach us, not a CalcVBA design choice, so it does not conflict with "early-bound COM first, never
  late binding as a start" (CalcVBA architecture); keep early-bound slot tests first in the order, as COM0 does.
- **Refusals.** C.1's "classify expected Excel denials separately from unsupported-host refusals and intentional safety
  divergences" is compatible with "refused counts as fail" only if all three still count as fail in the headline. Write
  the four counts so the headline stays matched vs (mismatched + refused + diverged).
- **Process.** C.2 wants each step "not declared compatible until the same binary agrees with Excel". That is our existing
  measure-don't-assert rule; it needs no new ceremony (no per-step receipts beyond the comparison summary).

---

## 6. Proposed new beads

**N1. XLL comparison profile manifest and four counts (verifier).** Extend `crates/calc-xll-reference/verify.py` and its
summary JSON with the C.1 manifest (Excel build, bitness, locale, calc mode, MTR threads, entry vs recalc trigger, add-in and
workbook hashes, load order, process-identity adaptation) and report four counts side by side: value cases, ordered boundary
cases, lifecycle/ownership cases, framework feature lanes, each as matched vs (mismatched + refused + missing). An Excel-to-Excel
repeat is recorded before any comparison treats ordering as nondeterministic. Part of dc-6332.10's text today; split because it
is tooling work that steps 0-4 all use. Windows. Blocks nothing in the code.

**N2. XLL invocation-time volatility events (adapter side of R4).** Split from dc-6332.11. Replace the parser's combined
`volatile` flag with `explicit_bang` and `reference_sensitive_macro`, emit the `#` call-start event only when a reference is
actually passed in a U/R slot, and update the four consumers (help descriptor, early execution, pool capacity, call start).
Oracle: D.6. Depends on dc-hsa5.20.2 for the engine's ordered reduction (the `volatile || built_in` OR at four sites in
`recalc.rs`), which stays the one owner of that change. dc-6332.11 then keeps status 64 only.

**N3. xlFree, xlAutoFree12 and callback-storage lifetime (step 1 core).** Optional split of dc-6332.10 if the Opus lane wants
two closes: step 0 (constants, CallbackReply, refusal audit, tracer args_after, docs) and step 1 (xlFree mutation, FreeOnly
frames, shared allocation registry, xlAutoClose-only unload, 16385/16391). Both use one D.3/D.22 Excel batch.

**N4. Add-in surrogate: boundary design note.** Design only, no code: the B.8 table turned into our own document under
`docs/architecture/` (session or runtime-group surrogate, what stays local, bidirectional callbacks with parent IDs, caller
descriptors, async incarnation in tokens, RTD/COM in the surrogate's STA, controller document checkpoint, no automatic replay).
Feeds dc-snz.21 (xlOil safe reload) and dc-snz.19 (AOT). dc-6332.13 says this starts during steps 0-2; it fits no step bead.

**N5. COM identity, apartments and a no-Office lane.** From B.9 and D.15's second half: canonical IUnknown identity for
Window/Parent, type information (`GetTypeInfo`; the README says none is exposed), a second-STA client through
`CoMarshalInterThreadInterfaceInStream`, and one acceptance run on a machine without Office. Too large for dc-6332.12 and
not needed for the three OneCalc failures.

**N6. xlfRtd numeric topics.** Small: `callbacks.rs:814-818` refuses a non-text topic, while F.4 measured that the number 1
and the text "1" are one topic. Coerce topics as the typed `RTD()` route does. Can be folded into dc-6332.12 instead of
a bead.

Items to add to dc-6332.13's list (no new bead): incremental registration publication instead of the 800 ms quiet interval;
the one-argument REFTEXT form; xlGetName module vs origin for split registrations; `#&` rejection after measurement;
function-number flag bits; batched xlAsyncReturn; xlUDF children not recorded as invocations; settle loop raising
AfterCalculate during load.

---

## 7. Top five actions by expected value

1. **One cheap Excel batch first: D.3 + D.22 (16385, 16391, 16403, flag bits) + the D.9 sentinel pass** on the existing
   fixtures. It decides xlFree mutation, raw XLFree bits, auto-free permissions and the two cheap callbacks, i.e. most of
   step 1. -> dc-6332.10.
2. **Status 64 on the existing engine route**: typed `Pending` read from `pending_producer`, 64 plus
   `request_reevaluation(XllUncalcedUnwind)` in xlCoerce, after-64 latch, and keep pool parallelism in `callable.rs`.
   Oracle D.1/D.2 on the P03 workbooks. -> dc-6332.11.
3. **Ownership fixes**: xlAutoFree12 in a FreeOnly frame, a callback-allocation registry that outlives the worker's State,
   xlFree pointer write (per D.3), and the refusal-classification audit so measured denials stop poisoning formulas.
   -> dc-6332.10.
4. **Unload calls xlAutoClose only** (`native.rs:1188`): a measured lifecycle divergence (X2 AOT traces) with a one-line fix,
   plus the A.3 doc corrections and the stale VOLATILITY.md lines. -> dc-6332.10.
5. **Exact RTD protocol against the original Excel-DNA corpus**: no invented first `#N/A` (`invoke.rs:303-308`), numeric
   topics (N6), signal/disconnect order. It is the largest recorded failure count in the add-in area (16 client and 26 native
   positions). -> dc-6332.12.
