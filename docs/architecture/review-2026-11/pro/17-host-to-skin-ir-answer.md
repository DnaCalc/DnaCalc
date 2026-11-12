# 1. Verdict on today’s design, with evidence

**Keep the current vertical architecture. The next step is to finish and verify its consistency, lifecycle, and resource contracts—not introduce another workbook model, command framework, or tile protocol.**

The supplied implementation has moved materially beyond the October audit. It contains useful working mechanisms: host subscription deltas, a shared command/model package, bounded tile records, publication admission and completion controls, mirror continuity checks, a skin registry, and a reusable render plan. The most important remaining architectural question is whether these mechanisms form one coherent product route: **a command changes the authoritative workbook once, and every reader within a view observes a complete, correctly identified projection of that change.**

All citations below refer to the supplied excerpts at DnaCalc **`3b5aa86b3ba47cbd216daec70c5d6586ad520deb`**. The CalcVBA pin is **`8115aa78135114c1dea79f26cbc12c8a3b00a4cf`**. This review does not claim a build, an Excel run, or inspection of code outside the packet.

## What to keep

| Product route | What the supplied evidence establishes | What it does not establish |
|---|---|---|
| **Host changes → subscribed projections** | **[CODE]** `HostDelta` carries subscription identity, sequence, authored revision, reset semantics, changed cells, and explicit empty cells. `dispatch_from` provides a common ingress point. Keep these as the authoritative route. `crates/calc-host/src/session.rs:114–129,887–900`. | The excerpt does not show every command’s execution, validation, or subscription-diff implementation. |
| **Host projections → worker → tile mirror** | **[CODE]** The worker translates engine publication identity, updates streams, and publishes events. Admission, fragments, and completion exist; the mirror checks continuity and stages transactions. `apps/dna-calc-worker/src/server.rs:3776–3819`; `apps/dna-calc-model/src/projection.rs:1782–1784`; `apps/dna-calc-model/src/tile_mirror.rs:69–116`. | Full fragment decoding, completion, acknowledgements, and consumer installation are not supplied. Atomic application is an implemented contract with a remaining end-to-end proof obligation. |
| **Shared commands → browser adapter → skins** | **[CODE]** Common commands belong to `dna-calc-model`; the adapter executes their effects. The registry exposes open skin identities, shared editor surfaces, view fields, and focus tickets. `apps/dna-calc-model/src/command.rs:1–21`; `apps/dna-calc/src/skin.rs:18–38,58–76`. | Comments describe intended routing; they do not prove every action uses it or that delayed callbacks cannot steal focus. |
| **Committed projection → DOM/Canvas2D grid** | **[CODE]** `PlannedCell` already contains spans, borders, clipping, measured text information, physical anchors, and display rectangles. A Canvas2D adapter exists, with a DOM/Canvas selector. `apps/dna-calc-model/src/render_plan.rs:59–82`; `apps/dna-calc/src/grid/painter.rs:1–34`. | The painter body and complete interaction/accessibility routes are not shown. This is not evidence of WebGPU support. |
| **Names and notebook** | **[CODE]** Passive name records and explicit, anchor-qualified evaluation are separate. The notebook mounts through the shared controller and projects saved placements plus worksheet cells. `apps/dna-calc-model/src/names.rs:1–34`; `apps/dna-calc-model/src/projection.rs:824–832`; `apps/dna-calc-worker/src/server.rs:3862–3928`. | The shown notebook block cannot explicitly identify a defined name. A complete name-backed notebook route is not established. |
| **Browser and desktop execution** | **[CODE]** The wasm worker uses a finite evaluation budget and task scheduling. The desktop entry identifies WebView2/native-worker composition and exports the XLL callback. `apps/dna-calc-worker/src/worker.rs:94–143`; `apps/dna-calc-desktop/src/main.rs:1–31`. | The native owner/STA loop, reentrancy policy, and equivalent completion semantics are not shown. An evaluation-count budget is not a wall-clock bound. |

Three distinctions should remain explicit.

**First, current skin switching is not simultaneous viewing.** The current contract says exactly one skin is shown and the previous skin is dropped. Multiple logical editor surfaces do not establish multiple live workbook views. **[CODE]** `apps/dna-calc/src/skin.rs:4–10,26–31,74–76`.

**Second, the linked-boxes document is an undecided proposal.** It is a good candidate for exercising the shared interface, but not evidence of delivered name-based authoring, reverse import, or Excel round trips. **[DESIGN]** `docs/architecture/ideas/LINKED_BOXES_SKIN.md:1–3,27–48`.

**Third, tile completion must remain separate from calculation completion.** The existing documentation gets this right: a tile publication closes a projection of installed host results; it does not declare the workbook fresh. Preserve `calculation_through` as correlation and preserve individual provenance. **[CODE]** `apps/dna-calc-model/src/tiles.rs:1–5,25–28`; `apps/dna-calc-model/src/projection.rs:9–21`.

## What the reported measurements establish

| Population | Reported result | Appropriate interpretation |
|---|---:|---|
| Oracle corpus | **76,903 / 78,264**, with 1,361 failures | **[MEASURED-EXCEL]** Reported formula-cell comparison; not current UI/protocol validation. `docs/research/gamma-family-2026-11-12.md:344–348`. |
| Full bank | **3,895,333 / 3,910,795**, with 15,462 failures | **[MEASURED-EXCEL]** A different comparison population; unread and excluded rows remain failures. Same source, `:306–311`. |
| Capture batch 1 | **130,849 / 134,577**, across 289 captured files | **[MEASURED-EXCEL]** Partial capture coverage, not a rate over every selected file. `docs/plan/capture-batch-1-results.md:24–39`. |
| Enron saved caches | **12,961,186 / 19,666,149** saved-value agreements; **zero live passes** | Saved-cache comparison only. Its provenance rules explicitly prohibit treating agreement as a live Excel pass. `docs/plan/enron-saved-cache-triage.md:8–21,38–50`. |

The tracker’s reported checks likewise remain historical reports, not fresh verification of this review’s route. **[DESIGN]** `.beads/issues.jsonl:455`.

# 2. Specific defects and risks, ranked

**No catastrophic current corruption defect is proved by these excerpts.** There are demonstrated implementation costs and representational limitations, plus correctness risks whose decisive consumer code is missing. The table deliberately distinguishes them.

P1 means a correctness or capability gate before claiming the relevant feature. P2 means an important performance or completeness issue.

