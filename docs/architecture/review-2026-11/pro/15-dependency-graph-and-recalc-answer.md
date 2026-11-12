# DNA Calc dependency discovery, invalidation and recalculation review

DnaCalc revision: `3b5aa86b3ba47cbd216daec70c5d6586ad520deb`. CalcVBA revision: `8115aa78135114c1dea79f26cbc12c8a3b00a4cf`.

This review uses the supplied packet. Repository links identify its pinned excerpts; they are not claims of additional inspection. No repository build, Excel run, or external research was performed for this review. The proposed implementation work below is a review and rework plan, not code already implemented.

Evidence labels have their requested meanings: **[MEASURED-EXCEL]** means a measurement reported in the packet for its stated cases and build; **[CODE]** identifies behavior or machinery visible in supplied source; **[DESIGN]** identifies a documented contract or model; **[EARLIER-REVIEW]** identifies an earlier assessment; **[INFERENCE]** identifies a consequence or risk not established by an executed reproducer; **[PROPOSED]** identifies this review's recommendation or experiment. Source definitions at the end retain the full pinned paths and line ranges.

## 1. Verdict on today's design, with evidence

**Keep the existing engine and graph. Rework the semantics of completed evaluation, invalidation obligations and publication before substantial performance work.** There is enough implemented machinery to extend coherently. The packet does not justify another evaluator, another name graph, another range engine, or a second host model.

The strongest demonstrated current defect is the publication rule that ORs a UDF's volatility request with the plan's volatile flag. The second major problem is the documented choice to retain evaluator progress where the supplied Excel measurements show abandonment and reentry. Those require different repairs: the first is a completed-evaluation event reduction; the second involves both execution boundaries and the scheduler's choice of initial entry order. A topological scheduler that avoids the measured early entry can be observably wrong even if its continuation implementation is perfect. [CODE] [engine/recalc.rs:2724–2735][publish-volatility]; [MEASURED-EXCEL] [VOLATILITY.md:24–47][volatility-seed], [EVALUATOR_SUSPENSION.md:25–58][suspension]; [DESIGN] [EVALUATOR_SUSPENSION.md:60–87][suspension].

### 1.1 What to keep

**The one graph already has useful distinctions.** `FormulaNode` separates placed prerequisites from a runtime overlay; `set_dynamic` replaces the overlay, removes old reverse entries and inserts new ones, avoiding work when the vectors are equal. The reverse index already contains cell, rectangle, axis, span, resolved-name and namespace-proof machinery. Namespace misses and shadowing are therefore coverage questions, not missing concepts. [CODE] [state.rs:21–54][formula-node], [state.rs:1390–1416][dynamic], [graph.rs:343–402][graph-index].

**The geometry index is worth preserving.** Rectangles are filed at a size-related block level, in at most four blocks, with point queries visiting one block per populated level. Whole axes and spans have separate indexes. This avoids expanding a range into every physical coordinate in the durable index. It does not establish the complexity of candidate filtering, axis queries, or subsequent scheduling expansion. [CODE] [graph.rs:343–367][graph-index].

**The host has actual command distinctions.** Manual mode can decline implicit calculation; Automatic Except Tables has a deferral switch; explicit scoped calculation reaches `begin_recalc_scoped_with_poststate`; Manual forced range calculation selects `StillRequired`. `HostSession` separately accepts RTD updates, async completions, cancellation and calculation advancement. These are useful executable seams, not merely proposed types. [CODE] [host/recalc.rs:146–185][host-recalc], [session.rs:549–611][session].

**The native status-64 work is a correction to preserve.** The packet reports ordinary/`$` reference callbacks unwinding, discarded returns, cleanup, nested reentry, `#` cached reads and value-argument preparation, with engine matches for the reported 28 native and 12 Excel-DNA rows. The callback latch, return-disposal branch and engine `Attempt::Reevaluate` route corroborate important parts of that behavior. Do not reopen the older claim that this path merely retains a continuation. [MEASURED-EXCEL] [XLL_COM_HOSTING.md:1263–1297][native-measurements]; [CODE] [callbacks.rs:323–327][callback-latch], [invoke.rs:303–325][native-disposal], [engine/recalc.rs:2162–2189][reevaluation].

**RTD is already distinct from a cell edit and an async result.** The shown invalidator checks current published stamps, prunes obsolete candidate readers, invalidates the affected closure and advances external-input versions. The packet also records canonical topic coercion and adoption of initial `ConnectData` inputs without duplicate UDF entry. Retain both improvements. [CODE] [state.rs:671–718][rtd-invalidation]; [MEASURED-EXCEL] [XLL_COM_HOSTING.md:1299–1319][native-measurements].

### 1.2 What requires rework

**Volatility must be recomputed for each completed semantic evaluation.** The source's `volatile || built_in` contradicts the recorded `RAND()+VFalse()` and `VFalse()+RAND()` results. The fix must also handle an evaluation with no new request, executed names, registration defaults, shape events and abandoned attempts; simply replacing OR with the last UDF boolean is insufficient. [CODE] [engine/recalc.rs:2724–2735][publish-volatility]; [MEASURED-EXCEL] [VOLATILITY.md:24–146][volatility-seed].

**The suspension model is an explicitly documented compatibility divergence.** Excel's reported cold-chain cases reenter prefix UDFs and redraw RAND; the documented DNA model retains those effects and operands. The supplied evaluator implementation is not sufficient to prove every current route still follows that document, so this is a measured-versus-documented discrepancy requiring a current-route check, not a claim that every wait is defective. The explicit XLL reevaluation route does not by itself repair ordinary static-reference cases. [MEASURED-EXCEL] and [DESIGN] [EVALUATOR_SUSPENSION.md:33–87][suspension]; [CODE] [engine/recalc.rs:2162–2189][reevaluation].

**One graph must support several questions without equating them.** Potential compatibility dependencies answer what can invalidate a formula. Typed observations establish what a particular semantic evaluation queried and against which view. Current waits answer which admitted producers must finish. Deliberately cached VBA observations answer a different question again. The earlier evaluator review explicitly warns that the existing footprint is not an exact list of consumed values. Preserve its conservative coverage until typed evidence supports a narrower interpretation. [EARLIER-REVIEW] [8-evaluator-design-answer.md:697–709][earlier-evaluator]; [DESIGN] [RECALC_CONTRACT.md:501–521][read-contract].

### 1.3 End-to-end route assessment

| Product route | What the packet establishes | What remains unestablished |
|---|---|---|
| Authored edit → bind → invalidate → calculate | Placed/runtime edges, namespace proofs, closure construction and host calculation entry points exist. [CODE] | Complete edit admission, structural transform coverage and atomic result/edge/spill installation across all commands. |
| Worksheet/native UDF → callback → reevaluation | Status-64 latch/disposal and explicit engine reevaluation; reported native fixture matches. [CODE] [MEASURED-EXCEL] | Universal scheduling/callback parity outside those captures, and ordinary formula abandonment. |
| RTD → host → affected formulas | Published-reader filtering, version updates, session ingress and reported initial-connection repair. [CODE] [MEASURED-EXCEL] | Multi-topic batch atomicity, first unpublished readers, lease retirement and every mode's command-admission behavior. |
| Async return → result → dependents | Native mailbox validation, token-bearing host API and `AsyncDependents` hook. [CODE] | Full semantic stale-token validation, handle reuse and product publication under cancellation/edit races. |
| VBA/JS/UI command → host → skin | A host seam suitable for normalization and finite advancement. [CODE] | No JS adapter, skin-protocol or CalcVBA enforcement implementation is supplied. Their absence from the packet is not proof of absence from the product. |
| Save/reopen; external links/query/pivot refresh | A documented save pin/cache policy and explicit reported provenance limits. [DESIGN] | Serializer, save-event, refresh-provider and browser product routes are not demonstrated by these excerpts. |

Sources: [formula-node][formula-node], [graph-index][graph-index], [closure][closure], [host-recalc][host-recalc], [session][session], [async-callback][async-callback], [reevaluation][reevaluation], [contract-decisions][contract-decisions].

There is no implementation evidence here for WebGPU rendering. A display-list or IR boundary can make that renderer possible; it does not establish that it exists. There is likewise no basis for claiming that the evaluator rework's evidence/summary/worker proposals are all implemented. [INFERENCE, scope of supplied evidence].

### 1.4 What the reported numbers establish

| Population | Reported result | Valid interpretation |
|---|---:|---|
| Oracle formula cells | 76,903 / 78,264; 1,361 failures | Reported retained comparison at its documented revisions; not a fresh check of this pin. |
| Full function bank | 3,895,333 / 3,910,795; 15,462 failures | A different row population, with unread/excluded rows still failures. |
| Partial live capture batch | 130,849 / 134,577; 289 captured files | Rate over captured cells only, not all 486 selected files. |
| Enron saved caches | 12,961,186 / 19,666,149 saved-value agreements | Unknown-freshness saved-decimal comparison; **zero live passes** in that run. |

[MEASURED-EXCEL, reported populations] [gamma-family:305–312,344–348][reported-bank], [capture-batch-1-results:24–39][reported-capture], [enron-saved-cache-triage:8–21,35–56][reported-enron]. Do not merge these denominators or use a high function-bank rate as evidence for callback order, command freshness, save correctness or dependency retirement. The tracker additionally reports checks from earlier work; it does not replace a run at this review's pin. [DESIGN / REPORTED CHECKS] [.beads/issues.jsonl:455][reported-tracker].

## 2. Specific defects and risks, ranked

P0 means a compatibility blocker for a claimed supported route. P1 means a high-consequence invariant or route that needs a targeted gate. P2 means a scaling or coverage risk to resolve before expanding the corresponding capability. A source locator for a risk identifies the relevant supplied seam, not an assertion that omitted surrounding code is defective.

