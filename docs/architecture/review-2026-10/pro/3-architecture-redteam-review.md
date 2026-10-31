# DNA Calc architecture red-team review

Review date: 31 October 2026  
Basis: the supplied architecture pack, especially Appendix A and the overriding owner rulings in Appendix B §§4.1–4.8.

## Verdict

Do not freeze the recalculation contract in A §1.3 or implement 3b-1 against its present meaning of "settled". The largest remaining risks concern observable execution, scoped freshness, and object identity across edits. They cross the evaluator, scheduler, VBA host, persistence layer and UI; adding them after scalar partial publication will change the contract already implemented.

Three changes should precede that work:

1. Separate execution order, the calculated values an invocation may observe, and publication to subscribers.
2. Give commands, semantic passes and individual invocations distinct identities; distinguish a scheduler retry from a new semantic evaluation.
3. Make authored transactions and result certificates revision-aware, including permitted stale reads, blank-cell occupancy, binding resolution and formula identity.

The formula-block work also needs an early correction: normalized text is not a complete semantic identity unless formula dialect and placement rules have already been normalized into it.

### Evidence boundary

This is a review of the supplied pack, not a fresh source-code audit. The pack identifies DnaCalc 5a9348b and OxVba 0ad2ad28; its later OxVba account supersedes the older audit in Appendix F. Neither repository nor a running Excel instance was available here. Code-location claims remain attributed to the supplied appendices. Microsoft and other primary documentation was checked for the external claims cited below. The workbook cases are proposed reproductions and measurements, not claimed executed tests.

All recommendations retain the owner's plain-coordinate model, cells and defined names as the semantic graph-node kinds, Excel's limits as the immediate scale target, early binding in both directions, and the permitted optional asupersync adapter. Activation records, scenarios, certificates and iteration sessions below are execution metadata, not additional semantic graph-node kinds.

## 1. Findings ranked by late redesign cost

The ranking reflects how much already-written code and persisted behaviour would have to change if the issue were discovered late.

| Rank | Finding | Main contracts affected | Decide before |
|---|---|---|---|
| 1 | Cached VBA observations make pure-work ordering observable | Evaluation, effects, scheduler, callbacks | 3b-1 and V1 |
| 2 | Command completion, freshness and invocation identity are conflated | Result state, cancellation, replay, external returns | 3b-1 |
| 3 | Structural transactions and workbook identity are incomplete | Model, undo, async, COM Range, commands | C2, U1, V1 |
| 4 | Formula blocks omit semantic dialect and general edit semantics | Templates, codec, binder, VBA setters | C2/C3 |
| 5 | Name definitions are not universal result slots | Notebook, memoization, bindings, closures | 2a/2b |
| 6 | Spill, iteration and data-table boundaries exceed scalar/SCC transactions | Scheduling, readiness, volatile passes | 3b contract |
| 7 | Typed hosting lacks a complete continuation and ABI contract | VM, JIT, COM, reload, OneCalc | V1/V3 |
| 8 | Partial publication is ahead of a coherent save contract | Caches, XLSB, opaque parts, recovery | C3 |
| 9 | Context validation is not an invalidation policy | Formatting, precision, metadata, cutoff | 1c/1e |
| 10 | Bounded work and Exact execution are not yet reconciled | Numeric kernels, worker responsiveness | 3a, R1 |
| 11 | Wire ordering is incomplete without transaction and semantic views | Tiles, accessibility, commands, replay | U1/U2 |
| 12 | Runtime and seed choices cannot supply workbook determinism | Adapters, effects, replay, fairness | calc-sched API |

### 1. Cached VBA observations make otherwise pure scheduling observable

**Target: A §§1.3–1.4 and S4–S5.**

The synthesis correctly separates VBA's internal cell reads from dependency edges, but then proposes synchronously calculating an uncalculated cell before returning its value. These are independent choices. Microsoft permits VBA UDFs to observe uncalculated cells; an observation read is not automatically a demand for a fresh value. [S1]

Use this workbook:

~~~text
A1 = 1
B1 = A1+1
C1 = PeekB1()
~~~

~~~vb
Function PeekB1()
    PeekB1 = Sheet1.Range("B1").Value2
End Function
~~~

After calculating, switch to Manual, change A1 to 10, and calculate C1 alone. Compare its observation before and after explicitly calculating B1. Record the initially cached value and the full call trace. Also test a VBA UDF observing its own caller.

The architecture problem extends beyond this probe. Suppose an unrestricted UDF will observe B1's cache. Computing B1 early because it is visible can change the answer the UDF observes, even if all UDF calls retain their original order. Serializing effectful units only against other effectful units is insufficient.

**Required distinction:**

~~~rust
struct ReadRequest {
    purpose: ReadPurpose,
    policy: ReadPolicy,
    observation: ObservationFrontier,
}
enum ReadPolicy {
    DependencyWithinCommand,
    CachedMacroObservation,
    RegisteredWorksheetCallback,
    DiagnosticSnapshot,
}
~~~

The purpose answers whether the read contributes an invalidation dependency, a replay observation or a diagnostic. The policy answers which values it may return and whether calculation is permitted. The observation frontier answers which completed calculations are visible to this invocation.

Initially, put conservative fences around unrestricted callbacks and the pure-result transitions they may observe. Later, a logical read view can permit physical computation to run ahead without exposing those values to earlier invocations. Do not promise that a fence covering only the cell's known argument references is sufficient: the callback's actual read set may be discovered inside user code.

This establishes a real tradeoff. If arbitrary VBA can inspect any cached cell, fully unrestricted visible-first publication and exact observations cannot both be assumed. Measure the compatible calculation order and choose the permitted visibility frontier explicitly. A serial reference implementation chosen by DNA Calc is not, by itself, evidence of Excel's order.

### 2. "Settled" conflates a completed command with workbook freshness

**Target: A §1.3, including completion points, effect admission and volatile ticks.**

A result may correctly implement a narrow calculation command while using an out-of-scope cached precedent. Scope is therefore part of command completion; it cannot erase the age or dirtiness of inputs. Range calculation resolves dependencies within its selected range. [S2]

~~~text
A1 = 1
B1 = A1*2
C1 = B1+1
~~~

Calculate all; set Manual; change A1 to 5; calculate only C1. Then calculate B1, then C1 again. The implementation must reproduce each operation's permitted read behaviour without advertising the first C1 result as current with every workbook input.

Use two dimensions:

~~~rust
struct ResultStamp {
    computed_by: CalcCommandId,
    reads: ReadCertificate,
    completion: CommandCompletion,
    freshness: Freshness,
}
~~~

A read certificate needs value versions and invalidation versions. A dirty formula can still have the same published value version it had before the edit; validating only that value version misses the fact that the value is awaiting recomputation. For an intentionally cached read, preserve that condition in provenance instead of rejecting it or silently forcing calculation.

Correct the completion-point sentence now:

- Reading Range.Value/Value2 is a read with defined semantics, not an unconditional calculate-and-drain operation.
- Saving may preserve dirty caches. In Manual mode, CalculateBeforeSave explicitly controls pre-save calculation. [S3]
- An explicit calculation command has a scope and a completion receipt.
- A snapshot export can request a stronger freshness policy, but that must be a separate declared operation.

#### Invocation identity is also missing

"Never retry effectful units" needs to mean never accidentally repeat the **same semantic invocation**. It must not mean one UDF call per cell per tick. Excel's calculation process can evaluate formulas more than once while adjusting its chain. [S2]

~~~rust
struct InvocationId {
    command: CalcCommandId,
    pass: CalcPassId,
    locus: EvaluationLocus,
    call_site: CallSiteId,
    occurrence: u32,
}
~~~

A suspended effectful invocation resumes its existing activation; it is never re-executed because of a scheduler retry. A pure attempt may be recomputed with the same recorded nondeterministic inputs. A new semantic evaluation receives a new invocation identity, including within one pass. Occurrence must distinguish repeated calls inside a LAMBDA, array traversal or recursion.

The random key quoted in A §1.3, (seed, tick, cell), is consequently an incomplete contract even if random.rs internally has more machinery. Contrast RAND()-RAND() with LET(r,RAND(),r-r). Also test a named RAND used twice and RAND inside a repeated LAMBDA.

More decisively, Microsoft documents indeterminate-size #SPILL! for SEQUENCE(RANDBETWEEN(1,1000)) when shape does not stabilize across calculation passes. Freezing the same random draw across every pass could manufacture stability. [S4] Retain the selected generator; specify draw identity and replay semantics separately.

#### External completions are different kinds of events

~~~rust
enum ExternalOperation {
    AsyncReturn { invocation: InvocationId, result: Value },
    RtdUpdate { subscription: SubscriptionId, sequence: u64, value: Value },
    LinkRefresh { source: ExternalSourceId, revision: SourceRevision },
}
~~~

An async return completes a particular outstanding call. An RTD message updates an ongoing subscription. A link refresh changes an external input snapshot. Provider generation alone cannot distinguish two calls to the same provider from the same cell with different arguments.

Microsoft's XLL async interface is explicitly call-handle based and imposes argument-lifetime and callback restrictions. [S5] An adapter must preserve these obligations. Do not turn every return into an ordinary edit that restarts the formula and repeats effects preceding the asynchronous call.

Separate yielding, superseding pure work, user cancellation, and shutdown. "Cancel means yield" is not enough to describe outstanding external operations or a cancelled calculation command.

### 3. Structural changes need atomic authored transactions and revision-qualified identities

**Target: A §1.2, S2, U1, C2/C3 and the saved-Range proposal.**

An address at a revision and (BlockId, offset) are storage locations. Neither alone authenticates a late result.

Construct this trace:

1. B10=SlowAsync(A10) begins at revision 20.
2. Insert row 5; the formula moves to B11.
3. Put an unrelated formula at B10.
4. Return the original asynchronous result.
5. Undo the insertion while another old return is pending.

The host needs to decide whether each original activation transfers through the edit or becomes invalid. Looking up its old address, reused slot or old block offset is unsafe.

~~~rust
struct EvaluationTarget {
    workbook: WorkbookInstanceId,
    origin_revision: Revision,
    formula_generation: FormulaGeneration,
    locus: EvaluationLocus,
}
struct PreparedTransaction {
    base: Revision,
    command: CommandId,
    forward: ModelPatch,
    inverse_authored: ModelPatch,
    coordinates: AddressTransform,
    bindings: BindingDelta,
}
~~~

Commit formula membership, inputs, references, names, styles, merges, rules and related geometry as one authored-state transition. Effects and calculation results have separate lifecycles.

**Undo must create a new revision.** It restores authored intent; it does not rewind time, RTD deliveries, arbitrary module state or an external UDF's effects. An inverse engine patch is valid only for derived state whose certificates still hold. Otherwise invalidate and recompute. Reusing old calculation caches is an optimization with a validity check.

#### Saved Range needs more than SheetId and a rectangle

A live Range object needs workbook incarnation, sheet identity/generation, AreaSet and a structural-tracking policy. Resolve its values against the invocation's appropriate read view, while preserving the workbook and location to which the object actually belongs.

For example, a saved Range acquired in workbook A must not silently refer to workbook B's coincidentally equal local SheetId. A multi-area Range cannot be reduced to its bounding rectangle. [S26] Structural edits may require moving or invalidating the retained descriptor; this needs an Excel probe.

Use an index of outstanding handles or retained address transforms. This adds cost to live objects, not to every cell, and does not introduce stable row/column keys.

#### Define the calculation domain above workbooks

The ownership of application-wide calculation mode, iteration settings and Application.Calculate needs a named home. Workbook-specific date systems and precision settings should not accidentally absorb those application policies. Microsoft distinguishes application-wide calculation settings from workbook settings. [S2], [S6]

A CalculationDomain can coordinate workbook actors and open/closed external links without implementing distribution. At minimum, type workbook identity into references, callable frames, asynchronous targets and save requests. Probe opening two workbooks with different saved settings in both orders.

### 4. Formula blocks need semantic identity and a general edit algebra

**Target: A §1.2, S2, C2/C3 and 5b.**

Two identical input strings need not describe the same calculation. Microsoft's Formula/Formula2 documentation distinguishes implicit-intersection evaluation from array evaluation. Formula entry can prevent spilling even where Formula2 entry of the same text can spill. [S7]

~~~vb
Range("B2").Formula = "=SQRT(A1:A4)"
Range("D2").Formula2 = "=SQRT(A1:A4)"
~~~

Before coalescing formulas, either retain FormulaDialect at ingress or normalize intersection/lifting into the authoritative IR. Cover UI, clipboard, Formula, Formula2, R1C1 setters and file input. TemplateKind::Cell alone does not encode this.

A possible contract is:

~~~rust
struct SemanticTemplateKey {
    normalized_ir: TemplateIrId,
    placement: PlacementSemantics,
    evaluation: EvaluationSemantics,
}
~~~

Authored spelling and per-member calculation/file attributes remain separate from interning. Sharing compiled code is not permission to discard semantic attributes.

Row insertion also proves too little. Consider B1=A1:

- Copy B1 to B2: a relative reference moves with the copy.
- Move B1 to B2: the formula still refers to its original precedent.

These are distinct documented operations. [S24]

The operation must distinguish moving a formula, copying it, moving its precedents, partial-cell insertion/deletion, and sheet copy/move/delete. A block relocation cannot implement all these operations with one offset adjustment. Sheet order is also a binding dependency of 3-D references; the statement that 3-D references are left alone during row insertion must not become a universal transform rule. [S25]

**Before C2 freezes:** one sparse shared group, a Formula/Formula2 pair, absolute/relative/cross-sheet references, a relative name, a 3-D reference and CF/DV rules. Exercise copy, cut/move, insert, delete, undo and same-format save/reopen. Compare formulas and shapes as well as values.

#### Sparse membership must imply sparse results

"Columnar per block" must mean storage proportional to allocated members or populated chunks, not the bounding rectangle. Excel's full sheet contains 17,179,869,184 coordinates. A sparse bounding extent must not allocate a result vector or bitmap across all of them; logical area cardinalities also need checked wide arithmetic before conversion to platform-sized allocation counts.

Benchmark fragmentation after scattered edits and repeated insert/undo, not only the initial million-row fill. Report block count, index bytes, result bytes, retained snapshots and compaction work. A bounded, optional compaction policy is needed to avoid turning edit history into permanent storage growth.

### 5. Name definition, evaluation and notebook observation still collapse together

**Target: A §§1.2, 1.5 and decisions 4(a)–4(b).**

The following promise is impossible as written: a notebook can show the evaluated result of any defined name, yet inspection adds no evaluations.

Define an unused name Unseen=RAND(), or an unused counter-UDF name. Excel does not calculate unused names merely because a full calculation occurs. [S2] Showing a new value for Unseen is an additional evaluation. Giving it an explicit anchor resolves ambiguity; it does not remove the evaluation.

Define the distinction in the UI and API:

~~~rust
enum NameView {
    Definition,
    ExistingEvaluation { invocation: InvocationId },
    Evaluate { anchor: CellAddress, command: CalcCommandId },
}
~~~

Passive inspection displays the definition and any existing qualified result. An explicit Evaluate operation may create a new activation, with visible anchor, scope and freshness. A name used at different caller locations can have several legitimate results.

#### Binding caches need resolution proofs

Negative watches must include misses that permitted a successful fallback. If Sheet2!B1 resolves Foo to workbook Foo because Sheet2!Foo was absent, storing only the global NameId omits a relevant namespace fact. Local/global scope and qualification are part of Excel's name resolution. [S27]

~~~rust
struct BindingProof {
    resolved: ResolvedSymbol,
    probes: SmallVec<NamespaceProbe>,
}
~~~

Probe an existing formula and a newly entered formula after adding/removing the local Foo. Excel may preserve or rewrite particular already-bound references; do not infer that outcome only from lexical scope rules. The architecture must represent whichever rule the observation establishes.

#### Memo purity is transitive, and scenario identity is insufficient

~~~text
Inner = RAND()
Outer = Inner+1
~~~

Outer is not memo-safe just because its own syntax has no RAND token. Eligibility depends on transitive names, callees, captured values/references and context facilities. Editing Inner must invalidate that classification.

Even a pure SUM over absolute references needs read-certificate validation inside its scenario. ScenarioId alone is not a validity proof; it may still be a lookup key if certificates establish validity separately. An Arc<CompiledPlan> solves plan lifetime, but not captured environment lifetime, reference semantics or binding validity.

Finally, per-table Arcs do not make large-name edits cheap if changing one name clones a retained million-entry map. Measure name edits with live snapshots and closures before choosing the table representation. Use paged or structurally shared storage if copying the whole table appears. Include long chains, cycles, misses, shadowing and repeated notebook paging in the scale fixture.


### 6. Spill absence, iterative calculation and table scenarios exceed scalar publication units

**Target: A §1.3 and 3b-2.**

#### Blank reads need proof that no pending producer can occupy them

~~~text
Z1 = 1
A1 = SEQUENCE(Z1)
C10 = A10+1
~~~

After calculation, change Z1 to 10. A10 is still blank in the published grid until the new spill is installed. A1 need not be a producer in C10's old backward cone. Publishing C10 as settled at 1 is therefore unsound.

The read certificate must include occupancy/absence and unresolved-producer barriers, not just existing edges. The proposed may-spill index can implement this, but "conservative reach" needs an operational definition.

Unknown shape can force a very broad barrier: many dirty anchors above a viewport may potentially reach its blank cells. Measure thousands of such anchors, most ultimately returning scalars. Count how many block a visible reader and measure time to its first **settled**, not merely provisional, value.

Spill reservation also needs a deterministic conflict policy, dependency invalidation when an old footprint disappears, and treatment of obstructions, merged cells, tables and grid edges. Treating reservation, edges and values atomically is necessary; it does not select the policy.

#### An SCC is a graph-analysis unit, not a complete iteration algorithm

~~~text
A1 = (B1+1)/2
B1 = A1/2
C1 = LogPair(A1,B1)
~~~

Starting from cached zeros, one A-first sweep gives (0.5,0.25); one B-first sweep gives (0.5,0). These are mathematical examples, not asserted Excel outputs.

An IterationSession needs initial caches, prepasses, participant order, stopping rules and the effects of every semantic sweep. Microsoft's account includes circular dependents and volatile work in iteration, and describes sheet ordering. [S8] Running each SCC privately to convergence and its downstream DAG once can change both values and UDF traces.

Probe one/two iterations, nonconvergence, errors, sheet order and a downstream counting UDF. Dynamic edges may merge or split SCCs during the process. Execution metadata should permit that without treating every discovered cycle as an ordinary permanent cycle error.

#### A data-table run is a scenario transaction

The table must isolate substituted inputs, reference resolution, name memoization, cached callback reads, volatile invocation identity and temporary outputs. Publishing a scenario value into the base workbook's cache would contaminate unrelated evaluations. The base workbook must remain unchanged whenever other work can observe it. Retain the scenario overlay and execution state across yield; release or restore temporary state on completion, cancellation or error.

