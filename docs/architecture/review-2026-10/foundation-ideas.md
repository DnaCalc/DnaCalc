# Foundation ideas worth reconsidering: the Feb-May 2026 maximum-ambition plans

Date: 2026-10-31.

**Source.** `C:\Work\DnaCalc\Foundation`, read-only. The repository has 79 commits. The first is `fcc6d4b` (2026-02-22); the October plan bundle is `c1a1a00`.

**Comparison points.** `parked/arch-review/risk-engine.md` (code at 5a9348b) and sections 3 and 6 of `DnaCalc-review/docs/plan/DNA_CALC_REIMPLEMENTATION_PLAN.md`.

This review does not argue for a return to February. It recovers mechanisms that were specified, then deferred or lost, and that bear on today's targets: Webs-class scale, visible-first recalculation, explicit layer contexts, skins, early-bound VBA and bit-exact functions.

**Status tags.** **KEPT**: in plan and code. **PLAN-ONLY**: in the plan, not the code. **DEFERRED**: deferred in February, with a trigger for revisiting. **LOST**: in neither. **REJECTED**: the history gives a reason.

**Abbreviations**

| Short | Path | Commit |
|---|---|---|
| AR | `ARCHITECTURE_AND_REQUIREMENTS.md` | |
| BN | `notes/BRAINSTORM_NOTES.md` | |
| CEFM | `CORE_ENGINE_FORMAL_MODEL.md` | |
| FMI | `notes/archive/formal-model/FORMAL_MODELS_IDEAS.md` | `d91e01b`, 02-25 |
| DAG | `research/runs/20260305-201430-dag-computation-theory-deep-research-pass-01/outputs/` (07 is Claude's report, 09 is the reconciliation) | `c0f97b7`, 03-06 |
| JS | `research/runs/20260222-123425-run4-janestreet-oxcaml-incremental-internal/outputs/response.md` | 02-22 |
| FEC | `prompts/runs/20260308-171858-core-engine-fec-f3e-deep-research-pack-01/responses/claude_opus_response.md` | `4ceda07` |
| DEC | `synthesis/runs/20260307-223304-core-engine-dag-fec-synthesis-pass-01/outputs/03_stable_topic_entries.md` | `4ceda07` |
| PPnn | `prompts/runs/20260222-011351-prompt-pack/responses/nn_*.md` | `ce858d6` |
| OCT | `plans/calc-reimplementation-2026-10/calc-reimplementation-planning-gpt6pro.md` | `c1a1a00`, 10-13 |

## 0. The ambition at its peak, and when and why it was cut back

**Peak, 2026-02-22 to 03-09.** The mission was "a near-formal, state-of-the-art spreadsheet platform" (CHARTER, `fcc6d4b`). It rested on:
- two engines, Rust (Red) and .NET (Blue), behind an identical protocol;
- a "Green" stack of Lean, TLA+, an OCaml oracle and obligation packs;
- four rounds: VisiCalc, PreCalc, SuperCalc, DnaCalc;
- a universal OpLog with MVCC epochs, and versioned profiles;
- collaboration through OpLog replication, with full RTD and XLL support.

The 2026-02-22 synthesis pass 02 (`cb5fe27`) added "Alien Artifact leverage" and "Design for Evolution".

The recalc model peaked in the "Option B" staged plan (AR §3.19 and CEFM §6.8, `4ceda07`/`4b7c233`, 2026-03-09):
- **Stage 1**: a sequential coordinator.
- **Stage 2**: partitioned parallel evaluators.
- **Stage 3**: "dynamic-topology/SAC-inspired lanes and stream-heavy advanced lanes" (SAC is self-adjusting computation).

FEC's Phase 3 went as far as "Distributed evaluation: Partition large workbooks across compute nodes using the MVCC snapshot model".

**Cutbacks, in order**
- **02-23, independent review** (`notes/archive/independent-review-20260223/INDEPENDENT_REVIEW.md`, `b9731f5`):
  - criticised "The Dual Engine Trap", the language proliferation and the scope of verification;
  - concluded "The Process Is Producing Documents, Not Software".
- **02-27, `f50611d`**: Round 0 was bound to the DnaVisiCalc v0 scope (single sheet, A1..BK254). Multi-sheet support and full coercion became non-goals. Missing formal artifacts were named as "the principal Round 0 exit blocker" (AR §6.3).
- **03-09, `3583a6d`**: spec ownership moved to OxFml and OxCalc. Stage 3 stayed, but only behind "parity/equivalence evidence".
- **05-20, `b7dccb7`**: ambition actually rose. "High-throughput full-model recalculation is a core long-term architecture goal", with optimiser metadata and value-only versus trace-rich modes.
- **July and August**: OxCalc realised part of the model. The 08-30 investigation (`notes/PROGRAM_INVESTIGATION_2026-08-30.md` §6.3) found:
  - a per-cell `value_epoch`;
  - an optimised grid lane plus an oracle lane;
  - `WorkbookRecalcTick` "for coherent NOW()/RAND".

  It also found that Foundation still described "two independent engines", "OpLog", and "Lean + TLA+ + OCaml", none of which existed.
- **10-05, OCT**: "About ten months of work produced strong pieces and a tangled whole." The plan made these cuts:
  - "drops the invented 'tree' reference language and the dual-profile model. There is only Excel";
  - no Lean and no formal methods;
  - "Single-threaded recalc for now, as the reference implementation for a future optimised (incremental/parallel) version".

**Why it was cut (OCT §6, "Lessons")**
- "Conservative full rebuild is always legal" became the default path. February's Stage 1 had explicitly allowed a "conservative fallback".
- "No seams without traffic": interfaces that nothing called "rotted and misled audits".
- "One of everything": two engine lanes meant "every feature landed in only one".
- The measured baseline: "a warm F9 that evaluates 2 cells: 53 s".

These are good reasons to cut the ceremony and the duplicate lanes. They are not reasons against the mechanisms below. The constraint they leave: re-adopt a mechanism only with a real caller, inside the one scheduler, and with every fallback counted.

## 1. The envisioned recalc model

**R1. Per-value epochs; staleness allowed but visible.** LOST in its per-value form.
- **Was**:
  - `committed_epoch`, `stabilized_epoch` and a per-value `value_epoch`.
  - "Stale values are allowed but **must be detectable**".
  - "clients can pin an epoch for consistent reads while newer epochs progress" (AR §3.3, `fcc6d4b`).
  - PP05 adds that `E` "may remain partially pending while new work starts".
- **Now**: OxCalc had a per-cell `value_epoch`; the October rewrite dropped it. Today there is one generation, published only when it finishes (risk-engine §2). `Calculation` per result and `StaleCause` are what is left.
- **Matters**: incremental publication, cancellation and distribution all need versions per value. The independent review called the epoch model "the crown jewel of the architecture". Nothing rejected it.

**R2. Atomic commit per node, not per generation.** LOST.
- **Was**: "`Applied` commits publish exactly one atomic `DerivedBundle` per committed node" (CEFM §5.3; CONSTR-018). The node's value, its read edges and its spill shape travel together.
- **Now**: plan 3.3 makes the generation the unit of commit. Risk-engine §2(b) names that as the blocker.
- **Matters**: this is the missing half of visible-first. It keeps "no new values with old edges" and still allows partial publication.

**R3. Visibility is scheduling metadata, never semantics.** DEFERRED, then LOST.
- **Was** (DEC-CALC-008; FEC VIS-RULE-001..003):
  - `VisibilityState {visible_regions, visible_nodes, visibility_version, priority_policy}`.
  - The scheduling key `(is_visible, topological_height, node_id)`, with a starvation bound `MAX_VISIBILITY_DEFER`.
  - "Same operations + same visibility-event stream => same publication stream".
  - Final values are unchanged across policies (CONSTR-021, CONSTR-023).
- **Now**: `Interest` is used only for delivery (`session.rs:56-68`). Plan 8.1 even states that state separation "prevents a viewport move from triggering recalc".
- **Matters**: this is the owner's visible-first requirement, already specified. The non-obvious part is to record viewport moves as a replayable input stream, so runs stay deterministic.

**R4. An observer-driven "necessary" set (the Incremental/Anchors hybrid).** DEFERRED; the reason for deferring has lapsed.
- **Was** (DAG/07 lines 183-193 and 272):
  - node states clean, dirty and necessary;
  - "observer markers on output cells (visible viewport, chart data sources, external API consumers)";
  - recalculation from a height-ordered min-heap.
- **Why deferred**: DAG/09 kept Topo+SCC and reserved "hybrid internals for targeted next-step prototype lanes" because proofs were immature.
- **Now**: the dirty closure is ordered up front in `begin_recalc`.
- **Matters**: this is the route to Webs-class "work proportional to change". Excel's Automatic mode still computes everything, so "necessary" orders the work. It may drop work only in a declared mode (Manual, or a Webs profile).

**R5. Early cutoff with verifying traces.** DEFERRED, then LOST.
- **Was**:
  - "Early cutoff is the single most impactful optimization missing from Excel's architecture" (DAG/07 l.13).
  - Compare "the hashes of its actual inputs" (l.276), described as "Salsa-style backdating" (FEC).
  - Proof obligation DAG-PO-008.
- **Now**: none. The closure is fixed before evaluation starts.
- **Matters**: skipped evaluations are observable only through impure calls (UDF call counts, VBA, volatile functions), so cutoff needs per-function purity facts (O1).

**R6. Incremental topological order with a resumable stabiliser.** DEFERRED.
- **Was**:
  - Pearce-Kelly ordering, "O(δ·log δ)" per edge change (DAG/07 l.104).
  - Incremental's two heaps and `do_one_step_of_stabilize` (JS §4.2).
  - Promotion was tied to `PACK.dag.dynamic_topo_vs_rebuild`.
- **Now**: `RecalcWork` is kept, but the order is rebuilt for each closure, and the readers cache drops on any edge change.
- **Matters**: an up-front ordering costs O(closure) before the first visible cell can be computed.

**R7. Dynamic references as an overlay on the structural graph.** KEPT. "structural deps union runtime-observed deps" (CORE_ENGINE_THEORY §3.4); DAG-PO-006: same reads, same result; INDIRECT as observed reads, not volatile. Implemented per plan 6.2.

**R8. Three invalidation classes; external updates are operations.** KEPT in substance: `Standard`, `Volatile`, `ExternallyInvalidated` (CONSTR-016) and `OpExternalUpdate(topic_id, topic_seq)`. Still open is PP05's "Async completion is reified as an explicit external-update op that creates a later epoch", which fits async UDFs, off-worker OxVba calls and remote partitions.

**R9. Format and environment tokens.** LOST.
- **Was**: `FormatDepToken` (DEC-CALC-007). "Format changes that do not affect any format dependency token do not trigger recalculation" (FEC FORMAT-RULE-002).
- **Now**: setting the clock makes every formula pending (risk-layering).
- **Matters**: this is the invalidation half of principle 4.1, with one token per context facility.

**R10. Stable row and column identity; addresses are derived.** PLAN-ONLY. This is the costliest omission today.
- **Was**:
  - CONSTR-010: "ID-based (`RowId`/`ColId`/`CellId`), not coordinate-string-based".
  - FMI §11 warned that with a persistent tile DAG, "Row/col inserts are hard if tile coordinates are absolute".
  - It recommended a Roslyn-style hybrid: "immutable axis trees (row order and column order)", with tiles keyed by stable `(RowId, ColId)`.
- **Now**: `calc-doc-model` is exactly that tile DAG, with absolute coordinates.
  - The predicted failure has arrived: a row insert rewrites every formula, and undo rebuilds the engine.
  - Plan 3.2's "axis intervals/order maps" were never built.
- **Matters**: stable identity would make structural edits O(change) and give the stable node keys that dc-8abf lacks. FMI §11.4-11.5 already contains a benchmark matrix (W1-W6, T1-T3).

**R11. Ranges as graph nodes; formulas that differ only by offset share an identity.** Partly KEPT.
- **Was**:
  - "Range references `A1:A1000` add a single edge from a synthetic range node" (FEC D-RULE-001).
  - Regions "exist only in Layer 2" (FMI §3.2).
  - Nominal memoisation for formulas "structurally identical with offset" (DAG/07).
  - An R1C1-relative formula identity (`notes/GRID_EXTENSION_PROFILE_IMPACT_2026-06.md`).
- **Now**: the engine has rectangle edges, but still one node and one text per formula.
- **Matters**: this points to one node per formula block, with interval edges.

**R12. Deterministic spill rules.** Partly KEPT. Spill priority "row-major, then column-major"; on a spill change, only readers of "the *symmetric difference* of old and new spill ranges" are invalidated (FEC SPILL-RULE-002/004). Plan 6.2 has spill participants; spills still disable parallel recalc. Check the code against both rules.

**R13. Cycles: SCCs plus a declared iteration mode.** KEPT. A monotone fixed point was guaranteed only "when assumptions are declared" (DAG/09); plan 6.3 follows the order observed in Excel.

**R14. One publisher, workers on immutable snapshots, partitions later.** Partly KEPT.
- **Was**:
  - "Never call user code (UDF/plugin callback) while holding engine internal locks" (PP05; CONSTR-008).
  - A serialised lane for UDFs that are not thread-safe.
  - "Fixed-order reduction trees" for parallel aggregation.
  - Partitioning by snapshot (FEC Phase 3).
- **Now**: plan 3.5 keeps a single-thread oracle. Parallel recalc is native-only and turned off whenever a budget applies. The UDF registry is single-threaded.
- **Matters**: the serialised lane and the lock rule fit OxVba and XLL. Partitioning by snapshot is the corpus's only idea for distribution.

**R15. Results are independent of the schedule.** KEPT. "Parallel runs at fixed profile/seed produce bit-identical value outputs" (DAG-CONF-006). The reference twin (plan 6.5) checks this, and it is what makes R3-R6 safe to adopt.

**R16. Differential aggregates (DBSP-style, e.g. "SUM: add new − old").** REJECTED, for a good reason: an inverse update is not Excel's left fold, so it breaks bit-exactness (4.4). Usable only in a declared non-Excel profile, or for exact counts.

**R17. Analyzer-first introspection.** LOST.
- **Was**:
  - Incremental's `save_dot`, `For_analyzer` and per-node change stamps (JS §5.1).
  - DAG-CONF-001: recalc-order metadata.
  - Value-only versus trace-rich modes, with no "per-call trace structures in hot value paths" (AR §3.4, `b7dccb7`).
- **Matters**: this is the basis for the web formula debugger, and it costs nothing when turned off.

**R18. Calculation over ID-keyed nodes that are not on the grid.** REJECTED in October, with cause.
- **Was**:
  - "We don't have arbitrary bags of calculation nodes - they are rooted as tree(s)" (FMI).
  - DNA TreeCalc first, with "no coordinates" (CEFM §6.1.1).
  - Augmentation trees that "partake in the formal calculation 'from the outside'" (charts and controls).
- **Why rejected**: OCT dropped "the invented 'tree' reference language", about 75-85k lines in a second lane. That was the right call for an invented language and a duplicate lane.
- **Matters**: Webs-class definition from Rust or the web, strong typing, and the Tree and Canvas skins still need non-cell nodes. The open question is whether these can be ordinary participants in the one graph (for example, names or blocks with stable IDs) rather than a second language. The grid could then be one profile, `strict-excel-grid`.

## 2. Other ideas

**O1. The Formula Evaluation Context as declared facilities.** Partly KEPT.
- **Source**: `reference/conformance/excel-worksheet-engine/model/archive/fec-f3e-legacy/EXCEL_FORMULA_EVALUATION_CONTEXT_FEC.md`, `c6915f6`.
- **Was**:
  - "`FEC` is the set of host-provided facilities that are external to the pure formula/function/format core."
  - "A function or operator declares which `FEC` facilities it is allowed to observe".
  - A per-function `fec_dependency_profile` covering references, caller, time, random, external, locale and feature checks.
  - Checks that "no undeclared FEC facility is observed".
- **Now**: calc-fml has no evaluation context (risk-layering).
- **Matters**: this is owner principle 4.1 almost word for word. The same declaration would also drive the R9 tokens, supply the purity facts that R5 and parallel recalc need, and define what OneCalc must supply.

**O2. Optimiser metadata is part of the contract.** LOST.
- **Was**: use "resolved OxFunc call-site handles and metadata rather than repeatedly routing hot paths through broad string dispatch" (AR §3.4, `b7dccb7`; CONSTR-036/037).
- **Now**: functions are identified by their spelling.
- **Matters**: this is also the early-bound UDF and VBA path (4.5): resolve once at bind time, then call a typed handle.

**O3. One protocol surface for every writer.** PLAN-ONLY.
- **Was**:
  - "dispatch ops/transactions, query snapshots, subscribe to deltas, capability negotiation" (AR §3.1).
  - "no 'special cases' for UI, scripts, VBA, XLL, or collaboration" (CHARTER §2.2).
  - "COM automation is a separate facade layered on top of the identical protocol surface" (AR §3.7).
- **Now**: there is no command layer, and the object model lives in OxVba (risk-skins, risk-vba).

**O4. A canvas grid, tested through a RenderPlan IR.** PLAN-ONLY; the code drifted.
- **Was**:
  - "Never create DOM-per-cell" (BN §E).
  - Geometry and hit-test invariants UI-GEO-001..007.
  - "compare normalized JSON `RenderPlan` output, not pixels" (PP09).
- **Now**: the code rebuilds a `<div>` per cell on every scroll. 97 references in the test harness key on DOM attributes.

**O5. Scaling slopes from operation counters.** LOST.
- **Was**: "Time-only metrics are insufficient for complexity diagnosis". Fit log-log slopes of per-phase counters over a geometric ladder of input sizes (PP10; CONSTR-009).
- **Matters**: "nodes touched per one-cell edit versus workbook size" catches every O(workbook) cost in risk-engine §6. The check is deterministic and cheap enough to run on every commit (4.6).

**O6. Degradation classes and capability manifests.** KEPT. `Native/Lowered/Opaque/Rejected` and `profile_id + profile_version`; extend them to the 4.5 platform profiles (full on Windows, subsets on Linux and the web).

**O7. Capture discipline.** The Replay appliance as a system was REJECTED (08-30 investigation; OCT). Its cheap rules still hold: capture modes run from off to forensic, and "`forensic` is never allowed to change semantic outcomes"; no JSON on the hot path; shrinking "must fail explicitly" when the oracle is non-deterministic (`REPLAY_APPLIANCE.md`, `84b19a0`).

**O8. UDF/XLL admission states and lanes.** PLAN-ONLY. "discovery success does not imply enablement"; volatile and thread-safe flags at registration; "Keep a symbolic type layer first" (xll-non-interesting pack, pass 2). Typed signatures at registration are what an early-bound shim needs.

**O9. A function oracle as a product, "DNA Prove".** LOST.
- **Was**: it "Takes a function identifier + typed arguments, returns the Excel-correct result", shipped as a crate, as WASM and as an MCP server (`notes/SUGGESTION_DNA_PROVE_20260315.md`).
- **Matters**: a candidate home for the 4.4 research. Candidates and witnesses sit behind one interface, and the product links only the chosen program.

**O10. Evidence pinned to the Excel build, with both sources kept.** KEPT. `excel_exe_sha256`; disagreements "retained as explicit dual-source records" (`REFERENCE_SPEC_FORMAT_AND_CONFORMANCE.md`, `eb7a2ed`).

**O11. A small TLA+ model of the recalc protocol.** REJECTED (OCT: no formal methods), against the review's advice.
- **Was**: `INV-NoStaleCommit`, `INV-SnapshotConsistency`, `LIV-EventualStabilize` (PP03). The independent review said "TLA+ for the epoch/concurrency protocol: **Yes, definitely, do this**".
- **Matters**: incremental publication, cancellation, priority and partitions together are where interleaving bugs hide. A small model kept off the inner loop is cheap.

**Rejected for good reasons; do not revive:** dual Red/Blue engines; Lean and the OCaml oracle; rounds and packs; a second lane of any kind (OCT lesson 4); a full SAC runtime and a lock-free speculative coordinator (CORE_ENGINE_THEORY §4.2 and §4.6); inverse-update aggregates (R16).

## 3. Ranked list for the architecture review

1. **R1-R3: per-value epochs, per-node bundles and a recorded visibility stream.** These form the recalc contract that risk-engine §2 asks for. They were specified in February, keep determinism, and have a real caller (the viewport), so OCT lesson 3 is met.
2. **R10-R11: stable row and column identity with axis order maps, and one node per formula block with interval edges.** FMI predicted today's failure. Decide this before the save path and the function ports settle.
3. **R4 and R6: an observer-driven necessary set and a height-ordered heap, in the one scheduler.** This is the Webs-class route. Automatic mode still finishes the rest of the work.
4. **O1, R9 and O2: declared FEC facilities, invalidation tokens and resolved call-site metadata.** Together they deliver principle 4.1, end the clock-makes-everything-pending bug, supply the purity facts that cutoff and parallel recalc need, and give UDF and VBA calls an early-bound path.
5. **R5: early cutoff for functions declared pure**, checked against the reference twin, with every fallback counted (OCT lesson 2).
6. **O3: one protocol surface** for skins, agents, the VBA COM library and collaboration.
7. **O4: a RenderPlan IR and a canvas grid**, before the per-cell DOM harness grows further.
8. **O5: counter-slope checks** on every commit.
9. **R18 recast: non-grid nodes as participants in the one graph**, not a second language. Decide whether this belongs in the Webs-class definition surface.
10. **O11 and R17: a small TLA+ model of the new contract, plus analyzer-first tracing** for the debugger.