| Rank | Exact pinned file:line | Trigger and observable consequence | Evidence status | Smallest reproducer or missing evidence |
|---|---|---|---|---|
| **P0 D1** | `crates/calc-engine/src/recalc.rs:2724–2735` | A valid false request follows a formula token; OR leaves the cell volatile, causing extra later calls and wrong persistence behavior. | **[CODE] demonstrated rule defect** against supplied measurements. | E7: `RAND()+VFalse()`, `VFalse()+RAND()`; clean F9 and save/reopen. |
| **P0 D2** | `docs/architecture/EVALUATOR_SUSPENSION.md:33–58,62–87` | Cold reader encounters an unfinished chain; retained progress changes UDF counts, random draws and request survival. | **[MEASURED-EXCEL] versus [DESIGN] documented divergence**; current general evaluator path not independently shown. | E6: preserve recorded entry order and compare `pre,pre,post` with `pre,post`. |
| **P1 R1** | `crates/calc-engine/src/state.rs:1390–1404`; `docs/architecture/RECALC_CONTRACT.md:517–521` | Result and runtime footprint installed separately, or partial failed reads replace the last complete footprint: later edits miss readers or retain obsolete targets. | **[INFERENCE]**; replacement exists, enclosing transaction not shown. | E2; yield before install, edit old/new targets, then complete. |
| **P1 R2** | `crates/calc-engine/src/state.rs:709–718`; `docs/architecture/RECALC_CONTRACT.md:497–519` | Second invalidation while already dirty; an older candidate incorrectly satisfies the newer obligation. | **[INFERENCE]**; `contract.invalidate` exists. Do not repeat the old boolean-only criticism as fact. | Engine gate: candidate for requirement r completes after invalidation to r+1. |
| **P1 R3** | `crates/calc-engine/src/recalc.rs:3576–3608`; `crates/calc-engine/src/graph.rs:122–125` | Old dynamic cycle persists, or dropping old dynamic constraints hides a new actual cycle; geometry-only reconstruction may omit name/span relations. | **[CODE] mitigation exists; [INFERENCE] completeness unknown.** `meets_area` and full name expansion are omitted. | E9 cycle break/create; E4 names; E10 middle-sheet cycle. |
| **P1 R4** | `crates/calc-engine/src/graph.rs:350–354,385–402`; `crates/calc-engine/src/state.rs:38–42` | Local name creation, missing-name repair or nested redefinition fails to rebind a dependent. | **[INFERENCE] route-coverage risk**, not absent namespace machinery. | E4: add/delete a shadow without editing the formula; repair an initially missing name. |
| **P1 R5** | `crates/calc-engine/src/state.rs:47–52`; `docs/architecture/RECALC_CONTRACT.md:29,513–520` | Spill growth/shrink/blocker changes occupancy but value-only tracking misses a released tail or publishes mixed shape/value state. | **[INFERENCE]**; typed occupancy and atomic publication coverage not shown. | E10: block, unblock, shrink, read blank tail, save/reopen. |
| **P1 R6** | `crates/calc-engine/src/state.rs:682–718` | Topic changes during its first unpublished reader; published-reader lookup alone cannot find that candidate. | **[INFERENCE]**; pending-read/certificate handling may already cover it. | E12: delayed first reader plus later update; verify successor requirement and no stale-current publication. |
| **P1 R7** | `crates/calc-xll-host/src/callbacks.rs:533–547`; `crates/calc-host/src/session.rs:576–593` | Async completion after edit/cancel/delete-recreate targets the wrong formula generation. | **[INFERENCE]**, not a demonstrated race; active-handle checks exist. | E13: stale/duplicate completions; inspect semantic token validation and allocation reuse. |
| **P1 R8** | `crates/calc-host/src/recalc.rs:164–178`; `crates/calc-host/src/session.rs:558–563` | Range completion treated as global cleanliness, or same-authored-revision updates deduplicated by consumers. | **[CODE] existing range distinction; [INFERENCE] transport risk.** Other delta fields may solve it. | E5 and E15; two status/result publications at unchanged authored revision. |
| **P1 R9** | `crates/calc-engine/src/recalc.rs:3462–3465,3492–3503,3576–3580,3615–3621` | Compressed range dependencies expand into huge closure adjacency, then more map/index forms; memory exhaustion or checked `u32` panic. | **[CODE] expansion and hard bounds; [INFERENCE] workload cost unmeasured.** | 100k formula producers and 100k readers of their common range can imply 10^10 incidences. Measure before expansion; reject safely until handled. |
| **P2 R10** | `crates/calc-engine/src/graph.rs:361–363`; `crates/calc-engine/src/recalc.rs:3473–3488` | Global edge churn destroys reader-cache reuse, or missing sheet-order/footprint context permits stale cache reuse. | **[INFERENCE]**; cache internals and lifetime omitted. | Retarget repeatedly; count cache hits, epoch changes and candidate scans; change spill footprint/span membership. |
| **P2 R11** | `crates/calc-engine/src/state.rs:1399–1404`; `crates/calc-engine/src/graph.rs:350,357` | Removing a runtime subscription also removes an equivalent structural subscription if ownership is not disjoint or counted. | **[INFERENCE]**; `NodeSet` update details omitted. | `=A1+INDIRECT(C1)+VFalse()`, C1 `"A1"`→`"B1"`; then edit A1. |
| **P2 R12** | `crates/calc-engine/src/state.rs:1409–1416`; `crates/calc-engine/src/graph.rs:343–347` | A span type or manual graph test is mistaken for complete 3-D product support; stale comments drive unnecessary reimplementation. | **Coverage uncertainty**, not proven unsupported behavior. | E10 through actual formula entry, middle-sheet edit, move and insertion. |
| **P1 R13** | `docs/architecture/RECALC_CONTRACT.md:12–29,501–521`; `crates/calc-host/src/session.rs:549–611` | Save, JS, skin or refresh adapters bypass canonical authority or lose freshness qualifications. | **[DESIGN] required contract; product-route evidence absent from packet.** | E5/E14/E15 plus one actual save path; no invented file:line for unsupplied adapters. |

The old numeric RTD-topic refusal and duplicate `ConnectData` entry are **not** current findings: their corrections are recorded. Neither is “status 64 has no unwind path.” [MEASURED-EXCEL] [XLL_COM_HOSTING.md:1263–1319][native-measurements].

## 3. Recommended target design and the smallest migration from today's code

This section is **[PROPOSED]** except where a current implementation or recorded result is explicitly cited. Extend `FormulaNode`, `Prerequisite`, `ReverseIndex`, `EngineState`, `RecalcWork`, existing result stamps and `HostSession`. The sketches name required information; they are not a parallel API to implement alongside equivalent existing fields.

### 3.1 State owners and the five-review interaction contract

| Shared contract | One semantic owner | Object model / JS | Graph / recalc | Engine–host–doc | Host-to-skin IR | Concurrency / isolation |
|---|---|---|---|---|---|---|
| Authored objects, stable identity and transactions | `calc-doc-model` | Resolve object handles and commit canonical edits. | Consume committed deltas; never edit authored state as a calculation side effect. | `calc-doc` imports/serializes the canonical model. | Project authored revisions and object identities. | Single authorized writer; immutable read snapshots. |
| Command admission, mode, scope, origin and cancellation | `calc-host::HostSession` | Translate UI/VBA/JS requests into the same admission path. | Execute precisely the admitted demand. | Save/refresh are explicit commands with their own policies. | Preserve admission/completion order and cause. | Coordinator authorizes lanes and revokes work. |
| Binding and evaluation evidence schema | `calc-fml` interface | Host providers supply typed reads. | Project evidence into subscriptions and certificates. | Preserve versions of the environment used to evaluate. | Expose useful diagnostics, not evaluator internals. | Workers return evidence; they do not commit results. |
| Requirements, dependency semantics, read qualification and installation | `calc-engine` | Read qualified views; request calculation explicitly when needed. | Own the one graph, obligations and result store. | Supply a pinned result view to save/export. | Preserve result/status identity separately from authored revision. | One installation owner validates candidates. |
| Live input admission and connector lifetime | `calc-host` runtime | Expose typed capability/status results. | Own the meaning of the resulting invalidations. | Distinguish link input cuts from materialized model changes. | Deliver same-revision input/status changes. | RTD/COM and callbacks obey owning-thread rules. |
| Save pin and cache-policy choice | `calc-host` | Save requests use the same event/admission rules. | Supply qualified caches, pending/circular/refused states. | `calc-doc` serializes the selected model/result pair. | Report save completion independently of calculation completion. | Pin after admitted BeforeSave edits; do not serialize a moving target. |
| Wire ordering and presentation projection | Host publication adapter using `dna-calc-protocol` | Consume the same publication stream. | Define freshness semantics; do not duplicate them in DTO logic. | Carry model and result-view references coherently. | Own transport sequencing, viewport deltas and resynchronization. | Ordered commits, bounded transport and recoverable dropped deltas. |
| Execution authority and execution-lane selection | `calc-host` coordinator | Identify ingress and project grant. | Describe job requirements and safe yield points. | Keep native-code trust distinct from file/macro state. | Browser/WebView receives capability outcomes. | VBA owner thread; registration-specific native rules; explicit process boundary if used. |

There can be several physical indexes of this graph. `rtd_readers`, namespace probes and spatial indexes need not be forced into a giant map. They must share the same installation lifecycle, subject identity and invalidation semantics. An ephemeral SCC projection or ready queue is an algorithmic view, not a second authoritative dependency model.

Identities must distinguish **where something is** from **which incarnation it is**. Retain the current `NodeId`/anchor split. A formula replacement, deletion/recreation, workbook reopen or native-registration replacement must revoke old invocation authority even if the displayed address or native handle address is reused. Use existing generations where available; add a generation only where an address currently stands in for identity. Name, table and sheet identities likewise outlive renames but not arbitrary deletion/recreation.

### 3.2 Dependency kinds and the calc-fml boundary

For formula node n, keep:

- **S(n): structural compatibility subscriptions** obtained by binding, including potential references where Excel's invalidation contract requires them.
- **R(n,e): typed observations** made by semantic evaluation e, with subject, facet, context, read policy, selected view, outcome and proof/version.
- **D(n,e): runtime invalidation subscriptions** projected from completed evidence, excluding only subscriptions already covered by an equivalent S subscription.
- **O(n,e): deliberately cached observations** that validate against an invocation pin but create neither fresh demand nor a dependency edge.

The installed invalidation relation is S plus the last accepted D and engine-owned publication requirements such as occupancy. Current waits are derived from admitted pending reads. S and D do not mean “all these producers must execute before this formula may enter.”

| Subject | Binding interface required | Runtime interface required | Invalidations and qualification |
|---|---|---|---|
| Cell, rectangle, whole row/column | Placed references with stable sheet identity and caller placement. | Typed value/reference/blank read; actual admitted area and read policy. | Retain compressed geometric subscriptions; no million-cell expansion just to express `A:A`. |
| Defined name and namespace lookup | Complete ordered resolution proof, including higher-priority misses, resolved definition/body version and nested uses. | Evaluate in caller context; report executed uses and their reads/events. | Definition edits and relevant namespace changes trigger rebind; unrelated shadowed names need not if captures permit narrower invalidation. |
| Structured reference/table | Stable table/column identity, row-context binding and relevant schema/extent versions. | Actual selected reference, shape and values. | Separate data changes, table extent/schema changes and reference rebinding. |
| 3-D sheet span | Endpoint identities, spelling-resolution proof and sheet-order/membership context. | Resolve all current member sheets in the pinned snapshot. | Interior value changes and membership/order changes; do not rely on endpoint-only geometry. |
| Spill reference, shape, occupancy | Anchor identity and structural `#` reference. | Spill-owner/version, shape, requested occupancy, blockers and blank proofs. | Old/new footprint changes affect direct cell, shape, blank and obstruction readers. |
| LET and LAMBDA | Potential references and syntactic volatility seed according to the measured rule; no separate body graph. | Executed body/call context, caller placement, name uses and actual observations. | Latent references may invalidate without demanding execution. E1 resolves SCC policy separately. |
| OFFSET, INDIRECT, reference-valued INDEX/CHOOSE/name | Their structural inputs, not guessed final addresses. | Report a reference as a reference, and its later admission/coercion as a read. Include negative resolution evidence. | Replace runtime targets on accepted completion; never call a requested rectangle proof that every value was consumed. |
| CELL/INFO/formula text/host facts | Identify known fact subjects and syntactic seed. | Typed fact facet, including unknown/negative outcome, selected fact view and version. | Format, filename, selection or other facts invalidate according to measured command/event policy; coordinate equality does not imply value-facet equivalence. |
| Worksheet UDF/XLL reads | Registration/callable identity and argument admission rules. | Distinguish explicit formula arguments, strict callbacks, cached callback policies and macro observations. | A cached value source does not automatically remove a dependency. Registration and environment versions participate in reuse. |
| RTD/external input | Callable identity and constant/topic inputs when statically known. | Topic/provider identity, lease, sequence and input-cut proof. | External versions and current readers determine dirtying; no authored cell edit is fabricated. |

The current `Prerequisite` enum need not acquire every fact in one change. Existing fact/name/topic side indexes can carry these typed subscriptions under the same result stamp and commit. The essential change is to stop reducing unlike facets to an untyped area before the engine can distinguish their semantics.

```rust
// PROPOSED information at the existing calc-fml -> engine boundary.
// Reuse existing types/fields when they already provide these guarantees.
struct EvaluationEvidence {
    invocation: InvocationId,          // includes semantic evaluation ordinal
    target: TargetIdentity,             // node generation + formula/binding identity
    requirement: RequirementVersion,
    observations: Vec<Observation>,
    volatility: CompletedVolatility,    // present even when no request occurred
}

enum Observation {
    Dependency {
        subject: Subject,
        facet: Facet,
        policy: ReadPolicy,
        proof: ReadProof,               // includes negative/absence outcomes
    },
    CachedObservation {
        subject: Subject,
        view: PinnedViewId,
        proof: ObservationProof,
    },
}

enum EvaluationStop {
    Complete { value: CalcValue, evidence: EvaluationEvidence },
    Yield(EvalContinuation),
    Wait { continuation: EvalContinuation, on: Producer },
    Reevaluate { on: Producer, reason: ReevaluationReason },
    AsyncPending(AsyncToken),
    Refused(NotModelled),
}
```

