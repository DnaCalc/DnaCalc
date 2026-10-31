# Review round 1: Opus 5.5 on Fable 5.1, GPT-Astra 6 and Gemini 4 Argon

Code checked read-only with `git show` at DnaCalc-review 5a9348b and OxVba c7ce1349. BRIEFING 4.7
(asupersync usable, ring-fenced) and foundation-ideas.md applied. "R"/"O" numbers refer to
foundation-ideas.md.

## 1. The other drafts

### 1.1 Fable 5.1

**Adopt.** The *Affine* edge kind (`Affine { block, drow, dcol }`) as a named edge type next to
`Rect`; `BlockId` versus `NotebookBlockId` kept apart (S2), a real naming collision my draft missed;
`HostQuestions` as *answers* (`width_characters`, `prefix_char`) rather than raw style records;
`ArrayShape` to u32 (verified: `calc-value/src/value.rs:108-111` is `usize`); the explicit list of
O(workbook) costs to kill in step 3; "stop adding atomic-generation tests the day this starts".

**Wrong or unverified.**
- 1.4, the publication invariant "a node's published value was computed from prerequisites all
  stamped with the same tick as itself, which topological evaluation gives for free" is false: a
  prerequisite outside the dirty closure keeps an older tick, and a blank that an unsettled spill
  will fill carries no tick at all. The workable invariant is "no dirty node and no unresolved
  may-spill or dynamic-topology root in the backward cone at T".
- S2/3, "a structural edit transforms the template once per block and splits blocks the edit
  crosses" understates the split. Inserting k rows at p changes a relative offset only for
  members whose row and target lie on opposite sides of p, so a block entirely *below* p splits
  too: `C20:C30 = R[-15]C` with an insert at row 10 gives offsets -16 for old rows 20-24 and -15
  for the rest. The bound is 1 + 2r pieces per block for r relative references, over every block
  whose references straddle p, not only blocks whose extent crosses it.
- 1.8, the research repository's ceiling "calc-value and calc-verdict only" leaves candidates
  unable to run on the product's x87 operations, so a found program must be re-transcribed by
  hand onto a different substrate: the translation step where fitting creeps back in. The
  substrate has to be shared (my `calc-numeric`), the search not.
- 1.9 lists Exact as "x86-64" on Windows and Linux. Hardware x87 matches Excel only when the
  running CPU's vendor matches the target (BRIEFING 4.4: AMD and Intel differ on 15 of 15
  inputs). An Intel Linux host is not an Exact host for an AMD target.

**Missed seams.** An off-screen anchor spilling into a visible blank is outside a static-edge
`DemandSet` cone (Gemini 2.3 and GPT seam 4 catch it). VBA UDF call order and count are
observable and measured (dc-snz.5); Fable treats VBA only as thread affinity.

**Principles.** Consistent with section 4. The status row `Fitted(reverting)` should not exist as
a steady state under 4.4; it can be a migration label only.

### 1.2 GPT-Astra 6

**Adopt.**
- "A formula region is compressed storage, not one indivisible calculation node." Members keep
  their own dirty, ready and cycle identity. A recurrence `B3=B2+1` is not a vector.
- Exact continuations must not insert stores: a cancellation checkpoint inside an x87 sequence
  that spills to binary64 changes the program (section "Recalculation"). This constrains where
  `step` may yield and none of the other drafts says it.
- Early cutoff must compare every observable facet (value, shape, references), not numeric
  equality only (seam 10).
- A separate DNA-owned `calc-object-model` crate (descriptors, generated stubs, `ObjectServices`
  traits), used by calc-vba, the Windows COM adapter and XLL callbacks. The object library is not
  VBA-specific, so it should not live in the VBA glue (my draft put it there).
- Sequencing in step 4: a budgeted actor, inbox draining and interest priority first, partial
  publication after. The product gains responsiveness before the riskiest contract change lands.

**Wrong or unverified.**
- Library table: "asupersync and related projects: unusable pending counsel." This contradicts
  the owner ruling in BRIEFING 4.7 (usable, rider noted in third-party notices, ring-fenced). The
  draft predates the ruling; the position must change.
- "There is one formatting implementation, owned by CalcFml" is presented as an existing owner
  ruling. Plan 4.4 (`DNA_CALC_REIMPLEMENTATION_PLAN.md:350`) says that; BRIEFING 4.1 only rules it
  out of calc-func. The plan's choice is still the right default (see section 3), but it is the
  plan's, not a new ruling.