| Priority | Exact pinned location | Trigger and observable consequence | Evidence classification | Smallest reproducer or missing evidence |
|---|---|---|---|---|
| **P1 — stale lifecycle messages** | `apps/dna-calc-model/src/tile_mirror.rs:69–80,93–99`; `apps/dna-calc-model/src/projection.rs:1785–1788` | An old-workbook admission reaching a new mirror causes an identity error followed by `invalidate()`. `ProjectionFailed` itself carries only numeric `id` and reason. | **[CODE]** These shapes and branches are shown. **[INFERENCE]** Actual damage depends on whether the outer dispatcher already rejects stale incarnations. | Replace a workbook, reuse a numeric subscription ID, then deliver old admission, completion, failure, and reply messages. New state must remain usable. Inspect the outer routing guards. |
| **P1 — composite-view atomicity** | `apps/dna-calc-worker/src/server.rs:3799–3819` | One scene consumes several subscriptions affected by one host change; streams receive distinct transaction identities and may become visible separately. | **[CODE]** Per-stream transaction allocation is demonstrated. **[INFERENCE]** Tearing depends on how the consumer composes those streams. | Split a spill or frozen-pane view across two subscriptions; delay one. Read painting, hit-testing, committed editor source, and accessibility between deliveries. |
| **P1 — reply causality** | `apps/dna-calc-worker/src/worker.rs:104–124`; `apps/dna-calc-model/src/projection.rs:1805–1809` | A reply callback assumes its subscribed facts are installed merely because events were posted first. Deferred application or backpressure can invalidate that assumption. | **[CODE]** Posting order is shown. **[INFERENCE]** The stronger “already applied” claim requires missing consumer/barrier code. | Withhold publication completion or installation, then dispatch the reply. The callback must wait for its declared visibility condition or receive an explicit pending/failure result. |
| **P1 — aggregate resource bounds** | `apps/dna-calc-model/src/tiles.rs:9–19`; `crates/calc-host/src/session.rs:869–876`; `apps/dna-calc-worker/src/publication.rs:102–110` | Many large subscriptions, staging roots, and stalled consumers compose individually bounded allocations into excessive retained memory. | **[CODE]** Local limits and retained stores are present. **[INFERENCE]** A global cap or sharing mechanism may exist elsewhere; neither is evidenced. | Fill overlapping and disjoint subscriptions, stop acknowledgements, continue editing, and account for every retained copy and queued message. |
| **P1 — name-backed notebook capability** | `apps/dna-calc-model/src/projection.rs:1878–1883`; `apps/dna-calc-worker/src/server.rs:3874–3885,3919–3926` | A notebook block must target a name without a worksheet cell. The shown projection has placement, area, prose, and cell fields, but no name identity. | **[CODE]** A limitation of the shown route, not proof that no other name-aware code exists. | Open a workbook containing only names. Place, inspect, edit, explicitly evaluate, rename, save, and reopen a name-backed block without creating a surrogate cell. |
| **P1 — responsiveness during a single operation** | `apps/dna-calc-worker/src/worker.rs:95–98,106–110,127–143` | One slow evaluation, foreign call, or large serialization/posting task delays edit and cancellation processing despite a 64-evaluation budget. | **[CODE]** The count budget is shown. **[INFERENCE]** Wall-clock blocking is workload-dependent; interruption support is not shown. | Queue an edit/cancel behind one two-second call and behind a large publication. Measure owner-task duration and acknowledgement delay separately. |
| **P2 — notebook paging scans before paging** | `apps/dna-calc-worker/src/server.rs:3872–3918` | Large saved layouts are fully collected before `skip/take`; outline keys are checked against the collected placements. A 40-block response can require substantial work and temporary memory. | **[CODE] Demonstrated algorithmic cost.** For \(P\) placements and \(K\) outline keys, the visible code includes \(O(P)\) collection and up to \(O(PK)\) containment checks. | Use 100,000 placements; request first/last pages. Count visited entries, temporary objects, and latency. An upstream limit could constrain exposure, but is not supplied. |
| **P2 — retained-byte recount** | `apps/dna-calc-worker/src/publication.rs:107–110`; `apps/dna-calc-worker/src/server.rs:3799–3807` | A small edit, including an edit outside a subscription, can trigger a fold over every retained cell merely to recompute its byte charge. | **[CODE] Demonstrated implementation cost**, not measured latency. | Compare one-cell edits with 1,000, 10,000, and 65,536 retained cells. Instrument accounting visits; replace the recount with an incrementally maintained counter. |
| **P2 — same-revision presentation changes** | `apps/dna-calc-model/src/tiles.rs:35–38`; `apps/dna-calc-worker/src/server.rs:3799–3811` | Conditional formatting or another presentation fact changes without an authored-revision change. The shown fallback update is revision-driven. | **[INFERENCE]** This is a propagation obligation, not a proved omission: an earlier delta may already cover it. | Change an offscreen conditional-format dependency while visible numeric values remain identical. Verify a coherent new presentation root. |
| **P2 — codec allocation/amplification** | `apps/dna-calc-protocol/src/tiles.rs:44–60` | Unique cell content prevents whole-atom deduplication, while cloning and JSON serialization retain several representations. | **[CODE]** Cell clone, serialization, and dictionary-key string clone are explicit. **[INFERENCE]** The material cost needs measurement. | Compare repeated values, unique numbers, unique formulas, escaped strings, and repeated styling; measure allocations, encoded bytes, dictionary reuse, and peak retained memory. |
| **P1 before linked-box authoring — alias semantics** | `docs/architecture/ideas/LINKED_BOXES_SKIN.md:13–18,31–38` | Prefix stripping captures a LET/LAMBDA local, another name, a cell reference, or the wrong scope; “unwire to empty” invents unspecified semantics. | **[DESIGN]** Unresolved proposal, not a current code defect. | A binding matrix containing locals, real unqualified names, callable names, cell-looking suffixes, strings, qualified names, and relative name bodies. |

The earlier review’s warning about transaction closure remains useful test motivation. It should **not** be repeated as “the current protocol has no transaction closure.” **[EARLIER-REVIEW]** `docs/architecture/review-2026-10/pro/3-architecture-redteam-review.md:472–489`; compare the current admission/completion types cited above.

# 3. Recommended target design and the smallest migration

## 3.1 One semantic system, several projections

**[PROPOSED]** Preserve this ownership structure:

- **`calc-doc-model`** owns canonical authored content and persistent identities, including names, styles, geometry, and layout metadata.
- **`calc-doc`** owns file representation, preservation, and save/reopen behavior.
- **`calc-engine`** owns the graph, calculation state, spill semantics, publication scope, and freshness.
- **`calc-host::HostSession`** owns session command ordering, transactions, execution ingress, supported undo history, and Excel-sensitive host facts.
- **`dna-calc-worker`** adapts that host to transports and bounded projections.
- **`dna-calc-model`** owns reusable client state, committed projection mirrors, neutral commands, view state, and pure presentation planning.
- **Adapters and skins** own toolkit input, actual focus, IME surfaces, painting, and browser-authorized effects.

A projected mirror is a disposable cache of host facts. It is not a second authored workbook.

Presentation state needs a further distinction:

| State | Owner and treatment |
|---|---|
| Formulas, names, values, styles, row/column geometry, merges, conditional-format rules | Canonical authored/engine state, projected through the host. |
| Notebook block placement, box position, saved layout labels | Authored presentation metadata. Persist and undo through the host, but do not add calculation nodes. |
| Current scroll, viewport, transient selection, hover, focus, editor draft | Per-view session state. |
| Conditional-format results, editability, formatted-value facts, provenance | Derived host facts with publication identity. |
| Font measurements, shaped glyphs, pane transforms, GPU buffers | Local disposable presentation caches. |