This is an interface requirement, not a request to redesign formula bytecode or numerical kernels. `MissingInput` alone cannot distinguish an unfinished in-scope producer, unavailable external input, permitted boundary cache, unresolved blank occupancy and a real cycle.

### 3.3 Name identity is not a globally calculated name value

`Prerequisite::Name(NameId)` already describes a participant independent of caller-relative evaluation. Keep that distinction. [CODE] [graph.rs:106–118][prerequisite].

Share immutable name body plans. Execute caller-relative names under the caller's sheet/cell/reference context. Attach the resulting dynamic reads and effects to that caller's evaluation. Repeated uses of a stateful name must remain separate executed uses unless a capture proves reuse permissible. Do not invent a cached “name cell” whose value is shared by every caller.

Binding proofs must include the unsuccessful local lookup that allowed a workbook-level name to win. Creating the local name must therefore invalidate the proof. Deleting a local name exposes the lower-priority definition and requires a new proof. Failed compilation must retain enough inspected definitions and namespace probes to recover when a missing name is created; the existing `FormulaNode` comment explicitly describes that intention. [CODE] [state.rs:38–42][formula-node].

The caller's syntactic volatility seed does **not** inherit the name body's token. An executed name use can set the caller true, while explicit requests inside name evaluation can also affect the caller in their measured order. A nonvolatile name body is not an implicit false request. [MEASURED-EXCEL] [VOLATILITY.md:92–116][volatility-events].

### 3.4 Requirements, results and certificates

The required invariants are more important than the field names:

1. **Each logical invalidation advances the requirement version, even while already dirty.** Deduplicate multiple visits to a node within one invalidation closure; do not collapse two distinct admitted invalidation events.
2. **A candidate satisfies only the obligation it was admitted and validated to discharge.** It carries target generation, formula/binding version, command/pass identity, input cut and requirement version. An r candidate cannot silently satisfy r+1.
3. **A published result has its own identity.** An accepted installation can advance result/status identity with identical numeric bits. Equality must not erase execution obligations, volatility changes, read qualifications or completion events. A payload hash may save transport bytes; it is not a freshness proof.
4. **A strict in-scope read cites a producer satisfying that command's demand**, including force demand, or a producer proof in the same installation unit. Its certificate includes producer identity, result identity, requirement/satisfaction state and the facets read.
5. **An out-of-scope formula read cites a pinned boundary cache.** It retains reverse subscriptions and explicitly says that an outside formula may remain dirty. Authored constants come from the pinned model even when outside the calculation scope.
6. **A cached VBA observation cites the invocation's pinned view.** It is validated against that historical view, without converting it into fresh demand or an automatic reverse edge. Explicit worksheet arguments to that VBA function can still create ordinary dependencies.
7. **Force is an execution obligation.** A numerically equal cache, or even a valid old read certificate, cannot by itself discharge a newly requested forced evaluation.
8. **Validation failure installs no candidate result/topology/volatility.** External effects already performed remain real. Do not retry an effectful candidate merely because a worker result lost an installation race.

For ordinary satisfied work, the contract's local rule is `satisfied == required`. Range poststate needs one further distinction: a result can be certified for command K and installed while `StillRequired` deliberately leaves a subsequent calculation obligation. Represent that as a qualified command result plus outstanding requirement; do not force it into “globally clean.” The existing host already asks for this poststate. [DESIGN] [RECALC_CONTRACT.md:497–520][read-contract]; [CODE] [host/recalc.rs:164–178][host-recalc].

Example: invalidate A to r=8; admit A's evaluation; another accepted event advances A to r=9; the old candidate finishes with exactly the same number. Its equality is irrelevant. It may be retained as historical evidence, but cannot be installed as satisfying r=9. Pure carryover into a successor command is allowed only after revalidation of its authority, reads and demand; a new force or a required new semantic entry still has to occur.

### 3.5 Dynamic-edge and attempt lifecycle

| Disposition | Installed graph/result treatment | Attempt-local treatment |
|---|---|---|
| Accepted complete value or Excel error | Atomically replace D; install result/status, stamp, volatility, spill ownership and satisfaction/poststate. | Retire temporary interests. A worksheet error is a value outcome, not automatically abandonment. |
| Scheduler yield | Keep previous installed D and qualified result. | Preserve continuation, reads, events and invocation ordinal. |
| Retained dependency wait, where semantically permitted | Same. | Register current wait and enough transient interests/version checks to detect intervening input changes. |
| Semantic abandonment/reentry | Do not install candidate D, result or final volatility. Previous installed D remains an invalidation subscription until replacement. | Preserve external effects and required observation fences; discard candidate semantic state; next entry gets a new ordinal. |
| Async pending | No completed formula value from the original entry yet. | Retain token, evidence and pending status; an eventual completion is not an authored edit or automatic fresh UDF entry. |
| Refusal, cancellation, failed validation | Do not certify partial reads as a completed footprint. Keep old caches only with their proper stale/refused qualifications. | Revoke authority; retain explicit recovery subscriptions where needed, such as missing-name probes. Never accumulate failed-attempt edges indefinitely. |
| Formula replacement/deletion | Retire the old incarnation's subscriptions, spill ownership and result authority. | Revoke continuations, waits and async handles. |

Every subscription needs an owner/lifetime. Either D excludes equivalent S subscriptions, or reverse entries count distinct owners. Removing D must not erase S. Equivalence includes subject, facet and invalidation/read policy; a cell-value subscription does not subsume a format fact or blank-occupancy proof at the same address.

Candidate reads can remain private to `RecalcWork` if certificate validation and successor invalidation make lost updates impossible. If the running system accepts concurrent live changes, its attempt interests must cover first-time reads not yet present in published stamps. Do not solve that gap by publishing incomplete D early or by restarting every formula.

### 3.6 Atomic publication and spill arbitration

Use the existing single installation owner. In one logical operation:

1. Check command authority, target generation, required version and environment/callable versions.
2. Validate strict reads, boundary-cache selections, cached observations, name proofs, host facts and blank/occupancy proofs.
3. Resolve/reserve the spill against that same model/result view. An unknown unfinished producer is not a proved blank cell.
4. Install value/error/status, result version, runtime subscriptions, spill ownership/shape, volatility outcome and satisfaction/poststate together.
5. Apply any additional invalidations warranted by newly exposed value/topology/shape/occupancy consequences, using the union of the **old and new** relevant footprints, and emit the coherent publication. Ordinary result delivery wakes or discharges already admitted obligations; it must not blindly advance requirements and repeat the dirty closure for the same original cause. Reuse existing cause/pass accounting to distinguish new consequences from work already owed.

Do not let a spill's own occupancy subscriptions become accidental self-value cycles. A blocked spill needs subscriptions to the obstruction/attempted footprint even though it owns no successful spill range. Shrinking a spill releases cells and invalidates blank/ownership readers; those cells do not remain dependents merely because they used to belong to the spill.

A `#SPILL!` result may add a volatility event at spill resolution. Its position relative to a false request must be established, then reduced in the same completed-evaluation event model. E7 explicitly tests the unprovided ordering; appending an unconditional true after an evaluator's “final” flag without deciding that ordering is not an adequate fix.

### 3.7 Volatility as a completed-evaluation reduction

Initialize each semantic evaluation from the formula's syntactic seed S. Reduce eligible events in their semantic execution order. Publish the final value for **every** completed evaluation, including one with no requests. Physical yields keep this reduction; semantic abandonment discards it.

| Source of flag state | Required behavior in the supplied measured cases |
|---|---|
| Syntactic token | Present even in untaken branches and latent LET/LAMBDA bodies for the recorded functions, including CELL/INFO/FORMULATEXT/SHEET/SHEETS. It is not a “built-in executed” event. |
| `!` registration | Set true at an executed call's start, including the recorded async `!` case; no token from an untaken call. |
| `#` with R/U parameter | Set true at call start only when a reference is actually passed in an R/U position. A value, Q/P/B argument, or no-argument `Q#` does not suffice. |
| Explicit valid request | True or false overwrites the current flag. Only a false request clears it; a function's failure to request does not clear it mid-evaluation. |
| Executed name use | A true name-body flag can set true; false/no-event is not an implicit caller reset. Preserve requests made during name evaluation. |
| SUMIF/AVERAGEIF shape | Executed undersized sum-range case sets true; untaken calls do not. The measured matching/larger shapes and listed other functions differ. |
| Spill obstruction | Runtime event setting true; E7 fixes its precise integration with placement and later/earlier requests. |
| Implicit scalar lifting | The recorded Double-parameter multi-element UDF requests are ignored; 1×1 and explicit MAP/BYROW calls differ. Do not generalize this to every other event source without captures. |
| Invalid request context | Preserve the recorded rejection/status for ordinary, `!`, `$` and async cell registrations without a permitted `#` context. The measured `Q!#` case does permit a false request. `$#` is unmeasured in the packet and refused by DNA; do not infer support. |

These are **[MEASURED-EXCEL]** for the packet's cases/builds, not universal claims about every function or platform. [VOLATILITY.md:24–146][volatility-seed].

Production need not retain a full event vector for every formula forever. Calc-fml can reduce events in order and return a compact completed summary; capture mode records the ordered events needed to explain the result. Include context so that helper requests, nested names and suppressed lifted calls cannot leak into the wrong caller.

Separate the formula's direct final volatility from dependent execution and file cache flags. The packet reports `ca` on volatile dependents as well as source cells, and names a bare-reference exception whose detailed fixture is not included. Compute dependent behavior through the measured graph policy; derive persistence metadata through one save projection. Do not equate every `ca` bit with a cell's own token, direct volatility, or generic dirty state. [MEASURED-EXCEL] [VOLATILITY.md:140–146][volatility-events].

### 3.8 Scheduling, scopes and command completion

Normalize commands once in `HostSession`. The following is the target mapping to validate across actual UI/VBA/JS routes; it is not a claim that all mappings are shown in the packet.

| Command / mode | Demand and scope | Required qualification |
|---|---|---|
| Automatic edit | Admit the measured affected work and appropriate volatile work after the committed edit. | Visible interest may prioritize eligible work, but cannot eliminate obligations. |
| Manual edit | Mark/increment requirements and expose stale status. | No implicit ordinary calculation merely because a view asks for values. |
| Automatic Except Tables | Defer the what-if data-table calculation class until the appropriate explicit request. | Do not defer every structured table, query refresh or pivot merely because it is called a table. |
| F9 / ordinary application calculation | Smart required work plus measured volatile/dependent work in the admitted application calculation domain. | Verify open-workbook routing; do not silently narrow an application request to one active sheet. |
| Shift+F9 / worksheet calculation | Requested worksheet scope with its specified boundary-cache policy. | Other sheets/workbooks may remain dirty. |
| Range.Calculate | Force the selected targets under the measured range-order/read policy. Preserve the existing Manual `StillRequired` behavior. | Range completion need not satisfy the later application obligation; do not widen silently. |
| Ctrl+Alt+F9 / full calculation | New force demand for the admitted domain. | Reuse of old caches does not count as forced execution. |
| Full rebuild / Ctrl+Alt+Shift+F9 | Rebind/rebuild derived graph state and perform full demand. | Keep authored state canonical; invalidate derived-cache epochs and retire invalid learned order. |
| Cancel | Revoke authority and handles; issue cancelled completion with any permitted prior publications identified. | Already performed external effects are not undone. |

An application-level command may need coordination over several open workbook hosts. Use the existing host/domain organization; do not pretend that the supplied `ModelRoot` excerpt establishes a multi-workbook implementation. Cross-workbook live reads and external-link boundary caches require explicit identities and qualifications.

**Ready queues and learned order.** Keep compact ready/waiting state per admitted command. Use prior order/calcChain as a validated hint, not dependency truth. Preserve the build-specific early-entry behavior needed by captured cold-chain cases. An unconditional “all static precedents first” policy would suppress the recorded restart; changing only `Suspended` into `Reevaluate` cannot repair that.