A shared FunctionContext decorator is not enough: specify an InputView plus scenario-specific result/read view and publication destination. Probe duplicate scenario inputs with RAND and a counter UDF, a reference-valued name, callback reads, and Automatic Except Tables. Preserve the relevant Excel treatment of table-induced cycles; Microsoft describes a special case for these. [S9]

### 7. Typed hosting must include semantic ABI, reentry and project-state lifetime

**Target: A §1.7, V1–V4.**

A fast Double-to-Double entry is necessary but insufficient to freeze calcvba-host-api.

The descriptors need argument passing, aliasing/writeback, omitted arguments, Empty/Null/Error distinctions, array bounds and element types, ownership of strings and objects, error propagation, permitted callbacks and suspension. VBA defaults to ByRef; FUNCDESC includes substantially more than a slot offset. [S10], [S11]

Define a callable lease over **both code and state**:

~~~rust
struct CallableLease {
    project: ProjectInstanceId,
    generation: CodeGeneration,
    signature: SignatureId,
    code: CodeLease,
    state: ProjectStateLease,
}
~~~

A code lease must not leave an old activation accessing newly reset module state. Test reload from F(Double) to F(String) while an invocation is suspended; retained callbacks must either remain coherently old or be rejected under a defined lifetime rule.

Native typed entry generation and browser call adapters may share semantic descriptors without pretending that a native pointer, COM ABI and wasm function-table entry are interchangeable. Keep the scalar path free of unnecessary conversions; measure string/array/object paths separately. "Allocation-free per call" is too broad for outputs that genuinely require owned storage.

#### The owner mailbox can deadlock without holding any lock

Engine waits for a VBA invocation; VBA calls a host method; the host enqueues a request behind the engine activation that is waiting for VBA. No lock is held, yet nothing can proceed.

Cached reads resolve some cases, but WorksheetFunction callbacks, external COM reentry and macro operations still need an invocation-aware path. COM STA dispatch requires message pumping and can reenter objects. [S12]

~~~rust
struct InvocationFrame {
    id: InvocationId,
    parent: Option<InvocationId>,
    view: ReadView,
    lease: ProjectLease,
    mode: InvocationMode,
    reentry: ReentryPolicy,
}
~~~

Prove one real typed callback through VM3 and JIT, including permitted nesting and reload exclusion, before freezing the API. OneCalc should supply the same abstract services from its fixture environment without acquiring an engine dependency.

#### An Excel IID is a complete interface contract

Matching a few DISPIDs and implemented slots is insufficient. COM interfaces are immutable contracts. [S13] Reusing an Excel IID requires the full inherited and declared layout, compatible signatures, ownership and identity behaviour, and correctly typed refusals for unsupported members. A truncated table can produce memory corruption instead of a useful refusal. Typed unsupported-member errors make a partial implementation memory-safe; they do not establish behavioural compatibility for clients that require those members.

Keep type-library compatibility separate from activation identity. A LIBID does not by itself redirect CreateObject; CLSID/ProgID policy and coexistence with Excel are distinct. Inspect real type information before V1/V3 descriptors freeze; external registration can remain later.

#### Read-only workbook access is not a sandbox

A trusted native extension can still perform arbitrary native effects or block indefinitely. WIT is an interface description; the runtime and supplied imports determine authority. Wasmtime explicitly treats host imports as the route outside the sandbox. [S14]

Specify trusted native versus sandboxed execution, granted host capabilities, memory/work limits and effect policy at registration. Do not infer these from a developer's "pure" annotation. A hard stop requires an isolation boundary that can actually terminate the work; it is not supplied by cooperative cancellation. Sandboxed instruction fuel also does not bound a blocking native host import.

### 8. Persistence has not caught up with partial publication or the XLSB promise

**Target: A §0, §1.3 and C2/C3.**

Saving must pin a model revision and a policy for formula caches. Walking "latest values" while units publish can serialize a combination never valid for the authored snapshot.

~~~rust
struct SaveRequest {
    model: Revision,
    results: ReadViewId, // pinned results and external-input frontier
    cache_policy: CachePolicy,
    format: SaveFormat,
}
enum CachePolicy {
    PreservePermittedCaches,
    RequireCalculatedScope(CalcScope),
}
~~~

Pin both the model revision and the result view: several calculation passes or external-update frontiers can exist at one authored revision. The appropriate policy depends on the requested Excel-compatible operation and calculation settings. It is not always "recalculate everything". A stale cache is allowed only with the intended cache/recalculation metadata; a mixed-revision cache must not be passed off as a fresh snapshot.

The described XLSB path converts binary formulas to templates/XML. C3 describes XML splicing, but the plan does not identify a corresponding binary writer. That is a **design gap**, not proof that unseen code has none. Same-format XLSB saving needs record/token reconstruction and all affected index mappings. If the actual deliverable is conversion to XLSX, say so explicitly in the capability surface.

Define ownership of opaque package parts and relationships. Preserving bytes for a part whose referenced names/sheets have changed may itself be incorrect. Every unsupported construct needs a preserve-when-safe, transform, explicit conversion or refusal policy. Macro parts and signatures make silent "best effort" especially dangerous.

Keep authored state, recoverable session/journal state, calculation caches and external-effect records distinct. File-format round-trip is not a substitute for session recovery, and a recovery journal must not rerun unknown completed effects.

### 9. Fact validation and function metadata do not yet define invalidation

**Target: A §§1.1, 1.3, 1.10 and S1.**

Recording a fact version when a calculation runs protects an in-flight commit. It does not automatically dirty an already-published reader when that fact later changes. Context facilities require an invalidation mechanism or an explicit policy to preserve old observations.

The "presentation-only" rule is too broad. Local repaint at another device scale is different from an Excel action that triggers calculation. Some divider actions are documented triggers; conditional formatting has refresh-driven evaluation; precision-as-displayed changes numeric semantics and may change stored values. [S6], [S8], [S9]

Keep distinct:

- Authored semantic changes, including number formats when precision-as-displayed applies.
- Changes to observable host facts.
- Conditional-format evaluation demand.
- Local rendering changes that only reuse committed values.

The shared resolver should expose calculation-relevant numeric precision separately from final display text. Do not let a conditional-format overlay accidentally create a numeric feedback loop through that service.

Early cutoff needs **observable facets**, even for pure formulas:

~~~text
A1: =1+1 -> =2          B1: =FORMULATEXT(A1)
A1: =2   -> constant 2  B1: =ISFORMULA(A1)
Hide a referenced row   B1: =SUBTOTAL(109,A1:A10)
~~~

Equal f64 values do not establish equal formula text, formula kind, shape, reference identity, row visibility or topology. Range/reference-valued results especially need identity-aware comparison. Use facet-aware invalidation and structural equality where needed; a hash alone is not an exactness proof.

#### Explicit context does not require expensive per-element dispatch

Construct a borrowed evaluation frame once. Keep hot scalar fields and pointers direct; put rare host operations behind narrow service interfaces. Provide span/range readers and batch read stamps so a SUM does not allocate or hash a certificate record for every element.

Benchmark the same evaluator with arithmetic-only formulas, range aggregation, name lookup, host facts and VBA callbacks. Measure allocations, cycles, bytes of read metadata and cold-service dispatches. The pack's per-cell memory and per-call allocation estimates are hypotheses until measured.

Catalogue entries should contain policies where behaviour depends on arguments, registration or context. Thread safety, volatility and permission to reorder are different properties. The MTR documentation has argument-sensitive restrictions; runtime volatility declarations also require an explicit update path. [S15]

### 10. Finite budgets are not yet compatible with every Exact kernel or foreign call

**Target: A §§1.3, 1.4, 1.8 and 3a.**

Counting completed cells does not bound latency. A single cell can scan a very large range, sort an array, recurse through LAMBDA or execute a long VBA loop. A budget checked only between cells leaves the worker unresponsive.

The x87 rule adds a second constraint: a continuation must not introduce an extra binary64 rounding point or change evaluation order. Either yield at an existing safe program boundary, or explicitly preserve the required extended-precision state and floating-point environment. Intel documents the control/status and register state that makes this more than saving an f64 accumulator. [S16]

Give long kernels resumable state, including their cursor and algorithmic/numeric state. Measure the longest non-yielding segment. JIT loops need safepoints if resumability is promised; arbitrary external COM/XLL calls cannot acquire a universal bound merely by being placed on a lane.

In a browser, returning through a chain of already-resolved promises is not sufficient evidence that queued worker messages are serviced. Test an actual task/event-loop handoff. The HTML event-loop model distinguishes tasks and microtask checkpoints. [S17]

Retain the owner's hardware-x87 decision. Record program identity, numeric backend and relevant floating-point environment alongside witness provenance. Backend selection alone does not establish that every function's algorithm is identified.

On the soft path, make refusal depend on the executed unsupported operation where the evaluator can do so safely. An untaken LN branch must not cause an otherwise evaluable expression to be refused. Preserve the distinction between engine refusal and an Excel worksheet error. Switching Fast/Exact must invalidate incompatible calculation caches and memo entries.

### 11. Binary tiles require transaction closure, bounded queues and a separate semantic projection

**Target: A §1.6 and U1–U3.**

Revision, publication sequence and dictionary generation are necessary, but do not specify the atomicity of a multi-tile result. A spill can occupy two tiles; installing only the first exposes half a committed spill to the renderer, hit testing or accessibility.

Choose either complete frames or a transaction marker:

~~~rust
struct TileFrame {
    base: PublicationSeq,
    next: PublicationSeq,
    revision: Revision,
    dictionary: DictionaryGeneration,
    transaction: PublicationUnitId,
    final_fragment: bool,
}
~~~

The mirror applies a complete unit coherently. Dictionary definitions must precede references; removal cells must be represented explicitly. Geometry and styles need version relationships too, otherwise a correct value tile can be painted or hit-tested against old row positions.

Distinguish intentionally omitted/coalesced delivery from missing required deltas. A sequence number over all engine publications cannot be assumed contiguous in each filtered subscription. Define per-subscription continuity or explicit coverage.

Backpressure must be bounded. A slow tab cannot retain unlimited tiles, snapshots and dictionaries. Coalesce only after recording the full logical event stream when replay needs it; on reset, atomically install a new mirror base.

#### One command model does not mean one undifferentiated message enum

~~~rust
enum ActorMessage {
    Command(CommandEnvelope),
    External(ExternalOperation),
    Interest(InterestUpdate),
    TransportAck(Ack),
}
~~~

The semantic command model can remain shared while separating edit authority, undo semantics, query traffic, external returns and UI-local intents. CommandEnvelope needs command identity, base revision and a resolved target. Revalidate availability/protection at execution time; a prior availability query is advisory.

Keyboard/focus and browser-authorized effects require a UI adapter. An old edit response must not focus a newly mounted skin or paste into a different target after a structural edit.

#### Accessibility cannot be delegated to RenderPlan snapshots

Define semantic cell identity, row/column coordinates and total counts, headers, spans, selection, editability and pending/refused descriptions independently of pixels. W3C documents the extra positional metadata required for virtualized grids. [S18]

Accessibility focus can demand cells beyond the visible pixel viewport. A screen reader's navigation must not depend on which tiles happened to be painted. Derive the accessible projection from the same committed units.

Keep a small real-browser test for focus, keyboard navigation and IME, plus a screen-reader check in renderer selection. Semantic and RenderPlan tests cannot establish that the actual accessibility tree is usable.

### 12. A runtime seed is not a workbook replay contract

**Target: A §§1.3–1.4 and Appendix G's deterministic-publication claims.**

Identical operations and interest changes do not guarantee identical publication streams when wall-clock budgets, worker completion timing, external returns and foreign code are uncontrolled.

Define two different replay goals:

1. Semantic replay: the same authored state and admitted semantic events produce the same permitted values and effect trace.
2. Publication replay: reproduce the chosen publication order and externally observed intermediate states.

The second needs recorded admission/publication decisions or a deterministic logical commit policy. A seeded runtime alone does not observe an external COM server, native CPU pool, OS entropy or unmediated I/O.

Record only what the chosen replay contract requires: accepted commands, semantic passes, external input values/sequences, volatile samples or deterministic draw identities, effect admissions/outcomes, relevant focus/interest events and publication choices. Keep diagnostic trace level from altering those choices.

Finally, fairness must be explicit. Retaining off-interest work does not ensure that it runs. Add aging or a finite-work service guarantee within permitted ordering, and report completion relative to a named command/frontier. An unending RTD stream need not reach one global quiescent state. A save or Calculate completion therefore needs a bounded admission frontier, not "wait until nothing can ever update again".


## 2. Implementation order

### The step most likely to be built twice

