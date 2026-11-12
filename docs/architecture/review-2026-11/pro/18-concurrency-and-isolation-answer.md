# DNA Calc concurrency and isolation review

DnaCalc revision: `3b5aa86b3ba47cbd216daec70c5d6586ad520deb`.

CalcVBA revision: `8115aa78135114c1dea79f26cbc12c8a3b00a4cf`.

This review uses the supplied packet. Repository links identify the pinned excerpts; they do not imply additional repository inspection. No builds, cargo commands, Excel runs or external research were performed for this answer. Proposed interfaces and experiment predictions are not implemented or measured behavior.

## 1. Verdict on today's design, with evidence

**Keep the architecture's existing spine: one owner-controlled host and graph, resumable local evaluation, explicit invocation/read identities, checked result admission, and a separate bounded native-call pool. Complete those contracts before broadening concurrency. Add process isolation around the existing session composition.** The packet supports a substantial implementation, but not a claim of complete Excel MTR, lifecycle or recovery compatibility. **[INFERENCE]**

The most immediate functional gap is the explicit refusal of dynamically discovered prerequisites inside concurrent scalar callbacks. The most concrete availability hazard is an unconditional join of native workers during pool destruction. The most consequential design mistake would be to treat thread safety, permission to execute concurrently, permission to call the host, and permission to replay an invocation as the same property. They are independent.

### 1.1 What is executable and worth keeping

**The general engine parallel route is real and deliberately narrow. [CODE]** It is native-only and entered only for `WorkBudget::UNLIMITED`. Finite work sets a local quantum of 1024. The displayed eligibility predicate requires multiple threads, no interest selection, no fences, no suspended/local activation or placement, a silent observer, no existing failure, an initial wave with empty ready/wait sets, a sufficiently large order, and no iteration, cycles, spills or data tables. The excerpt ends during the predicate: it does not establish that these are all the gates. [recalc.rs:1922–1929][engine-budget] [recalc.rs:2322–2344][engine-eligible]

Workers take chunks of 32 positions, create local scratch and place attempts in position-indexed `OnceLock` slots. Owner participation and worker-local activations are sensible. This establishes stable collection positions, not by itself a stable order of user-code effects or final publication. Unfinished speculative work is reset for subsequent owner processing; that requires an explicit replay-safety proof at admission. It is not, from these trimmed excerpts, a demonstrated unsoundness. **[CODE] [INFERENCE]** [recalc.rs:2554–2601][engine-workers]

**The XLL scalar pool is a different concurrency route. [CODE]** Its evaluator and borrowed reader stay on the main lane; owned arguments/messages cross to workers. Capacity is available parallelism clamped to 1–4. Submission checks the owning main thread, retains native modules, and creates worker-local native state. These are useful boundaries, not scaffolding. Do not move the evaluator's `Rc`, borrowed reader, apartment pointers or libffi state into the general engine pool to unify the implementations. [worker.rs:1–33][xll-pool] [worker.rs:168–213][xll-submit]

**The status-64 work materially advances compatibility. [MEASURED-EXCEL]** The packet reports two agreeing x17 runs on Excel **16.0.20527.20150**, with 28 native cases and a separate 12-row Excel-DNA fixture comparison. The recorded cases establish first-read 64 with `#VALUE!`, subsequent non-free callbacks returning 64 without overwriting their result operands, discarded attempted returns, DLLFree cleanup, fresh parent/child entries after nested `xlUDF` failure, cached reads for `#`, value-argument preparation before entry, and owner-thread entry for `QU$` under the tested `Range.Calculate`. The report says DNA Calc matches those rows. That is reported evidence for those cases, not fresh validation of this pin. [XLL_COM_HOSTING.md:1263–1297][x17]

The latch and return-discard/cleanup code are visible. They must survive refactoring and IPC unchanged, including result-buffer write semantics. **[CODE]** [callbacks.rs:313–327][xll-callbacks] [invoke.rs:305–325][xll-unwind]

**Async admission already has the right shape. [CODE]** It checks exact held-token identity, command, input and engine revisions/generations, workbook, target formula generation and locus, callable availability, requirements and continuation. The excerpt then begins environment/read-stamp validation. Extend this path with process incarnation; do not replace it with a coarse RPC request number. The complete read-stamp predicate is outside the supplied excerpt. [recalc.rs:6513–6568][async-admission]

**Owner-local VBA and RTD are appropriate foundations. [CODE]** CalcVBA's frame contains parent identity and an `Rc<dyn Any>` read view; its continuation is same-thread and single-use. `InvocationScope` restores its parent. The RTD client distinguishes normal owner polling from polling during evaluation to preserve a completed native cache until its consumer reads it. These are more specific and useful than an undifferentiated async task abstraction. [CalcVBA invocation.rs:7–10,44–71][vba-invocation] [rtd.rs:181–188][rtd-owner]

**The browser driver provides a real task handoff. [CODE]** It schedules a timer callback, advances work, posts events and schedules another task. Preserve this; a chain of already-resolved promises is not equivalent. This establishes handoff between advances, not bounded duration of every advance or of a synchronous callback within one. [worker.rs:127–166][browser-worker]

### 1.2 End-to-end route assessment

| Product route | What this packet establishes | What remains unproved or incomplete |
|---|---|---|
| Native finite-step calculation | A finite local quantum and the serial/resumable direction; existing host cancellation entry. **[CODE]** | General parallel levels are not used under finite budgets. Complete driver call sites, every long-kernel checkpoint and UI latency are not supplied. |
| Native unlimited calculation | Gated parallel levels with local scratch and ordered collection slots. **[CODE]** | Effect eligibility, exact publication order, cancellation responsiveness and full route selection require integration evidence. |
| Native XLL calculation | Owned-message pool, owner submission, local native state; measured owner-thread x17 route and visible 64 cleanup. **[CODE] [MEASURED-EXCEL]** | Dynamic prerequisites in concurrent scalar callbacks are explicitly refused. Range-based x17 success does not certify the worker path. |
| VBA/COM/RTD workbook host | Owner-local continuation/frame and RTD polling pieces; owner/apartment requirements in the CalcVBA specification. **[CODE] [DESIGN]** | Full nested dispatch, ByRef events, reload/close ordering, all VM3/JIT adapter paths and RTD teardown are not demonstrated here. |
| Browser/wasm | Actual task-level worker handoff; native general parallel branch is compiled out. **[CODE]** | No evidence here for native XLL/COM, a wasm JIT, native-style shared-memory parallelism, or a WebGPU renderer. |
| UI, document save and process recovery | Publication/save-related requirements in design prose; open surrogate design task. **[DESIGN]** | No complete desktop shell/save/supervisor implementation excerpt is provided. Treat the proposed integration contracts below as requirements, not existing defects inferred from absent excerpts. |
| OneCalc | The requested product contract is values-only. **[DESIGN]** | No OneCalc implementation excerpt here establishes all native lifecycle routes. Isolation must preserve values-only calls and explicit absence of workbook services. |

Evidence for this table is the code cited above, [SYNTHESIS.md:180–196][synthesis], [RECALC_CONTRACT.md:705–725][recalc-contract], and the open [dc-6332.15 record][surrogate-task].

### 1.3 Reconcile old prose with current code

There are three important corrections.

* **Finite parallelism is a target, not today's general level route.** The synthesis says parallel levels accept finite budgets; the code requires unlimited budget. **[DESIGN] [CODE]** [SYNTHESIS.md:184][synthesis] [recalc.rs:1928][engine-budget]
* **The broad restart statement is stale or incomplete.** The rework plan says Excel restarts while DNA resumes, and asserts that values never differ. The newer x17 route demonstrably models fresh native entries. That does not settle whole-formula prefixes, static references, VBA effects or RAND draws. Neither retain the old statement universally nor declare the whole issue fixed. **[DESIGN] [MEASURED-EXCEL] [INFERENCE]** [REWORK_PLAN.md:63–66][restart-plan] [x17][x17]
* **`$` is not a promise of worker execution under every command.** The older threading section is a design/documentation paraphrase; the recorded `Range.Calculate` case runs `QU$` on the owner. Definitions and name wrappers also need command-specific experiments before accepting the old prose as a universal rule. **[DESIGN] [MEASURED-EXCEL]** [XLL_COM_HOSTING.md:201–218][xll-threading] [XLL_COM_HOSTING.md:1280][x17]

The appendix heading mentioning RTD lifecycle does not create RTD measurements: the numbered x17 excerpt contains no RTD disconnect/termination results. Likewise, the shown thread test contains a 70,000-cell setup with two `INDIRECT` formulas, but the excerpt stops before its comparison assertions. Its title is not proof that it executed or passed. **[CODE]** [threads.rs:105–139][thread-test]

### 1.4 What the reported totals mean

| Evidence population | Reported result | Limit |
|---|---:|---|
| Oracle formula-cell corpus | 76,903 / 78,264; 1,361 failures | Reported retained oracle comparison; refusals and missing evidence remain failures. [source][oracle-count] |
| Full function bank | 3,895,333 / 3,910,795; 15,462 failures | Different row population and earlier recorded calculation baseline; not a concurrency benchmark. [source][bank-count] |
| Capture batch 1 | 130,849 / 134,577; 97.22% | Only the captured subset: 289 of 486 selected files. Capture/compare failures remain in the denominator. [source][capture-count] |
| Enron saved-cache comparison | 12,961,186 / 19,666,149 saved-value agreements | **Zero live passes.** Saved decimal caches have unknown freshness. Other link-cache agreements are separately reported. [source][enron-count] |

The first three are reported **[MEASURED-EXCEL]** comparisons of their stated populations. The last is explicitly saved-cache evidence, despite its appendix classification. None establishes correct invocation counts, callback permissions, owner affinity or failure recovery. Do not merge these denominators or claim fresh runs.

## 2. Specific defects and risks, ranked

Severity here means priority for the requested capability. A demonstrated refusal is a real compatibility limit; a missing proof is not an invented implementation defect. All locations below refer to the two pins above.