Time slicing is a physical yield retaining progress. A formula abandonment is a new semantic evaluation. Native status-64 is one explicit abandonment reason, not the only possible reason. Pure work may be reordered or carried over only when it cannot change visible calls, random/time consumption, reference decisions or the command's required execution. Absence of a UDF call is not sufficient proof of purity.

**Completion** means that K's admitted obligations have reached their specified outcomes. A completion receipt names scope, pass/frontier, result view, outstanding requirements and qualifications. A finite advance may return Pending; if K still requires an async installation, that is not successful completion of K. An API that returns after admission must expose an admission result separately from the later calculation completion. Deliberate boundary-cache use, circular holds and typed refusals cannot disappear into an unqualified “current.” Viewport/skin reads and cached JS reads do not themselves demand fresh execution unless the API command explicitly specifies that behavior.

### 3.9 Cycles and iteration

Retain the current fallback that stops treating old dynamic edges as immutable scheduling constraints after an ordering cycle. It is necessary because the next evaluation may choose a different target. Then use current admitted waits to distinguish a newly real runtime cycle from an obsolete one. [CODE] [engine/recalc.rs:3576–3608][cycle-fallback].

Do not infer that **only** executed reads can establish Excel circular behavior. Potential self-references in untaken IF/LAMBDA bodies need E1. Conversely, invalidation coverage alone does not establish a compulsory execution wait. Preserve each edge's role so the cycle policy can follow the measured distinction.

An async producer, pending RTD connection, outside-scope cached formula or unproved blank is not automatically a circular dependency. The host must make progress on external completions while the engine waits; lack of a ready formula is not sufficient evidence of a cycle.

For iteration, keep the measured sheet-name/row/column member order for its supported fixture family. The code's comment ties it to a specific Excel build and experiment. It does not imply entry order is irrelevant to acyclic cold-chain calculation. [CODE, reported measurement provenance] [engine/recalc.rs:3689–3718][cycle-order].

An iterative component needs explicit policies for seeds, update visibility within a sweep, iteration count, convergence test, errors/types and topology changes. E9 discriminates in-place versus simultaneous sweeps and seed sources. Do not choose Jacobi updates for parallel convenience or invent a universal `#CIRC!` value. With iteration disabled, reproduce the observed value/held-cache/diagnostic behavior; where not modelled, refuse honestly. With iteration enabled, a limit reached is distinct from convergence, even if the measured policy publishes the last sweep.

During iteration, invalidate convergence evidence when current dynamic topology changes; merge/split affected components and preserve the measured sweep policy. An iteration is an allowed new semantic evaluation with its own observable calls. Do not replay a partially executed effectful sweep as an invisible worker retry. External pending input remains a wait, not a convergence result.

### 3.10 Four ingress traces

**RTD batch.** `UpdateNotify` signals connector work; the owning COM lane obtains `RefreshData`; the host validates topic identity, current lease and sequence, and stages the accepted batch as an external-input admission; the engine invalidates current topic readers and their dependent closure; the host admits the corresponding calculation under its measured mode policy; publications advance result/status identity without inventing an authored revision. Coalesce transport notifications only where it preserves observable batch semantics. Manual-mode admission is an E12 question, not an automatic consequence of the cell-edit policy.

Initial `ConnectData` is separate: the current invocation receives its initial value. Preserve `adopt_connected_inputs` as a narrow, certified extension that cannot rewrite an earlier conflicting observation or absorb an unrelated later topic update. The fixed-frontier contract and this initial-admission exception must be stated together. [DESIGN] [RECALC_CONTRACT.md:14–19][contract-decisions]; [MEASURED-EXCEL] [XLL_COM_HOSTING.md:1314–1319][native-measurements].

**Async completion.** Admit invocation I and its opaque token; enter the native async procedure once; let the native callback validate its physical handle and place a value in the mailbox; on the host owner, validate I's session, target generation, registration, command authority, requirement and read evidence; complete that invocation or reject the stale result; open the needed dependent pass; publish without a fictitious cell edit or second original UDF entry. Native acceptance into a mailbox and semantic permission to install are separate decisions. The existing callback checks and `AsyncDependents` hook are retained. [CODE] [callbacks.rs:525–556][async-callback], [engine/recalc.rs:2157–2160][reevaluation].

**Status-64 abandonment.** In recorded reader-before-producer range order, ordinary/`$` reference-taking B3 enters; C3 is unfinished; first `xlCoerce` returns 64 with the measured error operand; the frame latches; later callbacks remain 64 with their specified untouched outputs; `xlFree` is permitted; the procedure's return, including a caught-error `-999`, is discarded; permitted DLLFree cleanup still runs; the engine ends that semantic attempt, discards its result/dependency/volatility candidates, retains external effects and surviving observation fences, calculates C3, then gives B3 a new evaluation ordinal. Keep `QU#`'s recorded cached one-entry behavior and value-argument preparation for `QQ`/`QQ$`/`QUQ`. The `$` range case remains on the measured owner thread. [MEASURED-EXCEL] [XLL_COM_HOSTING.md:1268–1297][native-measurements].

**Refresh.** Admit a provider/target refresh command; stage provider output without background mutation of the model; validate target authority and generation; an external-link-cache update advances an input/fact cut, while row materialization or table/pivot structure changes pass through the canonical model transaction; invalidate affected value, schema, binding and occupancy subjects; admit only the authorized calculation; publish refresh completion separately from any later calculation completion; save a coherent model/result pin. Deliver one actual provider route before inventing a generic refresh framework. A what-if data-table recalculation is not a table/pivot data refresh.

### 3.11 Platform, macro and OneCalc boundaries

Keep the capability axes separate:

- **Off:** no VBA project execution.
- **Safe:** an admitted project executes under CalcVBA's Safe capability policy.
- **Unguarded:** an admitted project executes under Full capability policy.
- Worksheet-function restrictions remain independently enforced during calls in either execution level.
- Native XLL/COM trust/loading is independent of a VBA project grant.
- OS/process isolation is a separate boundary; Safe is not an OS sandbox, and `$` is not a purity guarantee.

These are target mappings constrained by the prompt; no CalcVBA policy implementation is supplied to certify current enforcement. VBA and its callbacks stay on the owning thread. Worker evaluation cannot borrow live thread-affine host state. Background/native completion handlers return owned messages, not mutable model access. Process isolation, if introduced, must preserve callback sequencing and lifetime or explicitly refuse unsupported interactions.

Wasm reuses the same model, graph and evaluator semantics for available capabilities. Missing COM/XLL/RTD facilities produce explicit unsupported/refused outcomes and qualified caches according to policy; they do not silently become fabricated successful constants. WebView2 is an ingress/rendering environment, not a second calculation host. Skin IR carries coherent values/status/shape and supports a future WebGPU renderer without claiming it has been built.

OneCalc continues to accept values and callable capabilities only. Reusing function/evaluator interfaces does not authorize a synthetic workbook, hidden graph or workbook-service privilege in OneCalc.

### 3.12 Structural changes before tuning

First fix the semantic lifecycle and add observability counters. Then reduce temporary duplication in closure/order construction. The present closure explicitly appends reader incidences and its cyclic path creates additional representations. The `u32` conversions are real hard bounds; resource exhaustion must become a typed refusal, counted as failure, before an oversized expansion reaches a panic. [CODE] [engine/recalc.rs:3454–3503][closure], [engine/recalc.rs:3576–3621][cycle-fallback].

For large shared ranges, retain grouped subscriptions through invalidation and scheduling. One interned range subject with producer membership and many reader subscribers can express a dense relation without enumerating every producer–reader pair. If an auxiliary range vertex is needed for an SCC projection, it is a compact representation of this same graph. Preserve exact reachability and measured entry semantics; do not use an all-producers barrier to erase a required early-entry behavior.

Canonicalize runtime subscription order so identical sets do not churn merely because observation order changed; retain execution order separately for effects. Measure namespace-proof duplication and intern immutable lookup/binding descriptions where context allows. Measure global epoch invalidation before introducing finer epochs; any partitioned reader cache must still depend on sheet order, footprints, graph generation and query policy. Replace allocation-heavy sets or duplicate adjacency maps only where recorded allocations and candidate counts justify it. Never optimize names by caching a stateful value.

## 4. A staged implementation plan with measurable acceptance

All stages are **[PROPOSED]**. Reuse the current fixtures, capture lineage, corpus harness and verdict path. A new trace dimension belongs in that path; it does not justify a separate scheduler test product or a new definition of “pass.” Each stage ends when its stated capability and gate are satisfied. Do not add unrelated cleanup or repeat broad testing without a concrete remaining risk.

### Stage A — Establish the current route and protect corrected behavior

**Changes and dependency:** no semantic rewrite. Add missing correlation from ingress → admission → invocation → installation → completion to the existing trace format. Run the relevant retained captures at the selected implementation revision. Identify actual callers of name/span binding, installation, save and JS/skin publication; classify routes as supported, refused or unverified.

**Retain:** x17 status-64 behavior, x18 topic coercion and initial RTD adoption, Manual range poststate, cached observation fences, typed refusals. Keep every existing failure in its denominator.

**Product acceptance:** one UI edit and one explicit calculation reach the same engine admission; the native reference/value-argument cases retain their observed entry counts and cleanup; the RTD connector's first value does not cause a duplicate entry; cancellation produces a completion outcome and revoked handles.

**Measurements:** value/type/error verdicts plus entry counts/order, callback statuses, thread roles, pass causes and publication order. Separately reproduce the reported corpus populations available at this revision; do not assume historical totals are the new baseline.

**Stop:** every claimed supported route has an actual trace or a named typed refusal, and the protected fixture verdicts are recorded. Sonnet can instrument and run this stage; Opus is needed only to resolve ambiguous semantics exposed by a trace.

### Stage B — Correct completed-evaluation volatility

**Changes and dependency:** extend the existing calc-fml result evidence to deliver a complete volatility outcome; seed syntactically, reduce eligible events in order, and replace current volatility at every accepted completion. Integrate names, registration context and lifted-request suppression. Resolve spill-event order with E7 before finalizing the placement hook. Depends on Stage A's trace identity.

**Retain:** callback permission/status rules and the distinction between physical continuation and new semantic evaluation. Do not alter numerical kernels or registration semantics opportunistically.

**Product acceptance:** E7's three-way discriminator; true-once then no-request; executed/untaken name uses; SUMIF shape changes; `!`/`#` defaults and rejected requests; lifted versus MAP/BYROW calls; completed-error versus abandonment; saved `ca` and Manual reopen through the actual save route. Exclude unresolved bare-reference cases from a parity claim, not from failure accounting.

**Measurements:** final self-volatility, dependent calls, raw values, file flags and call counts over three consecutive commands. Capture trace size and normal-mode metadata cost.

**Stop:** all covered event-order cases match their retained Excel observations and no abandoned candidate contributes final volatility. Sonnet can implement the reducer and plumbing after an Opus decision on name/event ownership and spill placement. This can be a small independently useful release.

### Stage C — Make graph evidence and installation one transaction

**Changes and dependency:** route every accepted result through the same validation/install primitive; expose missing facets and requirement versions; define S/D subscription ownership; cover first unpublished live-input readers; make name-miss recovery and formula-generation revocation explicit. Depends on Stage B's completed metadata contract.

**Retain:** old installed D until a valid replacement; last caches with honest qualification during waits/refusals; no value-based propagation cutoff; compact range/axis subscriptions.

**Product acceptance:** E2/E3/E4/E10; changing an old target after a successful switch causes the measured behavior, current target edits still work, a second invalidation cannot be satisfied by an old result, deleting/recreating a formula revokes old handles, and spill growth/shrink/obstruction publishes coherent states. Add deterministic interleavings around validation/installation, not broad random tests that cannot identify a failure.

**Measurements:** requirement/result transitions, installed versus provisional subscription counts, binding probes, footprint changes and retained memory after retargeting.

**Stop:** each disposition in §3.5 has one implementation path and a meaningful acceptance case; no partial candidate can certify a result. Sonnet can implement after Opus decides the certificate, recovery-subscription and poststate invariants. Remove superseded adapters in this stage rather than leaving both active.

### Stage D — Reconcile entry order, scoped calculation and cycles