**3b-1: scalar certificates and publication.** Its later extension is not merely changing SchedUnit::Node into SchedUnit::Chunk. Scoped cached reads, observation frontiers, semantic passes, absence certificates and irreversible effect admission change what a certificate means.

Specify the common envelope first, then implement it narrowly:

~~~text
CalcCommandId / CalcPassId / InvocationId
EvaluationTarget / ReadPolicy / ObservationFrontier
ReadCertificate / ResultStamp / ObservableChange
PreparedTransaction / PublicationUnit / CompletionReceipt
~~~

These can be small types used by immediate callers. They are not a separate framework or a second engine.

**The step most likely to stall is 1b if "every duplicate table folded in, every consumer moved, help export" is one prerequisite change.** It spans parser, file writer, evaluator, function kernels, help and generated data. Establish the authoritative schema and first consumers early; move remaining consumers in working slices. All declarations should still have one owner. Do not block the calculation-contract experiment on handbook generation.

C2/C3 are the other rebuild risk: formula dialect, general transforms and same-format save must be decided before template/block storage hardens.

### Revised order

| Move | Reason |
|---|---|
| Bring the highest-risk Excel probes forward immediately | Cached UDF reads, Manual-range freshness, semantic invocation counts and reload determine API shape |
| Introduce a thin host command envelope before full U1 | The actor needs a real input path; it does not need all six skins extracted first |
| Combine the FEC split with read policy and invalidation facets | Otherwise the new explicit context carries the old ambiguous semantics |
| Bring one V3 typed callback/reentry proof alongside V1 | A scalar arithmetic entry does not prove hosting |
| Define formula dialect, operation algebra and save mode before C2 | Prevent lossy coalescing and one-way codec assumptions |
| Implement serial yielding plus narrow publication together | End-to-end behaviour is visible early; unsupported units retain explicit serial boundaries |
| Move a minimal spill/effect/async sentinel into that proving work | Their metadata should not be retrofitted after scalar publication |
| Add broad SCC/table support and optimized name memoization later | Their envelope is already demonstrated; their algorithms can grow incrementally |
| Add the native pool and block scheduling after the contract works | Executor changes should not alter read, invocation or publication semantics |
| Keep asupersync and renderer comparisons off the critical path | Both should be judged against the same working contract |
| Keep 0a, deletion work and research independent where possible | They need not become prerequisites for unrelated semantic corrections |

### Smallest first slice that proves the recalculation contract

Use one tiny loaded workbook and the existing UI/worker route, with a thin headless harness driving the same commands:

~~~text
A1 = 1
B1 = A1+1             // off-screen
C1 = B1*2             // visible
D1 = PeekB1()         // real typed callback, cached-observation probe
F1 = 1
E1 = SEQUENCE(F1)
G10 = E10+1           // visible blank-reader before spill growth
~~~

Keep the execution serial. Do not implement a worker pool, all skin commands, general iteration or a full macro object model for this slice.

The end-to-end trace must demonstrate:

1. A finite work slice returns to the actual worker event loop.
2. An interest change affects eligible pure scheduling without changing authorized calculation scope.
3. A command receipt precedes the values caused by the command; C1 can publish before unrelated work completes.
4. An edit arriving between slices invalidates conflicting pure work without losing reusable validated results.
5. A typed callback sees the permitted read view. An activation paused at a supported VM/JIT safepoint resumes without repeating effects; an external native call that cannot suspend follows the declared non-preemptible policy.
6. Manual calculation of C1 completes without falsely claiming global freshness.
7. Changing F1 from 1 to 10 prevents G10 from being labeled settled against an unproven blank.
8. Save with CalculateBeforeSave disabled preserves the chosen cached-state policy, and reopen is coherent.

PeekB1 alone does not prove suspension. Add a counting UDF with a controllable safepoint between incrementing module state and reading B1; resumption must increment only once. If the JIT does not yet support that safepoint, make it the immediately following hosting sentinel; cell-boundary yielding has not proved this invariant.

An asynchronous test provider should then deliver a result after overwrite/undo and after provider reload. It can be a deterministic provider fixture initially; a real XLL lifecycle adapter is a subsequent integration test. The stale result must be rejected by invocation/target identity.

Use a bounded, conservative serial boundary for unsupported units, with a reason and counter. This is an honest initial implementation of the shared contract. An uncounted whole-workbook fallback would hide whether visible-first is actually delivered.

## 3. Measurements that settle the unproven claims

These are targeted experiments, not a new mandatory per-commit process.

| Question or claim | Decisive experiment | Record |
|---|---|---|
| VBA read "calculates first" | Peek a dirty non-argument cell in Manual/range/full calculation | Old/new observations, nested calls, dirty state |
| Effect order survives interest changes | Same workbook, alternate viewport priority around Peek and a pure producer | Invocation order and values seen |
| Range.Value and save are completion barriers | Read dirty values; save with CalculateBeforeSave false/true | Cache XML/binary data before reopen |
| One effect invocation per unit | Counter before a late dynamic dependency; cold/warm/rebuild chain | Semantic passes versus scheduler retries |
| Tick/cell random key is sufficient | RAND pairs, LET, named RAND, repeated LAMBDA, documented volatile spill | Draw count, shape passes, final errors |
| SCC convergence is the iteration contract | One/two iterations across sheets with logging dependents | Prepasses, sweep order, seed caches, termination |
| Table context forwarding is sufficient | Scenario overrides with names, RAND, UDF cache reads and cancellation | Base/scenario values and restoration |
| Spill shadow is selective | Many unknown-shape anchors potentially covering visible blanks | Blocked readers, first settled latency |
| Equal template text is safe to coalesce | Formula versus Formula2 SQRT example through all entry/save routes | IR, @ behaviour, shape, SavedAsArray |
| Formula transforms generalize from row insert | Copy, cut/move, partial insert/delete, sheet copy/move, undo | Rewritten formulas and affected bindings |
| NameId binding invalidates correctly | Add/delete shadowing local name after successful global lookup | Existing/new formula bindings |
| Names scale after per-table Arcs | Edit one of 100k–1m names with snapshots/closures retained | Copied bytes, retained generations, rebind count |
| Callback ABI survives reload | Suspend F(Double); load F(String); resume and invoke retained handle | Code/state generation, failures, reclamation |
| Stored Range survives safely | Insert/delete/cut, switch workbook, close/reopen, multi-area Range | Parent, Address, Areas, Value2, errors |
| Typed path has the intended performance | Empty/scalar/string/array procedures and one Range callback | Allocations, conversion bytes, latency |
| Finite budget means responsiveness | One very expensive formula plus queued edit/interest | Longest non-yielding span, p95/p99 latency |
| Explicit FEC overhead is acceptable | Arithmetic, large range, name-heavy and callback workloads | Dispatches, allocations, bytes/read, throughput |
| Block storage survives editing | Dense fill, sparse membership, scattered edits, insert/undo cycles | Peak memory, block fragmentation, save size |
| Tiles remain coherent under pressure | Multi-tile spill, dictionary rotation, slow subscriber, reset | Atomic mirror state and bounded retained bytes |
| Native runtime improves the product | Same provider churn/CPU workload on adapter alternatives | Lifecycle failures, responsiveness, overhead, binary/build cost |

