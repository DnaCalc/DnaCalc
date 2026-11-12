# Harvest 18: concurrency and isolation (GPT-6 Pro answer, read against 3b5aa86b)

Reader: Opus, 2026-11-12, for dc-6332.1. Answer: `pro/18-concurrency-and-isolation-answer.md` (741 lines); prompt:
`pro/18-concurrency-and-isolation-prompt.md` (1,362 lines). Pins: DnaCalc 3b5aa86b, CalcVBA 8115aa78 (read from the
read-only checkout). Every `file:line` below is at those pins.

**Drift.** `git diff --stat 3b5aa86b HEAD` over calc-engine, calc-xll-host, calc-com-host, calc-host/src, calc-vba,
calc-host-vba, calc-macro-security, calc-fml, calc-func `udf.rs`, the desktop apps, calc-addin-host and the four design
documents is empty, except `apps/dna-calc-worker/src/server.rs` (IMAGE projection of stale values, a match-arm
reformat, a test) and new calc-host tests. None changes a disposition below.

**How the checks were done.** I read AGENTS.md, the prompt and the answer in full, harvests 9 and 13, and the code
named below with `git show 3b5aa86b:<path>` (copies in the scratch folder `harvest-18\`). Tracker lookups were
read-only Python over `.beads/issues.jsonl`. Nothing was built or run: no cargo, no Excel, no `br` writes. "Latent" and
"inferred" below mean read from the code, not executed.

**What the answer is.** It saw only the prompt's excerpts and says so (line 7). It is careful about evidence classes.
Its weak spots are things the excerpts left out: who calls the parallel level route, XllHost's teardown, the
status-64 route under pool scheduling, and the Excel measurements of x13, P01, P06 and the iteration probes, which
already answer several of its proposed experiments.

---

## Findings that change how the answer should be used

1. **The general parallel level route is not on any product route, and if it were turned on it would refuse
   add-in formulas.** No product code calls `set_threads`: the callers are `calc-host/tests/threads.rs:34`,
   `tests/incremental.rs:223` and `tools/xtask-harness/src/bench.rs:550,1354`; the default is 1
   (`calc-engine/src/state.rs:776`). Both product drivers also step with a finite budget of 64 evaluations
   (`apps/dna-calc-desktop/src/app.rs:740`, `apps/dna-calc-worker/src/worker.rs:97`), and the level route needs
   `WorkBudget::UNLIMITED` (`recalc.rs:1928`). Latent defect: level workers call `evaluate_with` with no callable table
   (`recalc.rs:2565-2574`). A formula with a user call then returns `Done(refused "a callable provider missing from its
   admitted command")` (`recalc.rs:5812-5819`). `run_level` stages it as the result (`recalc.rs:2517`). A workbook whose
   only callables are `$` XLL functions creates no fence (`recalc.rs:1503-1511`), so it stays eligible
   (`recalc.rs:2322-2345`). The thread test registers no callables, so nothing catches it. The answer's 1.1 "real" route,
   its 1.2 "native unlimited" row and its speculative-replay risk (section 2, row 4) rest on the wrong premise. No user
   code can run on a level worker, so the replay concern does not arise there; the wrong refusal does. Bead 18.a.
2. **The dynamic-prerequisite refusal is narrower than the answer says, and its proposed fix already exists.** The
   refusal at `recalc.rs:2193-2211` applies only to `Attempt::Waits`. A `$` or ordinary XLL reading an unfinished
   producer through xlCoerce has the policy `XllUncalcedUnwind` (`calc-xll-host/src/native.rs:1365-1370`). It ends as
   `Attempt::Reevaluate` (`recalc.rs:6018-6045`), and that is handled under pool scheduling (`recalc.rs:2162-2189`). So
   the measured 64 route already runs concurrently (`callable.rs:140-150`, "gave 64 on worker threads (P03)"). The
   refused case is a formula with a pool callable that suspends on a dynamic read of its own, for example
   `=F$(INDIRECT("A"&B1))` with a dirty target. The answer's reproducer (a `$` probe that coerces a runtime-built
   reference) would take the 64 route and miss the refusal. Its section 3.4 steps 1-5 describe today's code. Also, pool
   capacity is `available_parallelism` clamped to 1-4 (`worker.rs:30-33`). On a one-CPU machine `$` functions get
   `parallel = 1` (`native.rs:1006-1012`), there is no pool scheduling, and the same formula is deferred normally. So
   the outcome class depends on the machine. dc-hsa5.20.5 already owns this ("thread count changes an outcome class").
3. **The "unbounded join" High risk is mostly handled already.** Every pool job is registered in `pending_async` at
   submit (`worker.rs:196`, `callbacks.rs:240-253`) until its physical return (`worker.rs:264`). `close()` refuses while
   any are outstanding (`native.rs:1179-1181`). `XllHost::drop` then forgets the `Rc<Pool>`, so `Pool::drop` never joins
   a hung task (`native.rs:1259-1275`, comment: "Final teardown cannot join a hung native task"). The residual hang is
   different: the non-resumable route `worker::invoke` blocks the owner on `recv()` (`worker.rs:394-412`). Ordinary XLL
   calls run inline on the owner anyway. At the pin the owner is the UI thread (finding 5), so a hung native call freezes
   the window, as it does in Excel. Only the surrogate fixes that (dc-6332.15).
4. **The async quiescence hazard is real, and its consumers exist.** `physical_returned` is set inside `xlAsyncReturn`,
   before control goes back to the add-in (`callbacks.rs:539`). `reap_cancelled_async` (`native.rs:1145-1162`) and
   `Pending::drop` (`callbacks.rs:199-215`) remove the entry on that flag. After that, `close`/`unload_module`
   (`native.rs:1164-1225`) can proceed to `FreeLibrary` in `Module::drop` (`native.rs:533-541`) while the add-in's own
   thread may still be running module code. Pool calls are not affected: there the flag is set after the native return,
   and after the worker's own module leases are dropped (`worker.rs:209-264`). Inferred, not demonstrated. Bead 18.b.
5. **Several "open" questions are already measured or already in code.** The x13 boundary run measured the callback
   statuses that E2 asks about (`XLL_COM_HOSTING.md:1152-1190`): xlUDF from `$` is 32 (not 128), SET.NAME from `$` is 2,
   EVALUATE and commands from `$` are 128, and `$` ran on a worker at formula entry. The host matches
   (`callbacks.rs:574-588`). These contradict the older threading prose at `XLL_COM_HOSTING.md:205`. Further: P01
   measured the cached VBA read (E3, `RECALC_CONTRACT.md:33-43`); formula restart with re-entered prefix UDFs is measured
   (E4, `RECALC_CONTRACT.md:443-444`, `EVALUATOR_SUSPENSION.md:126-131`); iteration order with MTC off is measured (E5,
   `RECALC_CONTRACT.md:51-53`); save pins include BeforeSave edits (E9, `RECALC_CONTRACT.md:27-28`). RTD leases carry an
   incarnation and are never recycled (`calc-host/src/recalc.rs:489-490`, dc-snz.23.2). Command receipts are retry-safe
   with an incarnation (dc-971p.26). At the pin, the WebView2 shell, the XLMAIN/XLDESK/EXCEL7 windows, the STA, the host,
   the engine, VBA and the XLL main lane all share one OS thread (`apps/dna-calc-desktop/src/app.rs:703-740`,
   `platform/calc-platform-desktop/src/shell.rs:108-120`). The answer's table in 3.1 treats them as separable; they are
   not yet.
6. **Section 1.3's second correction is wrong.** The rework plan's restart line (`REWORK_PLAN.md:64-66`) describes
   Excel's measured formula restart on the first calculation after entry. x17 is about native status-64 entries. It
   does not touch that line. The open item is dc-hsa5.20.10. Only the words "Values never differ" deserve a
   qualification (bead 18.c).
7. **There is no user calculation interrupt.** The wire has no cancel request (`apps/dna-calc-worker/src/wire.rs`, no
   match for "cancel"). `cancel_calculation` (`calc-host/src/session.rs:576-594`) is reached only from workbook
   replacement (`dna-calc-worker/src/server.rs:1674-1677`), save cancellation (`calc-host/src/save_calculation.rs:603`)
   and test commands (`dna-calc-worker/src/names.rs:601`). The answer's cancel-key design (3.6, E6) is therefore a new
   capability to measure and build (beads 18.d, 18.e), not a correction to an existing route.

---

## Disposition of every section

| Section | Content | Disposition |
|---|---|---|
| 1 (head) | Keep owner host, resumable evaluation, invocation identities, checked admission, bounded pool; add isolation around the session | No action: summary; the parts are placed below |
| 1.1 engine parallel route | "Real and deliberately narrow" | **18.a** (latent refusal). The route itself: no action until perf work (deferred list) |
| 1.1 XLL scalar pool | Owned messages, 1-4 threads, owner check, module leases | No action: confirmed. The 4-thread cap against Excel's 16 is a known profile point (13-harvest 2.6) |
| 1.1 status 64 | x17 results; latch and discard must survive | No action: confirmed; dc-6332.11 holds it |
| 1.1 async admission | Right shape; add process incarnation | **dc-6332.15** (incarnation is a surrogate concern) |
| 1.1 VBA/RTD owner-local | Frame, single-use continuation, two RTD polls | No action: confirmed |
| 1.1 browser handoff | Timer task between advances | No action: confirmed; finite budget 64 per advance (`worker.rs:97`) |
| 1.2 route table | Seven product routes | No action as a table. Two rows are wrong: "native unlimited" is test/bench only (finding 1); "native finite-step" does run the pool's concurrent scheduling (`recalc.rs:1498-1540`) |
| 1.3 first bullet | Finite parallelism is a target | **18.c** (doc correction of `SYNTHESIS.md:184`) |
| 1.3 second bullet | Restart statement stale | No action: wrong (finding 6). Wording qualification in **18.c**; the parity item stays **dc-hsa5.20.10** |
| 1.3 third bullet | `$` is command-sensitive | No action: already measured (x17 Range on main; formula entry on worker, `XLL_COM_HOSTING.md:1185,1280`). Old prose fix in **18.c** |
| 1.3 last paragraph | RTD heading; thread test excerpt | No action. The full test asserts serial = parallel (`threads.rs:150-178`); it registers no callables (finding 1) |
| 1.4 | Populations kept apart | No action: agrees with the honesty rules |
| 2 row 1 | Dynamic prerequisite in concurrent callback | **dc-hsa5.20.5** (owns it). Add finding 2's formula shape and the CPU-count dependence to its acceptance |
| 2 row 2 | Unbounded pool join | Mostly handled (finding 3). The owner-blocking `worker::invoke` route: **dc-6332.15** |
| 2 row 3 | In-process crash/hang | **dc-6332.15** (design), then dc-6332.13 step 11 |
| 2 row 4 | Speculative reset needs replay proof | No action for replay (no user code runs on a level worker); the real defect is **18.a** |
| 2 row 5 | `physical_returned` not quiescence | **18.b** |
| 2 row 6 | Callback queues behind waiting parent / borrow panic | **dc-hsa5.12** (events) and **dc-hsa5.20.7** (explicit frames); not checked further here |
| 2 row 7 | Safe ancestor across worker/IPC | No action now: no VBA runs on workers or over IPC. Policy is bound per project at load (claim 22). Deferred: cross-project calls |
| 2 row 8 | Cancel vs physical interruption | **18.d** (measure), **18.e** (build) |
| 2 row 9 | Finite-budget contract drift | **18.c** |
| 2 row 10 | Old restart prose | Same as 1.3 second bullet |
| 2 row 11 | Late RTD/provider traffic after restart | Within a process: no action (RTD leases and async keys are never reused, claim 25). Across processes: **dc-6332.15** |
| 2 row 12 | Engine values presented as UI/save evidence | No action: nobody makes that claim; the measure-don't-assert rule already covers it |
| 2 paragraph | mpsc queue not proof of unbounded work | No action: confirmed; reservations exist (`recalc.rs:4179-4205`), with a typed refusal at `:2010-2024` |
| 3 (head) | Proposed | No action |
| 3.1 owner table | Owners per facility, native and wasm | **dc-6332.15** input. Note for it: today UI, STA, windows, host and VBA share one thread (finding 5) |
| 3.2 contracts and sketch | `ExecutionLane`, `CallAdmission`, `StepProgress`, `AttemptEnd` | **dc-hsa5.20.5** for lane/permit; `AttemptEnd` exists as `Attempt::{Reevaluate, Waits, AwaitExternal}` (`recalc.rs:2100-2215`): no action |
| 3.3 invariants 1-7 | Command scope, yield retains, reevaluation ends attempt, worker candidates, atomic publication, stable slots, replay proof | No action: 1-6 already hold (`RECALC_CONTRACT.md:24-28,714-724`); 7 holds for the level route (functions absent) and for pure retry (`RECALC_CONTRACT.md:438-440`, `recalc.rs:6064-6072`). Ordering work stays **dc-hsa5.4** |
| 3.4 | Dynamic native prerequisites via 64 | Steps 1-5 exist (finding 2). The remaining Waits case: **dc-hsa5.20.5** |
| 3.5 | One VBA owner, invocation-aware reentry, ByRef events, read views | **dc-hsa5.12** (events, BeforeSave carrier); read views are measured (P01) and pinned (`RECALC_CONTRACT.md:26`). Macro-variant read frontier: **18.d** |
| 3.6 | Yield/wait/await/reevaluate/supersede/cancel/interrupt/fail/shutdown | Table: no action (matches the contract). User interrupt: **18.d**, **18.e** |
| 3.7 (head) | One compatibility process per session; controller journal; no replay; inert recovery | **dc-6332.15** |
| 3.7 runtime grouping and unload | Groups, residency until exit | **dc-6332.15**; residency for native-async modules now: **18.b**; reload work: dc-snz.21, dc-snz.19 |
| 3.7 protocol and memory boundary | One dispatcher, local and IPC transports; boundary table | **dc-6332.15** |
| 3.7 RTD, reload and teardown | Two-phase close, leases, user-visible unknown outcome | **dc-6332.15**; RTD part with dc-6332.12 |
| 3.8 | Authority intersection, macro levels, HAL is no sandbox | No action: levels confirmed; "no native load on a Safe grant" already holds (claim 29). Cross-project authority: deferred |
| 3.9 | Save cut, off-thread encoding, save ID, skin patches with incarnation | Save cut: no action (P06 pins exist). Off-thread encoding: deferred. Skin patch identity: answer-17 harvest |
| 3.10 | Five-review contract table | No action here: hand to the harvests of answers 14-17 |
| 3.11 | Continuation vs semantic replay vs publication replay; seed insufficient | Deferred (waits on the R2 tape, dc-hsa5.20) |
| 4 (head) | Stages are proposals | No action |
| Stage 0 | Trace fields, prose fixes, route baseline | Prose: **18.c**. Trace fields and route baseline: no action (no consumer depends on them; light process) |
| Stage 1 | Owner admission, reentry, leases | **dc-hsa5.12**, **dc-hsa5.20.5**, **dc-hsa5.20.7**; leases with dc-snz.21 |
| Stage 2 | Bounded native concurrency | **dc-hsa5.20.5** and **dc-snz.23** |
| Stage 3 | OneCalc values-only surrogate | Deferred: waits on dc-6332.15. Consistent with the values-only OneCalc rule |
| Stage 4 | DNA Calc session isolation, journal, recovery | Deferred: waits on Stage 3 and dc-6332.15 |
| Stage 5 | Finite parallel work where it pays | Deferred: perf after publish (owner, 2026-10-27); needs 18.a first |
| Correctness vs performance targets | Trace-level checks; 8 ms / 100 ms / 50 ms targets | Correctness list: no action (honesty rules cover it). Targets: deferred with Stage 5 |
| 5 common capture protocol | Fixture conventions, private logs, thread IDs from the fixture | No action: the x-series and P-series fixtures already work this way |
| E1 | Command-sensitive ordinary/#/$ and 64 | Mostly measured (x17, P03, x13, X0). Remainder (application-level F9 with `$` and a dirty producer) goes to **18.d** |
| E2 | Callback permissions, caller shapes, nested capacity | Statuses measured (finding 5). Name-wrapper lane and caller shapes: dc-6332.12 / dc-6332.13 (D.11). Nested pool exhaustion through xlUDF: no action, Excel answers 32 |
| E3 | Cached VBA reads and frontiers | Measured (P01). Macro read / Range.Calculate / read variant: **18.d** |
| E4 | Restart, leaf reentry, draws | Measured; implementation **dc-hsa5.20.10** |
| E5 | Iteration update order | Measured with MTC off. MTR-on variant: **18.d** |
| E6 | Cancel key | **18.d** |
| E7 | Async identity, stale, duplicate, ownership | dc-6332.13 (answer 13's D.7) |
| E8 | RTD topic identity, coalescing, old leases | dc-6332.12 (D.14); lease identity done in dc-snz.23.2 |
| E9 | Synchronous events, ByRef cancel, save cut | **dc-hsa5.12**; P06 already fixes the save pin |
| E10 | Office.js batches | Deferred to the answer-14 harvest (object model and JS) |
| Fault injection table | 14 implementation tests | Split: rows 1, 11 to dc-hsa5.12; rows 2-4 to dc-hsa5.20.5; row 5 (cancel) to 18.e; rows 6-8, 12-14 to dc-6332.15; row 9 to **18.b**; row 10 deferred (cross-project). Row 3's refusal persists today as stated |
| 6.1 | Describe routes separately; fix finite claim | **18.c** |
| 6.2 | Delete "values never differ" | **18.c** (qualify, do not delete the measured restart) |
| 6.3 | `$` and `Send + Sync` are not purity | No action: already separate fields (`callable.rs:300-306`: lane, cached reads, reevaluation policy, reentry, parallel) |
| 6.4 | No blind retry after user entry | No action: already true (`recalc.rs:6064-6072` refuses an effectful continuation; `RECALC_CONTRACT.md:438-440`) |
| 6.5 | Retire general-inbox adapters | No action: none identified at the pin |
| 6.6 | Cancellation is not termination; audit `physical_returned` | **18.b** |
| 6.7 | No process per DLL | **dc-6332.15** |
| 6.8 | No automatic replay | **dc-6332.15** |
| 6.9 | No second model for surrogate/JS/UI | No action: the extend-never-duplicate rule |
| 6.10 | No async framework prerequisite | No action: `SYNTHESIS.md:190-195` already says this |
| 6.11 | Defer GPU, wasm shared memory, trace machinery | No action: agrees with the rework plan's "later, bold" list |
| 6.12 | Preserve evidence | No action: the honesty rules |
| Closing paragraph | Next decision and next implementation | dc-6332.15 plus 18.a/18.b; see Proposed beads |

---

## Claims checked against the code

| # | Claim (answer section) | Verdict | Evidence |
|---|---|---|---|
| 1 | Level route is native-only, `UNLIMITED` only; finite quantum 1024 (1.1) | confirmed | `recalc.rs:1922-1930`; `mod parallel` is `cfg(not(wasm32))` (`:2313`). The quantum is the per-evaluation instruction slice |
| 2 | The eligibility excerpt "may not be all the conditions" (1.1, 2) | partly | The predicate is complete at `recalc.rs:2328-2344` (brace at `:2345`). Two more conditions exist: `levels` returns `None` when readers are unknown (`:2348-2357`), and levels below `parallel_minimum.level` run serially (`:2498`) |
| 3 | Chunks of 32, `OnceLock` slots, worker-local scratch (1.1) | confirmed | `recalc.rs:2320`, `:2554-2601` |
| 4 | The general level route is a real executable path (1.1, 1.2) | partly | Executable in tests and bench only; threads default 1 (`state.rs:776`), no product `set_threads`, product budgets are 64 evaluations (finding 1) |
| 5 | Unfinished speculative work may replay effects or inputs (2 row 4, 3.3.7) | wrong for this route | Workers get no callable table, RTD connector or references (`recalc.rs:2565-2574`); a user-call formula is refused (`:5812-5819`) and staged (`:2517`). Pure re-evaluation redraws the same keyed stream (`random.rs:1-13`) |
| 6 | XLL pool: owned messages, capacity 1-4, owner check, module leases (1.1) | confirmed | `worker.rs:1-2`, `:30-33`, `:190-192`, `:196`, `:209-210` |
| 7 | x17: 28 + 12 rows on 16.0.20527.20150 (1.1) | confirmed as reported | `XLL_COM_HOSTING.md:1263-1297`; not re-run |
| 8 | 64 latch and return discard are visible (1.1) | confirmed | `callbacks.rs:323-328`, `invoke.rs:305-325` |
| 9 | Async admission checks; add process incarnation (1.1) | confirmed; incarnation absent | `recalc.rs:6513-6568`. Token id comes from a process-static counter (`:2116-2123`); `CommandId.client_session` is the document identity (`:1459`, `:1471`), not a process |
| 10 | CalcVBA frame, single-use continuation, `InvocationScope` restores the parent (1.1, 3.5) | confirmed | CalcVBA `calcvba-host-api/src/invocation.rs:7-11`, `:44-51`, `:61-73` |
| 11 | RTD `poll` vs `poll_during_evaluation` (1.1) | confirmed | `calc-com-host/src/rtd.rs:181-188` |
| 12 | Browser timer handoff (1.1) | confirmed | `dna-calc-worker/src/worker.rs:127-168`; budget 64 at `:97` |
| 13 | Finite budgets get no general parallelism (1.3, 2 row 9) | confirmed, and incomplete | True for the level route. The pool's concurrent scheduling has no budget condition (`recalc.rs:1498-1540`, `:2010-2024`) and runs under the product's finite budget |
| 14 | Rework plan's restart line is stale (1.3) | wrong | `REWORK_PLAN.md:64-66` restates measured Excel behaviour (`RECALC_CONTRACT.md:443-444`; `EVALUATOR_SUSPENSION.md:126-131`); x17 is a different question |
| 15 | `$` placement is command-dependent (1.3) | confirmed (measured) | Range.Calculate on main (`XLL_COM_HOSTING.md:1280`); formula entry on a worker (`:1185`); X0: 64 `$` calls over 16 threads (`:376-377`) |
| 16 | Thread test excerpt has no assertions shown (1.3) | true of the excerpt | Full test asserts equal published cells and values for 1 vs N threads (`threads.rs:150-178`); no callables are registered |
| 17 | Concurrent callback with a dynamic prerequisite is refused (2 row 1) | partly | Only `Attempt::Waits` is refused (`recalc.rs:2193-2211`); status 64 becomes `Reevaluate` and is supported (`:2162-2189`, `:6018-6045`). Machine-dependent through pool capacity (finding 2) |
| 18 | Pool destruction can join forever (2 row 2) | mostly wrong | `native.rs:1259-1275` forgets the pool; `close()` refuses with outstanding calls (`:1179-1181`). Residual: blocking `worker::invoke` (`worker.rs:394-412`) |
| 19 | `catch_unwind` is not native-fault containment (2 row 3) | confirmed | The code says so: `callbacks.rs:282-283` ("An untrusted native DLL cannot be isolated by this in-process adapter") |
| 20 | `physical_returned` set before the add-in regains control; "unload consumers not supplied" (2 row 5) | confirmed; consumers exist | `callbacks.rs:539`; `native.rs:1145-1162`, `:1164-1225`, `:533-541`. Pool calls set it after return (`worker.rs:264`) |
| 21 | Callback may queue behind its waiting parent or hit a borrow (2 row 6) | not checked | The VBA dispatcher was not traced. `RECALC_CONTRACT.md:597` says `Range.Value2` goes through the frame |
| 22 | A Safe ancestor's restriction lives in TLS and does not cross boundaries; verify `PolicyScope` drop (2 row 7, 3.5, 3.8) | partly | `Drop` exists (`calc-vba/src/policy.rs:64-68`). The policy is applied when a project loads: `calc-host-vba/src/lib.rs:292-299` enters the scope, and the host wrapper is built then (`calc-vba/src/runtime/mod.rs:344`, `runtime/cache.rs:134`). Stricter-outer applies to nested loads, not to calls. Each project's host keeps its own mode at run time |
| 23 | Cancel revokes and records; no physical interruption (3.6) | confirmed | `session.rs:576-594`. No user-facing cancel exists (finding 7) |
| 24 | Upstream reservation outside the excerpt (2 paragraph) | confirmed | `recalc.rs:4179-4205`, `:4261-4277`, refusal at `:2010-2024` |
| 25 | Late RTD/async after restart with reused ids (2 row 11) | partly addressed | RTD leases never recycled (`calc-host/src/recalc.rs:489-490`); async keys never reused within a process (`callbacks.rs:224-227`). Nothing survives a process restart |
| 26 | Owner thread is a semantic boundary, not necessarily the UI thread (3.1) | true as a target; today they coincide | `dna-calc-desktop/src/app.rs:703` creates the STA shell with XLMAIN/XLDESK/EXCEL7 (`shell.rs:108-120`); the server is built on the same thread (`app.rs:738-740`) |
| 27 | Off/Safe/Unguarded and grants (3.8) | confirmed | `calc-macro-security/src/lib.rs:36-45`, `:77-85`; `calc-host-vba/src/trust.rs:52-78` |
| 28 | HAL is a policy wrapper, not a sandbox; Now/Timer/DoEvents unlogged (3.8, 3.11) | confirmed | CalcVBA `calcvba-hal/src/capability.rs:12-21` |
| 29 | Do not load native code on a Safe grant (3.8) | already true | No workbook-triggered XLL load; `RegisterXLL` appears only in the member table (`calc-vba/src/runtime/spreadsheet.rs:1652`); `Declare` is policy-checked |
| 30 | `calc-func` should carry the callable/result vocabulary; typed read is the seam (3.2) | already true | `calc-func/src/udf.rs:278-292` (`UserReevaluationReason`, `ReferenceRead`) |
| 31 | Old prose predicts `xlretFailed` / `xlretNotThreadSafe` (E2) | measured, partly contradicts | `XLL_COM_HOSTING.md:1176-1180`: xlUDF from `$` = 32, SET.NAME from `$` = 2, commands and EVALUATE from `$` = 128, xlEventRegister from `$` = 0; host `callbacks.rs:574-588` |
| 32 | E3 read frontier unknown | largely measured | P01: old cache 2 when calculating only the observer; reads around WorksheetFunction.Sum stayed 2 (`RECALC_CONTRACT.md:33-43`) |
| 33 | E5 iteration predictions | arithmetic confirmed; order measured | A-first (0.5, 0.75) then (0.875, 0.9375); simultaneous (0.5, 0.5) then (0.75, 0.75): checked. Gauss-Seidel in cell order measured with MTC off (`RECALC_CONTRACT.md:51-53`) |
| 34 | E9 save cut timing unknown | partly measured | `RECALC_CONTRACT.md:27-28` (P06; pins include admitted BeforeSave edits) |
| 35 | Nested `$` children can exhaust the pool (2, E2, Stage 2) | not reachable via xlUDF | Excel and the host answer xlUDF from `$` with 32 (claim 31). Other nested routes (worksheet functions through callbacks) are serviced by the owner (`worker.rs:332-337`), not by a pool slot |

Claims about Excel the answer itself labels as hypotheses are in the next section.

---

## Experiments

Calls are Excel runs or answer-service calls. "Desktop" means the Windows Excel desktop with our fixtures (XLL,
VBA events, keystrokes). "Service" means the Excel answer service. Predictions are the answer's, corrected where a
measurement already exists.

| Exp | Question | Inputs | Predicted outcome per hypothesis | Calls | Where | Status / bead |
|---|---|---|---|---|---|---|
| E1-rest | Under application F9 (smart, full, rebuild), does a `$` reader of a dirty producer get 64 on a worker and a fresh entry? | x17 `QU$`, `QU`, `QU#` probes; A1=1, C3=`100*A1`, B3=`ReadRef(C3)`; reader first; A1 to 2 in Manual; 200 copies so workers are used; MTR off, 1 thread, 16 threads | Worker 64: 64 on a worker thread, return discarded, new entry reads 200. Producer first: one entry reads 200, no 64. Owner only: all entries on main | 3 commands × 3 MTR settings × 3 registrations × 2 orders = 54 runs | desktop | **18.d**. P03 and x13 already cover Range and formula entry |
| E1-ours | Does our engine refuse `=F$(INDIRECT("C"&D1))` when the target is dirty, and does the CPU count change it? | Same formula in the host with the fixture `$` export; pool capacity forced to 1 and to 4 | Today: refusal "dynamic prerequisite inside a concurrent scalar callback" at capacity ≥2; deferral at 1. Fixed: same result at both | 0 Excel; 2 host runs | Windows host | **dc-hsa5.20.5** acceptance |
| E2-rest | Does a name wrapper or CSE/spill caller change the lane or caller shape? | D.11 inputs of answer 13 | Name forces owner vs propagated eligibility; Caller SRef vs CSE area | about 24 runs | desktop | dc-6332.12 / dc-6332.13 |
| E3-macro | Does a macro see a new frontier after a nested `Range.Calculate`? | Manual; A1=1, C3=`10*A1`; macro reads C3, A1:=2, `Range("C3").Calculate`, reads C3; VBA UDF variant attempting the same | Fixed snapshot: 10 then 10. New frontier: 10 then 20. UDF variant: error or refusal recorded | 2 variants × 10 fresh workbooks = 20 | desktop (VBA) | **18.d** |
| E4 | Restart extent and draws | (measured) | Excel restarts the formula and re-enters prefix UDFs | 0 | - | measured; build in **dc-hsa5.20.10** |
| E5-MTR | Is the measured sequential iteration order unchanged with MTR on? | D1=0, A1=`IF($D$1=0,0,(B1+1)/2)`, B1=`IF($D$1=0,0,(A1+1)/2)`; iterations 1 and 2; MTR off/on | Sequential A-first: (0.5, 0.75), (0.875, 0.9375). Simultaneous: (0.5, 0.5), (0.75, 0.75) | 2 counts × 2 MTR × 2 (application, Range) = 8 | service if it can set iteration and MTR; else desktop | **18.d** |
| E6 | What does Esc during calculation keep, accept and leave dirty? | A1=1, B1=`2*A1`, C1=`LongProbe(B1)`, D1=`C1+1`; Manual; A1 to 2, full calc, Esc after the probe's entry marker; variants: built-in array kernel, VBA loop with/without DoEvents, XLL polling xlAbort, finite 5 s native call; then edit + smart calc; one save/reopen case | Prefix kept vs rollback; late native return accepted vs rejected; cooperative-only vs runtime interruption of VBA | 5 variants × 3 repeats + 2 save cases = 17 | desktop (keystroke, `EnableCancelKey`) | **18.d** |
| E7 | Async stale, duplicate, close, ownership | answer 13 D.7 inputs | Invocation-bound vs cell-bound; single-use vs repeatable; owning copy vs shared buffer | about 18 | desktop | dc-6332.13 |
| E8 | RTD sharing, coalescing, old leases | answer 13 D.14 inputs | Shared topic one ConnectData (measured in dc-snz.23.2); coalescing vs per-notification | about 12 | desktop | dc-6332.12 |
| E9 | Change nested in BeforeSave; CalculateBeforeSave × Cancel | A1=1, B1=`A1*10`; handlers as in the answer | Nested vs queued Change; prior file kept on Cancel; B1 cache 10 vs 30 by save mode | 2 × 2 × 2 = 8 plus reopen | desktop (VBA events) | dc-hsa5.12 |
| E10 | Office.js batch, load and event order | answer's outline | Snapshot vs post-calculation load; event before/after sync | about 10 per build | desktop with a sideloaded JS add-in | deferred to the answer-14 harvest |
| F-a | Level workers and user calls (our code) | threads.rs-style workbook with a registered test callable; threads 1 vs 4, `parallel_minimum(1,1)` | Today: refusal at 4, value at 1. Fixed: equal | 0 Excel | any native | **18.a** acceptance |
| F-b | Async return then more module code during close (our code) | Fixture export: completes with xlAsyncReturn, then sleeps 200 ms and writes a module static; host closes right after the completion is accepted | Today (inferred): `FreeLibrary` may run while the thread is in the module. Fixed: module stays resident | 0 Excel | Windows host | **18.b** acceptance |

Excel batches of tens of runs take seconds to minutes; E6 is the only one that needs keystroke timing control.

---

## Proposed beads

**18.a Level-parallel workers must not refuse formulas with user calls** (priority 2, Sonnet, **start first**).
In `crates/calc-engine/src/recalc.rs`, `parallel::evaluate_level` calls `evaluate_with` with no callable table
(`:2565-2574`), so any node whose plan `has_user_calls()` returns the refusal "a callable provider missing from its
admitted command" (`:5812-5819`), and `run_level` stages it (`:2517`). The route is reachable whenever threads > 1 and
the only callables are pool callables (no fence, `:1503-1511`). Fix: return a non-`Done` attempt for such nodes on a
worker, so the owner processes them in order (the existing `_ => process(...)` arm), or exclude user-call nodes when
the levels are built. Acceptance: a `calc-host/tests/threads.rs` case with a registered test callable shows identical
published cells and values at 1 and 4 threads, and no refusal. Why first: one small change, and it removes a trap that
the first performance lane to call `set_threads` would hit. No dependencies.

**18.b Native async completion is not module quiescence** (priority 2, Sonnet; Opus reads the residency rule,
**start first**). `callbacks.rs:539` sets `physical_returned` inside `xlAsyncReturn`; `reap_cancelled_async`
(`native.rs:1145-1162`) and `Pending::drop` (`callbacks.rs:199-215`) treat it as "returned"; `close`/`unload_module`
then reach `FreeLibrary` (`native.rs:533-541`). Change: a module (and its origin) that has issued a native async handle
stays resident until process exit, as `XllHost::drop` already does on failure (`native.rs:1265-1271`). Rename the async
use of the flag to say what it means (completion observed). Pool calls keep today's meaning (`worker.rs:264`).
Acceptance: fixture test F-b; the X2/X3 lifecycle traces stay equal. Feeds dc-snz.21 and dc-snz.19. Why first: it is a
possible crash on add-in reload, a route dc-snz.21 is working on now.

**18.c Correct three design statements about threading** (priority 3, Sonnet). Docs only.
`docs/architecture/SYNTHESIS.md:184`: the level route is unlimited-budget, test and bench only, threads 1 in both
products; the pool's concurrent scheduling is the product's parallel route and runs under finite budgets.
`docs/architecture/XLL_COM_HOSTING.md:201-208`: replace the `xlretNotThreadSafe` predictions for xlfSetName and unsafe
xlUDF with the x13 measurements (`:1176-1180`), and note command-dependent `$` placement (`:1185`, `:1280`).
`docs/architecture/review-2026-11/REWORK_PLAN.md:64-66`: keep the measured restart, qualify "Values never differ" (the
re-entered prefix UDFs and RAND draws differ in count), and point to dc-hsa5.20.10. Acceptance: the three passages
cite the measured lines. No dependencies.

**18.d Excel batch: calculation interrupt, macro read frontier, MTR iteration, application-level `$` 64**
(priority 3, Opus to run and interpret; Windows desktop). Experiments E6, E3-macro, E5-MTR and E1-rest above, with the
existing fixtures (`calc-xll-fixture` `QU$`/`QU`/`QU#`, `crates/calc-engine/tests/fixtures/excel-calc-probes`) and one
new `LongProbe` export. Cases and Excel answers go beside the existing P-series and x-series case files. Acceptance:
recorded answers for each case; each prediction marked confirmed or refuted. Feeds 18.e, dc-hsa5.20.5 and dc-hsa5.11's
successors. No dependencies.

**18.e User calculation interrupt on the product route** (priority 3, Sonnet after Opus reads the 18.d answers).
Add an interrupt request to the worker wire (`apps/dna-calc-worker/src/wire.rs`) and the desktop key binding. It calls
`HostSession::cancel_calculation` (`calc-host/src/session.rs:576-594`) and implements what E6 shows Excel keeps:
published prefix, dirty state, late native return. VBA loops stop at instruction boundaries (CalcVBA
`invocation.rs:44-46`). An uncooperative native call is reported as still running, never as stopped. Acceptance:
fault-injection row 5 (cancel before entry, after effect, after return, after publication) with the E6 outcomes.
Depends on 18.d.

Start first: **18.a** and **18.b**. Both are small, have code-level acceptance, need no Excel, and remove defects on
routes other lanes are about to touch (performance work and add-in reload).

Notes for existing beads (no new bead):
- **dc-hsa5.20.5**: the refusal concerns `Attempt::Waits` only, for example `=F$(INDIRECT(...))`. It depends on the CPU
  count through `available_parallelism` (`worker.rs:30-33`). Use E1-ours as acceptance. Also check 18.a's case there.
- **dc-6332.15**: inputs from this answer, in addition to answer 13 B.8. Today one thread holds WebView2, the
  XLMAIN/XLDESK/EXCEL7 windows, the STA, host, engine, VBA and the XLL main lane (`app.rs:703-740`). The async token's
  id is a process-static counter and its command identity is per document (`recalc.rs:2116`, `:1459`). RTD leases
  already carry an incarnation. Command receipts are retry-safe within a live session (dc-971p.26). The blocking
  `worker::invoke` route (`worker.rs:394-412`) is the owner-hang case. Also from the answer: section 3.7, unknown
  outcome wording, inert recovery mode (loading a DLL is execution), two-phase close, no replay.
- **dc-hsa5.12**: E9 inputs and fault-injection rows 1 and 11.

---

## Deferred list

| Item | Waits on |
|---|---|
| Stage 3: OneCalc values-only surrogate (implementation) | dc-6332.15 design note |
| Stage 4: DNA Calc session in the surrogate, controller journal, recovery, save reconciliation | Stage 3 working, and dc-6332.15 deciding the commit point |
| Runtime-group splitting, .NET/Python coexistence matrix | A concrete incompatible pair (dc-6332.13 D.19) |
| Stage 5: finite-budget level parallelism, 8 ms / 100 ms / 50 ms targets | Publication of dnacalc.com (owner: perf after publish); 18.a; dc-hsa5.20.5 |
| Off-thread save encoding over an immutable cut | Stage 4 or the performance phase |
| Policy across projects (Safe caller into a Full project) | Cross-workbook `Application.Run` or project references becoming reachable; today policy is per project at load (claim 22) |
| Reproducibility record beyond the capture lineage (3.11) | The R2 tape in dc-hsa5.20 |
| Office.js batch semantics (E10) | Answer-14 harvest |
| Five-review contract table (3.10) | Harvests of answers 14-17 |
| E7, E8 Excel runs | dc-6332.13 step 5, dc-6332.12 |