**Changes and dependency:** use explicit semantic reevaluation for the captured ordinary cold-chain cases; validate learned/calcChain hints; normalize keyboard/VBA/JS commands through one demand contract; complete dynamic-cycle handling and the measured iterative policy. Depends on Stage C so reentry cannot publish partial topology.

**Retain:** owner-thread range XLL execution, value-argument admission, `#` cached visibility, existing observation fences and Manual `StillRequired`. Refuse unsupported iteration/type/topology combinations rather than guess.

**Product acceptance:** E1/E5/E6/E9 and the native E11 controls. Test first and steady-state commands separately. A range result uses the measured boundary cache and leaves the measured poststate; an obsolete cycle clears; a newly active cycle is recognized; an async wait does not become a circular error.

**Measurements:** semantic entries, physical yields, producer waits, order changes, SCC members/sweeps, convergence/limit disposition, random draw sequence and event order. Compare bits and invocation traces; final values alone are insufficient.

**Stop:** every implemented scheduling rule has a discriminating capture or an explicit restricted capability. Do not claim a universal Excel scheduler from a few fixtures. Opus owns the entry-order/restart and iteration policy decisions; Sonnet implements the resolved cases and route normalization.

### Stage E — Deliver complete live-input and publication routes

**Changes and dependency:** enforce RTD batch/lease semantics and narrow initial adoption; validate async semantic tokens; deliver one real refresh provider; propagate result/status identity and qualifications through actual JS/skin/save routes. Depends on Stages C/D, although DTO plumbing can proceed earlier once fields are fixed.

**Retain:** distinct authored, live-input, async and refresh causes; existing capture lineage; Off/Safe/Unguarded and worksheet restrictions as separate axes; native-only capabilities remain explicit in wasm.

**Product acceptance:** E12–E15 plus Manual dirty save/reopen and BeforeSave edits. Two same-authored-revision result/status changes arrive in order; a cancelled or replaced invocation cannot install; initial RTD values do not duplicate entry; refresh materialization uses the canonical transaction; browser absence of a native facility produces the specified refusal/status.

**Measurements:** accepted/rejected event counts, lease/token lifetimes, selective dirty closures, admission/publication/completion order, refresh scope and save pins. Count topic/handle retention after release.

**Stop:** the supported E12–E15 and save acceptance cases have recorded verdicts and complete their required ingress-to-publication/save paths, including at least one native and one wasm-supported path. Unimplemented providers retain explicit refusals; completing one provider does not certify another. Do not wait for every provider or renderer. Sonnet can wire adapters and lifecycle checks; Opus resolves input-cut adoption, async authority and refresh ownership where the existing contract is ambiguous.

### Stage F — Remove scaling bottlenecks without changing verdicts

**Changes and dependency:** remove duplicate closure representations; preserve shared range subscriptions during scheduling; bound work before expansion; compact measured hot allocations; refine reader-cache invalidation only if needed. Depends on the preceding semantic gates for any changed path.

**Retain:** invocation behavior, qualified reads, observed order constraints and failure accounting. A resource refusal is safer than a panic but remains a failure, not a performance success.

**Product acceptance:** open/edit/calculate/save synthetic large workbooks through the normal host, not only hand-built graph nodes. Repeat current supported corpus comparison after the structural changes; investigate individual losses rather than accepting a good aggregate rate.

**Measurements and stop:** use the populations below. Stop when correctness is unchanged and measured cost is bounded by the compact graph/affected work for the intended workload; further micro-optimization waits for evidence. Sonnet can remove duplicate storage and implement counters. Opus is needed if compressed range scheduling changes reachability or entry semantics.

### Correctness gates and proposed performance budgets

No timing or memory baseline for this pin is supplied. **The numbers below are proposed engineering targets, not measurements or parity requirements.** Select and record a reference native machine, release configuration, allocator, thread count and browser/runtime before evaluating them. Never change semantics to meet a budget.

| Population | Required measurements and correctness check | Initial performance target |
|---|---|---|
| 1M formulas `B[r]=A[r]+1` across sheets | Cold binding, first calc, warm full calc, one-cell edit; allocations, peak/retained graph bytes, certificate bytes; exact results. | Aim for ≤512 MiB retained graph/requirement/certificate metadata, excluding authored formulas, values and opt-in traces; report all excluded memory separately. |
| Sparse edit in a 1M-formula book | Candidate scans, visited formulas, expanded incidences and owner-thread latency. | No full-book traversal for a truly isolated edit; provisional p95 ≤50 ms on the chosen desktop, including relevant invalidation/publication work. |
| Long chain and broad fanout | Time versus affected nodes; maximum stack use; readiness work. | Roughly linear growth for these sparse graphs; no recursion-dependent stack failure. Report a 2×-size ratio, not only one attractive timing. |
| Shared whole column / repeated 100k-cell range | Subscription count, candidate/hit ratio, scheduler incidence expansion, peak memory. | Storage proportional to producers + distinct subjects + subscribers, not their Cartesian product where grouping preserves semantics. Fail admission safely before a known expansion budget is exhausted. |
| Overlapping rectangles and spans | Tree/block probes, candidate rectangles, output readers, deduplication, middle-sheet topology costs. | Report output and false-positive cost separately; do not promise logarithmic lookup from the index comments alone. |
| Name-heavy: 1M uses, nested names, local shadows | Body-plan reuse, proof storage, affected rebind count, lookup probes. | Rebind the affected proof set; no global name-value cache and no history-proportional proof retention. |
| Rapid dynamic retargeting and topic churn | 100k target switches; live/retired subscriptions, leases, reader-cache hit rate, memory after cleanup. | Live structures plateau with current subscriptions and bounded retained work, not switch count. Explain allocator high-water RSS separately from live-object retention. |

Also report end-to-end time and time spent in binding, invalidation, queue construction, evaluation, validation, spill arbitration and publication. For 1M formulas, establish cold/warm timings first; assigning a universal five-second requirement without hardware or formula-cost evidence would be invented precision. Measure wasm separately: native timings and thread behavior do not establish browser budgets.

## 5. Discriminating experiments against Excel

Every experiment and prediction in this section is **[PROPOSED]**. Where a case already has a supplied **[MEASURED-EXCEL]** capture, that capture is a regression anchor; the predicted outcome of a new run is still a hypothesis until recorded. No Microsoft source or disassembly is needed.

### Common capture method

Extend the existing Excel probe/capture/verdict route. Retain workbook bytes or construction script, exact formula-entry order, actual initial caches, active cell for relative names, edits and commands. Record Excel build/bitness/OS, CalculationVersion, mode, iteration settings, MTR enabled/count, registrations and add-in versions. For Office.js also record host/platform, runtime version where available and supported requirement sets.

Record raw FP64 bits before decimal serialization, Variant/XLOPER/value type, error code, reference kind/address, array shape and each element's type/bits. Record native/VBA entries, callback results and untouched operand bytes, ordered volatility requests, thread identities, events, command-return points and async deliveries. Compare thread roles and required order relations across runs while preserving raw identities; do not normalize away extra calls or impose a fictional total order on genuinely concurrent independent callbacks.

Use module-memory logging during worksheet functions; flush only after the tested command. A minimal numeric probe is:

```vb
' PROPOSED fixture helpers; integrate with the existing raw-value capture.
Public Entries As Collection
Public NameCounter As Long

Public Sub ResetProbe()
    Set Entries = New Collection
End Sub

Public Function MarkD(ByVal tag As String, ByVal x As Double) As Double
    If Entries Is Nothing Then Set Entries = New Collection
    Entries.Add tag
    MarkD = x
End Function

Public Function VFalse() As Double
    Application.Volatile False
    VFalse = 0
End Function

Public Function NextName() As Double
    NameCounter = NameCounter + 1
    NextName = NameCounter
End Function
```

`MarkD` deliberately requests a value through a Double argument. Use the existing Variant/reference-capable fixture for error/array/reference tests; do not let probe coercion erase the behavior under test. Log event handlers without writing cells or requesting calculation. Separate formula-entry calls from explicit calculation calls.

For ordinary nonvolatile cases, unless the case says **cold**, enter constants first and formulas in the listed order under Manual, calculate fully, then ordinary Calculate until a clean control produces no probe entries. If it does not become clean, stop that trial and diagnose the confound. Clear the log, not formulas/caches, before the tested edit. Cold-chain tests deliberately omit this priming. For intentionally volatile, RTD, async or iterative cases, use the case's fixed priming sequence and a separate nonvolatile control; expected recurring entries are part of the observation, not a warm-up failure.

There is no supplied read-only API for every cell's dirty flag. Infer operational requirements through subsequent nonvolatile probe calls and controlled commands; `Application.CalculationState` is application-level evidence, and `ca` is not a universal dirty bitmap. Save clones with calculation-before-save disabled, record save events, then reopen under Manual and inspect caches before the first Calculate. Distinguish synthetic cached seeds from values actually produced by Excel. Use deterministic local RTD/async/refresh fixtures and explicit release barriers rather than wall-clock sleeps.

### E1 — Untaken/latent dependencies versus executed reads

**Setup:** warm Manual workbook: A1=TRUE, B1=11, C1=22; enter D1 `=MarkD("D",IF(A1,B1,C1))`, then E1 `=MarkD("E",D1+0)`. Initial D/E caches are 11. Set only C1=23 and call `Application.Calculate`.

| Hypothesis | Expected observable outcome |
|---|---|
| Structural potential dependency invalidates | D runs and remains 11; E follows the separately established unchanged-value propagation rule in E3. |
| Only prior executed reads invalidate | Neither runs. |
| First calculation is conservative, steady state becomes observed-only | Repeated warm trials change from execution to no execution. |

Control with an edit to B1 and a toggle of A1. Repeat the unused-reference edit with `=MarkD("L",LET(f,LAMBDA(x,C1+x),1))` and `=MarkD("I",IF(TRUE,1,C1))`. These cases need not share a rule.

**Separate circular control:** fresh copies, iteration disabled, A1 `=IF(FALSE,A1,1)` versus `=IF(TRUE,A1,1)`. Potential-edge cycle classification predicts a circular diagnosis for both; executed-path classification predicts value 1 for the first and circular behavior for the second. Record the actual circular value/cache/diagnostic rather than inventing an error code.

**Design change:** determines compatibility subscriptions and whether a potential reference alone suffices for circular classification. A successful untaken case does not imply that untaken references need no invalidation edge. This probes the distinction in [EARLIER-REVIEW] [8-evaluator-design-answer.md:697–709][earlier-evaluator].

### E2 — Dynamic-edge replacement, error completion and overlapping ownership

**Setup:** A1=`"B1"`, B1=11, C1=22; D1 `=INDIRECT(A1)+VFalse()+MarkD("D",0)`. Warm to D1=11 and verify two clean Calculate commands produce no D entry. The false request is essential: INDIRECT's volatility would otherwise conceal whether an old edge remains.

Set A1=`"C1"`; Calculate; verify D1=22. Then change B1=12 and Calculate, followed by C1=23 and Calculate.

| Hypothesis | Old B1 edit | Current C1 edit |
|---|---|---|
| Replace the accepted runtime overlay | No D entry; D stays 22. | D enters, becomes 23. |
| Accumulate every historical target | D enters unnecessarily, remains 22. | D enters, becomes 23. |
| Do not track runtime targets | No entry. | No entry; stale 22. |

**Completed-error variant:** use `=MarkD("D",0)+VFalse()+INDIRECT(A1)` so the false request occurs before the read. Retarget to a cell containing an Excel error and complete the calculation. Before editing the former target, require a no-edit Calculate with no D entry: execution of the false request alone does not prove the error result retained it. If this control holds, completed-error replacement predicts the former target no longer triggers D; retain-last-success predicts it does. If the control fails, the uncaught-error arm is a joint error/volatility investigation and cannot discriminate retirement by call counts. Use a caught-error arm such as `=IFERROR(INDIRECT(A1),-1)+VFalse()+MarkD("D",0)` for a separately verified nonvolatile retirement check. Test an unresolved text reference separately; an error before target resolution may need a negative binding proof rather than a cell edge.