| Priority | Exact pinned file:line | Trigger and observable consequence | Evidence status | Smallest reproducer or missing evidence |
|---|---|---|---|---|
| **High** | `crates/calc-engine/src/recalc.rs:2193–2210` | A concurrently scheduled scalar callback discovers an additional unfinished prerequisite. Its activation is taken and a typed refusal is staged. | **[CODE] Demonstrated limitation.** Not transparent serial fallback; not proof of a cycle. | A `$` probe takes a row number, constructs the target reference at runtime and coerces a dirty producer. Confirm actual concurrent admission; compare owner, pool and Range routes. |
| **High** | `crates/calc-xll-host/src/worker.rs:57–62,203–205` | An entered native job never returns, or waits for a callback from the thread destroying its pool. Pool destruction cannot finish joining. | **[CODE] Unbounded join. [INFERENCE]** Close/UI impact depends on ownership and call sites not shown. | Hold a worker at a native barrier; independently request cancel, close and exit. Repeat with a worker awaiting owner callback. |
| **High** | `.beads/issues.jsonl:82`; `crates/calc-xll-host/src/invoke.rs:300` | An in-process native crash/hang can exceed host recovery mechanisms. dc-6332.15 is still an open design-note task. | **[CODE]** direct FFI; **[DESIGN]** missing delivered surrogate; **[INFERENCE]** failure exposure. `catch_unwind` is not arbitrary native-fault containment. | Disposable fixture process: crash after an external marker, hang after entry, then verify controller recovery of acknowledged edits. |
| **High** | `crates/calc-engine/src/recalc.rs:2576–2579`; `:2322–2344` | An unfinished speculative evaluation is reset/reprocessed. If an observable prefix can enter, replay may repeat effects or consume different inputs. | **[CODE]** reset exists; **[INFERENCE]** proof obligation, not established unsoundness. Eligibility excerpt is incomplete. | Count user entries and nondeterministic admissions before a dynamic wait. Show exclusion or retained semantics; do not assume an omitted gate is absent. |
| **High** | `crates/calc-xll-host/src/callbacks.rs:538–545` | `physical_returned` becomes true inside `xlAsyncReturn`, before control returns to add-in code. Using it as module-quiescence proof would permit premature unload. | **[CODE]** marker placement; **[INFERENCE]** unsafe if so consumed. Actual unload consumers are not supplied. | Return async value, then block and execute more module code. Reload/close with retained timers, COM objects and queued callbacks. |
| **High** | `docs/architecture/SYNTHESIS.md:187–189`; `CalcVBA/crates/calcvba-host-api/src/invocation.rs:53–71` | A synchronous native/VBA callback queues behind its waiting parent, or reenters a borrowed `HostState`. Deadlock or borrow panic may occur. | **[DESIGN]** known architecture hazard; **[CODE]** frame restoration exists; full dispatcher evidence missing. | VBA → WorksheetFunction → child UDF → cached read, with unrelated edit queued. Require child progress, correct parent restoration and no unrelated mid-event edit. |
| **High** | `crates/calc-vba/src/policy.rs:25–59`; `crates/calc-host-vba/src/trust.rs:52–76` | A Safe ancestor crosses a worker/IPC boundary or calls a Full-granted project. TLS alone does not transport its restrictions. | **[CODE]** stricter same-thread entry is preserved; **[INFERENCE]** cross-boundary audit needed. No demonstrated bypass. | Safe → Full → host/HAL operation through local, worker and IPC paths; check denial, ancestry and audit. |
| **Medium–high** | `crates/calc-host/src/session.rs:576–594`; `CalcVBA/crates/calcvba-host-api/src/invocation.rs:44–50` | Cancel removes demand and revokes handles when serviced; a synchronous callback or native routine can continue physically. | **[CODE]** cancellation/continuation contracts; **[INFERENCE]** interruption latency is not established. | Cancel separately in a yieldable kernel, VM3 loop, JIT loop, callback and uncooperative native entry; then edit/calculate/save. |
| **Medium** | `docs/architecture/SYNTHESIS.md:184`; `crates/calc-engine/src/recalc.rs:1922–1929` | Finite-budget product routes are assumed to receive the documented general parallelism. They do not enter that branch. | **[DESIGN] [CODE] Demonstrated contract drift**, not necessarily a correctness bug. | Record chosen route and owner-block time for finite/unlimited native runs and wasm. Desktop driver choices are missing. |
| **Medium** | `docs/architecture/review-2026-11/REWORK_PLAN.md:64–66`; `docs/architecture/XLL_COM_HOSTING.md:1269–1296` | Old “resume/no value difference” prose is used either to excuse effect mismatch or to ignore current fresh-entry behavior. | **[DESIGN]** stale broad claim; **[MEASURED-EXCEL]** narrower newer result. | Count a RAND/counting prefix before a forced uncalculated read; distinguish whole-formula restart, leaf reentry and no pending read. |
| **Medium** | `crates/calc-com-host/src/rtd.rs:181–188`; `crates/calc-engine/src/recalc.rs:6520–6568` | Late provider/RTD traffic after reload or process restart is accepted using reused local identifiers. | Existing admission checks are **[CODE]** strengths; process/RTD incarnation coverage is **missing evidence**, not a demonstrated stale acceptance. | Reuse local request/topic IDs after restart and inject old messages; verify rejection before publication and no duplicate events. |
| **Medium** | `docs/architecture/RECALC_CONTRACT.md:719–724`; `crates/calc-host/tests/threads.rs:105–139` | Isolated engine values or type shapes are presented as evidence for UI publication, save cuts or event ordering. | **[DESIGN] [CODE] Evidence gap.** No complete save/UI route is supplied. | Edit → recalc → callback observation → rendered patch → save/reopen, with cancel and late input at each boundary. |

The unbounded `mpsc` queue in the pool does **not** by itself prove unbounded submitted work: upstream reservation/backpressure code is outside the excerpt. Request that evidence instead of inventing a queue-exhaustion defect.

## 3. Recommended target design and the smallest migration from today's code

Everything in this section is **[PROPOSED]**, except explicitly cited descriptions of current code or earlier recommendations.

### 3.1 State owners and product compositions

Use one portable semantic implementation with several hosting compositions. Do not create a second scheduler, evaluator, graph, workbook object model or verdict system for remote execution.

| Facility/state | Native owner | wasm owner and behavior |
|---|---|---|
| Live `CalcHost`/session, authored mutations, graph mutation, command admission and publication | One session owner thread; an STA in the Windows compatibility composition | One dedicated worker's event loop |
| VBA VM3/JIT execution, project globals, runtime objects, finalizers, active invocation stack | That same owner; compiled code/state generations are leased | Portable backend on the same worker where supported; no assumption that native JIT is available |
| Ordinary and `#` XLL calls, commands, COM activation/events, RTD server calls and compatibility windows | Same owner STA; the tested Range.Calculate `$` route also uses it | Native facilities explicitly unavailable; typed platform/capability refusal |
| Eligible pure formula work | General workers with immutable cuts/plans and worker-local scratch/continuations | Serial continuation execution initially; any extra workers receive the same checked immutable jobs |
| Eligible `$` native calls | Existing bounded stable XLL pool; native frames, operand storage and frees stay on the executing lane | Unavailable |
| Async native producer work | Add-in-controlled threads within its process; only admitted completion operations enter the host | Portable external providers use the host's existing async contract |
| Source/IR compilation | Workers only for demonstrably non-affine inputs; installation remains owner work | Owner or separate compiler worker when available |
| Compilation needing COM/type libraries/HAL services | Owner-dispatched service work; “compiler” does not imply pure | Explicit portable alternative or refusal |
| Actual WebView2 shell/windows and rendering | Controller UI thread/process | Browser UI thread |
| Recoverable authored checkpoint and committed journal | Controller storage, written from the single live host | Portable persistence service, with explicit durability capability |
| Save encoding | Worker over an immutable owning save cut; events/native persistence callbacks stay owner work | Worker-local or separate serialization work over the same kind of cut |

The owner thread is a semantic execution boundary, not necessarily the physical UI thread. If framework compatibility requires local `XLMAIN`/`EXCEL7`-style discovery, those compatibility windows belong to the native owner in its process. They do not make every task pane, ActiveX control, subclassing operation or modality interaction with the real shell transparent.

The CalcVBA specification already explicitly keeps native references on their apartment and execution on the VBA owner; `Arc` ownership is not permission to execute on another thread. **[DESIGN]** [HOSTING_AND_EARLY_BINDING.md:40–45][vba-affinity]

### 3.2 Existing contracts to extend

Keep `EngineState`, `RecalcWork`, `EvalLocal`, `UserFunctionContext`, `ReferenceRead::{Value, Pending}`, current command/pass/invocation identities, checked callable generations, `AsyncToken`, `CalcHost`/session, CalcVBA `InvocationFrame` and the native pool. The packet either shows these types or describes their current integration. Add transport/permission fields where needed; do not maintain a competing “remote workbook” or parallel validity test.

A small descriptive sketch illustrates the boundary; names are proposals, not claims about existing API signatures:

```rust
// PROPOSED: permission and placement are separate from representation safety.
enum ExecutionLane {
    Owner(OwnerLaneId),
    PureWorker,
    NativeWorker(WorkerReservation),
}

struct CallAdmission {
    invocation: InvocationId,       // reuse current identity family
    parent: Option<InvocationId>,
    command: CommandId,
    provider: ProviderGeneration,
    lane: ExecutionLane,
    caller: CallerDescriptor,
    reads: ReadViewHandle,
    callbacks: CallbackPermit,
    authority: AuthorityHandle,     // issued/resolved by the host
}

enum StepProgress {
    Yielded,                       // same retained activation
    NeedsPrerequisite(RequirementId),
    AwaitingExternal(AsyncToken),
    Returned(OwnedResult),
}

enum AttemptEnd {
    Completed,
    Reevaluate(ReevaluationReason), // a new semantic attempt
    Cancelled,
    Superseded,
    ProviderLost,
}
```

`calc-func` should expose the engine-neutral callable capability/result vocabulary needed by `calc-fml`; it should not expose a WebView2 object, Windows handle, scheduler-runtime type or general mutable host. The existing typed reference read is the right seam. `calc-engine` decides dependency waits and attempt transitions; `calc-host` resolves authority, caller and contextual read access; runtime adapters implement ABI and affinity.

Ownership transferred between threads needs the appropriate `Send` bound; shared references need `Sync`. Require both only where the route actually needs both. These bounds are not evidence of purity, deterministic behavior, freedom to change call count, or eligibility under the current command. An activation may remain worker-local across slices without being `Send`. No unsafe trait implementation should make an owner-bound object movable.

A checked pure-worker view contains only immutable inputs, placed references/plans, admitted nondeterministic inputs and owned output/evidence buffers. A native-worker permit allows precisely the callbacks that its invocation context permits. `$` alone grants neither pure-worker eligibility nor authority to execute a worksheet command. A registration can be worker-safe yet effectful, stateful or externally timing-sensitive.