Persisted workbook view settings may constrain a view, but a local scroll operation should not silently become an authored calculation operation.

## 3.2 Extend existing identities

**[PROPOSED]** Reuse the existing workbook, command, name, publication, and subscription identities. Add missing lifecycle qualification around them.

The essential distinctions are:

- **Skin identity:** a kind of presentation.
- **View identity:** one live instance of that presentation.
- **Mount/editor/interest generation:** which asynchronous continuation still belongs to that instance.
- **Workbook incarnation:** which opened or replaced workbook an object belongs to.
- **Name identity and definition version:** which definition an edit or evaluation addresses.
- **Host delta sequence:** continuity of the host subscription.
- **Tile sequence/transaction:** continuity and closure of the delivered projection.
- **Engine publication identity:** which calculation publication contributed results.

These are different meanings, not interchangeable revision counters.

A minimal sketch is:

```rust
// [PROPOSED] Extensions around existing contracts.

struct ViewLease {
    view: ViewId,
    mount: u64,
    interest_generation: u64,
}

struct PendingUiEffect {
    request: RequestId,
    workbook: WorkbookIncarnation,
    view: ViewLease,
    editor: Option<EditSessionVersion>,
    target: Target,
}

struct CommittedViewRoot {
    stamp: TileStamp,
    coverage: InterestCoverage,
    facts: Shared<ProjectionFacts>,
}

// Extend the canonical block-target contract;
// project it into the client rather than defining another authored model.
enum BlockTarget {
    Cells(ExistingCellOrRangeTarget),
    Name(NameId),
    Prose(ExistingProseId),
}
```

A name’s definition version is a precondition/result qualifier, not a replacement for its identity.

**Validate routing identity before entering a mirror.** An old-workbook message should be discarded as obsolete; it should not be handed to a healthy new mirror merely to fail its internal integrity checks. Qualify failures as carefully as successful publications.

A committed edit survives the disappearance of its initiating view. Its obsolete focus, clipboard, completion, or editor continuation does not.

## 3.3 Make one committed view root the common read boundary

**[PROPOSED]** Start with **one coherent projection subscription per view**, covering a bounded union of interests:

- visible viewport regions;
- frozen panes;
- required merge anchors and overflow lookahead;
- the active cell’s committed source;
- the semantic accessibility region.

A union must not become the bounding rectangle between frozen row 1 and a viewport at row 500,000.

Coverage must be explicit. The current reset rule says omitted cells **inside the interest** are empty. It does not authorize treating unloaded cells outside that interest as empty. **[CODE]** `crates/calc-host/src/session.rs:121–124`.

Capture cells, geometry, styles, name metadata, source facts, and calculation status at one stable host read boundary. An authored revision alone is insufficient: calculation and presentation can advance at the same revision.

The publication sequence should be:

1. The host commits an authored operation or installs an engine publication.
2. The worker captures the interested projection from a stable host state.
3. Admission reserves the bounded staging requirement.
4. Fragments populate private staging.
5. Completion validates the complete publication and atomically replaces one `CommittedViewRoot`.
6. Painting, hit-testing, accessibility, and committed-source readers use that root.
7. The consumer acknowledges application.

An editor draft remains a local overlay with its own base-source version. Atomic projection application must not overwrite an IME composition or uncommitted draft.

If a view genuinely requires several streams, extend the existing barrier contract with a **bounded manifest of exact stream barriers from the same host state**. Unchanged members can carry an unchanged-through-state witness. Commit the composite root once all members are satisfied.

Do not synchronize every window’s painting. Different views may lag independently; each must be internally coherent.

### Incremental facts that must travel together

| Change | Required projection behavior |
|---|---|
| Authored operation | Carry the resulting authored revision and invalidate affected projected facts. Do not make the skin replay workbook semantics. |
| Changed values/areas | Send changed projected cells or bounded area replacements, preserving types and provenance. |
| Spill expansion, contraction, movement, removal | Include root/shape facts, new values, and removals in the same view publication. Offscreen cells can remain unloaded. |
| Styles and conditional formats | Publish affected facts even when numeric values and authored revision are unchanged. |
| Geometry and merges | Commit geometry with the cells and hit regions that depend on it. |
| Name definition changes | Send passive definition/version changes; invalidate affected explicit previews without silently evaluating them. |
| Name evaluation | Return a qualified result: identity, definition version, caller/context, result kind, and execution outcome. |
| Calculation progress/failure | Preserve engine scope and freshness; do not derive workbook freshness from tile completion. |
| Interest movement | Advance interest generation and explicitly replace coverage. |
| Dictionary/fragment lifecycle | Definitions precede use; incomplete data stays invisible; reset retires abandoned staging without exposing it. |

Do not reconstruct typed values from formatted display strings. `CellShown.text` is presentation data; value/object-model operations must retain or request the existing typed semantic representation. **[CODE]** `apps/dna-calc-model/src/projection.rs:38–56`.

## 3.4 Specify command, reply, and acknowledgement meanings

**[PROPOSED]** Keep four distinct events:

| Receipt | Meaning |
|---|---|
| Transport/host admission | The request was accepted or queued. |
| Authored commit | The operation and its supported history effect committed. |
| Engine terminal receipt | The specified calculation scope finished, failed, or was cancelled. |
| Projection applied | A particular view root is available to its readers. |

A fast admission acknowledgement must not be presented as calculation completion.

A reply that promises “earlier events are applied” needs explicit projection barriers. Posting events before the reply is useful, but the consumer must enforce the corresponding read guarantee.

Distinguish two reply contracts:

- **At-least-command visibility:** the view has applied a state causally after processing the command. A later reset may satisfy it, even if a subsequent edit or undo has replaced the transient value.
- **Exact read result:** the reply describes one particular host state. A later view root must not absorb that older reply’s cells or source.

A larger revision number alone is not a supersession proof. Use explicit same-incarnation ordering or reset coverage.

The host must never wait for a view’s application acknowledgement while holding the workbook owner, a workbook borrow, or a native execution lock. A slow view cannot block another view’s commands or engine terminal processing.

Preserve ingress-specific Excel behavior. The comment about automation calculation preceding `Change` identifies one particular compatibility observation; it does not justify forcing all UI, VBA, COM, and Office.js operations into identical timing. **[CODE]** `crates/calc-host/src/session.rs:892–894`.

Retries also need an explicit contract. A correlation ID does not by itself prevent a repeated macro or edit. Reuse command identity and a bounded outcome record where retries are supported; otherwise report uncertain completion instead of silently executing again.

## 3.5 Bound the complete publication pipeline

**[PROPOSED]** Keep fragment-local dictionaries and the current format initially. Fix demonstrated costs, then measure.

### Capacity estimates from the supplied code