**Ownership variant:** `=B1+INDIRECT(A1)+VFalse()+MarkD("D",0)`, initially A1=`"B1"`, then `"C1"`. After the switch, B1 must still have the behavior warranted by its explicit structural reference. Lost B1 invalidation indicates an ownership/removal defect; an extra old-only target in the first fixture indicates retention instead.

**Wait/abandon variants:** reuse E6/E11 entry ordering with a dynamic target and pause at a pending producer. Compare final accepted footprint, later old-target edits and request counts. A completed error, a retained wait, abandonment and cancellation are distinct outcomes; Excel cannot directly establish the behavior of DNA's typed implementation refusal.

**Design change:** fixes D retirement and recovery-subscription policy. Current replacement machinery is at [CODE] [state.rs:1390–1405][dynamic].

### E3 — A precedent recalculates to equal bits

**Setup:** warm A1=1; B1 `=MarkD("B",MOD(A1,2))`; C1 `=MarkD("C",B1+0)`; D1 `=MarkD("D",C1+0)`. All result caches are 1. Change A1 to 3 and Calculate.

**H1, invalidation obligation:** B, C and D all run, with unchanged result bits. **H2, value-based cutoff:** B runs, C/D do not. **H3, some result/status transition drives propagation:** capture the exact conditions under which C/D run, then vary an equal numeric result versus a type/error transition to distinguish that narrower rule.

Change A1 to 4 as the value-changing control. In a separate copy, assign A1 its existing value; do not confuse a no-op authored write with a real upstream edit whose formula result happens to be equal.

**Design change:** whether any equality optimization is permitted. Until captured otherwise, retain the contract's no-cutoff rule. [DESIGN] [RECALC_CONTRACT.md:30][contract-decisions].

### E4 — Name misses, shadows, relative callers and stateful uses

**Shadow setup:** workbook name `Pick` refers to `=Sheet1!$B$1`; B1=11, C1=22. No Sheet1-local Pick exists. D1 `=MarkD("D",Pick)` is warmed to 11. Add local `Sheet1!Pick` referring to C1 without editing D1; Calculate; delete it and Calculate. Then recreate it and change the now-shadowed workbook definition.

**H1, lookup proof includes higher-priority misses:** values change 11→22→11; unrelated shadowed changes need not execute D. **H2, resolved-ID-only tracking:** local creation leaves 11 until another rebind/full rebuild. **H3, spelling-wide invalidation:** values are correct but the shadowed definition change also executes D. Repeat from an initially missing name, including a missing name in a nested definition, and repair it without editing the cell.

**Relative setup:** record active cell D2 and the exact accepted `RefersToR1C1` text when defining Rel as a relative reference one column left. C2=11, C3=22; enter D2 `=MarkD("r2",Rel)` then D3 `=MarkD("r3",Rel)`. Warm and change only C2. Caller-relative evaluation predicts D2 follows C2 while D3 remains 22. Definition-anchor or shared-name-value hypotheses predict both callers using the same target/value or the wrong invalidation. Verify the exported name definition and initial caches rather than assuming that displayed A1 text establishes relativity.

**Stateful setup:** define NmCount as `=NextName()`. A1=`NmCount+NmCount`; A2=`NmCount`. After entry, reset the counter and force one calculation. Per-use evaluation predicts three entries; per-formula memoization two; global-name memoization one. Record actual cell order to interpret values; do not predict it arbitrarily.

**Design change:** complete namespace proofs and caller-context evaluation in the existing graph, without a second name-value cache. [CODE] [graph.rs:350–354,385–402][graph-index].

### E5 — Scope, Range.Calculate poststate and persistence

**Setup:** A1=10; B1 `=MarkD("B",A1)`; C1 `=MarkD("C",B1+1)`; Sheet2!A1 `=MarkD("S2",Sheet1!B1*2)`. Warm caches: 10,11,20. In Manual, change source A1 to 20 and call only `Range("B1").Calculate`.

| Hypothesis | Immediately after range command | Next Application.Calculate |
|---|---|---|
| Bounded scope; target still required | B=20, C=11, S2=20. | B runs again, then C/S2 update. |
| Bounded scope; target satisfied | Same immediate values. | C/S2 update without another B entry. |
| Scope expands to dependents | C/S2 already update in the range command. | Less or no remaining work. |

Repeat on initially clean B1 with no edit to isolate poststate from pre-existing dirtiness.

**Boundary-cache copy:** A1=10, P1=`A1*2`, B1=`MarkD("B",P1+1)`; warm P1=20, B1=21. Set A1=30 in Manual; calculate B1 alone. Boundary-cache policy predicts P1=20/B1=21. Fresh upstream demand predicts 60/61. A no-usable-cache copy distinguishes a refusal or other actual Excel outcome from invented zero substitution. A direct constant read outside scope is a separate control and should not be confused with a stale formula cache.

Run controlled copies with worksheet/application/full/full-rebuild commands and the corresponding keyboard commands. Use two open workbooks to test application scope. Repeat Automatic and Automatic Except Tables rather than generalizing the Manual result. Save post-range clones without calculation-before-save, reopen Manual, inspect `ca`/caches, then call Calculate and record entries.

**Design change:** exact normalization, boundary reads and command-local versus later freshness. The supplied measurement and current Manual rule are anchors, not evidence for all other combinations. [MEASURED-EXCEL] [VOLATILITY.md:120–124][volatility-events]; [CODE] [host/recalc.rs:164–178][host-recalc].

### E6 — Cold-chain abandonment, retained progress and learned order

**Setup, deliberately cold:** Manual, C1=5. Enter B1=`C1*2`, B2=`B1+1`, B3=`B2+1`, then A1=`Tk("pre",1)+B3+Tk("post",2)` using the existing fixture. Record formula-entry calls and caches. Clear the log, set C1=6, then `Application.Calculate`. Do not prime with Full first.

**H1, captured abandonment:** the corresponding recorded case predicts `pre, pre, post`. **H2, retained continuation:** `pre, post`. **H3, producer-first initial scheduling:** also `pre, post`. DNA's own scheduler trace distinguishes H2 from H3; the unmodified Excel fixture supplies no producer-readiness API and distinguishes them only collectively from the measured duplicate prefix entry. A separately instrumented producer fixture may explore ordering, but added UDFs can change scheduling. H3 still fails the original captured call count; it is not a fix for H2.

Repeat with INDIRECT/OFFSET/static forms, SUM's reference argument, nested UDFs, a one-level producer, opposite entry order and subsequent dirty/clean commands. The supplied study already distinguishes many of these; reuse its exact fixtures. With RAND in prefix/postfix, capture all draws and check that the accepted value uses the completed attempt's draws and the formula's actual arithmetic order.

For request survival, disable requests during formula entry. Immediately before the cold command, reset a prefix UDF's counter and enable true-only-on-entry-1 versus true-only-on-entry-2. Completed-attempt semantics predicts the first variant becomes nonvolatile after reentry and the second volatile; retaining abandoned events predicts different subsequent clean-F9 entries. Preserve already observed prefix side effects in both cases.

**Design change:** initial scheduling and explicit reevaluation reasons, not merely a different continuation instruction. These cases have supplied **[MEASURED-EXCEL]** anchors at [EVALUATOR_SUSPENSION.md:25–58][suspension] and [VOLATILITY.md:125–130][volatility-events].

### E7 — Ordered volatility, context and spill placement

**Setup:** Manual, independent cells, forced completed evaluation, then clean Calculate commands and save/reopen. Wrap numeric cases in a nonvolatile logger only where it preserves the expression being tested.

| Formula | H1 sticky syntactic OR | H2 syntactic seed + ordered overwrites | H3 set only when built-in executes |
|---|---:|---:|---:|
| `RAND()+VFalse()` | Volatile | Nonvolatile | Nonvolatile |
| `VFalse()+RAND()` | Volatile | Nonvolatile | Volatile |
| `IF(TRUE,1,RAND())` | Volatile | Volatile | Nonvolatile |

Re-run these recorded discriminators, then the runtime-event order pairs: undersized SUMIF before/after VFalse, NmRand before/after VFalse, executed/untaken `!` and actual-reference/no-reference `#` R/U calls. A name with no event must not clear a previous true; valid false requests must. A function requesting true on one evaluation and nothing on its next distinguishes per-evaluation initialization from sticky history.

For callback permissions, compare the same false request from `Q!`, `Q!#`, ordinary, `$`, async entry and post-async worker contexts. The packet's recorded status/no-effect rules are the predicted controls; preserve raw callback statuses as well as the next F9 count. Unsupported `$`+`#` remains an explicit capability question, not a fabricated accepted registration.

For lifting, use the recorded Double-parameter UDF over 3×1 and 1×3 ranges/arrays, a 1×1 reference, and explicit MAP/BYROW. Request sequences F,T,T and T,T,F distinguish all-requests-folded from implicit-lift suppression and from state leaking between calls. Capture element order; a final flag alone cannot explain the mechanism.

**Spill-order uncertainty:** A2=99; A1=`SEQUENCE(2)+VFalse()`, then separate copy `VFalse()+SEQUENCE(2)`. Under a final placement event, both blocked copies end volatile. Under earlier expression-position blockage, their order may differ. Under finalization before arbitration, false incorrectly wins if Excel uses the late event. Compare unblocked copies. Record the actual completed request sequence and save flags, not just `#SPILL!`.

**Completed error:** with a recorded zero-valued input Z1, compare `=VTrue()+1/Z1` with a numeric result and E6's abandoned request. First verify that VTrue was entered and its request admitted; classify a skipped-request trial separately. If the error completion retains that request, later no-edit F9 reenters it; if that error path discards the request, it does not. This resolves error metadata without conflating it with cancellation or relying on evaluation of a constant error expression.

**Design change:** one completed-evaluation reduction and its context/placement hooks. Supplied anchors: [MEASURED-EXCEL] [VOLATILITY.md:24–146][volatility-seed].

### E8 — Resolve the bare-reference volatile-dependent exception

The packet mentions the exception at [VOLATILITY.md:144–146][volatility-events] but omits section 1's detailed fixture. Do not claim its exact shape or outcome. Test both plausible readings of “bare-reference user-function dependent.”

**Matrix A:** source B1 is a VBA UDF that requests true and returns an incrementing counter; first establish and record warm caches, for example source/dependents all 10. Consumers in separate cells are `=B1`, `=+B1`, `=B1+0`, `=SUM(B1)`, `=INDEX(B:B,1)`, `=IF(TRUE,B1,0)` and `=MarkD("value",B1)`. Perform clean Calculate commands in Manual. Do not wrap the bare-reference formula itself in a logger.

**H1, uniform volatile-dependent propagation:** next source 11 is reflected in every consumer. **H2, bare-shape exception:** the bare consumer remains 10 while arithmetic/coercing consumers become 11. **H3, value-change-sensitive exception:** the difference appears or disappears with a constant-returning volatile source. A separate downstream-logger copy distinguishes a stale bare cache with no downstream execution from downstream execution that reads that stale cache. Repeat with RAND and a native registered volatile source.

**Matrix B:** compare a Variant-argument VBA UDF that logs and returns a constant without dereferencing its argument, the same call receiving `B1+0`, and a Double-argument function. H1 predicts a syntactic argument dependency is sufficient for all to follow source recalculation. H2 predicts value admission/dereferencing changes which calls occur. H3 predicts a narrower user-function reference case. Record argument kind without probing `.Value` in the untouched-reference arm. Keep native `#` R/U defaults separate because they introduce their own volatility.

Save all arms and inspect flags/caches, then reopen Manual and repeat. **Design change:** only a captured narrow rule can justify an exception in propagation or persistence. This result must not be used to delete structural references generally.

### E9 — SCC seeds, sweep order, convergence and changing topology

**One-sweep setup:** create a test workbook with explicitly declared synthetic formula caches zero and verify zero on Manual open before calculation. On sheet `a`, A1=`(Z!A1+2)/2`; on sheet `Z`, A1=`(a!A1+4)/2`. Record construction/entry order. Enable iteration, maximum iterations 1, small positive tolerance; force calculation.