- Seam 9 cites `excel_numeric/mod.rs:5` as claiming hardware exactness "while lines 10-15
  describe differing software behaviour". The comment is self-consistent (hardware exact, soft
  transcendentals off by one 64-bit-significand unit). The real defect it does not name is that
  "bit-exact to 64-bit Excel on x86_64" (`mod.rs:5-6`) ignores the CPU vendor.
- All OxVba claims are unverified by its own statement ("its repository was not accessed").
  Verified for the record: `oxvba-embed/src/spreadsheet.rs:151` is `MAX_OBJECTS: usize = 4096`;
  `oxvba-jit/src/lowering.rs:497-540` passes `member.name` for `ComCallEarly`.
- Step 1 "compatibility constructors" keep the defaulted forwarding route alive while callers
  migrate. With six forwarders and a few hundred call sites this should be one change, not a
  migration period (4.6: fix couplings at the root).

**Missed or mishandled.** No key type for axis identity, no ID widths, no concrete template form,
so its structural-edit cost claims cannot be checked. Its test list is sound but heavy; under 4.6
most of it belongs in scheduled sweep runs.

### 1.3 Gemini 4 Argon

**Adopt.** `UnresolvedTopologyBarrier` (2.3.1) for may-spill and dynamic roots in the visible
cone; `CallablePurity::{Pure, Volatile, StatefulSequential}`; one `reveal_in_skin` command;
the formula-edit session (F4, point mode) moved into dna-formula-ui.

**Wrong or unverified.**
- 2.3.2 says the PRNG is "a shared mutable PRNG (`RandomSource::next_unit` at
  `calc-func/src/call.rs:80`)" whose draws depend on scroll order, and proposes philox. Wrong:
  `call.rs:80` is a trait; the engine already supplies a stream keyed by (seed, tick, cell)
  (`calc-engine/src/random.rs:1-8, 19-28`), order-independent by construction.
- 1.3/2.2, "a 1M-row fill is one FormulaBlock ... and one vectorized node". A block as one graph
  node manufactures false cycles (column B reads C of the previous row, C reads B of the same
  row: block-level cycle, member-level DAG) and turns a one-cell input change into a 1M-member
  recompute. GPT's members-stay-addressable rule is the correction.
- 2.2.2, insert "adjusts only relative references that cross r at the TemplateId level (at most
  O(unique templates))". Wrong for the reason given under Fable: members straddling the insert
  point get a different offset, so the work is per straddling run of members and blocks split.
- 1.3/2.9, `PageSummary { numeric_sum, ... }` maintained incrementally and `reduce_area` for SUM.
  An incrementally maintained sum is an inverse update and a tree combine is not Excel's
  evaluation order: both break 4.4 (R16 was rejected for exactly this). Usable only for COUNT,
  MIN, MAX and existence, never for floating sums.
- Decision 5, "verified software ext80 brings bit-identical Exact results to the browser and
  ARM". The repo's own measurement says otherwise: soft transcendentals round the exact value
  once where microcode rounds a wider value, differing by one significand unit
  (`calc-func/src/ported/excel_numeric/mod.rs:10-15`), and AMD and Intel microcode differ from
  each other. A soft model must model a specific vendor's microcode, which is research, not a
  backend switch.
- 2.5.1, swapping the trampoline pointer in place on reload with "zero formula recompilation":
  use-after-unload for in-flight activations, and VBA module-level state is reset on reload, which
  is a semantic change, not just a code swap. Generation bump plus leases (Fable, GPT, mine).
- 2.3.2 schedules stateful UDFs in "canonical `(SheetIndex, Row, Col)` order". That is not
  Excel's order; Excel follows its calc chain, and the dc-snz.5 observations
  (`crates/calc-vba/tests/fixtures/excel-recalculation/observations.json`) are the evidence to
  match, not an invented canonical order.

**Conflicts with section 4.** (4.6) A `cargo xtask check` rule scanning commits for match-count
citations is per-commit process; the research home is the fix. (Scope) Phase 5 schedules
`Unbounded64` sheets, a shard executor, Arrow IPC and RaptorQ as implementation; unbounded sheets
also break the A1 grammar, and R18 and GPT give non-grid participants their own keys instead.
(4.4) Redirecting the bit-exact lane to FEC plumbing removes the research instead of housing it.
(Layering) `calc-host` -> `calc-vba-host` makes VBA mandatory in the host; extension wiring
belongs to app composition (GPT) or an above-host crate.

## 2. foundation-ideas.md