These are **[INFERENCE] accounting estimates**, not observed process measurements.

- At the existing 1,024-byte accounting charge, **65,536 cells consume 64 MiB before added text**.
- The source comment reports approximately 896 native inline bytes per `Cell`, equivalent to about **56 MiB** for that population before collection and text allocation. That is a repository comment, not a fresh layout measurement.
- **32 × 80 MiB = 2.5 GiB** of publication-scale capacity if local limits can be simultaneously exercised. This does not prove such allocation occurs or that no global guard exists.
- If host subscription cache, worker root, and committed client mirror each own a full 64 MiB charged projection, they account for approximately **192 MiB**. Another full staging copy raises that scenario to **256 MiB**, before text and transport buffers.

The limits and charge are explicit at `apps/dna-calc-model/src/tiles.rs:9–19`.

At 256 records per fragment:

- 65,536 records require at least **256 fragments**, hence **258 logical admission/fragment/completion events**;
- a maximal disjoint removal-plus-installation update requires at least **512 fragments**, hence **514 such events**.

Byte limits can increase these counts. Actual transport-message count depends on the omitted posting implementation.

For a one-cell publication, the logical overhead is already admission, fragment, completion, and application acknowledgement, in addition to command traffic. Multiple calculation publications and subscribers multiply it.

The encoder visibly creates a cell clone, JSON text, a cloned dictionary lookup key, and output bytes. Whole-cell atom deduplication may do little for unique values/formulas even when styles repeat. **[CODE]** `apps/dna-calc-protocol/src/tiles.rs:44–60`.

Do not call the route zero-copy. The supplied transferable-buffer example is for saved packages, and even it first copies Rust bytes into a JavaScript `Uint8Array`. **[CODE]** `apps/dna-calc-worker/src/worker.rs:117–121`.

### Small changes before a new codec

1. Maintain checked retained-byte counters incrementally.
2. Page placements from an ordered index rather than materializing every placement.
3. Share immutable projection chunks where ownership permits.
4. Avoid cloning a whole cell merely to serialize it with a normalized address.
5. Measure whether separate style/presentation atoms materially reduce retained and transmitted bytes.

Only introduce further dictionary complexity if those measurements justify it.

### Flow control must not deadlock

A crucial rule is that **transport credit and semantic application acknowledgement are not necessarily the same thing**.

If the sender has 8 MiB of credit, a publication needs 40 MiB, and credit returns only after atomic completion, transmission stops before completion and neither side can progress.

Choose one initial mechanism:

- reserve enough aggregate capacity for one complete bounded publication before sending it; or
- return transport credit as fragments are received/staged, while separately charging staging memory until commit or discard.

Neither permits partial visibility.

Charge bytes while they remain in worker buffers, browser queues, decoder allocations, staging, committed roots, or render resources. Transfer moves ownership; it does not erase the allocation. Cancellation does not immediately erase already-posted bytes.

For a slow subscriber, retain a bounded combination of the acknowledged root, one in-flight publication, and a latest-state replacement marker. Coalesce visual state into an explicit reset; do not accumulate repeated reset attempts. Preserve command receipts, macro events, undo semantics, and capture evidence independently of visual coalescing.

## 3.6 Grid rendering, editing, and accessibility

**[PROPOSED]** Extend the existing `RenderPlan` rather than create a GPU-specific semantic model.

The current physical `anchor`, overflow `rect`, merge spans, boundary strokes, and pane clipping are useful foundations. **[CODE]** `apps/dna-calc-model/src/render_plan.rs:59–82`.

The reusable plan should contain:

- pane clips and transforms;
- fills, borders, and decorations;
- shaped text runs with glyph positions and font-resource keys;
- semantic target identities and hit regions;
- a separate accessibility projection from the same committed facts.

GPU buffers and atlases are painter caches. Device loss can rebuild them or fall back to Canvas2D without changing workbook semantics.

For geometry:

- Use the same indexed row/column geometry for painting and inverse hit-testing.
- Support sparse variable dimensions without scanning all preceding rows.
- A merge crossing frozen panes has several paint fragments but one semantic cell.
- Overflow text may paint over a blank neighbor without transferring that neighbor’s hit target.
- Interest planning must include relevant offscreen overflow sources and merge anchors.
- Preserve shared formatting decisions; do not let different painters independently reinterpret values.

For editing:

- Use a real DOM/native text input for IME.
- Capture the edit target and source version.
- Do not commit composition updates as formulas.
- Preserve drafts when committed source changes; mark conflicts explicitly.
- Validate view, mount, editor, and focus tickets before delayed effects.
- Keep local text undo separate from authored workbook undo.

For accessibility:

- Derive cell identity, coordinates, totals, headers, spans, selection, editability, and stale/refused/pending descriptions from committed facts.
- Virtualize semantic rows while retaining a stable active-cell identity.
- Do not infer semantics from glyphs.
- Coalesce announcements rather than announcing every cell of a large publication.

DOM and Canvas2D should pass the same interaction cases. WebGPU is a later renderer implementation, not a prerequisite for these contracts.

## 3.7 Notebook and linked boxes

### Notebook

**[PROPOSED]** Extend the current block target to names while preserving block identity separately from placement identity.

A semantic block may appear in several placements. Moving or reordering a placement must not alter dependency order, name identity, relative-reference origin, or evaluation caller.

Reuse `NameRecord`, its definition version, and `EvaluateName { id, version, anchor }`. A name is not guaranteed to have one context-free current value. A preview may be a scalar, reference, array, deferred callable, error, or refusal; do not flatten it to the top-left value.

Use revision-qualified cursors for placements and name pages. Insert/delete/rename during paging should produce a defined update or restart, not mixed-revision pages. Keep 40 as an initial notebook display size, not a claim of bounded preparation.

Runtime name IDs must not be assumed to survive an Excel resave. Persist through the canonical document mechanism and reconcile external edits explicitly. Missing or ambiguous targets become visible orphans.

### Linked boxes

**[PROPOSED]** Put linked boxes in DNA Calc only. OneCalc remains values-only.

A box is a presentation grouping of ordinary names:

- Wiring sets a normal definition such as `MyCell.Input1 = OtherCell.Output`.
- Array previews page an evaluated array; the skin creates no spill allocator.
- Grid consumers such as `=MyCell.Result` use ordinary engine spill behavior.
- Cycles use existing circular/iterative semantics; unsupported cases remain typed failures.
- Direct name-reference definitions can appear as editable wires. General formula dependencies are not automatically removable single wires.
- “Unwire” means restore a prior definition, enter an explicit replacement constant/formula, or delete the name. It must not invent an empty-name value.

Layout, roles, and labels belong in the existing custom-part mechanism. The idea’s “nothing new is stored” should be narrowed: calculation semantics use ordinary names, but custom XML is additional persisted layout content. **[DESIGN]** `docs/architecture/ideas/LINKED_BOXES_SKIN.md:7–18`.