### 3.3 Scheduling and observable order

Preserve these invariants:

1. **One admitted command determines demand and scope.** Interest changes priority within that command; it cannot expand its semantic range or recreate entered calls.
2. **A yield retains its invocation, prefix effects and consumed inputs.** Reaching a budget limit is never a reason to rerun a UDF.
3. **Excel-required reevaluation ends an attempt.** Fresh entries receive fresh invocation identities under the same command/pass lineage. Their repeated effects are recorded because they actually occurred.
4. **A worker completion is a candidate until owner validation.** It cannot independently mutate authored state, graph structure, spill ownership, dirty requirements, RTD subscription state or save state.
5. **Publication is an atomic semantic unit.** Values, spill ownership/removals and their stamps move together. The next observing callback enters only after the required preceding publication handoff. Preserve the existing `publish_ready` obligation for headless drivers. **[DESIGN]** [RECALC_CONTRACT.md:719–724][recalc-contract]
6. **Stable collection order does not repair earlier effect reordering.** Position-indexed slots solve collection; invocation admission and callback frontiers solve observability.
7. **Pure speculative replay needs explicit proof.** An unfinished worker attempt may be restarted only if no user entry/effect has occurred and its admitted nondeterministic inputs are reused appropriately, or another specifically justified replay contract applies. Otherwise retain a local continuation or choose the owner before entry.

Do not serialize every `$` call merely to obtain a convenient global trace. Some add-ins can observe concurrency and coordinate calls; imposing serialization can alter results or progress. Conversely, do not let work stealing reorder owner-only calls, prefix publication, iteration updates or volatile input admission.

The target is the measured causal order: owner versus worker relationships, prerequisite order, invocation ancestry, callback status/result writes, permitted overlap, read visibility, entry counts and accepted external inputs. Numeric OS thread IDs and wall-clock gaps are diagnostics, not portable identities. If they are actual UDF outputs, they are environment-dependent inputs and cannot be silently normalized into a bit-exact pass.

For nondeterministic native races, capture controlled schedules when possible. A set of permitted oracle traces needs separately retained evidence for its members; “MTR is nondeterministic” is not an excuse to accept an arbitrary wrong numeric result. A faster implementation can eliminate unobservable waiting without changing a recorded controlled case.

Keep iteration, cycles, spill competition, data tables and other presently excluded paths on their existing semantic route until their read/order contracts are established. Preserve each formula's arithmetic operation order and relevant worker floating-point environment; this review does not authorize numeric reassociation or a kernel redesign.

### 3.4 Finish dynamic native prerequisites without blind fallback

The initial migration should retain today's typed refusal while pre-entry classification routes known unsupported cases to the owner. Never call that classification a general fallback after effects have occurred.

The next supported concurrent route should be:

1. The owner reserves native capacity, prepares permitted value arguments and admits an invocation.
2. The native worker enters the UDF. A reference callback is serviced using that invocation's read policy and authority.
3. An **admitted, unfinished producer** produces the existing typed `Pending` result. For ordinary/`$` registrations with `XllUncalcedUnwind`, the adapter immediately returns the measured 64 behavior; it does not block the worker waiting for another worker. A reference outside admitted scope follows its contextual cached-read, explicit demand-admission or refusal policy. This concurrency repair does not silently expand Range.Calculate scope, and invalid references or forbidden callbacks do not become status 64.
4. The native frame latches 64. Nested `xlUDF` failure propagates to the parent as measured. Later non-free callbacks follow the latch; attempted return values are discarded.
5. Native stacks return through their original lanes; owning copies/frees and module leases finish correctly. Capacity is released after actual native return, not after logical cancellation or a completion notification.
6. The engine records and schedules the prerequisite **within the command's admitted demand** and, when ready, admits a **new semantic attempt**. Existing effects are neither rolled back nor silently erased. Independently admitted work may already have progressed concurrently; this sequence does not invent a global barrier around unrelated producers.

If the add-in ignores 64 and never returns, this becomes a hung provider, not a dependency scheduling problem. If a nested child is not permitted from a worker context, return the measured denial; do not secretly execute it on the owner and thereby turn a forbidden callback into success. Nested calls that are permitted must not depend on obtaining an additional exhausted pool slot. Their local/owner dispatch behavior needs explicit compatibility coverage.

This extends the existing `ReferenceRead` and native unwind route. It does not introduce a second evaluator or a special dependency graph inside the XLL host.

### 3.5 One VBA owner with invocation-aware reentry

All VM3/JIT execution, globals, runtime objects, COM references, finalizers and synchronous events stay on the owner. Background compilation may produce immutable code artifacts, but native type resolution and installing code generations remain owner-controlled.

On **every** start/resume/callback, install the appropriate invocation frame and effective policy, then restore the parent on exit. CalcVBA's shown `InvocationScope` already has this restoration. The policy excerpt proves Safe-preserving entry and comments on restoration, but does not show its `Drop` implementation; verify that separately. **[CODE]** [CalcVBA invocation.rs:61–71][vba-invocation] [policy.rs:40–60][vba-policy]

Use an invocation-aware dispatcher, not a second unrestricted inbox:

* Descendant callbacks of the currently active invocation can be serviced while its parent waits.
* Unrelated top-level edits remain queued until a permitted command boundary.
* Receiving async/RTD traffic is distinct from admitting it into the current read frontier.
* Apartment messages required for the active operation can be pumped under explicit reentry rules. A `DoEvents` path needs its own measured admission behavior; it is not permission to process every queued command.
* Do not hold a `&mut HostState`, dynamic `RefCell` borrow or broad mutex across external entry. Avoiding mutex deadlock does not prevent a same-thread reentrant borrow failure.

For a synchronous event with ByRef arguments or `Cancel`, the parent's continuation waits for the handler and consumes the returned argument state before proceeding. A queued notification that later updates `Cancel` is not equivalent. Native parameter pointers stay local; any remote boundary uses owning argument cells/logical handles and explicitly returns the changed values. Measure event sequencing; do not infer it from the UI queue.

Read views must describe **which** cache/frontier a context sees. A calculation UDF's pinned cached view is not automatically the same as a macro's subsequent `Range.Value2` read after a nested `Range.Calculate` or `DoEvents`. Keep a host-owned view handle with a documented policy; E3 below determines whether a nested operation creates a new visible frontier. Do not freeze an entire macro command at its entry snapshot by accident.

### 3.6 Yield, waits, cancellation and shutdown

| Transition | Identity and result rule | Effects and next work |
|---|---|---|
| Yield | Same activation/invocation; retain continuation and draws | Resume without repeating prefix work |
| Dependency wait | Record unmet producer; retain pure continuation where supported | No implication that an external provider is running |
| External await | Outstanding revocable provider token/lease | Admit its completion through existing freshness checks |
| Semantic reevaluation | End the attempt; create a fresh invocation as required | Previous effects remain; measured entries/prefixes may repeat |
| Supersession | New inputs/requirements revoke old publication rights | New edit can proceed without draining every old provider |
| User cancellation | Revoke demand and future acceptance; report the committed frontier | Already published units, authored edits and external effects remain |
| Physical interruption | Execution actually reaches a supported stop point | Different from receipt of a cancel request |
| Provider failure | Revoke provider/incarnation; outcome can be unknown | Explicit unavailable/incomplete state; no fabricated successful value |
| Shutdown | Stop root admissions, perform ordered owner cleanup, then drain or end process | Not a calculation restart and not rollback |

The current `cancel_calculation` revokes handles, takes work, records a cancelled completion and clears request flags. It does not show physical interruption of arbitrary native code. **[CODE]** [session.rs:576–594][host-cancel]

For a resumable kernel or VBA loop, check interruption at supported safe points. The existing CalcVBA contract permits pauses only between completed instructions and does not interrupt synchronous callbacks mid-effect. **[CODE]** [CalcVBA invocation.rs:44–50][vba-invocation]

A long kernel without internal checks, a synchronous external callback, and an uncooperative native call have no bounded cooperative stop guarantee. A timeout can revoke admission and report a problem; it cannot make in-process arbitrary native-thread termination safe. A supervisor may terminate the isolated process. Do not report that as “the operation had no effect.”

After cancellation, preserve published units and mark unfinished requirements according to the engine contract. Do not mark their old caches fresh. A subsequent edit creates new work independently of stale external leases. If the owner itself is stuck, recovery requires ending/replacing its process before another owner mutates that same session. UI responsiveness does not imply the stuck session can continue accepting semantic edits.

### 3.7 Concrete surrogate for dc-6332.15

**Choose one compatibility process per session first.** For OneCalc, place its existing values-only host and native runtimes there. For DNA Calc, co-locate the **one active** `CalcHost`, engine, canonical authored state, VBA owner, ordinary/`#` XLL lane, COM/RTD STA and compatibility windows. Retain an explicit in-process conformance composition using the same services.

This makes the first process cut mostly around session commands, publications, UI services and durability. It avoids immediately remoteizing every cached read and keeps VBA/native same-thread relationships intact. It develops, rather than treats as implemented, the earlier recommendation. **[EARLIER-REVIEW]** [answer 13:378–395][previous-surrogate]

The controller stores a recoverable checkpoint and ordered records of **resolved authored mutations**, plus any publication units promised to survive a crash. It is not a second running workbook implementation. `calc-host` is the sole semantic writer: it resolves and stages a mutation with a commit ID, expected predecessor revision, payload identity and current incarnation/write lease. The controller validates that envelope and durably appends the resolved data; this append is the recovery commit point. The owner then installs/acknowledges the recorded revision. No durable acknowledgment precedes the receipt.

A matching retransmission returns the persisted receipt; a conflicting payload, stale revision or retired writer is rejected. A lost reply is reconciled by commit ID before proceeding; it does not cause the originating macro or event to run again. The controller neither reinterprets commands nor calculates a competing model. This is one semantic writer with ordered durable acknowledgment, not consensus between independently editable workbook replicas. Retire the old write lease before admitting a replacement owner.

Recovery applies committed authored data and retained cache/publication records without invoking their original macros, events, UDFs or external services. Do not replay raw macro commands to reconstruct state. Partial macro progress that was already committed survives; uncommitted work may be lost. External effects can remain unknown. Checkpointing is not an atomic transaction across a database, file system, email service and native add-in.