For Excel observations, retain input files, operations, saved caches, build/platform/CPU provenance and trace outputs. Documentation identifies the questions; where order or edge-case behaviour is unspecified, the target Excel trace decides.

For scalability, use operation counters over a geometric ladder of workbook sizes. Large corpus and schedule-exploration runs remain explicit or scheduled, in line with owner 4.6. Keep the fast loop focused on concrete regressions and contract invariants.

## 4. Concurrency foundation and asupersync

**Agree with decision 7, with a narrower claim about what the adapter contributes.** The owner has settled dependency acceptability. The technical decision is whether asupersync improves provider/task lifecycle and debugging enough to justify its integration cost.

DNA Calc must own:

- Command demand and compatible ordering.
- Ready queues and interest priority.
- The decision to admit an effect.
- Work-budget checkpoints inside evaluators/kernels.
- Invocation identity and stale-return rejection.
- Atomic publication, freshness and replay semantics.

The adapter can own task execution, channels, timers, cancellation delivery, resource draining and provider supervision. It must not leak Cx, Region, Outcome or runtime-specific cancellation meanings into calc-fml, calc-func or the stable scheduler interface.

| Option | Appropriate role | What still belongs to DNA Calc |
|---|---|---|
| Owned serial stepping executor | Reference execution and browser worker driver | All calculation policy and actual event-loop yielding |
| Small bounded pool or Rayon | Coarse independent CPU chunks | Priority, checkpoints, admission and deterministic commit rules |
| Tokio | Native asynchronous I/O/providers | Bounded CPU execution and non-abortable foreign-call policy |
| asupersync adapter | Structured provider lifetime, cleanup, deterministic lab experiments | The same spreadsheet policies and boundaries |
| Shuttle / Loom | Exploring controlled schedules and small concurrency kernels | Excel semantics and external-event capture |

Tokio documents that an already-running spawn_blocking closure cannot be aborted. [S19] asupersync similarly cannot force noncooperative foreign work to stop; its lifecycle guarantees do not turn arbitrary external effects into database transactions. [S20] Neither runtime solves a blocking native UDF or preserves cached VBA observations automatically.

A particularly bad mapping is one runtime region per recalculation that must fully close before the next edit begins. A provider that ignores cancellation could then hold interactive editing hostage. Let edits advance the authoritative model while obsolete, revocable work drains under its own lifetime. Reject late publications; retain only leases genuinely needed by admitted effects. Truly conflicting edits may need to wait for a non-replayable invocation, and that exception must be explicit.

Do not cancel and respawn calculation tasks merely to reprioritize a viewport. Maintain interest in DNA Calc's own ready queue; wake or reschedule bounded chunks. Region lifetime should describe ownership, not be synonymous with "the workbook is fresh".

The library survey is not a permanent build contract. The currently retrieved asupersync documentation specifies a nightly default feature set and a stable subset with defaults disabled, differing from the survey's stable-default wording. Pin commit/version, feature set and toolchain for the spike, and verify an actual downstream consumer. [S21]

The strongest adoption experiment is provider churn with delayed async returns, RTD bursts, cancellation, reload and deterministic failure reproduction. Compare against the serial driver plus a modest native I/O/pool arrangement. Measure actual workbook latency, retained memory, cleanup failures and developer diagnosis—not author-reported spawn/join ratios.

For testing, Shuttle provides reproducible randomized scheduling; Loom explores modeled concurrent executions with documented limitations. Neither constitutes a proof over foreign code or a complete workbook. [S22], [S23] asupersync's lab has the same boundary problem: controlled scheduler events cannot make unrecorded outside behaviour deterministic.

Salsa, Arrow and differential/relational systems should remain narrowly evaluated tools or references. Formula parsing/binding caches, columnar interchange and streaming table features are different workloads from Excel's effect-sensitive calculation semantics. Do not replace the engine contract with a library's query or collection model to obtain a scheduler.

## 5. Owner decisions

| # | Position | Reason |
|---|---|---|
| 1 | Agree, with ABI conditions | A neutral hosting API and one app composition edge fit the boundary. Specify semantic signatures, code-and-state leases, reentry and platform entry adapters before freezing it. |
| 2 | Agree in principle; change sequencing | Retire workbook-wide atomic publication, but define scoped freshness and observation/effect semantics before 3b-1. Pure scalar certificates alone are the wrong first contract. |
| 3 | Agree, with prerequisites | Start codec blocks now. Formula dialect, edit operation semantics, member attributes and save identity must precede authoritative C2/C3 storage. |
| 4 | Mixed | Agree with definition/evaluation separation and cells/names only. Reject unconditional "inspection adds no evaluations" for newly evaluated names; make evaluation explicit. Memoization requires transitive eligibility and read validation. |
| 5 | Agree with the declared platform policy | Keep hardware x87 as ruled and label soft-path limitations. Exact refusal must be structured and follow the executed program; backend choice alone does not establish function correctness. |
| 6 | Agree | tools/calc-func-lab keeps search programs and witness machinery outside the product closure; xtask can forward without acquiring product dependencies. |
| 7 | Agree | Keep asupersync optional, native-only and measured. Core calculation semantics and the maintained serial path must remain independent of its lifecycle API. |
| 8 | Agree; promote the contract-changing probes | Add cached dirty reads, scoped calculation, volatile spill passes, Formula/Formula2, scenario effects, retained objects and same-format XLSB save. These inform signatures, not just later regression coverage. |
| 9 | Qualified agreement | Observe real type libraries early. Reuse IIDs only for complete compatible interface contracts; keep type-library identity, activation identity and coexistence policy distinct. |
| 10 | Agree, with a concrete exit | Freeze the two specified feature-growth fronts until command ownership and the neutral typed callback proof land. Do not let a broad catalogue or renderer project prolong the pause. |
| 11 | Agree with incremental migration | One source of function facts, including argument/context-dependent policies. Establish ownership now; migrate consumers without making every export/help conversion a prerequisite to the calculation slice. |