For reverse import, use an explicit convention such as splitting at the final dot, with scope kept separate. Dots do not create a new calculation namespace. Preserve original names and spelling; grouping must be passive. Do not infer input/output roles reliably from whether a definition happens to be a literal.

### Formula-name presenter/interpreter

**[PROPOSED]** Use the existing parser and binder with lossless source spans. Syntax-tree rewriting alone is insufficient.

The presentation result should carry canonical source, displayed text, source/display span mappings, referenced identities, and the relevant binding environment.

The central contract is:

> An unchanged draft reproduces the exact canonical source. An edited draft preserves untouched syntax and the symbolic meaning of surviving references. Inserted aliases expand through a validated binding environment.

Shorten a name occurrence only when the displayed spelling resolves to the intended identity **at that lexical position**, without changing relative-reference semantics.

Conservative initial rules:

| Situation | Behavior |
|---|---|
| Unambiguous `MyCell.Input1` | May display as `Input1`. |
| `LET(Input1,7,MyCell.Input1+Input1)` | Keep the external occurrence qualified; shortening would capture the local. |
| Existing unqualified workbook/sheet name `Input1` | Preserve ordinary Excel resolution; do not let the presentation alias silently override it. |
| Cell-looking suffix such as `A1` | Do not reinterpret a cell token as a name alias. |
| `"MyCell.Input1"` or `INDIRECT("Input1")` | Strings retain ordinary string semantics. |
| Function-call position | Use the normal binder; distinguish built-ins from callable defined names without evaluating them. |
| Sheet-qualified name | Remove qualification only when symbolic meaning is preserved. |
| Relative name body | Preserve authoring origin and relative offsets; a preview caller must not rebase the definition. |

An edit session must distinguish **definition-authoring context** from **evaluation caller context**. Matching a `NameId` at one preview anchor is not enough if the rewritten source changes meaning at another caller.

Completion returns an identity and canonical insertion recipe, not merely a label. Revalidate it after concurrent renames or callable-registration changes.

Diagnostics need display spans plus canonical source locations. `EvaluatePart` already uses UTF-16 offsets. Translate selections through the source map, preserving the full formula’s lexical context. Selections whose endpoints fall inside an expanded shorthand or surrogate pair need an explicit enclosing-expression choice or a diagnostic—not guessed offsets. **[CODE]** `apps/dna-calc-model/src/projection.rs:833–839`.

After expansion, reparse and rebind the whole formula to catch token-boundary issues around qualification, whitespace intersection, calls, `@`, and `#`.

Rename is a host operation on canonical identities. A box-label rename is a layout operation. Neither is string replacement throughout formulas.

## 3.8 Multiple views and execution boundaries

**[PROPOSED]** Make today’s switch-one-skin route a one-view instance of the new view map. Preserve its existing draft-transfer behavior before adding simultaneous views.

Each view owns selection, interest, scroll/page state, focus, and editor draft. Supported authored UI commands share host history. Ingress-specific undo availability or history clearing remains a compatibility question; do not make all automation effects undoable merely for uniformity.

Selection-sensitive macro facts require ordered host updates. `ViewId`, formula caller, active window, and active cell are distinct concepts. Capture the required command context at submission and revalidate it at execution.

For web:

- Multiple skins in one page share the existing dedicated worker.
- Cross-window sharing requires one designated owner reached through a message bridge or shared service.
- Do not create a separate workbook worker per window and call the result one session.

For desktop:

- WebView2 windows forward to the same native workbook owner.
- VBA execution and callbacks stay on their owning thread.
- COM/XLL capabilities remain native-only.
- Native calls may block their owner; a responsive shell does not make them interruptible.

Apply Off/Safe/Unguarded project execution policy using CalcVBA Safe/Full capabilities as required, while keeping worksheet-function restrictions, native-code trust, and process isolation separate. A capability policy is not an OS sandbox. Browser capability absence must produce explicit behavior, not an alternate guessed result.

### Interaction with the other four reviews

| Shared contract | Single owner | Required contribution across reviews |
|---|---|---|
| Authored state, identities, layout persistence, save policy | **Engine-host-doc:** `calc-doc-model` / `calc-doc` | Object model and skins reference the same identities. This review requires explicit preservation/refusal behavior for custom parts and name transformations. |
| Command admission, execution context, ingress sequencing, supported undo | **Engine-host-doc:** `calc-host` | Object-model/JS defines operation mapping and batch semantics. Concurrency defines safe reentrancy. This review consumes receipts and targets without re-executing semantics. |
| Formula binding, names, reference context | **Object model/JS coordination**, implemented in existing syntax/binding layer | Supply binding identities, symbolic reference information, spans, and completion hooks. This review adds presentation aliases, not another binder. |
| Graph, spills, dirty/fresh state, scoped publications | **Graph/recalc:** `calc-engine` | Supply complete installed publication facts and honest scope. This review must preserve them through projection and coalescing. |
| Projection continuity, coherent roots, reply visibility, view lifecycle | **Host-to-skin IR:** model/protocol contract | Worker and adapters implement the same contract; all view readers share it. |
| Owner thread, callbacks, cancellation, native isolation | **Concurrency/isolation:** execution boundary | Define what can yield or cancel and how callbacks reenter. This review keeps transport acknowledgements out of execution locks. |

# 4. Staged implementation plan with measurable acceptance

All stages below are **[PROPOSED]**. Correctness gates are mandatory; performance budgets are initial targets to measure and revise openly.

## Stage A — Establish the current route

**Changes:** Trace representative commands through model, transport, server, host, publication, mirror, and readers. Record existing identities and receipt meanings. Add only missing observability counters and route assertions.

**Ownership/dependencies:** Sonnet can trace and instrument. Opus decides any conflicting meaning of command completion, source context, or publication scope.

**Retained behavior:** Existing commands, single-skin switching, manual-mode stale values, refusals, and current file-preservation restrictions remain.

**Acceptance:** A browser-worker edit, desktop edit, undo, explicit name evaluation, and save are each explained by one authoritative route. No second evaluator or verdict path appears.

**Measurements and stop:** Count command executions, publications, callbacks, bytes, and root installations. Stop when the route is demonstrable; do not build a new tracing framework. Reuse the current capture lineage and corpus comparison.

## Stage B — Close lifecycle and atomic-read guarantees

**Changes:** Add missing identity guards, qualify failure messages, establish one committed view root, and make reply barriers explicit. Use one union-interest stream per view initially.

**Ownership/dependencies:** Opus settles identity and supersession semantics; Sonnet implements the router, barrier handling, and consumer changes. Depends on A.

**Retained behavior:** Keep the existing tile format and current single-view product. Preserve drafts across supported skin switches.

**Acceptance:** Delayed old-workbook messages cannot invalidate new state. Multi-fragment spill expansion/contraction is invisible until completion. Rendering, hit-testing, committed source, and accessibility observe the same root.

**Measurements and stop:** Zero partial-root observations and zero obsolete focus/editor effects across deterministic delayed-delivery cases. Stop once the existing route satisfies these cases.