Start a replacement in recovery mode with affected user-code entry disabled. Restoring package bytes and registration metadata must not load code images or initialize user runtimes when doing so executes user code. DLL loading/`DllMain`, managed module/static initialization, Python imports and COM activation can have effects before any explicit open hook. `xlAutoOpen`, `Workbook_Open`, RTD activation and automatic recalc also need an explicit recovery action/policy. Suppressing open events alone is insufficient. The failed process's incarnation is invalid even if a new process gets the same OS PID.

#### Runtime grouping and unload

Do not start with one process per DLL. Keep an xlOil core, its Python runtime/plugins and factories together; keep an Excel-DNA XLL, payload/runtime and generated COM/RTD infrastructure together. Test required .NET/Python combinations within the session process. Coexistence is a compatibility matrix to establish, not a presumed universal success or universal conflict.

Split additional runtime groups only when a concrete need justifies it and their interactions can cross as owned values, logical references and explicit services. Cross-group direct native calls, raw COM identity, shared interpreter state and assumptions about the same OS thread are not transparently preservable. For a route that requires them, co-locate the group or issue a typed capability refusal.

Default to logical deactivation plus native residency until process exit unless a real quiescence protocol proves unloading. A managed unload API, return from `xlAsyncReturn`, or disappearance of worksheet references does not establish that no runtime code, timer, queued message, class factory, COM reference or native thread remains. Preserve the earlier unload warning as a requirement, not a claim that the current pin demonstrably unloads unsafely. **[EARLIER-REVIEW]** [answer 13:397][previous-surrogate]

#### Protocol and memory boundary

Use one dispatcher with local and IPC transports. A remote message extends the existing invocation identity with session/process incarnation, runtime-group/provider generation, parent invocation, request sequence, caller descriptor, read-view handle and owner-issued authority handle. Every reply is matched to an active request and its expected kind. Requests/replies from old incarnations fail before they can affect state.

| Concern | Required boundary |
|---|---|
| Native ABI | Function pointers, operands, handles, libffi state, allocator stores and `xlAutoFree12` execute locally. Never transmit a pointer as if it were portable data. |
| Values | Owning wire representation preserves type, binary64 bits, strings, errors, array shape and missing/empty distinctions. Copy before native storage can be reused. |
| References/callers | Logical workbook/sheet/area identities, structure generation, caller kind and read-view handle. Do not eagerly coerce references or manufacture an A1 caller for OneCalc. |
| Callback replies | Preserve status **and** result-write action: write a value/error, leave unchanged, or zero storage. A plain `Result<Value, Error>` loses measured behavior. |
| Reentry | Bidirectional descendant requests carry parent IDs; pending parents service permitted callbacks. Nested `xlUDF`/WorksheetFunction uses the same permissions and attempt rules. |
| Async | Extend current token validation with process incarnation and provider/group generation. Completion receipt is not physical module quiescence. |
| COM | Stable process-local facades for logical object IDs; apartment-local lifetime. Arbitrary Office interfaces and `IUnknown` identity across unrelated processes need an explicit supported contract. |
| RTD | Server, update object and COM references stay on surrogate STA. Topic leases include provider incarnation/generation; logical input admission remains host-owned. |
| Native UI/discovery | Local exports, shims, windows and message pump where required. Real shell window discovery, subclassing, modality and embedded controls are individually supported capabilities. |

For current local async completion, preserve rejection before dereferencing a revoked handle's result operand. **[CODE]** [callbacks.rs:538–545][async-callback] IPC also validates header, size and identity before decoding or admitting a payload; it cannot protect against all memory corruption inside its native sender.

A reference handle pins the required read semantics, not just an address. If a later remote group cannot reproduce a cached frontier, do not substitute “the latest value.” The initial co-location deliberately avoids that coherence problem for most callbacks.

#### RTD, reload and teardown

RTD notification, native refresh, host input admission and consumer cache use remain separate operations. Preserve `poll_during_evaluation`'s cache-consumption distinction. A late signal from a retired topic/server may be received for cleanup, but it cannot become a new input merely because the numeric topic ID has been reused.

Retained VBA activations hold both code-generation and runtime-state leases. New entries bind the new generation; old activations finish or cancel on their owner under the old leases. Never splice new code or globals into a suspended old activation.

Close has two phases: first deliver supported cancellable close events; after closure commits, reject new roots and revoke ordinary execution and async/RTD/result admissions. Retain explicitly scoped cleanup permissions and leases for frees, finalizers and disconnect/unadvise operations. Complete that cleanup on the owner while its code, object tables, callback services and windows still exist. Preserve measured `FreeOnly` restrictions; cleanup is not a general callback exemption. Disconnect event sources/RTD and release apartment references before destroying their apartment. Finalizers require their own code/state leases and permitted callback context. Drain physically returned worker calls and free their results before releasing their leases.

Exact order among Excel close hooks, RTD termination, finalizers and events remains an experiment. A live worker can prevent physical drain indefinitely; the controller must not block on its `Pool::drop`. In an uncooperative group, end the process and report that graceful cleanup did not complete.

For an invocation whose result was not accepted, report: **“The add-in process stopped before this result was accepted. External work may have completed. Retrying will run the add-in again.”** For already accepted results, retain their receipt/publication status and report provider failure separately. Determine unknown outcomes per invocation; do not reclassify every past accepted result from the failed process. Preserve acknowledged authored edits and any durably acknowledged publications. Do not promise recovery of a publication that was only painted transiently unless the persistence contract recorded it first.

### 3.8 Authority, macro levels and native trust

Keep the existing product meanings. **[CODE]** Off holds a workbook project on open; explicit per-file enable/unblock grants can request Full. Safe runs under the Safe policy; Unguarded requests Full regardless of origin. This is not the same axis as whether a native add-in was trusted/installed or whether it runs in another process. [macro-security lib.rs:36–45,77–84][macro-levels] [trust.rs:52–76][project-grant]

Effective host authority is the intersection of session/platform capability, project grant, stricter ancestor policy, operation restrictions and callee/provider restrictions. A worksheet UDF remains restricted when physically running on the owner. A Safe ancestor cannot acquire Full authority by entering a Full-granted project, a worker, a COM facade or an IPC callback.

The host issues and validates the authority handle; the sender cannot promote itself by adding `policy: Full`. Install the validated context in destination-thread TLS for each entry/resume and restore it afterward. Audit parent/child lineage. Existing same-thread Safe precedence is a useful implementation, not a cross-process transport. **[CODE]** [policy.rs:44–59][vba-policy]

The HAL explicitly says it is a policy gate, not a sandbox; native add-ins can bypass it through direct native APIs. **[CODE]** [CalcVBA capability.rs:3–22][hal-capability] Therefore:

* Do not load arbitrary native code on the strength of a workbook's Safe execution grant.
* A Safe project reaching a native provider requires an explicit separately trusted exposure whose authority restrictions are meaningful. Otherwise refuse that bridge.
* A normal same-user surrogate provides failure containment, not automatic OS resource confinement.
* Code sharing a native process is one trust domain for hostile-memory assumptions. Per-invocation capability bookkeeping is not protection against malicious code reading another invocation's in-process state.

A separate restricted OS process policy could constrain native access, but that is a separately specified capability, not an implied meaning of Off/Safe/Unguarded. Do not expand this task into speculative sandbox infrastructure.

### 3.9 Save and host-to-skin integration

The host creates a save cut after the command's required synchronous events and calculation behavior. It contains an authored revision, matching publication/cache frontier, freshness/calculation metadata, code/project artifact generation and preserved package content. If a save event edits the document, choose the resulting cut according to measured event order; do not capture before the event and ignore its changes.

Serialize an owning immutable cut off-thread. Background encoding cannot call live VBA, COM or mutable host objects. Later RTD/async arrivals cannot mutate that cut. Saving revision R while edits advance to R+1 can succeed, but R+1 remains unsaved; a successful serializer is not permission to clear the latest document's dirty flag.

Serialize cancellation and the save commit decision by save ID. A cancellation **accepted before that decision** prevents file publication, abandons temporary output and preserves the previous file. Once publication has begun, do not acknowledge an aborted save while replacement can still occur; finish or reconcile the actual file outcome. A late cancel request cannot undo a completed save. A crash between file publication and acknowledgment requires reconciliation by save ID and committed file evidence; do not automatically execute save events again. Macro effects and authored edits made before cancellation remain. Keep existing typed save refusals for preserved references that cannot be transformed safely; the tracker reports such a deliberate refusal. **[DESIGN / reported checks]** [.beads/issues.jsonl:455][tracker-checks]

Host-to-skin patches carry session incarnation, authored revision and publication sequence. The skin can reject stale/out-of-order patches or request a snapshot, but it cannot decide calculation freshness or synthesize workbook events from paints. Use owning batched data suitable for a renderer; that admits a future WebGPU implementation without claiming one exists. JS request batches enter the same host command/admission path; Office.js-like `sync` semantics require their own observable contract, not deductions from the native scheduler.

### 3.10 Shared contracts across all five reviews

Each row names one contract owner. Other crates can implement adapters without owning a second semantic definition.

| Shared contract | One owner | Object model / JS | Graph / recalc | Engine–host–doc | Host-to-skin IR | Concurrency / isolation requirement |
|---|---|---|---|---|---|---|
| Authored identities, structural generations and revisions | `calc-doc-model` | Stable handles resolve against these identities | Graph binds to them | File import/export uses canonical authored state | Patches identify the same objects | No independently editable surrogate or JS model |
| Command admission, project grants and effective authority | `calc-host` | Batches and macros submit commands here | Engine receives admitted demand/context | Save/open/edit commands share the gate | UI requests are commands, not direct mutations | Owner-issued context survives nesting, worker and IPC transport |
| Calculation attempts, requirements, dynamic prerequisites and async freshness | `calc-engine` and its existing contract module | API observes explicit completion/freshness | Sole scheduling and graph semantics | Host consumes engine receipts | Display observes status, never invents completion | Reuse identities and token checks; differentiate yield from reevaluation |
| Callable capability/result vocabulary | `calc-func` | Host adapters project supported calls | Evaluator uses one checked interface | Runtime providers implement that interface | Only user-visible failure/result projections escape | Lane, read policy, callback permission and replay eligibility remain separate |
| Runtime invocation registry, contextual read-view binding and object lifetime | `calc-host` | JS/VBA/COM objects resolve through one host | Engine supplies calculation identity/read evidence | Host owns active code/state/object leases | UI never receives native pointers | CalcVBA's local frame maps to existing identities; no second invocation model |
| Publication unit and freshness evidence | `calc-engine` | Observing reads request the proper frontier | Values/spills/removals validated together | Host installs and snapshots published units | Renders monotonically identified units | Completion order cannot bypass fences or stale checks |
| Save-cut selection and durable commit receipt | `calc-host` | Save requests/events use the same command semantics | Supplies result/cache frontier | `calc-doc` only encodes the selected cut | Saved indicator refers to the saved revision | Controller persists resolved data; recovery does not reexecute commands |
| Wire commands and derived view-patch schema | `dna-calc-protocol` | JS bridge implements the agreed schema | No transport-specific evaluator | Uses host receipts and identities | `dna-calc-model`/skins consume derived state | Same schema supports local worker and process transport; explicit capabilities/versioning |