| Hypothesis | a!A1 | Z!A1 |
|---|---:|---:|
| In-place sweep, a first | 1 | 2.5 |
| In-place sweep, Z first | 2 | 2 |
| Simultaneous sweep from zero | 1 | 2 |

Reject a trial if open/entry has changed its intended seeds. Repeat reversed tab/entry order and renamed sheets. Then use initial caches a=10, Z=20: cache-seeded a-first predicts 11/7.5, zero-seeded predicts 1/2.5. These numbers are arithmetic predictions, not measured Excel outcomes. The current folded-name ordering is a supplied fixture-specific anchor. [CODE, reported measurement provenance] [engine/recalc.rs:3689–3718][cycle-order].

**Convergence discriminator:** in a separate zero-seeded copy use A=`B+0.75`, B=`A`, the established A-first in-place order, tolerance 1 and maximum iterations 5. A max-absolute-change rule predicts stopping after one sweep at 0.75/0.75; a sum-of-absolute-changes rule predicts continuing to the limit at 3.75/3.75 because each sweep's summed change is 1.5. For an absolute-versus-relative control, instead use A=`B+2`, B=`A`, verified cached seeds A=B=1,000,000, the same tolerance and limit: max-absolute change predicts five sweeps to 1,000,010/1,000,010, while change normalized by the prior magnitude predicts one sweep to 1,000,002/1,000,002. Condition these predictions on the seed/sweep policy established above. Exact boundary, error, blank/text and limit-reached cases require separate recorded outcomes. These deliberately nonconvergent recurrences test the stopping criterion, not mathematical convergence.

**Topology setup:** A1 is target text, D1=10, B1=`INDIRECT(A1)+VFalse()`, C1=`B1/2`. Start A1=`"D1"` and warm. Change to `"C1"` with iteration enabled, obtain/record a completed cyclic state, then switch back to `"D1"`. Current-target reconstruction predicts B1=10/C1=5, but those values alone do not prove retirement: an unnecessary stale SCC can also produce them. In a matched instrumented copy, add nonvolatile B/C entry probes from the start and compare sweep/entry counts with an acyclic control; extra iteration reveals continued cyclic handling where values cannot. In another copy, disable iteration before the retarget back and record circular indications and recovery. Keep DNA's internal SCC state separate from Excel-observable calls/diagnostics. Also run the entirely iteration-disabled transition acyclic→cyclic→acyclic: correct discovery detects the new cycle and recovers; ignoring all dynamic edges misses the real cycle; retaining old edges can prevent recovery.

Add a pending external producer control: no internal dependency cycle exists, so a valid eventual completion must allow progress. **Design change:** seeds/sweeps/convergence policy and the distinction between historical invalidation edges, current waits and actual SCCs.

### E10 — Spills, 3-D spans, tables and host-fact invalidation

Use independent warm Manual copies so one volatile feature does not conceal another's invalidation behavior.

| Case and exact intervention | Competing hypotheses and expected observations | Design decision |
|---|---|---|
| **Spill:** D1=2; A1=`SEQUENCE(D1)+MarkD("spill",0)`; A2 initially blank. Consumers read A1#, A2/A3 and ISBLANK(A3). While the free spill is still A1:A2, put a blocker in A3; then change D1 to3 and calculate. Remove the blocker and calculate the free spill, shrink to1 and calculate, then edit retired A3. Do not try to overwrite A3 while it is a live spill child. | Atomic current-footprint policy gives coherent shape/values/blank state after each command; retained footprint causes unnecessary anchor execution on the retired-tail edit; value-only or non-atomic policy leaves stale tails/blockers or inconsistent exposed views. Blocked-spill volatility is a confound for dirtying, so use the final free-shrunk state for retirement. | Typed occupancy/shape and atomic install; exact blocking event order remains E7. |
| **Pending blank:** enter a consumer before a producer that later spills into its referenced blank coordinate; preserve cold entry order and force the relevant scope. | Proved-absence/readiness policy uses the eventual spill where required; premature blank selection yields 0/blank or a different call trace; boundary-cache policy may legitimately differ only in the explicitly out-of-scope control. | Distinguish absent, pending and permitted historical blank. |
| **3-D:** sheets A/B/C have A1=1/2/4; D!A1=`SUM(A:C!A1)+MarkD("span",0)`, initially7. Change B to3, move B outside, then insert a middle sheet with8. | Current membership predicts8, then5, then13. Endpoint-only/stale membership misses one or more transitions. Add a middle-sheet formula referring back to D in a separate cycle copy. | Full product binding, topology invalidation and SCC projection; no inference from the span type alone. |
| **Structured table:** T has one numeric column and values10,20; formula `SUM(T[Amount])+MarkD("table",0)`, initially30. Append30, rename the column through Excel, then delete/recreate the table in separate copies. | Identity/schema-aware behavior follows supported reference transformations and gives60 after append. Text-only or stale-extent tracking may retain30, fail to follow rename, or attach to a replacement incorrectly. Record Excel's actual rename/delete formula rewrites. | Table identity, extent and binding facets, not an independent table graph. |
| **Format fact:** `CELL("format",A1)&IF(VFalse()=0,"","")`; warm and verify clean F9. Change only A1's format, then F9, then Full in a separate copy. | Event-invalidated fact policy updates on F9; full-only behavior preserves the old cache until Full. Record exact returned text for each build/format. | Do not force an eager fact invalidation that exceeds measured Excel behavior. |
| **Sheet/filename facts:** similarly suppress the syntactic token with a later valid false request; rename/reorder sheets or SaveAs, then compare ordinary versus full commands. | Fact/topology invalidation updates at the relevant ordinary command; token-only/polling or differently scheduled Excel behavior updates only on force. | Version the actual fact and preserve command timing; do not infer semantics from value-cell edges. |

Save/reopen each completed spill/span/table state. Observe formula text and authored transformations as well as values. These cases change object identity/topology, so they exercise object model, graph, publication and doc together.

### E11 — Ordinary/$ status 64 versus # caches and value arguments

**Setup:** reuse x17's native fixture and registration strings. Warm producer C3=`D3*2` with D3=5, cached C3=10. Reader B3 receives C3 as a reference. Change D3 to21 in Manual, then `Range("B3,C3").Calculate`, reader first. Use separate copies for ordinary `QU`, `QU$`, `QU#`, `QQ`, `QQ$`, and mixed `QUQ` as provided by the fixture.

The reference-taking implementation logs entry/thread, calls `xlCoerce`, logs status and operand bytes, makes the existing controlled second-callback probe, and returns a sentinel `-999` on status64. A DLLFree arm logs permitted cleanup to native memory. Nested-parent/child arms use the existing fixture. Do not add known-crashing free-frame callbacks to an ordinary regression run.

| Hypothesis / policy | Expected outcome |
|---|---|
| Measured ordinary/$ unwind contract | First coerce64 with the recorded error operand; later callbacks64 with specified untouched outputs; sentinel discarded; cleanup occurs; fresh entry after C3=42 reads42. |
| Retained continuation / accepted caught return | One native entry, or final sentinel/old result: distinguish by entry log and cell value. |
| Cached `#` policy | Status0, one entry reading old10 for the recorded case. |
| Fresh value-argument admission | QQ/QQ$/QUQ enter once after producer preparation and see42. |
| Blanket early-entry or blanket cache policy | Extra entries for value arguments or an old value where the corresponding measured control expects42. |

Record main-versus-worker identity; the `$` range arm's owner-thread behavior is part of the supplied capture. Repeat subsequent smart/full commands and later precedent edits to separate cached visibility during this invocation from future dependency invalidation.

**Design change:** preserve registration-specific argument/read/reevaluation policies and effectful abandonment boundaries. This is chiefly a regression gate for an existing correction, not a proposal to implement status64 from scratch. [MEASURED-EXCEL] [XLL_COM_HOSTING.md:1263–1297][native-measurements].

### E12 — RTD connection, batch admission, equal updates and leases

**Setup:** deterministic local RTD server, initially x=1/y=10, with logs for ConnectData, UpdateNotify, RefreshData and DisconnectData. Use the existing typed RTD and X5 callback probes. Enter a formula consuming both topics and a dependent; record entry-time connections and initial cache11. Use fresh copies in Manual and Automatic; do not assume that Manual suppresses every RTD behavior.

**Initial connection hypotheses:** in-place adoption predicts one connecting entry per executed call and initial11. Treating ConnectData as a later update predicts duplicate connecting formula entry. Reuse the packet's numeric/text topic equivalence controls; a second ConnectData for numeric1 versus text"1" contradicts the recorded canonicalization case.

**Batch:** hold delivery until both values are ready, then one RefreshData returns x=2/y=20. A coherent accepted batch predicts a reader seeing22, without an exposed accepted intermediate12 or21. Per-topic publication may expose those intermediate sums or extra entries. Record whether Excel actually admits one batch before interpreting the result; multiple server batches are a different intervention. Compare multiple notifications with one pending batch and two deliberately separate batches.

**Equal payload:** deliver another accepted server version with exactly the same typed2/20. Event-driven propagation predicts reader/dependent entries; equality suppression predicts none. Do not force calculation after delivery. Under Manual, use the same ordinary-calculation/pump sequence in notification and no-notification controls, so baseline RTD execution is not mistaken for event-driven dirtying. Change one payload as a positive control. This decides where equality filtering, if any, belongs; it cannot be inferred from DNA's invalidator alone.

**Mixed initial/later input:** a fresh formula establishes x/y while an already connected z is updated between the recorded connection steps. Narrow initial adoption predicts no duplicate entry for x/y while any admitted later z update has its own subsequent effect; restart-on-all-admissions predicts duplicate entries; unrestricted merging predicts an invocation observing a later z cut without that separation. Use connector barriers/logged delivery to establish the actual interleaving; notification time alone does not prove Excel processed it mid-invocation. In DNA also inject the equivalent admission interleaving directly to verify the certificate invariant.

**Lease reuse:** disconnect an old subscription, reconnect to a different topic and send a distinguishable retained old-topic delivery. Record server-instance and topic IDs: Excel's wire payload does not expose DNA's lease. With distinct old/new wire identities, correct retirement predicts no overwrite of the new topic's reader; wrong routing predicts cross-delivery. If the server reuses the same numeric topic ID on the same connection, a late old payload may be indistinguishable from a new one; do not invent a hidden Excel generation or call that provider protocol ambiguity a graph defect. Separately inject an explicit old lease/sequence at DNA's ingress: it must be rejected without changing the current subscription.

**Design change:** initial-adoption exception, batch unit, equality policy, pending-reader coverage and lifecycle retirement. Thread ownership and native COM behavior are platform-sensitive. Supplied initial-connection anchors: [XLL_COM_HOSTING.md:1299–1319][native-measurements].

### E13 — Async completion, stale authority and dependent passes

**Setup:** native async fixture enters once, logs its handle/thread and returns pending. A controlled server releases value11 only after the runner's signal. A dependent logs execution. Start in Manual with a force command and record actual pending caches/status, command return and completion events; do not prescribe an unmeasured temporary cell error.

**H1, completion of the original invocation:** one original entry; xlAsyncReturn admits the value; the necessary dependent pass runs and reaches11-derived results. **H2, completion starts a new original evaluation:** entry count increases. **H3, completion is incorrectly treated as an authored edit:** authored history/revision changes in DNA despite no authored operation.

Run separate stale cases: cancel; replace the formula with constant7; delete/recreate the formula at the same address; change its argument so a new invocation is pending; close/reopen. Return the old handle, then the valid new handle, and reverse their completion order. Reject-stale policy leaves constant7 or the new invocation's result intact; stale-install policy overwrites it with11; automatic-restart policy adds an entry. Duplicate return is another independent arm.

Capture native callback success/error separately from final workbook installation. A mailbox acceptance can precede a later authority rejection. Vary worker timing using release barriers, test handle reuse, and distinguish callback thread from installation/dependent threads.

**Design change:** semantic token contents, revocation, result installation and dependent-pass admission. Current physical checks and host hooks are evidence of a base to extend. [CODE] [callbacks.rs:525–556][async-callback], [session.rs:568–593][session], [engine/recalc.rs:2157–2160][reevaluation].