## Stage C — Bound work and memory

**Changes:** Incremental retained-byte accounting; indexed placement paging; aggregate projection credits; finite slow-subscriber recovery; bounded encoding/posting work.

**Ownership/dependencies:** Sonnet can implement the demonstrated accounting and paging fixes. Opus decides credit and cancellation boundaries. Depends on B for reliable accounting acknowledgements.

**Retained behavior:** Preserve explicit removals, provenance, and fragment-local dictionaries. Oversized requests receive typed narrowing/refusal behavior.

**Acceptance:** A stalled consumer cannot grow memory indefinitely or stall a healthy view. A publication larger than the transport window cannot deadlock. Repeated cancel/recreate cannot evade accounting.

**Measurements and stop:** Account for every retained store, visited entry, queued byte, and owner-task duration. Stop when memory plateaus under the declared limits and small changes no longer require full-root accounting scans.

## Stage D — Deliver name-backed notebook editing

**Changes:** Extend canonical block targets and projections; add revision-qualified paging; reuse passive name inspection and explicit evaluation; implement the shared presenter/interpreter.

**Ownership/dependencies:** Opus decides relative-reference and alias-context contracts using the experiments below. Sonnet implements projection, UI, source maps, and the binding matrix. Depends on B/C.

**Retained behavior:** Cell-backed and prose blocks continue to work. No hidden worksheet cells and no evaluation on mount, scroll, or name listing.

**Acceptance:** A name-only workbook supports placement, source navigation, editing, qualified evaluation, rename handling, and save/reopen. The alias matrix preserves bindings and unchanged source exactly.

**Measurements and stop:** Page-construction work, name-evaluation call counts, cursor consistency, and source-map errors. Stop at this complete product route before adding boxes.

## Stage E — Add simultaneous views

**Changes:** Instantiate view state by `ViewId`; route selection, focus, editor, and interest ownership independently; retain one host and history.

**Ownership/dependencies:** Opus decides active-window/macro context; Sonnet implements view instances and lifecycle. Depends on B and the relevant Excel ingress experiments.

**Retained behavior:** Single-view switching remains supported. Unsupported cross-window transport is explicit until it reaches the same owner.

**Acceptance:** Grid and notebook show one edit and one undo without reload, retain independent selections, and survive one view closing with pending replies. One blocked projection does not stall the other.

**Measurements and stop:** Commands execute once; subscriptions and retained bytes return to baseline after closure; stale continuations have no UI effect.

## Stage F — Add linked boxes; defer renderer replacement

**Changes:** Add grouping/layout over ordinary names, direct-reference wiring, explicit unwiring, bounded previews, and custom-part persistence.

**Ownership/dependencies:** Sonnet can build the skin after Opus resolves name/source/save semantics. Depends on D and the file-roundtrip experiments.

**Retained behavior:** All edits use existing name commands and graph behavior. Unsupported cycles or preservation transformations remain typed failures.

**Acceptance:** Create → wire → edit → undo → save → Excel calculate/save → reopen in DNA Calc, with formula, value, and layout outcomes compared separately.

**Measurements and stop:** No extra evaluator, graph, or command history. Stop when that route works. Begin WebGPU only if renderer measurements identify a bottleneck that the shared plan can address.

## Proposed performance budgets

Use recorded hardware, browser/WebView2 versions, build profiles, and cold/warm distinctions. A representative baseline should include 10,000 formulas, a 4,096-cell visible union, 100 simple dependents per edit, a 65,536-element spill, and a separate 100,000-name/placement workbook.

| Workload | Initial proposed target |
|---|---|
| Uncontended edit admission acknowledgement | p95 **≤30 ms**. This is not a promise of calculation completion. |
| Simple edit → first changed visible value | p95 **≤60 ms**. |
| Simple edit → current visible union | p95 **≤150 ms**. |
| Cached scroll → paint | p95 **≤25 ms**, with planner/painter work **≤8 ms** per frame. |
| Newly requested viewport | First coherent visible result p95 **≤150 ms**, with explicit loading until then. |
| Cooperative calculation cancellation | Observed at a safe checkpoint within **50 ms**; target owner slices around **4 ms** where operations permit yielding. |
| Slow foreign call | Report queued/cancel-requested state honestly; no finite interruption guarantee for a non-cooperative call. |
| 100,000 names/placements | Warm indexed first-page retrieval p95 **≤50 ms**; no name evaluation; bounded client page retention. Arbitrary search scans must be cancellable and separately measured. |
| Slow subscriber, 10 edits/s for 10 s | Retained memory reaches a fixed plateau; recovery uses one coherent reset when progress resumes. |
| Projection memory | Initial aggregate accounting targets: **128 MiB wasm**, **256 MiB native**, excluding canonical engine/document state but including projection caches, staging, queued data, and render resources. Narrow interests before exceeding them. |

These limits must not obscure total process memory. Report canonical workbook memory separately, and report total memory too. A large spill in the engine does not require projecting its entire extent into a small viewport.

For compatibility gates, compare the same captured inputs and verdict population before/after each relevant stage. Refused, missing, and unread cases remain failures. Do not substitute saved-cache agreement for live Excel evidence.

# 5. Discriminating experiments against Excel

All experiments below are **[PROPOSED]**. Every predicted outcome is a **hypothesis**, not a measurement.

## Common capture procedure

For each branch, restore the same saved baseline and record:

- Excel build/channel, OS, bitness, locale, file format, calculation mode, iteration settings, and MTC settings;
- exact entry order and commands;
- formula/name source text, types, errors, raw numeric bits, and array shapes;
- callback/event order, call counts, and observable thread identities;
- command return time, `CalculationState`, and save/reopen observations.

Distinguish document-modified state from calculation dirty state. Where per-cell dirtiness is not directly observable, use a controlled subsequent calculation and callback counts; do not fabricate a dirty flag.

Use the existing capture and verdict pipeline. API snippets below are runner outlines whose availability must be recorded for the tested build.

## E1 — Passive name inspection versus explicit evaluation

**Setup and caches:** Set A1 to 10. Create `Box.Input = Sheet1!$A$1`, `Box.Result = Probe("name",Box.Input)`, and B1 `=Box.Result`. `Probe` logs to an in-memory buffer and returns twice its argument; it does not write cells. Calculate once, record B1 and call counts, switch to manual mode, set A1 to 20, and clear the log.

From separate restored baselines:

1. Enumerate name spelling and `RefersTo` only.
2. Open and close Name Manager without edits.
3. Explicitly evaluate `Box.Result` through worksheet and application evaluation APIs.
4. Read B1 without issuing calculation.

**Hypotheses and expected observations:**

- **H1:** Metadata inspection is passive: branch 1 logs no calls and preserves B1’s old cache.
- **H2:** Metadata inspection evaluates: branch 1 logs calls.
- **H3:** Name Manager performs preview evaluation although metadata enumeration is passive: only branch 2 adds those calls.
- **H4:** Explicit evaluation is detached: branch 3 returns 40 while B1 retains its earlier 20.
- **H5:** Explicit evaluation also installs ordinary calculation results: B1 and associated events/publications advance.