The necessary decisions are narrow: read-frontier semantics, attempt restart extent, synchronous event boundaries and supported native contexts. Do not create a cross-review governance framework. Record those decisions in the existing contracts and implement them once.

### 3.11 Reproducibility without accidental effect replay

Distinguish three operations:

* **Scheduling continuation:** resumes retained work; does not execute its prefix again.
* **Semantic replay for testing:** reexecutes engine decisions using captured admitted inputs and stubbed/recorded external outcomes. It is not permission to replay real native side effects.
* **Publication replay for recovery/view reconstruction:** reapplies recorded result units/receipts without executing user code. It demonstrates recovery of observations, not recalculation correctness.

A random seed is insufficient. Retain command/scope/order, initial caches and freshness, code/provider generations, caller/reference/read-view identities, policy, entry/parent relationships, actual random values and draw ordinals, time/environment inputs, async/RTD admission order and rejection dispositions. Also retain raw numeric bits/types/errors, callback output-buffer writes and event order.

The HAL audit is not that input tape: `Now`, `Timer`, `DoEvents` and several other operations are explicitly unlogged in the shown capability wrapper. **[CODE]** [CalcVBA capability.rs:19–22][hal-capability] Native code can also obtain nondeterministic inputs outside host mediation. Mark such invocations non-replayable unless a fixture or provider-specific adapter records them. Extend the existing capture lineage; do not create a universal trace VM before delivering these capabilities.

## 4. A staged implementation plan with measurable acceptance

All stages are **[PROPOSED]**. Establish baselines on the actual implementation revision when work begins; the packet's reported runs are not substitutes. Use the existing test/capture/verdict machinery, add fields/cases to it, and keep refusals, missing traces and unknown outcomes as failures for requested behavior.

### Stage 0 — Establish the actual route and evidence baseline

**Changes and dependencies:** No semantic redesign. Add or expose trace fields for command kind, selected engine/native route, invocation/parent, owner/worker lane, provider generation, callback status/result-write action, publication sequence and completion disposition. Correct the finite-budget and broad restart prose. Depends only on the pinned code/evidence inventory.

**Retained behavior/refusals:** Preserve existing eligibility, the concurrent dynamic-prerequisite refusal, native cleanup restrictions, save refusals and values-only OneCalc.

**Product acceptance:** Run the existing thread test in its complete form; confirm which assertions it actually contains. Reuse x17's 28 native and 12 Excel-DNA cases. Exercise finite/unlimited native host routes, browser task handoff and ordinary/`$`/async native calls through their real product drivers. Unsupported cases must expose the existing typed result, not disappear from the report.

**Measurements and stop condition:** Obtain a route-labeled baseline of bits/types/errors, entry/free counts, callback result writes, publication order, queue occupancy and owner uninterrupted time. Run the relevant existing integration and corpus gates with recorded lineage and unchanged populations. Stop when every selected case has an actual route and verdict; do not launch a new general benchmark/capture system.

**Lane:** Sonnet can add trace fields, rerun existing cases and correct prose. Opus decides only disputed evidence classification or semantic ambiguity, not the mechanics of logging.

### Stage 1 — Make owner admission, reentry and lifetime explicit

**Changes and dependencies:** Extend the existing invocation/read-view/callable contracts with checked lane, caller, callback permission and authority. Implement or verify descendant callback dispatch without held host borrows. Make synchronous ByRef/cancel results explicit. Add code/state/object leases around reload and owner-bound destruction. Depends on Stage 0's route inventory and the read-frontier/event experiments E2, E3 and E9.

**Retained behavior/refusals:** Preserve native status-64 semantics and every supported typed VBA continuation. Keep synchronous Variant adapters where they still implement necessary behavior; label their non-yielding interval and migrate them only after equivalent coverage exists. Native-only facilities remain explicit wasm failures.

**Product acceptance:** Workbook VBA → WorksheetFunction → nested UDF → cached reference read returns without deadlock; an unrelated queued edit enters only at the permitted boundary. Safe → Full nesting preserves Safe and restores the outer/next-call context. ByRef cancellation affects the calling save/close operation synchronously. Reload a project while a supported VM3/JIT activation is retained; verify old code/state ownership and owner-thread finalization.

**Measurements and stop condition:** For supported paths, zero wrong-thread entries/destructions, no reentrant borrow failures, correct parent restoration, unchanged x17 traces and no duplicated prefixes under yield. Every unimplemented reentry case has a typed refusal. Stop once the common contract covers current routes; do not replace the host with a general actor framework.

**Lane:** Opus settles read-view transitions, event reentry, identity ownership and authority intersection. Sonnet implements adapters, assertions and focused tests after those decisions.

### Stage 2 — Complete bounded native concurrency

**Changes and dependencies:** Preserve the current pool. Make admission/reservation and callback-service capacity explicit. Support dynamic prerequisites through the measured 64 latch, actual native return/free, then fresh attempt. Define permitted nested-call dispatch so parents do not consume every slot while queuing children to that same exhausted pool. Audit speculative pure-worker reset eligibility. Depends on Stage 1 and E1/E2/E4/E5.

**Retained behavior/refusals:** Keep owner fallback **before** user entry where permitted. Retain the current dynamic-prerequisite refusal for remaining unsupported contexts. Keep iteration/spills/data tables and unsupported effectful paths outside general parallel waves. Do not broaden permissions merely to make a nested call complete.

**Product acceptance:** Fill all available XLL slots with callbacks discovering dirty producers; verify progress through supported unwind or explicit refusal, never false circularity or unreserved-worker dependence. Repeat with synchronous nested children that do not read a dirty cell. Compare ordinary, `#`, `$`, value arguments and Range.Calculate. Inject cancellation before entry, after effect, after native return and before publication.

**Measurements and stop condition:** Track maximum entered calls, reserved/queued jobs, callback latency, entries per semantic attempt, free counts and stale publications. Require zero leaked reservations and zero scheduler-caused duplicate effects. Supported controlled cases must match exact captured bits/types/errors and relevant traces. Stop when the explicit gap is closed for the admitted contexts; do not use this stage to make every callable generally parallel.

**Lane:** Opus decides the nested-call and 64 attempt rules. Sonnet implements the queue/reservation changes and fixture variants.

### Stage 3 — Ship a values-only OneCalc surrogate

**Changes and dependencies:** Put the existing OneCalc/native composition in one session process. Add a versioned local/IPC adapter, process incarnation, supervision, owning wire values, native runtime-group manifest and explicit capability handshake. The controller retains request/input state and receipts. Depends on Stages 1–2; the design note can be written earlier.

**Retained behavior/refusals:** OneCalc remains values-only. No fake workbook, fabricated caller address or remote reference implementation. Keep an in-process conformance adapter using the same semantics. Native UI features without a real bridge stay refused.

**Product acceptance:** Existing ordinary/worker/async calls return the same values/types/errors; results and strings/arrays remain valid after native storage is reused. Callback result-write actions and frees match local execution. Crash after an external marker, hang a call, restart with reused local IDs and deliver old completions. Confirm a recovered request does not automatically enter user code.

**Measurements and stop condition:** Zero accepted old-incarnation messages, zero remote pointer use, correct release ownership, visible unknown outcomes and a responsive controller under provider failure. Record startup, call and callback overhead against the local composition. Stop after one coherent session process works; no per-DLL process manager or speculative multi-runtime routing.

**Lane:** Opus owns protocol semantics and the native compatibility cut. Sonnet owns transport, launching, value serialization and fault fixtures.

### Stage 4 — Isolate the DNA Calc session and make recovery/save coherent

**Changes and dependencies:** Move the existing live DNA Calc session composition into the compatibility process. Add a controller-owned committed checkpoint/journal, incarnation-scoped write lease, reconciliation by commit ID, publication retention policy, inert recovery mode, immutable save cuts and versioned skin patches. Depends on Stage 3 and E3/E6/E7/E8/E9/E10.

**Retained behavior/refusals:** One active authored model and graph. Keep native reference reads local initially. Preserve existing file-format preservation/refusal rules. Recovery restores committed data; it does not restore VBA/Python globals, native heaps, open handles, COM identity or continuations. Keep affected automatic calculation and provider loading held until explicitly restarted.

**Product acceptance:** Open workbook → edit → calculate → VBA/native read → publish → render → save/reopen. Inject failure before/after authored-delta persistence, lost commit reply, before/after result publication, and before/after file replacement. Every acknowledged authored commit survives. A saved revision is not confused with later edits. Late async/RTD traffic cannot revive old results or events. Close/reload runs owner cleanup where possible and terminates a hung process without blocking controller destruction.

**Measurements and stop condition:** Exactly one accepted record for a retransmitted resolved commit ID; no reexecution of the originating macro/event; no lost acknowledged edits; no stale patch accepted; save receipts agree with actual file publication. Verify retained formats and existing verdict populations. Stop once one per-session composition satisfies these cases. Runtime-group splitting remains deferred.

**Lane:** Opus decides commit points, recovery semantics, read/save frontiers and lifetime ordering. Sonnet implements transport integration, persistence and route tests under those decisions.

### Stage 5 — Extend finite parallel work only where it pays

**Changes and dependencies:** Extend the existing parallel implementation to bounded checked work rather than introducing a competing scheduler. Replace an all-work scoped join at the product boundary with owner-visible completion admission if necessary. Any retained continuation stays on its worker; jobs receive immutable cuts rather than borrowed live mutable session state. First support proven pure bounded work. Depends on the preceding correctness gates and measured baseline bottlenecks.

**Retained behavior/refusals:** Serial finite stepping remains the portable reference/wasm route. The current unlimited pure route can remain until the new driver subsumes it without divergent semantics. Do not move VBA or native affinity objects. Continue refusing unsupported effectful parallel paths.