## 6. Missing decisions worth making now

| Area | Minimum decision |
|---|---|
| Authoritative state | Authored model, calculated cache, external inputs and per-skin view state have distinct ownership and persistence |
| Transactions | Atomic authored edits, base-revision checks, command identity, inverse authored patches and validation before commit |
| Collaboration hooks | A server-sequenced operation log with revisions and target transforms is sufficient now; no CRDT or stable axes are required |
| Command retry | Repeated delivery of a command ID cannot apply an edit or launch an effect twice |
| Irreversible effects | Record started/completed/indeterminate outcomes; no automatic replay of arbitrary effects after a crash |
| Application scope | Calculation policy and cross-workbook coordination have an owner above an individual document |
| Recovery | Journal/checkpoint schema, cache invalidation on restore and external-effect recovery policy |
| File preservation | Same-format versus conversion/export; ownership of unknown parts and signatures |
| Security | Trusted native and sandboxed profiles, capability enforcement, and honest stop/timeout guarantees |
| Observability | Explain "why dirty", "why waiting", "which read view", "why this refusal" and "why this completion was rejected" |
| Debugging | Observing a name or stepping a formula must declare whether it creates an evaluation |
| Corpus testing | Retain failing workbooks and operation traces; shrink deterministically where possible; compare state, shape, text, effects and persistence as well as values |
| Memory lifecycle | Bound retained snapshots, obsolete compiled plans, captured closure environments, dictionaries, queues and object handles |
| Progress | Report a named command's admitted frontier and pending reasons, rather than one perpetual global "calculating" flag |

The immediate deliverable should be an executable instance of the corrected calculation contract. It should accept an edit mid-calculation, publish a valid visible result, preserve a cached VBA observation, reject a stale completion, and save a coherent authored revision. Crate splits and executor substitutions can then be evaluated against those behaviours.

## Sources

[P] User-supplied 3-architecture-redteam-prompt.md, Appendices A–H. All statements about the current code and owner rulings are attributed to this pack.

- [S1] Microsoft: Excel Commands, Functions, and States.
- [S2] Microsoft: Excel performance—Improving calculation performance.
- [S3] Microsoft: Application.CalculateBeforeSave.
- [S4] Microsoft: How to correct a #SPILL! error, indeterminate-size section.
- [S5] Microsoft: Asynchronous user-defined functions.
- [S6] Microsoft: Change formula recalculation, iteration, or precision.
- [S7] Microsoft: Formula vs Formula2.
- [S8] Microsoft: Excel performance—Tips for optimizing performance obstructions.
- [S9] Microsoft: Excel Recalculation.
- [S10] Microsoft: Function statement, VBA.
- [S11] Microsoft: FUNCDESC structure.
- [S12] Microsoft: Single-Threaded Apartments.
- [S13] Microsoft: Interface Design Rules.
- [S14] Wasmtime: Security.
- [S15] Microsoft: Multithreaded recalculation in Excel.
- [S16] Intel: Intel 64 and IA-32 Software Developer's Manual, Volume 2A, FXSAVE.
- [S17] WHATWG: HTML Standard, event loops.
- [S18] W3C WAI: Grid and Table Properties.
- [S19] Tokio: spawn_blocking documentation.
- [S20] asupersync: Runtime lifecycle and cancellation documentation.
- [S21] asupersync 0.5.0 documentation, toolchain and stable-subset sections as retrieved.
- [S22] AWS Labs: Shuttle.
- [S23] Tokio project: Loom.
- [S24] Microsoft: Move or copy a formula in Excel.
- [S25] Microsoft: Create a 3-D reference to the same cell range on multiple worksheets.
- [S26] Microsoft: Range object.
- [S27] Microsoft: Names in formulas.

[S1]: https://learn.microsoft.com/en-us/office/client-developer/excel/excel-commands-functions-and-states
[S2]: https://learn.microsoft.com/en-us/office/vba/excel/concepts/excel-performance/excel-improving-calculation-performance
[S3]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculatebeforesave
[S4]: https://support.microsoft.com/en-us/excel/how-to-correct-a-spill-error
[S5]: https://learn.microsoft.com/en-us/office/client-developer/excel/asynchronous-user-defined-functions
[S6]: https://support.microsoft.com/en-us/excel/change-formula-recalculation-iteration-or-precision-in-excel
[S7]: https://learn.microsoft.com/en-us/office/vba/excel/concepts/cells-and-ranges/range-formula-vs-formula2
[S8]: https://learn.microsoft.com/en-us/office/vba/excel/concepts/excel-performance/excel-tips-for-optimizing-performance-obstructions
[S9]: https://learn.microsoft.com/en-us/office/client-developer/excel/excel-recalculation
[S10]: https://learn.microsoft.com/en-us/office/vba/language/reference/user-interface-help/function-statement
[S11]: https://learn.microsoft.com/en-us/windows/win32/api/oaidl/ns-oaidl-funcdesc
[S12]: https://learn.microsoft.com/en-us/windows/win32/com/single-threaded-apartments
[S13]: https://learn.microsoft.com/en-us/windows/win32/com/interface-design-rules
[S14]: https://docs.wasmtime.dev/security.html
[S15]: https://learn.microsoft.com/en-us/office/client-developer/excel/multithreaded-recalculation-in-excel
[S16]: https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf
[S17]: https://html.spec.whatwg.org/multipage/webappapis.html#event-loops
[S18]: https://www.w3.org/WAI/ARIA/apg/practices/grid-and-table-properties/
[S19]: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
[S20]: https://asupersync.com/
[S21]: https://docs.rs/crate/asupersync/0.5.0
[S22]: https://github.com/awslabs/shuttle
[S23]: https://github.com/tokio-rs/loom
[S24]: https://support.microsoft.com/en-us/excel/move-or-copy-a-formula-in-excel
[S25]: https://support.microsoft.com/en-us/excel/create-a-3-d-reference-to-the-same-cell-range-on-multiple-worksheets
[S26]: https://learn.microsoft.com/en-us/office/vba/api/excel.range(object)
[S27]: https://support.microsoft.com/en-us/excel/names-in-formulas