### E14 — External links, materialized refresh and data-table mode

**Setup:** controlled source workbook has value10; target has an external-link formula and a logged dependent, warmed to10. Close/update the source to20 through a separate controlled writer, retaining both versions. Under each target calculation mode, issue the actual link-refresh/update command, read caches, then ordinary Calculate. Record whether provider admission, formula calculation and command completion coincide.

**H1, refresh changes an input cut and calculation is mode-dependent:** Manual admits20 externally while dependent formula caches remain at their permitted old values until calculation; Automatic updates the admitted affected calculation. **H2, refresh forcibly calculates readers in all modes:** the Manual dependent also changes during refresh. **H3, refresh only stages data:** caches stay old until a separate admission/action. Capture direct linked-cell behavior separately from downstream formulas; they need not share a timing rule.

**Materialized table copy:** query/fixture output initially one row10, then two rows20/30. T's total formula initially10. Refresh under Manual and Automatic Except Tables. Distinguish source completion, actual grid/table resize, total becoming50 and events. Add a what-if data table in a separate area. A category-specific mode predicts structured-table formulas and what-if tables can differ; “defer every table” predicts the total incorrectly remains deferred wherever Excel updates it. Test a pivot cache/output refresh separately, with old/new source versions and actual pivot output recorded.

Run cancellation and target deletion/recreation while provider output is staged. Authority checking predicts no installation into a replacement object; address-only routing predicts a wrong update. Save/reopen after each admitted stage with calculation-before-save controlled.

**Design change:** refresh ownership, calculation admission, target generations and clear distinction between computational data tables and data refresh. The packet supplies no refresh implementation evidence, so use results to deliver one coherent provider route first.

### E15 — Cached VBA observations and the Office.js API contract

**VBA setup:** A1=10; B1=`Snap()` where Snap logs and returns `Application.Caller.Worksheet.Range("A1").Value2` without an explicit worksheet argument or volatility; C1=`MarkD("C",A1)`. Warm B/C=10. Set A1=20 and Calculate.

**H1, cached observation:** C becomes20 but B stays10 until explicitly forced. **H2, hidden read becomes a dependency:** B also becomes20. **H3, hidden read induces volatility:** B also enters on a subsequent unrelated clean Calculate. Force B to prove that it can observe20, then repeat with a dirty formula precedent and reversed range order to test pinned-cache versus fresh-demand behavior. This tests the documented distinction, without extending VBA privileges to ordinary XLL callbacks. [DESIGN] [RECALC_CONTRACT.md:503–509][read-contract].

**Office.js setup:** actual supported Excel JS runtime, pure workbook A1=10/B1=`A1*2`, warmed B1=20, Manual. Register available changed/calculated events with nonmutating logging. The following is an API-level script outline; bind names/enums to the SDK and requirement sets present in the host, since current Office.js documentation is not supplied:

```javascript
// PROPOSED outline, not a claim that every named capability is available.
await Excel.run(async context => {
  const sheet = context.workbook.worksheets.getActiveWorksheet();
  const source = sheet.getRange("A1");
  const result = sheet.getRange("B1");
  source.values = [[30]];
  result.load("values,valueTypes");
  await context.sync();
  record("after write/load sync", result.values, result.valueTypes);

  // Use the supported application calculation operation and recalculate enum.
  context.workbook.application.calculate(Excel.CalculationType.recalculate);
  result.load("values,valueTypes");
  await context.sync();
  record("after calculate/load sync", result.values, result.valueTypes);
});
```

**H1, Manual load is cached:** first read20. **H2, load/sync demands fresh calculation:** first read60. For the second read, synchronous supported-calculation completion predicts60; an API that admits work without awaiting its relevant completion can expose old/pending status followed by a later update. Test calculate/load in one batch versus separate sync calls, then Automatic and any supported sheet/range operations.

On native Excel add the controlled async fixture: a returned JS batch might precede completion of all async descendants. Capture later events/read values rather than assuming `context.sync()` means a globally fresh workbook. On web Excel use only available capabilities; native XLL/COM absence is not a scheduler observation.

**DNA product gate:** repeat equivalent commands through its actual JS adapter and skin transport; two result/status changes at the same authored revision must both be visible, and range completion must retain its qualification. This API experiment establishes observable batch/read/event behavior, **not Excel's internal scheduler or thread order**. Callback delivery order is insufficient to infer either.

### Build controls and interpretation

First reproduce the supplied 16.0.20527-family captures where that build is available. Then test intended supported builds, 32/64-bit native configurations and MTR settings separately. Keep observed differences attached to build/platform and fixture lineage; do not average them into a single result. Office.js desktop and web are distinct populations. For cold-chain and nondeterministic cases, repeat trials and preserve raw traces; define only the equivalences justified by the observations. For a new result that contradicts this proposed design, change the narrow policy or retain an explicit refusal—do not relabel the observation as an unimportant quirk.

## 6. What to stop or delete

**Delete the sticky-OR publication rule after the completed-evaluation reducer replaces it.** Do not leave a second path that merges the plan seed into a completed false result. Also remove any conditional “update volatility only when a request exists” assumption unless another demonstrated reset already provides the per-evaluation result. The OR defect is established; persistence in the no-request case still requires checking surrounding code. [CODE] [engine/recalc.rs:2724–2735][publish-volatility].

**Stop treating retained progress as a universal compatibility guarantee.** Keep cooperative scheduler yields. Replace the documented unconditional no-reentry assumption in the Excel workbook route with captured semantic abandonment policies. Preserve the suspension study and its raw observations; change the implementation claim, not the evidence. OneCalc's values-only use of the evaluator does not require workbook scheduling machinery. [DESIGN] and [MEASURED-EXCEL] [EVALUATOR_SUSPENSION.md:33–87][suspension].

**Remove duplicate semantic adapters when the replacement reaches the product route.** UI/VBA/JS-specific dirtying, shadow workbook stores, name-value evaluators, separate range schedulers or refresh code directly rewriting formula caches would duplicate the chosen owners. The packet does not establish that these all exist; this is a prohibition on adding them and an instruction to remove any found during route integration, not an invented list of current files to delete.

**Stop using one boolean for several meanings.** Pending external input, stale cached result, command completion, local satisfaction, unsupported capability, circular hold and globally fresh workbook are different outcomes. Keep compact typed state sufficient for actual routes; do not build a status framework beyond those needs.

**Stop value-equality cutoff and blanket restart on every event.** Equal bits do not prove that required calls/events can be skipped. RTD, async completion, initial ConnectData and refresh have distinct admission semantics. Restarting every formula discards the selective mechanisms already present and can repeat effects. Retain the selective graph and narrow initial-input adoption. [DESIGN] [RECALC_CONTRACT.md:24–30][contract-decisions]; [CODE] [state.rs:671–718][rtd-invalidation].

**Stop conflating cached reads with edge deletion.** Out-of-scope formula caches, cached VBA observations and `#` native reference reads have different contracts. Keep structural argument edges and explicitly project runtime policies. [DESIGN] [RECALC_CONTRACT.md:501–509][read-contract]; [MEASURED-EXCEL] [XLL_COM_HOSTING.md:1277–1289][native-measurements].

**Retire stale prose once current callers are checked.** In particular, reconcile the old pending-only sentence and the “no sheet-span product caller” comment with current routes. Keep historical documents as historical evidence, with their status clear. Do not reopen repaired status-64, numeric-topic or initial-RTD-entry defects without a new failing case. [DESIGN] [RECALC_CONTRACT.md:497–499][read-contract]; [CODE] [state.rs:1409–1416][dynamic]; [MEASURED-EXCEL] [XLL_COM_HOSTING.md:1263–1319][native-measurements].

**Remove unnecessary adjacency copies before exotic graph infrastructure.** Preserve spatial indexes and share range subscriptions when expansion is the measured bottleneck. Defer custom allocators, lock-free graph mutation, generalized distributed scheduling, parallel iterative sweeps and speculative name caches. They cannot repair the semantic gaps and can obscure them.

**Defer claims about complete browser/native parity, WebGPU, universal Excel scheduling and broad refresh support until a corresponding route works.** Deliver one coherent capability with explicit limits. Keep native ownership, Safe/Full capability policy and process isolation separate. Do not widen OneCalc to make workbook-host gaps disappear.

**Keep every useful compatibility artifact.** Preserve x17/x18 captures, the suspension and volatility studies, scope/save evidence, mismatch/refusal records and original cache provenance. Test existence is not a pass; saved agreement is not a live pass; a refusal is not an exclusion that improves the rate. The next useful milestone is a coherent supported route with the same values, calls, events and poststate—not a new architectural layer.

### Pinned evidence locators

All DnaCalc links below use `3b5aa86b3ba47cbd216daec70c5d6586ad520deb`. The separate CalcVBA pin is recorded at the top; no CalcVBA implementation excerpt was supplied, so this review makes no file-level claim about its internals.

[contract-decisions]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/RECALC_CONTRACT.md#L10-L31 "docs/architecture/RECALC_CONTRACT.md:10–31"
[read-contract]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/RECALC_CONTRACT.md#L495-L521 "docs/architecture/RECALC_CONTRACT.md:495–521"
[volatility-seed]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/VOLATILITY.md#L24-L146 "docs/architecture/VOLATILITY.md:24–146; two supplied excerpts"
[volatility-events]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/VOLATILITY.md#L77-L146 "docs/architecture/VOLATILITY.md:77–146"
[suspension]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/EVALUATOR_SUSPENSION.md#L23-L97 "docs/architecture/EVALUATOR_SUSPENSION.md:23–97"
[native-measurements]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/XLL_COM_HOSTING.md#L1263-L1319 "docs/architecture/XLL_COM_HOSTING.md:1263–1319"
[prerequisite]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/graph.rs#L106-L132 "crates/calc-engine/src/graph.rs:106–132"
[graph-index]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/graph.rs#L343-L402 "crates/calc-engine/src/graph.rs:343–402"
[formula-node]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/state.rs#L21-L58 "crates/calc-engine/src/state.rs:21–58"
[dynamic]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/state.rs#L1390-L1416 "crates/calc-engine/src/state.rs:1390–1416"
[rtd-invalidation]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/state.rs#L671-L718 "crates/calc-engine/src/state.rs:671–718"
[cycle-fallback]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L3567-L3621 "crates/calc-engine/src/recalc.rs:3567–3621"
[cycle-order]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L3680-L3721 "crates/calc-engine/src/recalc.rs:3680–3721"
[reevaluation]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L2157-L2192 "crates/calc-engine/src/recalc.rs:2157–2192"
[host-recalc]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/recalc.rs#L139-L185 "crates/calc-host/src/recalc.rs:139–185"
[session]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/session.rs#L549-L611 "crates/calc-host/src/session.rs:549–611"
[callback-latch]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/callbacks.rs#L305-L337 "crates/calc-xll-host/src/callbacks.rs:305–337"
[async-callback]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/callbacks.rs#L525-L556 "crates/calc-xll-host/src/callbacks.rs:525–556"
[native-disposal]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/invoke.rs#L298-L326 "crates/calc-xll-host/src/invoke.rs:298–326"
[earlier-evaluator]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-11/pro/8-evaluator-design-answer.md#L693-L710 "docs/architecture/review-2026-11/pro/8-evaluator-design-answer.md:693–710"
[closure]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L3454-L3508 "crates/calc-engine/src/recalc.rs:3454–3508"
[publish-volatility]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L2722-L2737 "crates/calc-engine/src/recalc.rs:2722–2737"
[reported-bank]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/research/gamma-family-2026-11-12.md#L305-L348 "docs/research/gamma-family-2026-11-12.md:305–312 and 344–348"
[reported-capture]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/plan/capture-batch-1-results.md#L24-L39 "docs/plan/capture-batch-1-results.md:24–39"
[reported-enron]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/plan/enron-saved-cache-triage.md#L8-L56 "docs/plan/enron-saved-cache-triage.md:8–21 and 35–56"
[reported-tracker]: https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/.beads/issues.jsonl#L455 ".beads/issues.jsonl:455"