**Product acceptance:** Native finite/unlimited and serial/parallel runs agree on controlled results, requirements and traces. Browser slices yield to actual tasks. Prioritizing a viewport changes scheduling only within allowed segments and never expands command scope or reenters user code. Cancellation during a long pure task produces a bounded cooperative response and no later stale publication.

**Measurements and stop condition:** Adopt concurrency only after measured benefit on named hardware with acceptable small-workbook overhead. Stop if correct finite stepping provides no worthwhile gain; do not keep adding abstractions to justify concurrency. Split runtime groups only for a demonstrated incompatible runtime or isolation requirement, with its own measured reference/callback contract.

**Lane:** Opus approves any new mobility/effect frontier. Sonnet can implement bounded job handling and measurements once the boundary is settled.

### Correctness gates versus performance targets

**Correctness gates:** exact recorded numeric bits/types/errors for comparable controlled inputs; measured call counts and callback statuses/result writes; correct owner/worker relationships; valid parent/policy restoration; no stale admission; no lost acknowledged authored commits; honest cancellation/unknown outcomes; preservation of existing supported routes and explicitly counted refusals. Corpus aggregate equality alone cannot satisfy trace-level gates.

**Proposed performance targets, to tune on named hardware:** cooperative owner slices around 8 ms; p95 cooperative edit/cancel acknowledgment below 100 ms; no unexplained uninterrupted controlled pure slice above 50 ms. These are engineering targets, not Excel semantics or guarantees for uncooperative user/native code. Measure owner wait, callback round trip, queued reservations, retained-view memory and physical return delay separately. A fast cancellation receipt cannot be counted as fast native termination.

For throughput, select a practical threshold after the baseline—for example, a material gain on large eligible sheets without a material small-sheet regression. Do not declare a release gate using an unmeasured speedup estimate. Reuse the existing harness and run broader corpus gates at semantic integration points, rather than repeatedly adding tests that merely mirror the implementation.

## 5. Discriminating experiments against Excel

Every new experiment below is **[PROPOSED]**. All outcomes described under hypotheses are **predictions to test**, not measured results. Reuse the existing fixtures/capture lineage, especially x17. Its recorded answers remain **[MEASURED-EXCEL]** only for the cases/build stated in the packet.

### Common capture protocol

Use a disposable workbook and add-in fixture, restored from the same saved starting copy for each variant. Record full Excel build/platform/bitness, Windows build, add-in/runtime hashes, other loaded add-ins, calculation mode, MTR setting/count, iteration options, active workbook/sheet, command/scope and formula-entry order. Treat automatic entry itself as part of the command sequence.

Record initial live caches and their provenance, edits/explicit `Range.Dirty` operations, raw binary64 bits, value types/errors, native entry/exit, callback opcode/status and output storage before/after, parentage, free calls, event order, async/RTD admission and subsequent save/reopen observations. Store logs in private memory and drain after calculation; UDFs must not mutate worksheet cells to log their execution. Per-thread buffers reduce instrumentation-induced serialization. Run separate global-counter/barrier variants when controlled order is the subject.

Obtain thread IDs through the fixture's native recorder, not extra Excel callbacks that could change permission state. Compare identity relationships and retain raw IDs. There is no supplied public per-cell dirty-state getter: infer dirty behavior from known edits/dirty commands, `CalculationState` and the cells entered by a subsequent smart calculation. Do not invent a dirty flag API.

Saved XML decimal caches are not the same evidence as live binary64 capture. Save with calculation-before-save explicitly controlled, reopen in a separate instance with a recorded mode, and record open/recalc behavior. For web/JS runs record runtime/requirement-set support. Unsupported API calls are outcomes, not silently omitted cases.

Repeat the x17 baseline on Excel 16.0.20527.20150 if available, then on each intended supported build/architecture. Build differences require retained profiles or a resolved compatibility decision. No Microsoft source or disassembly is needed.

### E1 — Command-sensitive ordinary, `#`, `$` and status-64 behavior

**Inputs and entry order:** Reuse x17's `QU`, `QU#`, `QU$`, `QQ`, `QQ$`, mixed `QUQ` and nested `xlUDF` probes. A simple variant uses A1=1, C3=`100*A1`, B3=`ReadRef(C3)`. Enter the reader before the producer in one case and reverse in another; warm to a recorded C3 cache of 100. In manual mode change A1 to 2, leaving C3's cache 100 and its producer dirty. Reset fixture counters after preparation.

**Commands:** Separate reset runs of `Range("B3,C3").Calculate`, reversed area order, worksheet calculation, smart application calculation, full calculation and full rebuild. Repeat MTR off, one worker and multiple workers. Use many independent copies and controlled finite delays for application-level cases; a tiny sheet that never uses a worker is not evidence that workers are forbidden.

**Probe outline:** Log entry thread/caller; call `xlCoerce`; initialize another result operand with sentinel bytes and repeat coerce/caller callbacks; return a marked DLLFree operand. Use the existing native API conventions and safe fixture recorder. In the nested variant, a parent calls a child through `xlUDF` and both log entries/returns.

| Question | Hypothesis and predicted observation | Competing hypothesis and predicted observation |
|---|---|---|
| `$` placement | Command-sensitive: the x17 Range route stays on owner; application calculation admits eligible worker entries | Always owner, or always worker irrespective of command: corresponding identity pattern appears |
| Dirty ordinary/`$` reference | When actually encountered unfinished: 64, attempted return discarded, producer finishes, new entry reads 200 | Producer-first scheduling: one entry reads 200 with no 64; this does not refute 64 semantics |
| `#` read | Cached frontier at entry: one entry reads 100 if entered before producer publication | Demand/new frontier: entry sees 200; entry/order evidence distinguishes scheduling from a different read policy |
| Value argument | Entry occurs once after argument preparation, with 200 | Provisional entry or repeated entry occurs before/after producer |
| Latched callbacks | First 64 writes `#VALUE!`; later non-free callbacks return 64 and preserve sentinel output; DLLFree cleanup occurs | Later callback succeeds/overwrites storage or attempted return is accepted |
| Nested unwind | Parent and child get fresh entries after the producer | Only child reenters, or parent resumes without a fresh native entry |

**Design consequence:** Admission must depend on command, registration and invocation context. Preserve exact latch/cleanup/result-write rules. A worker-path difference is not covered by owner-thread Range results. The existing concurrent dynamic-prerequisite refusal should be compared against these routes explicitly.

### E2 — Callback permissions, caller shapes and nested-call capacity

**Inputs/caches/order:** Start from a clean manual workbook with A1=7, A2=11 and no pending producers. Register ordinary, `#` and `$` versions of a probe that returns a fixed scalar and logs one selected callback. Use one callback per reset case: caller query; clean coerce; an ordinary worksheet function; XLM information; name mutation; safe/unsafe nested `xlUDF`; safe/unsafe evaluation. Add a separately forced pending read using E1. Test a direct formula, defined-name wrapper, supported multi-cell placement and macro entry.

**Command:** Range calculation versus sufficiently large application calculation, with MTR off/on. A separate legitimate async fixture attempts a benign non-async callback on its own background thread while calculation is held; compare with its permitted completion. Do not reuse the poisoned frame after a previous failure.

**Hypotheses:**

* **Physical-thread-only permission:** moving `$` to owner under Range.Calculate changes all permissions to owner behavior.
* **Registration/context permission:** `$` retains some restrictions even on owner; worksheet-function command prohibitions remain regardless of thread.
* **Broad background admission:** background caller queries succeed, versus **async-only admission**, where completion succeeds and other callbacks fail.
* **Name forces owner:** the name wrapper changes lane, versus **propagated eligibility**, where the same callable remains eligible. Capture actual entry and statuses.

For every variant the prediction is a specific permission pattern, exact status and output-write action, not merely “fails.” The old design prose predicts `xlretFailed` for some information calls and `xlretNotThreadSafe` for some unsafe worker callbacks; those are hypotheses outside the recorded cases. **[DESIGN]** [XLL_COM_HOSTING.md:201–208][xll-threading]

Add N simultaneous parent `$` functions that synchronously invoke permitted children; choose N equal to the observed active worker capacity. Use finite barrier timeouts so the experiment cannot hang indefinitely. **Same-worker nested execution** predicts child entry on the parent's worker; **owner nested execution** predicts child on owner; **separate-worker dispatch** predicts distinct worker entries or capacity-sensitive delay. A timeout alone does not identify the mechanism; use entry logs.

**Design consequence:** Caller descriptor, callback permit and physical lane need independent representation. The recorded nested behavior determines how to avoid an exhausted-pool deadlock without granting forbidden operations.

### E3 — Cached VBA reads and publication frontiers

**Minimal VBA outline:**

```vb
Function Peek(tag As String) As Variant
    Dim v As Variant
    v = ThisWorkbook.Worksheets("Sheet1").Range("C3").Value2
    TraceInMemory tag, v, Application.Caller
    Peek = v
End Function
```

`TraceInMemory` is a fixture-owned buffer with no worksheet mutation. The reference is deliberately obtained inside VBA, so the first variant does not supply C3 as an ordinary formula argument. A second variant takes a Range argument to test argument/dependency effects separately.

**Initial caches and order:** Manual mode; A1=1, B3=`Peek("before")`, C3=`10*A1`, D3=`Peek("after")`. Enter B3, C3, D3 in that order and warm all relevant caches to 10; explicitly calculate the readers during preparation. Change A1 to 2, then run `Range("B3,C3,D3").Calculate`. Reset for reversed area order, smart and full calculation.

For the smart-calculation variant, explicitly call `Range("B3,D3").Dirty` after changing A1 and record that command: the hidden VBA read need not create a static dependency that dirties those readers. Classify a case with no reader entry separately from a reader that entered and observed an old cache.

**Predictions:**

| Hypothesis | B3 | D3 | Additional observable |
|---|---:|---:|---|
| One old pass-wide cache | 10 | 10 | C3 nevertheless finishes as 20 |
| Publication before each reader | 10 | 20 | Reader order brackets producer publication |
| Producer demanded before entry | 20 | 20 | Producer precedes both entries or entries are deferred/repeated |

Log entry count/order and cached values observed by a calculation event. If Range order is not honored, classify that separately rather than forcing the numeric result into one of these models.

A macro variant reads C3, calls `Range("C3").Calculate`, then reads again. **A fixed macro snapshot** predicts 10 then 10; **a new visible frontier after nested calculation** predicts 10 then 20. Attempt the corresponding calculation from a worksheet UDF separately and record any error/refusal; macro permission must not be assumed for the UDF.