**Design-changing result:** H5 requires evaluation receipts to describe installed effects. H3 requires an explicit active-preview operation if matching that UI behavior; it does not justify hidden evaluation in every names page.

**Controls:** Repeat with ordinary arithmetic definitions, and record macro-disabled outcomes separately.

## E2 — Relative names, authoring origin, and window context

**Setup and entry order:** On Sheet1 set A2=11 and A3=23. Select B2, then create `Rel` using a relative R1C1 definition equivalent to `=Sheet1!RC[-1]`. Enter B2 `=Rel`, then B3 `=Rel`; calculate and save. Open two windows of the same workbook, selecting B2 in one and B3 in the other.

In manual mode, evaluate `Rel` after activating each window. Separately inspect/edit the name from a third cell and save/reopen. Capture `RefersTo` and `RefersToR1C1` at each point.

**Hypotheses and expected observations:**

- **H1:** Formula caller coordinates govern relative references: B2/B3 yield 11/23.
- **H2:** Definition-origin coordinates remain fixed: both yield 11.
- **H3:** Application evaluation uses active-window context while worksheet formulas use caller context: switching windows changes explicit evaluation but not the established formula meanings.
- **H4:** Merely editing/reassigning an unchanged displayed definition from another active cell rebases it: source or downstream results change.

**Design-changing result:** H4 makes lossless no-edit roundtrip and authoring-origin preservation essential. H3 requires distinct caller and active-window fields rather than one generic anchor.

**Controls:** Repeat with absolute references, a single window, and a second workbook. Record active window, sheet, cell, and selection independently of result values.

## E3 — Dotted names, lexical binding, and rename

**Setup:** Create workbook names `Box.Input=11` and `Input=19`, plus Sheet2-local `Box.Input=29`. Enter and calculate on both sheets:

```text
=Box.Input
=LET(Input,100,Input+Box.Input)
=LAMBDA(Input,Input+Box.Input)(100)
=Box.Input+SUM(1,2)
="Box.Input"
```

Save the fully calculated baseline. Rename `Box.Input` through Excel’s name-edit operation. In another branch, delete the old name and add the replacement. Save/reopen both branches.

**Hypotheses and expected observations:**

- **H1:** Lexical locals bind normally: LET/LAMBDA results are 111 on Sheet1 and 129 on Sheet2.
- **H2:** The workbook `Input` wins instead: those results are 30 and 48.
- **H3:** Direct rename rewrites dependent bound references: formulas change spelling while preserving results.
- **H4:** Rename leaves source references unchanged: subsequent evaluation resolves differently or fails.
- Delete/add may have different effects from direct rename; record those independently.

Add quoted sheet names, callable defined names, cell-looking suffixes, and string-based references as separate minimal cases.

**Design-changing result:** Determines canonical rename behavior and which aliases are safe. The string result must be observed separately from bound references; a layout label is not a rename command.

**Controls:** Record `Formula` and `Formula2` separately where supported, and retain original spelling/case and scope.

## E4 — Scoped calculation, spill changes, and save caches

**Setup:** D1=2; A1 `=SEQUENCE(D1,2)`; F1 `=SUM(A1#)`; F2 `=ROWS(A1#)`. Fully calculate the 2×2 spill and record F1=10, F2=2. Save. Switch to manual mode and set D1=3.

Run separate baseline-derived branches:

- calculate A1’s range;
- calculate the worksheet;
- calculate the application;
- save with calculate-before-save disabled and reopen under verified manual-mode controls.

**Hypotheses and expected observations:**

- **H1:** Range calculation updates only the requested spill: A1 becomes 3×2 while F1/F2 retain 10/2.
- **H2:** It also updates dependent consumers: F1/F2 become 21/3.
- **H3:** Save/reopen preserves mixed fresh/stale caches.
- **H4:** Save or reopen performs additional calculation despite the attempted controls; logs and values identify it.

Repeat from the baseline with D1=1, then with a blocker at B3 before expanding to three rows. Capture old spill removals, root error/type, and dependent caches.

**Design-changing result:** Establishes scope and save policy. It must not cause a completed spill projection to promote unrelated dependent caches to current.

**Controls:** Dynamic-array availability is build-sensitive. Callback observations establish observable Excel states, not Excel’s internal publication mechanism.

## E5 — Event ordering by command ingress

**Setup:** A1=1; B1 `=Probe("B",A1)`, returning twice its input. Fully calculate B1=2. Log UDF entry/exit, `Worksheet_Change`, `Worksheet_Calculate`, command return, and available thread identities.

Change A1 to 2 using keyboard commit, VBA/COM assignment, and Office.js. Run each in automatic and manual modes.

**Hypotheses and expected observations:**

- **H1:** Calculation precedes `Change`: a later instrumented `Change` callback reads B1=4, and the producing UDF call precedes it.
- **H2:** `Change` precedes calculation: it reads the old 2, followed by the UDF call.
- **H3:** Ordering depends on ingress.
- Manual mode may produce `Change` without a new UDF call until explicit calculation.

**Design-changing result:** Requires ingress-specific sequencing over the shared transaction/evaluator. Worker event-posting order cannot replace this semantic contract.

**Controls:** First log event identities without reading B1; then add the read. A changed trace exposes instrumentation-induced evaluation. Avoid `DoEvents` in the primary probe.

## E6 — Office.js synchronization and partial failure

This experiment requires Excel’s JavaScript runtime. It tests the **observable Office.js API**, not internal scheduler structure.

**Freshness baseline:** A1=1, B1 `=2*A1`, calculated B1=2.

```javascript
await Excel.run(async context => {
  const sheet = context.workbook.worksheets.getItem("Sheet1");
  sheet.getRange("A1").values = [[2]];
  const dependent = sheet.getRange("B1");
  dependent.load("values");
  await context.sync();
  record(dependent.values);
});
```

Run in automatic and manual modes. Separately request explicit calculation and load again.

- **H1:** Automatic `sync()` includes the required calculation: loaded B1=4.
- **H2:** It acknowledges the write without guaranteeing that calculation: loaded B1=2, with later observation/calculation yielding 4.

**Failure baseline:** Protect a sheet with A1 unlocked and B1 locked. Queue a valid A1 write, then an invalid B1 write, and call `sync()`. Reverse operation order in another restored run. Inspect through a fresh request after the error.

- **H3:** Batch rollback: A1 remains unchanged.
- **H4:** Partial execution: A1 changes if its operation executed before failure.
- If later operations run despite an earlier error, record that as a distinct observed ordering contract.

**Design-changing result:** JS batching must not silently inherit atomic UI transaction semantics. Replies may need per-operation outcomes and ingress-specific visibility guarantees.

**Controls:** Capture error details, events, supported undo behavior, and save/reopen state. Record requirement-set availability.

## E7 — Name arrays and named cycles