**Take up (none of the drafts has all of these).**
- R1/R3: per-value settle ticks (my run-length `settled_tick`, Fable's stamp) plus the
  **recorded visibility stream**: `SetInterest` messages carry an `interest_version` and are
  journalled like edits, so "same operations + same interest stream => same publication stream"
  is testable. No draft makes interest a replayable input; all four need it for the seeded
  schedule tests to reproduce a publication bug.
- R2: the per-node bundle (value, read edges, spill shape); all four drafts converge on it.
- R4/R6: maintained heights with a height-ordered heap, so the first visible cell can compute
  before the whole dirty closure is ordered. My draft and Fable keep the up-front
  `begin_recalc` closure; on a 1M-node closure that is the latency the owner wants removed.
  "Necessary-only" evaluation that drops work is a declared mode (Manual, a future Webs profile),
  never Automatic.
- R5: early cutoff for pure cones, comparing all facets (GPT), disabled where a cone contains
  `user_calls` or volatile plans, because skipped evaluations are observable through UDF call
  counts (measured in dc-snz.5).
- R8: async completion (async UDFs, off-lane VBA, RTD) as an external update opening a later tick.
- R9 + O1: per-function declared facilities (`facets_read` in the catalogue) drive invalidation
  tokens, purity for cutoff and parallelism, and the exact list OneCalc must supply. This is the
  strongest single idea in the file and turns 4.1 from a convention into data.
- R10 stable axis identity (see 4.A); R17 analyzer-first tracing, zero cost when off, with GPT's
  per-participant trace lease; O4 grid tests on a normalised RenderPlan, not DOM attributes.
- O5: counter-slope checks (nodes touched per one-cell edit across a 3-size ladder). It is
  deterministic and catches exactly the O(workbook) costs in risk-engine 6; run it in the touched
  engine crates' check, since it fails on real defects (4.6 is satisfied).

**Reject or narrow.** R14's "fixed-order reduction trees": any order but the identified Excel
program's breaks 4.4; parallelism is across blocks, never inside a reduction. R16 stays rejected
(it covers Gemini's `PageSummary`). R18 as a second graph: reject; as non-grid participants with
stable IDs in the one graph: a constraint on IDs now, built when placed. O9 as a product: reject;
its interface is OneCalc plus `calcctl`. O11 TLA+: OCT's ruling stands unless the owner reverses
it; seeded replay of edit/cancel/interest interleavings covers the risk inside Rust tests.

## 3. My position: changes and holds

**Changed.**
1. Formatting: no `calc-format` crate; plan 4.4's `calc-fml::format`, calc-func sees only the
   `NumberText` trait. Nothing below calc-fml needs the formatter, so a crate is a seam without
   traffic. Digit generation stays in `calc-numeric`. Likewise `calc-present` becomes a calc-host
   module.
2. The object library moves to GPT's `calc-object-model`, keeping my backend split (UDF-time
   reads over the FEC; macro-time writes as host commands).
3. Block members keep their own dirty, ready and cycle identity; SCCs are found at member or
   interval grain (my self-edge offset covered only single-block recurrences).
4. `step` yields only where the identified program already stores to binary64, never inside an
   x87 sequence.
5. My step 4 splits into 4a (budgeted actor, inbox drain, `reprioritise`, clock as a service) and
   4b (per-unit publication); adopt R3's journalled interest stream, R4/R6 heights, R5 cutoff.

**Held.** (1) Stable `AxisKey`s (R10), against Fable and Gemini. (2) A shared `calc-numeric`
substrate with research outside the product closure, against Fable's substrate-less repo and
Gemini's lab on calc-func's kernels. (3) Exact needs a vendor match or an identified vendor model,
else a counted refusal, against Gemini's generic ext80 and the vendor-blind tables. (4)
asupersync only behind `calc-sched`, native, for orchestration and lab replay after a measured
spike, against GPT's "unusable". (5) Separate UI and worker crates so the page bundle stops
carrying the engine (`src/bin/dna-calc-web.rs` builds both today); no other draft raises it. (6)
The one Cargo edge carries `#[repr(C)]` tables and descriptors only, no runtime types.

## 4. Proposed consensus

**Converge on (all four drafts).**
1. calc-fml owns a `FormulaEvaluationContext` with no default methods, built in one place; caller
   from the instance; `ReferenceReader` loses `environment()`/`host_facts()`; the data-table
   overlay wraps only the value reader. Fixes ROW() in OneCalc and CELL/SHEET in data tables.
2. `SemanticEnv` versus per-tick volatile services; the clock dirties only volatile nodes
   (today `calc-engine/src/state.rs:783-788` dirties every node).
3. Function identity by `FunctionId` metadata: volatility, subtotal, file prefix, purity,
   `facets_read` (O1). Delete the spelling lists and calc-doc's duplicate prefix table.
4. Formatting out of calc-func into `calc-fml::format` (plan 4.4); one presentation resolver in
   calc-host feeds both the grid and CELL; a `Format` dependency facet (plan line 338).
5. `CallableTable` in calc-fml; `UserCall { slot }` with generation in the plan key; reload is a
   new generation with leases, never an in-place pointer swap.
6. Formulas are R1C1 templates over blocks; shared formulas load as blocks; members stay
   individually addressable and invalidatable; a syntax crate below calc-fml lets the doc model
   hold templates (`architecture.json` forbids calc-doc-model -> calc-fml today).
7. Style runs replace per-cell format maps and axis-edit replay.
8. Interest orders work and never changes values; publication per unit (value, edges, spill
   reservation); SCCs, data tables and order-observable UDF regions are units; cancel means
   yield; RAND keeps its existing tick-and-cell keying; a cell is `Settled(T)` only when no dirty
   node or unresolved may-spill/dynamic root is in its backward cone.
9. A serial executor we own is the reference twin and browser default; a `calc-sched` trait
   isolates any pool or asupersync adapter.
10. Typed host commands in calc-host; DOM-free model with a shell keymap and neutral selection;
    skin-neutral projections; no skin-named wire requests.
11. Binary columnar viewport tiles with string and style tables, prose on demand, the worker
    drains its inbox between steps, tests read a semantic layer.
12. CalcVBA: neutral hosting API; DNA owns the object library; repr(C) vtables on every platform;
    `ComCallEarly` becomes a slot call in VM3 and the JIT before any late-bound DNA path; one Cargo
    edge, glue to the host-API crate.
13. Function research outside the product closure; kernels enter only as identified programs
    with witnesses; per-function status; no match-count commits.
14. Inner loop: thin xtask, `calc-harness`, receipts deleted, smoke cases beside the apps with a
    case filter, NOT-RUN counts printed.
15. Excel grid bounds stay; `ArrayShape` u32; partition seams as types only.

**Open disagreements, each a choice.**
- **A. Axis identity.** Stable `AxisKey`s with order-statistic position maps (Opus, GPT, R10):
  O(log n + straddling runs) inserts, inverse-op undo, durable keys for the debugger and
  partitions; costs a key-to-position lookup on reads and a harder first implementation. Versus
  shifting coordinates with block patches (Fable, Gemini): simpler, but O(blocks below p) per
  insert and no durable identity.
- **B. Block grain in the scheduler.** Block node with member dirty bitsets and interval cycle
  detection (Fable, Opus) versus storage-only regions with tiles materialised by the scheduler
  (GPT). The first gives fewer graph nodes; the second is simpler to make correct around
  irregular edits. Gemini's one-node-per-block is out.
- **C. When partial publication lands.** With interest priority (Fable, Gemini) or after a
  budgeted actor ships (GPT, now Opus): earlier visible values against lower risk per step.
- **D. Exact off-target.** Refuse unless the vendor matches or a vendor model is identified
  (Opus, Fable), versus generic soft ext80 (Gemini, contradicted by the repo), versus a remote
  Exact service (GPT). The owner also has to name the reference CPU vendor for published Exact
  claims (GPT decision 2).
- **E. Research home.** A separate workspace in the repo sharing `calc-numeric` (Opus), versus a
  separate repo with no product substrate (Fable), versus `tools/calc-func-lab` depending on
  calc-func (Gemini). The trade is translation risk against isolation of compile and review.
- **F. Up-front closure versus maintained heights (R6).** Cheaper now, against removing the
  O(closure) wait before the first visible result (heights must survive dynamic references).
- **G. Order policy.** `InterestFirst` with Excel's calc-chain order forced for order-observable
  regions (Opus), versus Excel order always with visible-first publication only. Speed shown
  against the cost of modelling Excel's calc chain exactly.
- **H. Object library.** Separate `calc-object-model` (GPT, now Opus; XLL and COM clients use it
  without VBA) versus inside the VBA glue (Fable, Gemini).
- **I. Grid surface.** Canvas plus an accessibility overlay (Gemini, O4) versus keyed, recycled
  DOM (Fable, GPT). Scroll cost against accessibility effort; both need the tile cache first.