**Design consequence:** Select when the host pins/advances a read view and where publication fences belong. Do not infer that a pinned calculation-UDF view freezes all macro reads.

### E4 — Whole-formula restart, leaf reentry and volatile draw counts

**Inputs/order/caches:** Use E1's reader-first dirty-producer setup, with B3=`Stamp("draw",RAND())+ReadRef(C3)` and C3=`100*A1`. `Stamp` returns its value unchanged and logs its raw bits; `ReadRef` is the x17 ordinary reference probe. Warm/reset, change A1 from 1 to 2 in manual mode and calculate B3,C3 in that area order. A dynamic-reference variant constructs C3 from a row-number argument if static scheduling prevents the intended pending read.

Separately start a fresh workbook, enter B3 before C3 with B3=`Stamp("prefix",RAND())+C3`, C3=`Stamp("producer",200)`, and compare manual full calculation with automatic formula entry. This addresses the broader static-reference claim in the rework plan.

**Hypotheses and predictions:**

* **Whole-formula restart with redraw:** a forced unfinished read yields two prefix `Stamp` entries and observed RAND outputs; the final result uses the later prefix. Distinct outputs support the redraw hypothesis.
* **Leaf-only reentry:** `ReadRef` has two native entries while `Stamp` appears once and its first value is retained.
* **Same native continuation:** one native entry resumes; this would contradict the matching x17 fresh-entry case and requires careful fixture validation.
* **Producer-first scheduling:** no 64 and one prefix; this run does not discriminate restart extent.

Run a deterministic counting-prefix variant, plus an uninstrumented control, so the logging UDF's effects/eligibility are visible experimental factors. Prefix-entry counts establish repeated prefix evaluation; differing recorded RAND values distinguish new output from retained output. Equal values alone cannot distinguish retention from coincidentally equal new outputs. The fixture does not directly observe Excel's internal RNG-call count. The deterministic prefix case establishes effect-count parity independently.

**Design consequence:** A scheduler yield retains the draw and prefix. A measured semantic restart repeats the required scope. Remove the old “values never differ” assurance unless its precise scope is proven; do not accept a named compatibility mode that still violates the stated parity target.

### E5 — Iterative update order and volatile entry frequency

**Workbook:**

```text
D1 = 0
A1 = IF($D$1=0,0,(B1+1)/2)
B1 = IF($D$1=0,0,(A1+1)/2)
```

**Initial state:** Enter A1 then B1 in manual mode, enable iteration, calculate with D1=0 and verify both live caches are exactly zero. Set D1=1, maximum iterations=1 and a recorded small convergence threshold. Run application calculation, then separate reset runs with each Range area order. Repeat with maximum iterations=2 and MTR off/on. Record whether the command actually evaluates both SCC members.

| Hypothesis | One iteration | Two iterations, same assumed order |
|---|---|---|
| A-first sequential update | (0.5, 0.75) | (0.875, 0.9375) |
| B-first sequential update | (0.75, 0.5) | (0.9375, 0.875) |
| Simultaneous previous-iteration snapshot | (0.5, 0.5) | (0.75, 0.75) |

These are exactly representable predictions under the stated update models, not claims about Excel's actual startup or iteration scheduling. Additional preliminary entries would be visible and require extending the model.

Add logging identity UDFs in a separate variant to count order/entries; keep the plain-formula control. Another variant records a RAND output supplied to each logged member entry. A per-entry redraw hypothesis predicts generally differing outputs; a retained-per-pass input hypothesis predicts repeated captured outputs for that member. Compare output sequences and entry counts without claiming direct visibility of internal RNG calls or treating equal outputs alone as proof of reuse.

**Design consequence:** Keep current iteration/cycle exclusions until the measured update model is represented. A simultaneous parallel SCC computation is incompatible if sequential values are observed.

### E6 — Cancel key, retained publication and next-command behavior

**Inputs/caches/order:** Use A1=1, B1=`2*A1`, C1=`LongProbe(B1)` returning its input after controlled work, and D1=`C1+1`. Prime caches to B1=2, C1=2, D1=3 in manual mode; change A1 to 2 and begin full calculation. Send Esc or Ctrl+Break after the long fixture signals entry. Test separate implementations: a large supported built-in array calculation; VBA numeric loop with/without `DoEvents`; cooperative XLL polling abort; and a finite five-second native routine with no callbacks. For the built-in variant, a prefix marker proves only that the formula prefix ran, not the exact instant an uninstrumented kernel begins. Record VBA `EnableCancelKey` setting and handler. Native probes must be finite for the Excel experiment.

**Predictions:**

* **Prefix publication survives:** completed values remain; interrupted/dependent work is revisited by the next smart calculation.
* **Calculation-wide rollback:** all caches return to pre-command values.
* **Late return accepted:** the finite native routine's result appears after cancel.
* **Late return rejected:** physical return occurs but the cell does not accept that result.
* **Cooperative interruption only:** response occurs at polling/safe boundaries; **runtime interruption** can stop the loop independently of explicit checks.

Record cancel request, UI acknowledgment, actual native/VBA exit, effects before/after the request, result publication and subsequent entry counts separately. Then edit again and smart-calculate. In another reset case save/reopen after cancellation with calculation-before-save controlled.

**Design consequence:** Determine partial-publication and dirty-cache behavior. This informs the host completion receipt but cannot justify force-stopping arbitrary in-process native code. An OS scheduling delay or human keypress timing is not a safe-point measurement; entry/exit markers are essential controls.

### E7 — Async identity, stale completion and memory ownership

**Inputs/caches:** A1=`AsyncProbe("old")`, B1=`A1+1`, with a recorded previously completed baseline. In manual mode start full calculation and hold the new completion using an external fixture gate. In separate reset cases edit A1 to `AsyncProbe("new")`, cancel, close the workbook, complete twice, or return an owning string/array then mutate the fixture's original storage after the completion callback returns.

**Entry/completion order:** For edit, explicitly start the replacement calculation in manual mode and wait for its distinct `"new"` entry/handle before completing new then old. If the runtime defers or refuses replacement entry while old work is pending, record that outcome instead of waiting indefinitely for a nonexistent handle. For close/cancel, send the old completion only after the action is acknowledged. Preserve handles according to the fixture's native API ownership; do not manufacture a dangling pointer.

| Hypothesis | Predicted observable |
|---|---|
| Invocation-bound completion | Old/cancelled/closed token rejected; new cell/dependent result remains |
| Cell-bound completion | Old completion can replace current cell result |
| Single-use completion | Second return is rejected and produces no second publication |
| Repeatable completion | Second return accepted or a second dependent execution occurs |
| Owning copy on acceptance | Later mutation of fixture storage cannot change the admitted value |
| Shared retained buffer | Later mutation affects the value, or lifetime demands remain observable |

Record exact callback statuses, result storage writes, dependent count and saved/reopened values. A separate background caller-query case tests general callback permission independently of async completion.

**Design consequence:** Preserve existing detailed token checks and add process incarnation. The test does not make a successful completion proof that the add-in has stopped executing.

### E8 — RTD topic identity, notification coalescing and old leases

**Fixture/workbook:** A COM RTD fixture records every method, thread/apartment relation and payload sequence. Two cells contain `=RTD("DnaCalc.TraceRtd",,"topic")`; a third depends on one. Record initial values and subscription IDs after connection. Use explicit external gates to request ten notifications with payloads 1 through 10 before allowing refresh. Record throttle settings.

**Sequence:** Run in automatic and manual modes separately. Allow refresh, change one cell to a new topic, delete the remaining old-topic formula, and close/reopen. Separate two stale cases: invoke a retained old server callback after termination/reconnection, recording server/sink identity; independently return a retained `(TopicID, value)` row for a disconnected topic from `RefreshData`, with controlled ID reuse. Trigger smart calculation at recorded points in manual mode. The fixture must obey valid COM lifetime/marshalling rules; a dangling callback pointer is not a semantic probe. The API outline here uses outside recollection: `UpdateNotify` is server-level, while refreshed rows identify topics; verify against the fixture's public interface declarations.

**Predictions:** Shared-topic identity produces one connection for identical requests; per-cell identity produces two. Coalescing produces fewer refresh/admitted-value events than notifications, potentially only the latest sequence; per-notification handling produces corresponding separate events. Lease-bound handling prevents old-server callbacks or retired-topic rows from changing a new subscription; identifier-only reuse can misattribute an old row. Record whether a notification causes refresh, which server supplies it and which rows are accepted. Notification receipt alone does not establish stale-value admission. Owner-bound dispatch produces the recorded owner/apartment relation; another supported pattern produces different traced dispatch.

Capture `ConnectData`, refresh, disconnect, termination, worksheet events, dependent entries and save/reopen cache behavior. Ten notifications do not by themselves imply ten promised value deliveries.

**Design consequence:** Keep signal reception, native polling, semantic input admission and cache consumption separate. Select lease identity/teardown order from observations; the packet's x17 heading supplies no RTD lifecycle answer.

### E9 — Synchronous events, ByRef cancellation and the save cut

**Minimal handler outline:** `Workbook_BeforeSave` logs entry, sets A1=3, optionally sets `Cancel=True`, logs exit. `Worksheet_Change` logs entry/exit and calls a benign `WorksheetFunction.Sum`. `Worksheet_Calculate` records the values it sees. Add separate `BeforeClose` cancellation and COM event-source variants. Use an explicit recursion guard; record `EnableEvents`.

**Initial caches/order:** Manual workbook A1=1, B1=`A1*10`, both calculated and saved as the baseline. Change A1 to 2 without calculating, so B1's recorded cache is 10. Invoke Save with calculation-before-save true/false and handler cancellation true/false in separate resets. Inspect/copy the tested file immediately after Save, then close with `SaveChanges:=False` for these save cases; a later close-time save must not overwrite the evidence from a cancelled Save. Reopen the retained file in a separate Excel instance. Run BeforeClose variants from separate resets.

| Hypothesis | Predicted observable |
|---|---|
| Synchronous change-event reentry | Change handler is nested between BeforeSave entry/exit |
| Queued delivery | Change handler appears after BeforeSave exits |
| Synchronous ByRef cancellation | Prior file is not replaced; in-memory A1 may still be 3 |
| Save snapshot before event | Saved authored values reflect the pre-handler cut |
| Save snapshot after event | Saved A1 includes 3 |
| Calculate before handler | A saved B1 cache may reflect A1=2, depending on later actions |
| Calculate after handler | B1 can be 30; event/cache trace establishes the timing |
| No calculation for that save mode | B1 remains cached 10 with appropriate subsequent dirty behavior |