**Caches and entry order:** Initially define `Box.A=0`, `Box.B=0`, and C1 `=Box.A`; calculate and save. In manual mode, replace A’s definition with `=Box.B+1`, then B’s with `=Box.A/2`. Prepare an equivalent cell cycle separately from zero caches.

Calculate with iteration off, then in independent runs with iteration enabled, 100 iterations, and a recorded tolerance. Compare explicit name evaluation, scoped calculation, and workbook calculation.

**Hypotheses and expected observations:**

- **H1:** Named cycles participate in iteration: consumers approach the recorded fixed point near A=2, B=1, with order-dependent intermediate bits captured.
- **H2:** Recursive name evaluation is rejected or held even when cell iteration works: capture the exact error or held-cache outcome.
- Different commands may expose different scopes; record rather than normalize them.

Separately define `Box.Array=SEQUENCE(2,2)` and compare explicit evaluation with a cell formula `=Box.Array`.

- **H3:** Explicit evaluation returns array shape/values without worksheet spill ownership; the consuming cell creates the spill.
- **H4:** Evaluation has additional installation effects.

**Design-changing result:** Determines whether linked-box cycles and arrays are supported through existing engine behavior or remain typed refusals. It never justifies a skin-specific cycle solver.

## E8 — Custom XML and external name edits

**Setup:** Add ordinary dotted names, a cell consumer, and a tiny namespaced layout part. Calculate and save. Test `.xlsx`, `.xlsm`, and `.xlsb` as separate format branches.

Open in Excel, edit a value, save, reopen in DNA Calc. Separately rename a referenced name in Excel and repeat.

**Hypotheses and expected observations:**

- **H1:** Layout part and relationships survive.
- **H2:** Package bytes are normalized but parsed layout content survives.
- **H3:** Layout is dropped.
- **H4:** Name/formula references update but opaque layout references do not.

**Design-changing result:** H3 requires explicit layout-loss behavior; H4 requires reconciliation/orphan diagnostics. Neither permits silently attaching a placement to a similarly named object.

**Controls:** Compare package structure, parsed custom content, name scope/source, formula source, and calculated values separately. Macro-enabled and macro-free controls must remain distinct.

## E9 — Slow foreign calls and cancellation

**Setup:** Use a native XLL `Slow(ms,x)` that logs entry, exit, monotonic time, and thread ID to an external buffer, then returns `x`. Establish fully calculated zero-delay caches. In manual mode, change the delay to two seconds and start calculation. After 100 ms, an independent UI driver attempts Escape and an edit.

**Hypotheses and expected observations:**

- **H1:** Cancellation is noticed only after native return.
- **H2:** The function returns, but its result is discarded after cancellation; the old cache remains qualified as old.
- **H3:** Its result installs before cancellation takes effect, while downstream work remains incomplete.

Record when the edit is accepted and when it actually commits.

**Design-changing result:** Defines cancellation receipts and stale-result handling. It does not authorize terminating an in-process native call or moving VBA callbacks to arbitrary threads.

**Controls:** Separate supported thread-safe and owner-thread registrations, MTC settings, and an ordinary fast-call baseline.

## DNA Calc fault-injection experiments

These are product/protocol tests, not claims that Excel has equivalent skins or messages.

| Case | Injection | Required outcome |
|---|---|---|
| Multi-tile spill | Delay final fragment/completion during expansion, contraction, displacement, and deletion | Every reader sees the old root or complete new root; never a partial spill. |
| Slow subscriber | Withhold acknowledgements while another view continues | Memory plateaus; healthy view progresses; stalled view gets finite reset/revocation behavior. |
| Credit-window deadlock | Publication exceeds transport window | Completion remains possible under the declared credit mechanism. |
| Rapid switching/replacement | Delay source, completion, paste, focus, failure, and tile messages | No obsolete target changes, focus theft, duplicate commit, or invalidation of the replacement workbook. |
| Reset during staging | Send a newer reset, then late old fragments/completion/acks | Exactly one coherent new root; no double-returned credits. |
| Malformed wire | Bad lengths, indices, overflow, duplicate/conflicting fragments, truncation, missing completion | Bounded rejection; no partial installation; recoverable old root. |
| Malformed authored batch | Invalid target or operation within a declared atomic UI transaction | Validate according to that transaction’s contract before mutation; do not assume Office.js uses the same contract. |
| Linked-name parsing | Locals, scopes, strings, Unicode, callable names, relative references, concurrent rename | Exact unchanged-source roundtrip; binding-preserving edited source; correctly mapped diagnostics. |
| Coalesced edit then undo | Both occur before a slow view applies either | Preserve both command outcomes; later root may satisfy causal visibility without pretending the transient value was painted. |

# 6. What to stop or delete

1. **Stop reopening the already-addressed architectural question of whether a neutral model or transaction-framed tile route should exist.** They exist. Finish the current route and retire older claims that ignore it. Keep the earlier reviews as historical evidence and test motivation.

2. **Delete the demonstrated full-root byte recount and pre-page layout materialization.** They are concrete costs with small, measurable replacements. Do not replace them with a general incremental-computation framework.

3. **Stop treating “events posted before reply” as the complete visibility contract.** Replace that assumption with explicit, tested barriers and lifecycle qualification.

4. **Remove redundant skin command paths as they are proved redundant.** Preserve browser-specific focus, clipboard, IME, and authorized effects in adapters. The packet does not identify every surviving redundant call site, so deletion should follow route verification rather than assumption.

5. **Do not add a notebook or linked-box evaluator, graph, hidden worksheet, or name cache with invented freshness.** Reuse name definitions, explicit evaluation, and the existing graph. OneCalc remains values-only.

6. **Delete prefix substitution as an implementation strategy for formula presentation.** Even parsed-token substitution is inadequate without binding identity, source maps, and relative-reference context.

7. **Stop treating every dependency edge as a wire that can be deleted independently.** A general formula is authored source. Wire gestures must have defined name-edit semantics.

8. **Do not replace the binary format before measuring it.** First fix accounting, paging, avoidable clones, and ownership. Keep fragment-local dictionary safety until a measured benefit justifies more lifetime complexity.

9. **Defer WebGPU until the shared plan and interaction route are correct and a renderer bottleneck is measured.** A display list is useful infrastructure, not evidence of a GPU implementation.

10. **Do not weaken existing preservation refusals to make a new skin appear complete.** The tracker explicitly reports refusals for transformations involving preserved chart/dialog references. Keep those failures visible until the responsible document transformation is implemented and tested. **[DESIGN]** `.beads/issues.jsonl:455`.

11. **Stop describing Safe capability policy as isolation or cancellation as interruption.** Keep project grants, worksheet-function restrictions, native trust, owner-thread execution, and process isolation as separate contracts.

12. **Preserve the compatibility evidence.** Raw Excel captures, source text, types, numeric bits, event order, scoped freshness, refusals, and saved-cache provenance should survive every migration. A cleaner UI boundary earns its place by carrying those semantics faithfully.
