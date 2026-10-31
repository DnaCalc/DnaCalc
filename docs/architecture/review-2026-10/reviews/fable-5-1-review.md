# Review round 1 by Fable 5.1

Checked against DnaCalc-review HEAD dbcc791 (three commits past 5a9348b; source changes are only
additions to `calc-host/src/{command,state}.rs` and the new `docs/plan/SYMBOLIC_SEARCH.md`, so line
numbers hold at 5a9348b) and OxVba c7ce1349 via `git show`. Nothing built or run. New input no draft
saw: `SYMBOLIC_SEARCH.md` (owner direction, 2026-10-31) changes section 1.8 of every draft (see 3.2).

## 1. The other drafts

### 1.1 Opus 5.5

**Adopt.**
- `calc-numeric`: the Fpu/x87 substrate, `ExactDecimal` and digit generation as a product crate
  apart from kernels; research depends on it, never on calc-func. SYMBOLIC_SEARCH.md asks for exactly
  this ("a DnaCalc research interface for private Fpu operations"; "never copy the old substrate").
- `Soft(VendorModel)` with typed, counted refusal for any x87 instruction without an identified
  vendor model. The code agrees: `calc-func/src/ported/excel_numeric/mod.rs:9-15` says soft87's
  transcendentals "round the exact value once, where the hardware's microcode rounds a value a few
  bits wider"; software is exact for arithmetic, loads, stores and remainder only.
