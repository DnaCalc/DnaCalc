# Harvest 14: one Excel object model through VBA, COM/XLL and JavaScript

Reader: Opus, 2026-11-12. Answer: `pro/14-object-model-and-js-answer.md` (843 lines); prompt:
`pro/14-object-model-and-js-prompt.md` (1,400 lines). Both read in full.

Pins: DnaCalc 3b5aa86b (2026-11-12 09:01); CalcVBA 8115aa78 (read-only checkout in the session scratchpad).
`git diff --stat 3b5aa86b HEAD` is empty for `crates/calc-vba`, `calc-host-vba`, `calc-com-host`,
`calc-host/src`, `calc-addin-host`, `calc-xll-host`, `calc-macro-security`, `apps/dna-calc-desktop` and the
Cargo files. Master (eccea1ab) changed only `apps/dna-calc*`, `dna-calc-worker/src/server.rs`, calc-host tests
and the oracle worker. So every `file:line` below holds at both commits.

How the checks were done: `git show 3b5aa86b:<path>` of the code named in the answer, plus the code it could
not see (the DNA Calc desktop composition, calc-addin-host, the calc-fml reference tracker, the event sources
in calc-vba, calc-host's event dispatch). I read the P22 Excel probe (`crates/calc-engine/tests/fixtures/
excel-calc-probes/P22/eng9-20261103/`) and the VBA recalculation probe (`crates/calc-vba/tests/fixtures/
excel-recalculation/`). I searched `.beads/issues.jsonl` (read only). One-line Python only for the bead search;
no other scripts. No cargo, no Excel, no tracker writes, no other file changed.

The answer says (line 8) it used only the packet. Its code claims are therefore claims about the excerpts. Where
the packet left code out, section 3 says what that code does.

---

## 1. Findings that change how the answer should be used

1. **The answer's identity design (3.3) already exists for the UDF route; extend it, do not build `ReferenceKey`.**
   `calc-fml` has a live Range tracker: "Same-thread live Range identities ... A structural edit updates live
   handles ... Deleted handles never revive on Undo" (`crates/calc-fml/src/reference/tracked.rs:1-2`). It has an
   owner identity per workbook, `apply(StructuralEdit)` (`:55-82`) and `retain`/`resolve` (`:83-160`).
   `StoredReference::{Inline, Tracked{owner, slot}}` and `ReferenceHandleError::{ForeignWorkbook, ClosedWorkbook,
   Deleted, ExpiredView, NotModelledEdit}` are in `crates/calc-func/src/udf.rs:255-277`. The host applies the
   transform at structural commit (`crates/calc-host/src/session.rs:1395`). VBA UDF Ranges are stored as these
   handles (`crates/calc-vba/src/lib.rs:888-913`, `:1011-1060`). The gap is narrower than the answer assumes.
   Event-handler Ranges are tied to one event lease and give error 91 after it (`crates/calc-vba/src/events/
   source.rs:469-489`). The COM shell's Range is a copied (row, column) (`crates/calc-com-host/src/
   projection.cpp:61`, `:603-606`). So there are three lifetime contracts, not none. One contract means moving the
   event and COM routes onto the tracker.

2. **The answer's top P1 risk (the local Book1 shell standing in for the real workbook) is current product
   behaviour, not a risk.** The DNA Calc desktop app builds its add-in set with `const WORKBOOK: &str = "Book1"`
   and `active_sheet: None` (`apps/dna-calc-desktop/src/app.rs:134`, `:763-770`), whatever file is open. The
   interactive app never calls `sheet_changed` or `after_calculate`. Only the `--engine` harness does
   (`apps/dna-calc-desktop/src/engine.rs:375`, `:419`, `:663`). So native add-ins in DNA Calc see a workbook
   named Book1 and get no SheetChange or AfterCalculate events. EnableEvents is two flags: the COM shell's
   (`projection.cpp:37`, `:241-244`, checked in `fire` at `:515`) and the session's (`crates/calc-host/src/
   events.rs:107-112`), which is what VBA sets (`crates/calc-host-vba/src/lib.rs:574-582`).

3. **E4 is largely measured already; the code follows the measurement.** P22 (Excel 16.0 build 20527, 70/70
   trials) says: "Ordinary automatic COM writes deliver Calculate before Change and Change sees new formula
   caches; Manual Change sees held caches" (`.../P22/eng9-20261103/README.md:7`). The `automatic-change-nested`
   trace shows synchronous nesting: Change A1 enters, Change D1 enters and exits, then
   `Application_SheetChange S!D1`, then A1 exits, then `Application_SheetChange S!A1`. `session.rs:1406-1447`
   raises Change before calculation for UI edits and after a completed synchronous calculation for Automation
   edits. When the Automation calculation does not finish, Change is not raised; it is recorded as an event
   failure (`events.rs:176-191`). The answer's "Change before automatic calculation is a hypothesis for E4"
   (3.7 step 4) is out of date for COM writes. UI order, handler errors and the async case are still open. The
   bead text of dc-hsa5.12 ("Change after authored commit and before automatic calculation") is stale in the
   same way.

4. **E3 branch A is measured; the "UDF read conflation" P1 is wrong.** The VBA recalculation probe's
   `HiddenProbe` reads `Range("Z1").Value2` without an argument (`crates/calc-vba/tests/fixtures/
   excel-recalculation/probe.bas:38-41`). After Z1 changes 30 to 31, `Application.Calculate` leaves it at "4:30";
   only `Range("B12").Calculate` gives "5:31" (`observations.json` steps `hidden-input` and `hidden-force`;
   `probe.bas:147-157`). So Excel leaves the hidden read untracked: the answer's H2. Our evaluator already has
   separate argument, hidden and cached-observation reads (`crates/calc-fml/src/eval/run.rs:4307-4382`), not a
   conflation. One limit: DNA refuses this probe cell today, because `ThisWorkbook` is not modelled
   (`crates/calc-host-vba/tests/excel-recalculation.known-differences.txt:1-4`).

5. **A VBA UDF that touches more than 4,096 distinct Range objects fails, for the rest of the project's life.**
   `SpreadsheetHost` interns every `Object::Range(range, axis)` by value. It only ever inserts
   (`crates/calc-vba/src/runtime/spreadsheet.rs:360-387`). `release_object_variant` returns Empty for DNA objects
   and removes nothing (`:963-968`). Each `Cells(i, j)` makes a new object (`:674-697`). So a loop over 5,000
   cells is refused at the 4,097th distinct cell, and every later call that needs a new one is refused too. The
   answer treats this as "not proof of a leak" (section 2). The code settles it: the limit is cumulative.
   Interning by value also makes `Range("A1") Is Range("A1")` True in DNA. From recollection, Excel answers
   False; E1 must check it.

6. **The answer's "put the core in calc-host" needs a trait seam that it does not mention.** calc-com-host,
   calc-xll-host and calc-addin-host do not depend on calc-host, and must not: OneCalc's desktop uses them, and
   the dependency ceiling forbids calc-host there (`AGENTS.md:76-77`; `tools/xtask/architecture.json`, rule
   `dna-onecalc`). So the full-host projection reaches the session through a bridge trait defined in
   calc-com-host (and calc-vba's existing `EventReadContext`/`SpreadsheetContext`). DNA Calc implements it over
   `HostSession`; OneCalc implements it as values-only refusals. calc-host-vba already plays this role for VBA
   ("Shared application composition of CalcHost and CalcVBA", `crates/calc-host-vba/src/lib.rs:1-2`).

7. **The null-EXCEPINFO split is real but minor (P3, not P1). The bigger early/late mismatch is the vtable.**
   `needs_workbook` returns `DISP_E_MEMBERNOTFOUND` without an EXCEPINFO (`projection.cpp:382-383`). But an
   early-bound PIA call of `Application.Caller` (slot 74, per `docs/architecture/XLL_COM_HOSTING.md:1344`) reaches
   the one-argument stub `unsupported(Object*)` that fills all unset slots (`projection.cpp:465`, `:475-489`). It
   gets `E_NOTIMPL`, not error 1004 "needs a workbook". That stub is harmless on x64 only because the caller
   cleans the stack; an x86 build would make it unsafe.

Owner-rule conflicts, stated plainly:
- The JS design (3.10) proposes a `DnaExcel` API with "proposed DNA behavior", to be adapted to Office.js later.
  Under bit-match-Excel-always, the JS surface follows Office.js's observable behaviour once J1-J5 measure it.
  There is no DNA dialect with its own semantics, such as hydrating reads before a rejected sync if Office.js does
  not.
- J1-J5 need a sideloaded Office add-in. The usual sideload routes write an HKCU registry key or a Trust Center
  catalog setting, and AGENTS.md (Excel section) forbids both. These experiments wait on an owner-approved route.
- The performance-budget table (section 4 end) waits for perf work after publish. It must not become a check now.
- Everything else respects the rules. The answer keeps OneCalc values-only, keeps one evaluator and graph, and
  keeps foreign late-bound COM refused. It never makes Full a worksheet-write permission (already true:
  `spreadsheet.rs:222-225`; app commands outside an event are refused, `events/source.rs:50-58`).

---

## 2. Disposition of every section

Bead keys: **14.a-14.f** are proposed below (section 5). Existing: dc-0f1k.24 (DNA Calc COM Caller/Range/Union),
dc-hsa5.12 (VBA events to the host), dc-hsa5.4 (continuations), dc-8ovd (one code-execution policy, incl. JS),
dc-6332.10/.12/.13 (answer 13 hosting steps).

| Section | Content | Disposition |
|---|---|---|
| 1.1 | Core is a small application coordinator above HostSession; VBA/COM/JS are projections | **14.d** (design note restates it with the trait seam, finding 6). No code by itself |
| 1.2 | Product-route table | Checked row by row in section 3. Adds two facts: Book1 in DNA Calc desktop, no native events → **14.b** |
| 1.3 | Keep: one edit/calc path, authored vs published, portable vs native ABI, grants and nested policy | no action: already true in code (section 3, C7-C9) |
| 1.4 | History: keep dc-svqh removal; X19 evidence; October review; own-GUID wording | dc-svqh: no action (closed, holds). GUID wording: **14.b** updates `docs/architecture/SYNTHESIS.md:241-245` when the real-workbook shell lands |
| 1.5 | What the reported numbers establish | no action (agrees with the prompt) |
| 2 row 1 | Book1 shell as full-host model | **14.b** (finding 2: it is current behaviour) |
| 2 row 2 | Null EXCEPINFO split | **14.c** (P3; add the vtable slot, finding 7) |
| 2 row 3 | Error after a committed macro write | **dc-hsa5.12** for the receipt; Excel's behaviour waits on E9 (deferred) |
| 2 row 4 | 4,096-object table | **14.a** (finding 5: cumulative) |
| 2 row 5 | Multi-area/other-workbook Target through the native bridge | **14.b** |
| 2 row 6 | UDF read conflation | no action: wrong (finding 4). E3 branch B goes to **14.e** |
| 2 row 7 | Reentry panic/deadlock while the session is borrowed | no action for the VBA route: wrong (section 3, C15). The graph-worker part belongs to harvest 18 |
| 2 row 8 | Retained references across close/reopen/rename | **14.d**, after **14.e** (E1) |
| 2 row 9 | A new route that omits the policy scope | **dc-8ovd**: add the fact that add-ins and loaded modules run with no policy wrapper (C17) |
| 2 row 10 | File handle opened under Full used under Safe | no action: no route today (C18). Deferred if policy becomes changeable while loaded |
| 2 row 11 | Multi-area, shape-mismatch, Value/format refusals | Excel facts: **14.e** (E6, E11). Implementation deferred |
| 2 row 12 | `CalculateFull` mapped to workbook scope | deferred: one workbook per session today (C20); waits on E8 and the multi-workbook design |
| 2 row 13 | Separate event enablement and event synthesis | **14.b** |
| 2 row 14 | Excel IIDs with partial slots; signatures guessed | **14.c** (advertised slots get real functions); full member coverage under **dc-0f1k.24** |
| 2 row 15 | "ComActivationDispatch supported" read as permission | no action: availability and permission are already separate (`spreadsheet.rs:822-840`; `capability.rs:571-578`) |
| 2 evidence paragraph | What the excerpts could not show | no action. I checked `fire` (C24) and the progress loop (C3) |
| 3.1 | Ownership table; one workbook registry first | Table: **14.d**. One real workbook: **14.b** (native), **14.f** (VBA) |
| 3.2 | `HostQuery`, `InvocationAccess`, `ExecutionOrigin` | deferred (waits on 14.d and harvests 15/16). Keep `dispatch_from` as the one executor |
| 3.3 | Identity invariants, structural transform, names, multi-area, collections, release | **14.d**: extend `ReferenceTracker`, not a new key type (finding 1). Release: **14.a** |
| 3.4 | What survives of SpreadsheetContext, SpreadsheetHost, event writes, native shell | no action as text; the concrete parts are 14.a, 14.b and 14.f |
| 3.5 | One descriptor, ABI lowering, early binding stays early, GUID decision, faults | Advertised-slot correctness: **14.c**. One descriptor: deferred (three hand-written tables today, C22). Faults: no action now |
| 3.6 | Cached reads, UDF reads, Value/Value2, Formula/Formula2, WorksheetFunction/Evaluate, scope, Run | Excel facts: **14.e** (E3B, E6, E7). Value conversion, Formula2 setter, Evaluate: deferred. Run/QueueAsMacro: **dc-6332.12/.13** |
| 3.7 | Outcome receipt, edit steps 1-7, one event authority, EnableEvents, BeforeSave, deltas, WebView2 | Receipt: **dc-hsa5.12**. One event source and one flag: **14.b**. BeforeSave: no action (P22 cancel cases plus the pre-pin route exist). Deltas/transport: harvest 17 |
| 3.8 | Owner-thread reentry, waits, DoEvents, browser | deferred to harvest 18 |
| 3.9 | Four authority questions, compile/bind/dispatch, nesting, host-class exemption, XLL, audit, isolation | dc-8ovd (with C17). The four questions are already separate in code: no action |
| 3.10 | JS: RequestContext, paths, load/sync, example batch, atomicity, tracking, cancel, custom functions, events, transport | deferred (owner-rule note above: follow Office.js, no DNA dialect) |
| 3.11 | Interaction table for the five reviews | deferred: reconcile once harvests 15-18 exist. The seam owner follows finding 6 |
| 4 Stage 0 | Truthful faults and receipts | **14.c** + **dc-hsa5.12** |
| 4 Stage 1 | One real workbook, one full-host route | **14.f** (VBA) + **14.b** (native) |
| 4 Stage 2 | Persistent references, names, real collections, second workbook | **14.d**; second workbook deferred |
| 4 Stage 3 | Native projections on canonical keys, one event authority, generated ABI | **14.b**, **dc-0f1k.24**; generated descriptor deferred |
| 4 Stage 4 | JS batches | deferred |
| 4 Stage 5 | Value conversion, Formula2, Evaluate, more Names/Areas | deferred (waits on 14.e results) |
| 4 Correctness vs budgets | Correctness rules and a budget table | Correctness: no action (the honesty rules already say it). Budgets: deferred (perf after publish) |
| 5 Common protocol | Capture fields, fresh processes, save/reopen | **14.e**: reuse the P22/P14 Stage/Snap/Typed logger; no new protocol document |
| 5 E1 | Retained Range, deletion, close/reopen, undo, `Is` | **14.e** |
| 5 E2 | Name scopes, Name vs returned Range, relative names | **14.e** |
| 5 E3 | Hidden reads, forcing | Branch A: no action (measured, finding 4). Branch B and controls: **14.e** |
| 5 E4 | Change/Calculate order, nesting, suppression | Mostly measured (finding 3). Remaining branches (handler error after write, UI order): **14.e** |
| 5 E5 | BeforeSave cancel and save pin | P22 `manual-cancel-only`/`manual-edit-cancel` cover SaveAs. Plain Save and handler error: **14.e** |
| 5 E6 | Early/late ABI, Value vs Value2, optional args, EXCEPINFO | **14.e** (X19 client `fixture/X19Com.csproj` exists); the OneCalc half goes in **14.c** |
| 5 E7 | Formula/Formula2; Evaluate vs WorksheetFunction | **14.e** |
| 5 E8 | Calculation scope across two workbooks | deferred (no two-workbook session to compare against yet) |
| 5 E9 | Write while calculation waits/fails/cancels | deferred (waits on dc-hsa5.4 and the X5/X17 async fixtures) |
| 5 E10 | XLL categories, Caller shapes, commands | **dc-6332.10/.12/.13** (answer 13 D.9, D.11, D.16 already cover it) |
| 5 E11 | Union geometry, multi-area assignment, collection iteration | **14.e** (X19 traces already cover first-area Rows/Columns) |
| 5 J1-J5 | Office.js experiments | deferred (sideload route needs owner approval; see above) |
| 5 DNA controls | Grant mapping, compile/bind/dispatch, handles, host-class, queued JS, isolation | Grant mapping and host-class: no action (existing tests: a Safe grant under a Full thread, `calc-host-vba/tests/opened_projects.rs:2272-2294`; host-class rule `capability.rs:571-578`). The rest: dc-8ovd / deferred |
| 6.1 | Stop growing the OneCalc pseudo workbook | no action: already true (dc-svqh; `projection.cpp:381-409`) |
| 6.2 | Stop using Book1 as a full application | **14.b** |
| 6.3 | Delete duplicate semantic routes as adapters migrate | deferred with the one-descriptor item |
| 6.4 | Retire the 4,096 table as lifetime policy | **14.a** |
| 6.5 | Stop guessing native interfaces | **14.c** |
| 6.6 | Error translations that confuse kinds | **14.c**; committed-then-failed: **dc-hsa5.12** |
| 6.7 | Do not treat reads as demand evaluation | no action: already true (finding 4) |
| 6.8 | Full/native trust/host object are not write permission | no action: already true |
| 6.9 | One event source and enablement | **14.b** |
| 6.10 | Defer the Office.js clone, descriptor compiler, command log, handle index | no action (agree); items on the deferred list |
| 6.11 | Preserve compatibility evidence | no action: kept (`crates/calc-xll-reference/traces/2026-11-11-x19-com`, `XLL_COM_HOSTING.md` section 32) |
| Recommended next unit | One real workbook through the VBA adapter: handles, Value2, calculation, receipts | **14.f** first, then **14.b** |

---

## 3. Claims checked against the code

| # | Claim (answer section) | Verdict | Evidence at 3b5aa86b |
|---|---|---|---|
| C1 | SpreadsheetContext supplies caller, reads, WorksheetFunction, volatility; active sheet and selection refuse by default (1.1) | partly | Defaults refuse (`spreadsheet.rs:58-101`). But `ContextBridge` answers ActiveSheet, Range and Selection from the user view when one exists (`calc-vba/src/lib.rs:1092-1145`) |
| C2 | The event route writes rectangular Value2 through Automation ingress (1.1, 1.2) | confirmed | `calc-host-vba/src/lib.rs:455-533` |
| C3 | Calculation commands map to Recalculate / workbook ForceAll / Rebuild / CalculateRange (1.2) | confirmed | `lib.rs:534-547`; the cut-off loop advances pending work and refuses Waiting/unfinished (`:553-572`) |
| C4 | Native objects describe one local Book1 shell (1.1) | confirmed, stronger | README `:30-35`; DNA Calc desktop composes it as "Book1" (`apps/dna-calc-desktop/src/app.rs:134`, `:763-770`); Workbooks.Count is 1 and Item takes name or 1 (`projection.cpp:250-266`) |
| C5 | No real-workbook operation through the native route (1.2) | confirmed | The shell gets names only (`calc-addin-host/src/host.rs:61-75`, `app.rs:605-611`); no Range or value member reaches the session |
| C6 | SheetChange bridge carries one cell (1.2, 2) | confirmed | `projection.cpp:603-612`; calc-host targets are `Vec<GridArea>` (`events.rs:155-174`). Not called by the interactive DNA Calc app at all (finding 2) |
| C7 | HostDelta separates Committed, Recalculated, RecalcFailed (1.3) | confirmed | `session.rs:83-129` |
| C8 | ObjectHeader is a separate versioned ABI (1.3) | confirmed | CalcVBA `crates/calcvba-host-api/src/object.rs:29-41` |
| C9 | Off/Safe/Unguarded map to none/Safe/Full; person enable gives Full; nested Safe stays Safe with a shared audit (1.3) | confirmed | `calc-macro-security/src/lib.rs:36-85`; `calc-host-vba/src/trust.rs:52-76`; `calc-vba/src/policy.rs:40-62` |
| C10 | Caller/ThisCell with and without EXCEPINFO differ (2 row 2) | confirmed; severity overstated | `projection.cpp:382-389`, `:409`. The early-bound slot is a third answer (finding 7) |
| C11 | A setter can error after its edit committed (2 row 3) | confirmed | `lib.rs:506-532`. Also: Change is then not raised but recorded (`events.rs:176-191`, `session.rs:1428-1446`) |
| C12 | 4,096-entry table; "not proof of a cumulative leak" (2 row 4) | confirmed; the hedge is wrong | Only inserts (`spreadsheet.rs:380-381`); release is a no-op (`:963-968`); table lives as long as the loaded module (`runtime/mod.rs:335-345`, `cache.rs:125-133`) |
| C13 | Native bridge cannot express multi-area Target (2 row 5) | confirmed | `projection.cpp:603-606` (one row, column) |
| C14 | UDF object reads may conflate cache, dependency and forcing (2 row 6) | wrong | Separate readers: argument (`run.rs:4307-4324`), hidden/cached observation (`:4349-4382`), uncalced (`:4325-4348`). Excel measured untracked (finding 4) |
| C15 | Reentry while the session is mutably borrowed may panic or deadlock (2 row 7) | wrong for the VBA route | Each handler gets a fresh `RefCell` over the reborrowed `&mut HostSession` (`lib.rs:424-430`, `:608-680`). Outer event Ranges are refused inside a nested event by lease (`events/source.rs:479-487`). UDF reads use `try_borrow` and refuse ("reentrant ...", `calc-vba/src/lib.rs:1093-1213`) |
| C16 | No shared lifetime contract for retained references (2 row 8) | partly | Three contracts (finding 1) |
| C17 | A route may omit the policy scope and fall back to the permissive default (2 row 9) | partly confirmed | `policy::wrap` returns the host unwrapped when the thread has no policy (`calc-vba/src/policy.rs:103-108`). Person-loaded modules (`apps/dna-calc-worker/src/workbook.rs:590-603`, `:611-637`; `calc-host-vba/src/lib.rs:228-245`) and calc-addin-host's VBA add-ins (`host.rs:239-310`; no policy call in the crate) load with none: no checks, no audit. Only file projects enter a scope (`lib.rs:295-300`) |
| C18 | A handle opened under Full stays usable under Safe (2 row 10) | not checkable as a route | Policy is fixed when a project loads (`policy.rs:103-111`, `runtime/mod.rs:335-345`), and each project has its own base host (`spreadsheet.rs:297-307`). No downgrade path exists today. CalcVBA's handle operations do pass through unchecked (`capability.rs:19-22`, `:598-686`) |
| C19 | Multi-area writes, shape mismatch, large writes and Value are refused (2 row 11) | confirmed | `lib.rs:455-471`; `spreadsheet.rs:222-225` |
| C20 | `Full` maps to workbook scope (2 row 12) | confirmed; no effect today | `lib.rs:539-542`. The worker holds one `Workbook` (`apps/dna-calc-worker/src/server.rs:88-94`), so Application and Workbook scope coincide |
| C21 | Event enablement is owned separately per adapter (2 row 13) | confirmed | finding 2 |
| C22 | Excel IIDs used for selected compatibility slots (1.4, 3.5) | confirmed | `projection.cpp:16-22`. calc-vba has a second, private description: an "Excel" reference resolves to DNA's Range with tokens 1-9, 100, 101 and Application tokens 100+n, 200+n (`spreadsheet.rs:1444-1660`). The event route has a third (typed slots, `events/source.rs:67-121`, `:490-599`) |
| C23 | ComActivationDispatch is turned on in the descriptor (2 row 15) | confirmed | `spreadsheet.rs:836-840`; exact host classes `:823-831`; exemption only for CreateObject of those (`capability.rs:571-578`) |
| C24 | Native event argument order cannot be judged (2 evidence) | checkable, correct | `fire` passes named DISPIDs [0,1] in declaration order, "measured in X0" (`projection.cpp:524-526`) |
| C25 | The event route has Application.Range, Calculate, EnableEvents (implied, 1.2) | confirmed | `events/source.rs:67-121`; document objects answer only `Name` (`:207-229`) |
| C26 | X19 measured Range/Union/Caller metadata and the LCID crash (1.4) | confirmed | `XLL_COM_HOSTING.md:1342-1361`; traces `crates/calc-xll-reference/traces/2026-11-11-x19-com/` |
| C27 | Change before automatic calculation is unestablished (3.7) | partly wrong | finding 3 |
| C28 | No working JS object model (1.2) | confirmed | No `Excel.run`, Office.js or `CustomFunctions` in `apps/` or `crates/` (the only "RequestContext"/"Excel.run" hits are PowerShell probe scripts) |
| C29 | Safe is a policy check, not a sandbox (3.9) | confirmed | `capability.rs:12` |
| C30 | Eager `Vec<Variant>` enumeration (3.3) | partly | For DNA objects, For Each is refused (`spreadsheet.rs:1001`); only foreign COM enumerates |
| C31 | Host-owned vs foreign delegation (3.4) | confirmed | `spreadsheet.rs:353-358`, `:310-325` |
| C32 | OneCalc's SyncMacro transport is unchanged (3.6) | confirmed | `projection.cpp:339-380`, `:408` |
| C33 | A Full grant cannot make a UDF write legal (3.9) | confirmed | Range.Value writes from a worksheet function are refused (`spreadsheet.rs:222-225`); application commands outside an admitted event give 1004 plus a refusal (`events/source.rs:50-58`) |
| C34 | `dna-calc-protocol` owns wire framing (3.11) | confirmed exists | `apps/dna-calc-protocol` |
| X1 | Excel: equal-coordinate acquisitions need not be the same object (3.3) | not checkable here | E1. DNA interns them (finding 5) |
| X2 | Excel: Union counts area multiplicity (E11) | not checkable here | E11 |
| X3 | Excel: Formula setter intersects, Formula2 spills (E7) | not checkable here | E7 |
| X4 | Excel: WorksheetFunction raises where Evaluate returns an error value (E7) | not checkable here | E7 |
| X5 | Excel: COM writes, Calculate before Change (E4) | measured | P22 (finding 3) |
| X6 | Excel: hidden UDF reads are untracked (E3) | measured | finding 4 |

---

## 4. Experiments

All need the Windows Excel desktop (VBA projects, COM clients, events); none fits the formula answer service
except the formula half of E2. "Runs" are fresh workbook runs; each takes seconds. Prior evidence is named so
nothing is re-measured. Predictions are hypotheses. "DNA now" is what our code does.

| Exp | Question | Inputs | Predicted outcome per hypothesis | Runs | Prior evidence |
|---|---|---|---|---|---|
| E1 | What happens to a Range held in a VBA global through structural edits, close/reopen, undo; is a second acquisition `Is` the first? | Victim.xlsx S!A1:A3 = 10, 20, 30, manual; observer project holds `Range("A2")`. Branches: insert row 1; delete row 2; rename S; reorder; delete and recreate S; close and reopen same path; UI edit then Undo; `Is` of two `Range("A2")` | Tracking: A3/20. Fixed: A2/10. Invalidated: an error (record the number). Close: error, detached reads, or reattach. `Is`: False (recollection) vs True (DNA now, by interning). DNA now: the UDF route tracks (A3/20; Deleted on row delete); the event route gives 91 after the event | 8 branches x 3 = 24 | none |
| E2 | Which scope resolves N; does a returned Range follow a redefined Name? | S!A1:A3 = 10, 20, 40; T!A1 = 30; book N→S!$A$1, S!N→S!$A$2, T!N→T!$A$1. Resolve via Application.Range, Worksheet.Range, Evaluate, Names on S and T; redefine to S!$A$3; rename; relative-name branch from C3 base | Local-over-global: 20 on S, 30 on T. Global-only Application: 10, 10. Redefinition: retained Name gives 40, returned Range stays 20; or both 40; or the Range errors. Relative base: definition A1, active cell B2, caller C3 | 12 x 3 = 36 (the `=N` formula half can be answer-service calls, about 10) | none |
| E3 B | Does a UDF's hidden read of a dirty formula cell see the cache, force it, or retry? | A1=1, D1=A1*2 cached 2, E1=FormulaRead() reading D1; manual; set A1=2; calculate only E1. Controls: force-all, volatile variant, reversed entry order | Cached: E1=2, D1 stays 2. Demand: D1=4, E1=4. Retry: 4 with a different call count. Restricted: a specific error | 6 x 3 = 18 | Branch A measured (finding 4) |
| E4 rest | Handler error after a write; UI edit order; disabled-events replay | P22 runner, add: Change handler writes C1 then raises; same edit through the UI (SendKeys or UIA in our own Excel session) | Error: A1=2/C1=7 kept, nested rolled back, or all rolled back. UI: Change before Calculate (DNA now) vs after. Disabled: dropped (P22 `automatic-events-disabled`) | 3 x 3 = 9 | P22 7 cases, 70 trials (finding 3) |
| E5 rest | Plain Save with BeforeSave edit and Cancel; handler error after a write | Saved book A1=1; BeforeSave writes A1=2, sets Cancel; branches Cancel False and error | Cancel keeps A1=2 in memory and no file write; or rolls back; or saves. Without cancel: pin before handler saves 1, after saves 2 | 3 x 3 = 9 | P22 `manual-cancel-only`, `manual-edit-cancel` (SaveAs) |
| E6 | Do early and late routes give the same logical values and faults; what do Value and Value2 give for date and currency cells? | One book: scalar, 2x2 block, empty, "", #N/A, 45292.25 date-formatted, 123.45678901234567 currency-formatted. PIA client and IDispatch client (X19 `fixture/X19Com.csproj`); omitted vs Empty vs Null vs Error optionals; Invoke with and without EXCEPINFO | Value: Date vs Double; Currency 123.4568 vs Double. Routes agree logically; or differ in omission, bounds or copyback. Null EXCEPINFO: DISP_E_EXCEPTION vs other | 2 clients x 3 = 6 | X19 (Version, Caller shape, metadata, Union, 1004 for another sheet) |
| E7 | Formula vs Formula2 setters; Evaluate vs WorksheetFunction for a failing MATCH | A1:A3 = 1, 2, 3; E2.Formula = "=A1:A3"; G2.Formula2 = "=A1:A3"; obstruction branch; `Evaluate("MATCH(99,A1:A3,0)")` vs `WorksheetFunction.Match(99, Range("A1:A3"), 0)` | Formula: E2 = 2 (intersection); Formula2: G2:G4 spill. H1 Evaluate returns Error 2042 and WorksheetFunction raises (recollection: 1004); H2 both error values; H3 both raise | 5 x 3 = 15 | none |
| E11 | Union area order and Count; multi-area assignment; enumeration under mutation | Union(B2:C3, F5, C3:D4), reversed, F5-first; Areas, Count, Rows.Count, For Each order. Assign scalar, 1x1, 2x2 to B2:C3,F5:G6. Enumerate S,T,U while renaming/adding/removing | Count 9 (multiplicity), 8 (unique) or 20 (bounding box). Rows.Count 2 vs 1 with F5 first. Assignment: all areas, first only, refused before writing, or partial. Cursor: snapshot, live or error | 12 x 3 = 36 | X19: first-area Rows/Columns (`XLL_COM_HOSTING.md:1344-1346`) |
| E8 | Application vs workbook vs sheet calculation scope across two books | Two books with a counting UDF, manual, inputs changed; five commands from two initial states; linked branch | Local: A=10, B stays 2. Application: 10/20. Dirty-only skips clean B; force-all reruns it | 12 x 3 = 36 | single-book scopes in `probe.bas:121-131` |
| E9 | A COM write while an async native provider is held | X5/X17 async fixture; B1 = Held(A1); write A1=2 from macro and from COM; release later | H1 setter waits; H2 returns early with cache 10; H3 typed pending value; H4 error with A1=2 kept. AfterCalculate before vs after release | 8 x 3 = 24 | P22 lists async as unrun |
| E10 | XLL categories, Caller shapes, commands | as the answer | as the answer | 0 new | answer 13 D.9, D.11, D.16 (dc-6332.10/.12/.13) |
| J1-J5 | Office.js ordering, partial failure, tracking, awaits, async custom functions | as the answer | as the answer | 5 scenarios x 3 = 15 | none; needs an approved sideload route |

First batch for 14.e: E1, E2, E6, E7, E11 (about 120 runs), then E3 B, E4 rest, E5 rest (36 runs). E8 and E9 wait
(deferred list).

---

## 5. Proposed beads

**14.a — VBA Range objects: release them, and keep the limit to live objects.** Priority 1. **Start first.**
A VBA function or event that creates more than 4,096 distinct Range objects over a loaded project's life gets
"spreadsheet object limit exceeded" for every new one. `SpreadsheetHost` interns `Object::Range(range, axis)` by
value and never removes entries (`crates/calc-vba/src/runtime/spreadsheet.rs:205-219`, `:360-387`).
`release_object_variant` returns Empty for DNA objects (`:963-968`). Each `Cells(i, j)` makes a new object
(`:674-697`). The per-module `Sheets` list also only grows, with a linear search (`crates/calc-vba/src/lib.rs:888-
913`). First find out whether the runtime calls release for compatibility objects. Then free entries whose last
VBA reference is gone, keep a live-object budget with the same typed refusal, and make `Sheets` a map. Keep a
Range stored in module state valid. Acceptance: a new in-book corpus UDF summing `r.Cells(i, 1).Value2` for
i = 1 to 5,000 over a 5,000-row argument matches Excel's capture (`crates/calc-vba/tests/corpus/inbook/`). Ten
such cells recalculated 20 times stay correct. A Range held in a module global resolves after 10,000 other
objects were released. Dependencies: none. Model: Sonnet (thinking), Opus review of the release rule. Why first:
a hard, common failure (any UDF that walks cells) with a small, local fix.

**14.b — DNA Calc desktop: native add-ins see the real workbook and its events.** Priority 2. **Start first.**
The desktop passes `"Book1"` and no active sheet to `AddinHost::new` (`apps/dna-calc-desktop/src/app.rs:134`,
`:763-770`). The interactive app never raises SheetChange or AfterCalculate for add-ins (only
`engine.rs:375`, `:419`, `:663` in the harness do). The native SheetChange carries one cell
(`crates/calc-com-host/src/projection.cpp:603-612`). EnableEvents is two flags (`projection.cpp:37`, `:241-244`
vs `crates/calc-host/src/events.rs:107-112`). Work: (1) define a small bridge trait in calc-com-host (the
dependency ceiling keeps calc-host out of it); DNA Calc implements it over the session, OneCalc keeps
values-only. (2) Workbook name/FullName/Path from the session's file identity, updated on open and Save As
(`dna_com_workbook`, `projection.cpp:589-595`); ActiveSheet from the selection. (3) Raise native SheetChange from
the session's committed input writes, all areas, after the VBA Worksheet_Change handler of the same target, as
P22's `automatic-change-nested` trace shows. Raise AfterCalculate when a calculation command completes. The
session's one handler slot (`events.rs:104-106`) needs a second observer or fan-out; one event source, not two.
(4) One EnableEvents flag, the session's. (5) Update `docs/architecture/SYNTHESIS.md:241-245` to say what the
Excel-IID facade covers. Acceptance: the xlOil and Excel-DNA AppEvents cases of the X3/X4 traces, run through
the DNA Calc page with a named workbook, match Excel's records: Workbook.Name, SheetChange address for a
rectangle and for a multi-area edit, AfterCalculate count. Dependencies: none; dc-0f1k.24 should then depend on
it (Caller, Range and Union use the same bridge). Model: Codex sol for the C++/Rust work; Opus designs the
bridge trait first. Why first: owner focus on COM add-ins, and it is the base of dc-0f1k.24's three failing cases.

**14.c — COM: the same answer on every route for advertised members.** Priority 3.
In OneCalc, `Application.Caller` and `ThisCell` answer 1004 "needs a workbook" through `Invoke`, but
`DISP_E_MEMBERNOTFOUND` with a null EXCEPINFO (`projection.cpp:382-383`). Through the vtable (Caller slot 74,
`XLL_COM_HOSTING.md:1344`) they reach the one-argument stub and get `E_NOTIMPL` (`projection.cpp:465`, `:475-489`).
Give every advertised getter a function with its measured signature; return the refusal through `SetErrorInfo` +
`0x800A03EC` on the vtable route and `DISP_E_EXCEPTION` on `Invoke` whether or not EXCEPINFO is given. Read the
ThisCell slot from Excel 16.0.20527's type library, as the other slots were (`projection.cpp:2`). Acceptance:
`dna_com_test_refusals` (`projection.cpp:861-877`) also covers the vtable slots and a null EXCEPINFO; an
Excel-DNA PIA call of `Application.Caller` in OneCalc gets the 1004 text. Dependencies: none. Model: Sonnet.

**14.d — One persistent Range identity for UDF, event and COM routes (design note).** Priority 2.
Restate answer 3.1 and 3.3 against the code: the calc-fml `ReferenceTracker` (`crates/calc-fml/src/reference/
tracked.rs`), `StoredReference` and `ReferenceHandleError` (`crates/calc-func/src/udf.rs:255-277`), the
structural transform at commit (`crates/calc-host/src/session.rs:1395`), the event lease
(`crates/calc-vba/src/events/source.rs:469-489`) and the COM row/column copy. Decide from E1, E2 and E11: what a
held Range does after the event ends (Excel keeps it; we give 91); `Is` semantics (DNA interns); workbook
incarnation (tracker owner plus the worker's workbook id, `apps/dna-calc-worker/src/server.rs:482-485`); Name
scope and returned-Range binding; area order; when the tracker releases slots (it never does now,
`tracked.rs:83-104`). Name the trait seam (finding 6). Output: one note, `docs/architecture/OBJECT_MODEL.md`,
plus the follow-up beads it names. Acceptance: the note cites each E1/E2/E11 result it relies on and says which
code moves where. Dependencies: 14.e. Model: Opus.

**14.e — Excel object-model probe batch (E1, E2, E6, E7, E11, then E3 B, E4 rest, E5 rest).** Priority 2.
**Start first.** Inputs and predictions are in section 4. Extend the P22 runner and its Stage/Snap/Typed logger
(`crates/calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/`) and the X19 COM clients
(`crates/calc-xll-reference/fixture/X19Com.csproj`). Write a new probe folder beside P22 in the same lineage;
record raw bits, VARIANT types, error numbers, `Is` results and handler order. Acceptance: about 156 runs
recorded, each with its build and settings line, and the README says which hypothesis each result supports.
Dependencies: none. Model: Sonnet (Windows, Excel). Why first: cheap; it decides 14.d and the deferred Value,
Formula2 and multi-area work.

**14.f — VBA event code: Worksheet and Workbook objects over the existing session loan.** Priority 2.
Event handlers today get Application.Range (active sheet, one area), Calculate, EnableEvents and Target
(`crates/calc-vba/src/events/source.rs:67-121`, `:490-599`). Document objects answer only `Name` (`:207-229`).
Common event code uses `Me.Range("C1")`, `Worksheets("S").Range(...)`, `ThisWorkbook.Worksheets(...)`,
`Range.Cells`, `Range.Value2` reads and writes. Add those members to the document and Range objects, routed through
the existing `Application` loan (`crates/calc-host-vba/src/lib.rs:424-583`), with the same refusals for
multi-area writes and for anything else. No new workbook state. Acceptance: a P22-style workbook (or the in-book
corpus) whose Change handler writes `Me.Range("C1")` and `Worksheets("S").Range("D1")` gives Excel's values and
handler order through the DNA Calc page, in automatic and manual mode. The calc-host-vba event tests stay green.
Dependencies: none (14.d may later change object lifetime). Model: Sonnet.

Notes for existing beads (orchestrator; no br writes here):
- dc-hsa5.12: its description says "Change after authored commit and before automatic calculation". That holds
  for UI edits only; COM writes are measured and coded as Calculate before Change (finding 3).
- dc-snz.5 (in progress) is titled "VBA UDF Range objects ... in DNA OneCalc". After dc-svqh, Range objects are
  DNA Calc only; its scope needs restating.
- dc-8ovd: add C17 (add-ins and person-loaded modules run with no policy wrapper and no audit).
- dc-0f1k.24: depends on 14.b.

---

## 6. Deferred list

| Item | Waits on |
|---|---|
| JS projection: RequestContext, load/sync, tracking, custom functions, events (3.10, Stage 4) | dc-8ovd's JS start; 14.b and 14.d; J1-J5 results |
| J1-J5 Office.js experiments | An owner-approved sideload route (the usual ones change HKCU registry or Trust Center settings) |
| `HostQuery` / `InvocationAccess` vocabulary (3.2) | 14.d; harvests 15 and 16 |
| Multi-workbook application registry, Application calculation scope, E8 (3.1, 2 row 12) | Harvest 16 (engine-host-doc) and harvest 15 (cross-workbook graph); today one Workbook per worker |
| One member descriptor for calc-vba's type library, the event slots and calc-com-host (3.5, 6.3, 6.5) | 14.b and dc-0f1k.24 adding members; revisit when JS starts |
| Outcome receipt for edits that commit and then fail or wait; E9 (3.7, 2 row 3) | dc-hsa5.4 continuations; the X5/X17 async fixtures. The typed refusal stays meanwhile |
| Owner-thread reentry, STA pumping, DoEvents (3.8) | Harvest 18 |
| Range.Value date/currency conversion, Formula2 setter, Evaluate (3.6, Stage 5) | 14.e results (E6, E7) |
| Multi-area and shape-mismatch Value2 writes (Stage 5) | 14.e (E11) |
| Policy checked at dispatch; handles after a downgrade (3.9, 2 row 10) | Policy becoming changeable while a project is loaded (dc-8ovd) |
| Performance budgets (section 4 table) | Perf work after publish |
| Interaction table across the five reviews (3.11) | Harvests 15-18 |