A fixture COM object can synchronously raise an event whose handler calls WorksheetFunction and reads a range; record whether native return waits for the handler and updated ByRef values. This distinguishes a synchronous event contract from queued observation without guessing Excel's internal pump.

**Design consequence:** Place event admission and save-cut capture correctly. Cancellation of output is not rollback of the handler's committed edits or external effects. Record after-save event behavior if supported; do not replay it during crash reconciliation.

### E10 — Office.js batches, loads, calculation suspension and events

This is an **observable Office.js API test**, not a test of internal Excel thread scheduling. It needs Excel's add-in JS runtime on each target desktop/web build. The API spelling below is an unexecuted outline based on recalled API usage; verify it against that runtime's declarations/requirement sets before running.

**Setup:** In automatic mode establish A1=1, B1=`A1*10`, B1 cache=10. Register supported change/calculation handlers and sync their registration before the experiment. Their logs are JS-side records, not workbook mutations. Reset the workbook before each case.

```javascript
// PROPOSED experiment outline; not executed for this review.
await Excel.run(async context => {
  const sheet = context.workbook.worksheets.getItem("Sheet1");
  context.workbook.application.suspendApiCalculationUntilNextSync();
  sheet.getRange("A1").values = [[2]];
  const result = sheet.getRange("B1");
  result.load(["values", "valueTypes"]);
  await context.sync();
  record("first sync", result.values, result.valueTypes);
  result.load(["values", "valueTypes"]);
  await context.sync();
  record("second sync", result.values, result.valueTypes);
});
```

Repeat without suspension, with manual mode and explicit calculation, and with two separately started `Excel.run` contexts whose submission order is externally controlled. With a supported async custom-function fixture, hold an external result while loading its cell, then release it. Events that load cells must use their own explicit context/sync, not stale request objects.

**Predictions:** A first-sync snapshot before resumed calculation gives 10 followed by 20; a post-calculation load gives 20 on the first sync. A held external result either appears as the API's pending/error representation or delays completion; record which. Events can be delivered before the batch promise resolves, after it, or coalesced; event/promise order discriminates the API contract. Two batches may be observably serialized or interleaved; do not infer either from JS submission alone.

Record typed values and API errors. Preserve JS numeric bits with a bit recorder where possible and corroborate desktop numeric oracle results through native capture; ordinary JSON number text is insufficient for every distinction. Unsupported methods/events are explicit capability results.

**Design consequence:** Define JS batching, load snapshots, event delivery and calculation suspension in the object-model contract. A native mailbox implementation and browser worker arrangement do not by themselves establish Office.js compatibility.

### Separate fault injection of DNA Calc's implementation

These are **[PROPOSED] implementation tests**, not Excel measurements. Feed them through the existing verdict path with explicit expected host outcomes.

| Injected condition | Required observation |
|---|---|
| Parent awaits VBA/native child; descendant host request arrives with unrelated UI edit queued | Child progresses under its frame/policy; unrelated edit obeys the command boundary; no borrow panic or lock-free inbox deadlock |
| Every native slot is occupied by a parent awaiting a nested child | Permitted nested dispatch progresses without unreserved capacity; forbidden nesting returns the correct refusal/status |
| Concurrent callback discovers a dynamic dirty producer | Current typed refusal persists until supported; afterward 64/cleanup/physical return/new attempt, with no false cycle or duplicated scheduler entry |
| Yield at each supported boundary before/after a user call and random admission | Same invocation/count/draw across scheduling yields; fresh entries only for explicit semantic reevaluation |
| Cancel before entry, after effect, after return, after publication | No job whose native-entry admission is after revocation enters; a raced entry admitted before revocation is recorded as potentially executing; effects remain recorded; stale results do not publish; prior published units remain |
| Native effect followed by process death before reply | Authored commits recover, external outcome is unknown, and no automatic repeat occurs |
| Uncooperative native call or permanently stuck callback | Controller remains responsive; logical revocation works; physical recovery terminates the process instead of joining forever on controller |
| Restart reuses local request/topic numbers; old messages arrive | Incarnation/provider/lease mismatch rejected before value admission or event generation |
| Async return followed by more module execution, retained COM object/timer and delayed free | No unload based on completion receipt; owning code/state remain valid through actual permitted cleanup or process exit |
| Safe → Full → worker/IPC; missing or forged authority; next unrelated call | No authority promotion; missing context fails closed; no leaked stricter TLS context into the next unrelated invocation |
| Old VBA activation retained across reload/close | Old code/state lease remains coherent; finalization on owner; no rebinding to new globals mid-activation |
| Crash after durable authored delta but before reply; retransmit delta | One committed delta/receipt, no macro/event reexecution; old process cannot commit after replacement |
| Save cancel/crash before event, after event, during encoding, before/after replacement | Receipt matches actual saved revision/file; prior file preserved before replacement; no replay of save handlers; newer edits remain unsaved |
| Late UI patch and result publication from a retired process | Patch rejected or reconciled by sequence/incarnation; no fabricated workbook event from repaint |

For owner-thread assertions, test both VM3 and JIT where available, including a finalizer that calls the host. For browser acceptance, place an input/cancel message between actual worker tasks and verify a long supported continuation does not starve it. For native memory faults, use a disposable surrogate process; do not label successful Rust panic handling as AV recovery.

## 6. What to stop or delete

1. **Stop describing all concurrency as one route.** Document the native unlimited general parallel path, bounded XLL pool and finite owner driver separately. Correct the finite-budget claim now; unify semantics, not necessarily executor mechanics.

2. **Delete the unconditional “values never differ” reassurance.** Replace it with the current x17 scope and the unresolved containing-formula/RAND experiment. Keep the historical record as history. A documented effect-count divergence remains a failure under this task's parity goal.

3. **Stop treating `$`, `Send + Sync` or a call-count limit as purity.** Keep placement, contextual permission, reference policy, effect/replay eligibility and actual memory safety distinct. Remove any duplicate metadata whose only purpose is to restate those properties inconsistently.

4. **Remove blind retry after user entry.** Preserve the explicit difference between pre-entry owner selection, retained continuation and measured fresh semantic attempt. Keep typed dynamic-prerequisite refusal until the supported 64 route is complete.

5. **Retire general-inbox callback adapters as invocation-aware dispatch covers them.** Do not retain two paths with different parent/read/policy behavior. Keep existing synchronous Variant adapters until equivalent owning/resumable support is demonstrated; then delete the redundant adapter, not its compatibility tests.

6. **Stop equating cancellation, completion receipt or timeout with native termination.** Audit uses of `physical_returned`; rename it if its actual meaning is “completion callback observed.” Do not infer current unsafe unloading without inspecting its consumers. Keep modules resident when quiescence is unproved.

7. **Do not build one process per DLL or immediately remoteize workbook reads.** First isolate the existing session composition. Defer additional runtime groups until a concrete interoperability/isolation need justifies their semantic boundary.

8. **Stop automatic replay as a recovery strategy for user code.** Recovery replays resolved authored data and optionally recorded publications. Loading DLLs, runtime initialization, macros, events, UDFs and RTD reconnect are execution, including during restart. Restored document state is not restored process state.

9. **Do not create a second workbook model, graph, async validator or verdict path for the surrogate, JS bridge or UI.** Replace coarse value-only callback RPC adapters where references/caller identity are required, but keep OneCalc deliberately values-only. Native local/remote hosting modes share one semantic implementation.

10. **Do not make an async-runtime framework a prerequisite.** The synthesis already permits such a runtime only as a native adapter and warns against a recalculation-wide region that must drain before the next edit. Keep framework-specific task/cancellation types out of evaluator contracts. **[DESIGN]** [SYNTHESIS.md:190–195][synthesis]

11. **Defer speculative GPU calculation, broad shared-memory wasm parallelism and certified trace machinery.** A WebGPU-friendly render boundary is worthwhile; it is not an implemented renderer. Deliver owner correctness, finite cooperative work and recoverable native hosting first.

12. **Preserve useful evidence, including failures.** Keep raw x17 records, callback output bytes, entry/free/event traces, old-build provenance, refusals and crash cases. Never delete a failure because a new architecture cannot represent it, and never convert a saved-cache agreement or missing capture into a live pass.

The smallest useful next decision is to settle the shared invocation/read/authority contract and the per-session surrogate cut. The smallest useful next implementation is to make current routes traceable, complete owner reentry and bounded native prerequisite handling, then wrap those same routes in a recoverable process. Each step delivers capability without requiring another semantic engine.

[engine-budget]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L1917-L1948
[engine-eligible]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L2319-L2344
[engine-workers]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L2554-L2607
[xll-pool]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/worker.rs#L1-L64
[xll-submit]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/worker.rs#L168-L213
[x17]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/XLL_COM_HOSTING.md#L1263-L1297
[xll-callbacks]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/callbacks.rs#L305-L337
[xll-unwind]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/invoke.rs#L298-L326
[async-admission]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L6513-L6568
[vba-invocation]: https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-host-api/src/invocation.rs#L1-L73
[rtd-owner]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-com-host/src/rtd.rs#L150-L188
[browser-worker]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-worker/src/worker.rs#L123-L170
[synthesis]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/SYNTHESIS.md#L180-L196
[recalc-contract]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/RECALC_CONTRACT.md#L705-L725
[surrogate-task]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/.beads/issues.jsonl#L82
[restart-plan]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-11/REWORK_PLAN.md#L59-L70
[xll-threading]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/XLL_COM_HOSTING.md#L199-L218
[thread-test]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/tests/threads.rs#L105-L139
[oracle-count]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/research/gamma-family-2026-11-12.md#L344-L348
[bank-count]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/research/gamma-family-2026-11-12.md#L305-L312
[capture-count]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/plan/capture-batch-1-results.md#L24-L39
[enron-count]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/plan/enron-saved-cache-triage.md#L8-L56
[vba-affinity]: https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/docs/spec/HOSTING_AND_EARLY_BINDING.md#L34-L47
[vba-policy]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-vba/src/policy.rs#L20-L62
[host-cancel]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/session.rs#L568-L594
[previous-surrogate]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-11/pro/13-addin-hosting-answer.md#L376-L397
[async-callback]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/callbacks.rs#L525-L556
[macro-levels]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-macro-security/src/lib.rs#L27-L85
[project-grant]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host-vba/src/trust.rs#L45-L85
[hal-capability]: https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-hal/src/capability.rs#L1-L24
[tracker-checks]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/.beads/issues.jsonl#L455