- One type library, two object backends: UDF-time (reads via the FEC, refuses writes, inside
  OneCalc's ceiling) and macro-time (`calc-host-vba`, writes as host commands). My draft put the
  whole library in calc-vba over a `SheetAccess` trait; a macro's `Range.Value = x` would then pull
  calc-host into calc-vba, the leak S6 was meant to stop.
- The straddle arithmetic: a relative reference with offset d changes only for members whose row and
  target straddle the insert, a run of at most |d| rows, so a block splits into at most 1 + 2r pieces.
  Correct, and it is what Gemini's "at the TemplateId level, never O(cells)" gets wrong.
- `calc-fml-syntax`: the model must own `TemplateId`s for blocks and `architecture.json` forbids
  calc-doc-model -> calc-fml, so the split is necessary, not tidy. It also ends calc-doc's pull of
  calc-fml for a lexer (`calc-doc/Cargo.toml:15`; the calc-func edge at line 35 is dev-only).
- Neutral `Target` for commands; separate UI and worker crates (today one wasm module serves page
  and worker, `apps/dna-calc/src/bin/dna-calc-web.rs:9-15`).
- `may_spill` per-sheet index in the demanded cone (Gemini's shadow zones, GPT's conflict group). My
  S3 missed it: a visible cell reading a blank that an off-screen dirty anchor will spill into is wrong
  if evaluated first.

**Wrong or unverified.**
- "CELL needs a `Format` facet edge or format edits leave it stale." CELL is volatile
  (`calc-fml/src/eval/mod.rs:190-201`), recalculated every tick; `set_workbook_facts` deliberately
  makes nothing pending for that reason (`calc-engine/src/state.rs:776-779`). The facet would make
  CELL("format") fresher than Excel, where a format change alone recalculates nothing; measure before
  designing. Facets for Existence, Shape, NameBinding stand.
- "Excel keeps old UDF results until a recalc reaches them" after reload: not in the measured
  observations (`docs/plan/VBA_UDFS.md:78-84` covers module state, not reload). Plausible, unmeasured.
- "a whole-column format is one run, not 1M entries (`definitions.rs:315`)": line 315 is the per-cell
  edit map; column formats already go through `with_axis_format` (`definitions.rs:349`). The real cost
  is `LoadedSheet.styles` plus the axis-edit replay in `facts.rs:36-47`.
- Decision 6 (Excel DISPIDs via `GetTypeInfo` on a running Excel) needs the owner's ruling that COM
  introspection is black-box; GPT draws the same line more narrowly. Owner decision, not plan step.

**Misses.** Early cutoff is observable through UDF call counts and Excel was measured not to cut off:
"A dirty upstream formula also calls its UDF dependent even when the upstream result stays unchanged"
(`VBA_UDFS.md:79-80`). I4/I6 allow cutoff only where the forward cone has no effectful callable, and
the draft's `facets_read` has no purity flag to decide that.

**Against section 4.** None substantive.

### 1.2 GPT-Astra 6

**Adopt.**
- Demand (completion obligation) distinct from interest (priority and delivery); Automatic completes
  its required work; a scroll cancels expendable work only. The cleanest recalc contract of the four,
  and it answers foundation R4 without a mode switch.
- Staging: budgeted actor and inbox drain first *retaining whole-calculation publication*, then
  validated publication units with their own witnesses (edits during work, new blanks read
  dynamically, competing spills, SCCs). I move to this order.
- "Merely exposing the staged result map is incorrect": a later spill can alter a read blank; a new
  dynamic edge can close a cycle. Publication units validate read/binding/topology versions at
  commit. Mine and Opus's per-node rules are too thin here.
- Budget inside long scans and kernels, never where an extended-precision intermediate is live
  ("inserting a binary64 store to yield would change the program"). My between-node `CancelToken`
  does not make a 1M-row SUM(A:A) interruptible.
- Early cutoff compares observable facets (references, shape, visibility), not values.
- Seam 11: `calc-host/src/conditional.rs:712` calls `calc_doc::conditional_rule(source)` inside a
  query (verified). Seam 10: `ReadersKey` keys on `Arc::ptr_eq(definitions)` (`recalc.rs:1363`,
  verified) so any Definitions change empties the readers cache; `project.rs:215` recomputes
  `stale()` per request.
- "Replacing Rc with Arc does not establish thread safety" (against Gemini 2.5.4): the
  `thread_local!` cache (`calc-vba/src/lib.rs:11-13`) and `Rc<OwnedSession>` (line 85) are
  owner-affine by construction; the fix is routing, not `Arc`.
- Formatting ownership, a model beneath skins, early binding in both runtimes and no fitted
  corrections are already owner rulings; my decision 3 and Gemini's 2 and 4 are padded by this measure.

**Wrong or unverified.**
- asupersync "Unusable pending counsel" contradicts BRIEFING 4.7 (owner ruling 2026-10-31): acceptable,
  ring-fenced. The draft read superseded text. Its technical reasons against building the evaluation
  loop on it stand and match the other three.
- "do not invent per-cell random streams and call them Excel-compatible": RAND is already a
  per-(seed, tick, cell) splitmix64 stream ported from OxCalc (`calc-engine/src/random.rs:1-8`).
  Excel's RAND is unseeded, so order independence is the only claimable property; nobody claims bits.
- OxVba claims taken from risk-vba only; re-read here and confirmed: `oxvba-jit/src/lowering.rs:497-521`
  passes `member.name` into the same `emit_project_member_call_to_slot` as `ComCallLate`;
  `helpers.rs:4359-4383` builds `DynamicCallRequest` with `TokenNamed { token, name }`;
  `oxvba-vm3/src/lib.rs:2175-2203` does the same.

**Misses.** It wants the model to own `TemplateRecord`s and to "keep the main library ladder"; only
its own `calc-syntax` reconciles the two. **Against section 4.** 4.7 as above; strictest on 4.1.

### 1.3 Gemini 4 Argon

**Adopt.**
- `UnresolvedTopologyBarrier` pulled into the demanded cone; the phantom-blank case is the right test.
- `SkinPlugin` with a shell keymap and `reveal_in_skin` replacing the `set_timeout` focus hacks
  (`atlas.rs:139`, `ledger.rs:432,448`, `canvas.rs:474,506,529`) and seven `crate::grid::focus_grid()`
  calls (`controller.rs:806-2626`); all verified.
- Quick O(N) kills before blocks: `Progress::new(nodes.len())` (`recalc.rs:552`),
  `vec![false; state.nodes.len()]` in `finish` (`recalc.rs:1018`), whole-`Definitions` clone on any
  format/pane/rule edit (`root.rs:657-683`), CF present => every delta is a reset (`session.rs:852-866`).
- No kernel commit may cite match-count deltas; `git log` shows the pattern (`f937b23`, `71b5de2`,
  `2335ccb`, `480481e`, `cbdbb7f`, all dc-j9k). But as a per-commit word scan it is the "source-word
  scan masquerading as architectural proof" GPT names, and 4.6 says periodic; make it a review rule.

**Wrong or unverified.**
- "shared mutable PRNG (`RandomSource::next_unit` at `calc-func/src/call.rs:80`)": line 80 is the
  trait; the implementation is the counter-keyed `RandomStream`. `philox4x64(...)` re-specifies it.
- Structural shift "at the `TemplateId` level, never O(cells)": straddling members need a different
  template (Opus's |d|-row run), so blocks split; cost is O(blocks + straddling runs). The draft's own
  Edge-Clipping rule (2.2.3) is the same phenomenon at sheet bounds.
- `PageSummary.numeric_sum` answering SUM/AVERAGE in O(log P) (2.9) changes the association order of
  Excel's left fold and breaks 4.4; foundation R16 rejected it for the same reason. Valid only for
  COUNT, COUNTA, COUNTBLANK, MIN, MAX, or in a declared non-Excel profile.
- Decision 5, "verified software ext80 brings bit-identical Exact to the browser": contradicted by
  `excel_numeric/mod.rs:9-15`. Opus's refusal rule is the honest form.
- "swaps the function pointer in place (zero formula recompilation!)": in-flight calls on another
  thread and code lifetime are unaddressed; GPT's "running calls pin old code, retire on lease
  release" is the version to keep. In-place swap is safe only as an atomic pointer plus a code lease.
- `SheetLimitsPolicy::Unbounded64`: the A1 grammar, xlsx and every reference rule assume
  1,048,576 x 16,384. Scale is sheets x partitions x non-grid participants (R18 recast); GPT agrees.
- "controller.rs forked F4 anchor cycling from dna-formula-ui": `controller.rs:2303-2308` calls
  `cycle_anchors` imported from dna-formula-ui (`controller.rs:33`, `editor.rs:364`); only the routing
  is forked. `architecture.json:21-26` does not forbid calc-doc for OneCalc; `sweep.rs:47` does.
- Replacing the authored page payload with 64x16 striped SoA tiles rewrites the one store all four
  keep (`cells.rs:1-35`) with no measurement. Columnar *results* in the engine are agreed; the
  authored layout waits for numbers.
- `DistributedShardExecutor` with Arrow IPC / RaptorQ frames: prose with no seam; GPT names the obligations.

**Misses.** A `FormulaBlock` as "one vectorized node" creates false cycles and stops local recalc
(GPT seam 2); members must stay individually dirtiable. **Against section 4.** 4.4 (summary trees,
decision 5); 4.6 (per-commit word scan).

## 2. Foundation ideas

Take up: **R1-R3** (all four converge; add GPT's unit validation and record viewport moves as a
replayable input). **R4/R6** as GPT's demand/interest with a height-ordered ready heap; Pearce-Kelly
only once the up-front order is measured as the first-visible-cell bottleneck. **R10** axis keys:
Opus adopts, I move half-way (open point 2); FMI predicted today's failure (`structural.rs:1-8`
delegates every rewrite to a `FormulaRewriter`; `command.rs:465` rebuilds the engine on restore).
**R11** blocks with affine/interval edges: all four. **O1/R9/O2**: declared facilities per function,
one invalidation token each (ends `set_clock` -> `set_environment` -> all pending,
`calc-host/src/state.rs:234-237`, `calc-engine/src/state.rs:783-789`), resolved call-site slots; the
facility profile is the purity fact R5 needs. **R5** only for cones without effectful callables
(`VBA_UDFS.md:79-80`), comparing facets. **O3** one protocol surface: all four in different words.
**O4** RenderPlan IR and semantic test layer now; canvas is open point 4. **O5** counter-slope
checks per commit: none of the four proposed it; all should. **O8** typed signatures and admission
states at registration: the early-bound shim's input. **R18 recast**: non-grid participants with
their own keys; rejects `Unbounded64`.

Reject or defer: **O9** DNA Prove (SYMBOLIC_SEARCH.md keeps candidates as research data "outside
the application path"; a product is unplaced scope). **O11** TLA+ until publication units and
leases are written down. **R16** stays rejected (Gemini 2.9 is R16 renamed). **R17** as GPT's trace
lease per participant and tick, after the recalc contract.

## 3. My own position

Changed:
1. **Formatting crate.** calc-func cannot see calc-fml anyway, so my mechanical argument for
   `calc-format` was weak; with `calc-fml-syntax` split out, a calc-fml module behind `NumberText`
   satisfies 4.1 and plan 4.4. I withdraw decision 3.
2. **Research location.** I proposed a separate repository. SYMBOLIC_SEARCH.md wants the search to
   "extend xtask's existing witness loader and calc-verdict encodings" and a research interface to
   private Fpu operations. So: research under `tools/` in this repository (Gemini's `calc-func-lab`
   name is fine), depending on `calc-numeric` and calc-verdict, never on kernels, never in the product
   closure. The owner's "enumeration command within xtask" collides with every draft's thin xtask;
   `calc-harness` is the product-dependent tool that still "extends the existing loader".
3. **Publication order**: GPT's two steps replace my single step 3.
4. **Cancellation grain**: checkpoints inside scans and kernels where no x87 state is live.
5. **Object library**: two backends, not one over `SheetAccess`.
6. **Spill shadow**: may-spill index in the demanded cone.
7. **Effect tape**: RAND is already order-independent; the tape adds clock and RTD. I also wrote that
   salsa's cutoff idea "is already in the plan key"; it is not: the plan key is compile caching,
   value-level cutoff is absent.
8. **Mis-cite**: my S1 cited `read.rs:197-211 vs 1096-1108` for two sheet-name matchers; `read.rs`
   is 282 lines and 197-211 are tests. The matchers are `bind.rs:844-847` (`eq_ignore_ascii_case`)
   and the host's `HostFacts::sheet_named`. The point stands, the lines did not.

Held: glue-only Cargo edge to a neutral CalcVBA hosting API (all four); asupersync behind a
scheduler trait, native, optional, measured (GPT's objection is superseded); blocks before the save
path settles, per-member identity, never one indivisible node; Excel grid limits fixed; no browser
transcendental without a vendor model; thin xtask, `calc-harness`, smoke cases in the apps, receipts
deleted (all four); freeze UI feature slices while `calc-model` lands.

## 4. Proposed consensus and open disagreements

Converge on:
1. One non-defaulted `FormulaEvaluationContext` in calc-fml, derived into one `FunctionContext`;
   caller from the instance; `ReferenceReader` loses `environment()`/`host_facts()`
   (`read.rs:47-55`); the `call.rs:385-398` fallback and six forwarders go; facilities declared per
   function, each with an invalidation token.
2. `SemanticEnvironment` (dirties dependents) apart from per-tick `VolatileServices` (never
   dirties). Keep `random.rs`; add clock and RTD to the same tape.
3. `CallableTable` in calc-fml, slot plus generation in the plan key, `UserCall { slot }`, typed
   signatures at registration, code leases on reload; function identity by `FunctionId` metadata,
   the 9-name list and the `_xlfn.` tables deleted.
4. Formatting out of calc-func, one implementation via `NumberText`; one presentation query in
   calc-host feeding grid, CF and `HostQuestions`.
5. Recalc: demand (completion) distinct from interest (order/delivery); interest never changes
   values; validated publication units replace the atomic generation; cancel keeps settled work;
   may-spill index and dynamic-edge discovery are part of the demanded cone; SCCs and data tables
   are units; effectful callables run in reference order on their owner; results are
   schedule-independent with the serial path as twin. Stop writing atomic-generation tests now.
6. Blocks: template plus area in the model, per-member identity and dirtiness, affine/interval
   edges, splits on edit and straddle; `calc-fml-syntax`; columnar results in the engine; authored
   page layout unchanged until measured.
7. `calc-model` (DOM-free): typed commands, availability, neutral `Target`, shell keymap,
   `ViewStateStore`; Excel rules from `server.rs` into calc-host; skin-neutral projections;
   tile-grained binary deltas; a semantic test layer replacing the 97 DOM selectors; UI and worker
   as separate crates.
8. CalcVBA: neutral hosting API crate as the only edge, glue-only; DNA owns the object library with
   generated `#[repr(C)]` vtables everywhere and real COM on Windows; `ComCallEarly` lowered to slot
   calls in VM3 and JIT before any late-bound DNA path; typed procedure entries.
9. `calc-numeric` substrate; research under `tools/` on it and calc-verdict only; kernels enter
   calc-func only with an identified program and witnesses; no match-count commits.
10. Inner loop first: thin xtask, `calc-harness`, smoke cases in the apps with `--case`, receipts
    and retired CI deleted, one `architecture.json` covering calc-vba and OneCalc's whole ceiling,
    counter-slope checks per commit, slow suites in sweep.
11. asupersync: optional native backend behind the scheduler trait for actors and waves, measured at
    the recalc step; serial path always compiled; the browser never on it.

Open, each as a choice with its trade-off:
1. **Order policy.** Opus: `InterestFirst` default, `ExcelOrder` forced for order-observable
   regions. GPT: required work always completes, interest only orders ready work. The difference is
   whether a Manual or Webs profile may *drop* off-interest work: work proportional to interest
   versus a second observable mode to specify and test.
2. **Axis keys now (Opus, R10) or shifted block areas (Gemini, Fable).** Keys make inserts
   O(log n + straddles) and give stable node keys for partitions; cost is every coordinate lookup
   through an order-statistic tree, u64 keys on the wire, compaction against retained snapshots (GPT).
   Decide by FMI's W1-W6 benchmark before step 6 touches the save path.
3. **Formatting as a calc-fml module (GPT, Gemini, plan 4.4) or a crate (Opus).** Packaging once
   `calc-fml-syntax` exists; compile parallelism versus one more crate. Fable now indifferent.
4. **Grid renderer: Canvas2D with a bounded a11y overlay (Gemini; Opus allows) or recycled DOM over
   the tile mirror (Fable).** 60 fps and no per-cell DOM versus accessibility, text-measurement
   parity with CELL("width") and a second test surface. Mirror and semantic layer agreed either way.
5. **Browser Exact.** Opus/Fable: arithmetic exact, transcendentals refused without a vendor model.
   Gemini: software ext80 everywhere. Honest refusal versus coverage labelled Exact.
6. **Where the search command runs.** Owner: "within xtask". Drafts: thin xtask. Fable:
   `calc-harness`. The owner must say whether "xtask" meant the tool or the repository.
7. **Object-library compatibility by COM introspection of a running Excel (Opus decision 6).**
   Needed for compiled Windows clients to bind; requires the owner's black-box ruling.
8. **VBA reload.** Gemini: in-place pointer swap when the signature is unchanged. GPT/Opus/Fable:
   generation bump with leases, lazy rebind. Zero rebinding versus one more lifetime rule; the
   atomic-swap-plus-lease form is the only safe version of the first.
