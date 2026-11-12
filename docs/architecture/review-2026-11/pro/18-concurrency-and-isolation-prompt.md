# Question for GPT-6 Pro: Excel-compatible threading, execution ownership and add-in process isolation

DnaCalc pin: **`3b5aa86b3ba47cbd216daec70c5d6586ad520deb`**. Tree: https://github.com/DnaCalc/DnaCalc/tree/3b5aa86b3ba47cbd216daec70c5d6586ad520deb. Every DnaCalc file:line in this packet refers to that commit.

CalcVBA pin: **`8115aa78135114c1dea79f26cbc12c8a3b00a4cf`**. Tree: https://github.com/DnaCalc/CalcVBA/tree/8115aa78135114c1dea79f26cbc12c8a3b00a4cf. Paths prefixed `CalcVBA/` refer to this separate repository and pin; all others are DnaCalc-relative. This is also the revision pinned by DnaCalc’s calc-vba dependency.

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet, for desktop and web. It must reproduce Excel bit for bit: numeric bits, types, errors, reference behavior and observable calls/events. DNA OneCalc is a values-only function host for formulas and add-ins. OneCalc stays values-only; DNA Calc supplies the full workbook host.

The stack is calc-syntax/calc-fml for formulas and evaluation, calc-func for functions, calc-engine for dependency management and recalculation, calc-host for the session and host, calc-doc-model/calc-doc for authored state and file I/O (xlsx/xlsm/xlsb), calc-vba/calc-host-vba for VBA through CalcVBA, calc-xll-host/calc-com-host for native add-ins, and apps/dna-calc*, dna-calc-worker/protocol/model for the application and skins. This review covers everything above calc-fml and its interface, through the skin IR, and the interface to calc-doc. Do not redesign numeric kernels or the formula language except to identify a required interface change.

You are reviewing a concrete implementation and its next architecture. You have one shot and no tools. Everything needed for the requested review is included below; GitHub links are optional extras. Do not make browsing a prerequisite or ask for another round before giving a useful design. These answers will become review and rework material for Sonnet and Opus implementation lanes, not an instruction to implement code now.

## What we ask

Produce these **six numbered deliverables in this fixed order**. Integrate the topic questions below into them.

1. **Verdict on today's design, with evidence.** State what to keep, what works, what is incomplete and what needs rework. Distinguish executable paths from scaffolding, older design documents and review recommendations. Assess end-to-end product routes, not isolated attractive types.
2. **Specific defects and risks, ranked.** Give a compact table: severity; exact pinned file:line; trigger and observable consequence; demonstrated defect versus inference; smallest reproducer or missing evidence. Include stale assumptions and dangerous gaps, but do not invent absent code or infer a passing test from its existence.
3. **Recommended target design and the smallest migration from today's code.** Name state owners, identities, invariants, interfaces and execution boundaries. Rust-flavored type sketches and short sequence traces are welcome; mark them PROPOSED. Show which existing types/routes are extended, which adapters disappear, and how current supported behavior survives each migration. Avoid parallel implementations of the same semantic model. Include an interaction table covering all five reviews: object model/JS, graph/recalc, engine-host-doc, host-to-skin IR, and concurrency/isolation. Assign each shared contract one owner and name what this review requires from the others.
4. **A staged implementation plan with measurable acceptance per stage.** Each stage needs concrete changes, dependencies, retained behavior/refusals, product-route acceptance cases, measurements and a clear stop condition. Identify what can be handed to Sonnet versus what requires an Opus design decision; do not build management apparatus. Separate correctness gates from proposed performance budgets. Reuse the existing tests, capture lineage, corpus harness and verdict path.
5. **Discriminating experiments against Excel.** We can run these on real Excel. For each high-impact uncertainty provide minimal workbook/VBA/XLL/JS inputs or a runnable script outline, initial caches and entry order, calculation mode and command, competing hypotheses, the expected observable outcome **under each hypothesis**, and what result would change the design. Record raw numeric bits/types/errors, callback/event order, counts, thread identities, dirty state and save/reopen observations as appropriate. Label predictions as hypotheses, never measured facts. State build/platform sensitivity and controls. Office.js-specific tests may need Excel's JS runtime; distinguish that observable API contract from internal Excel scheduling conjecture.
6. **What to stop or delete.** Name redundant routes, misleading abstractions, obsolete assumptions and work that should wait. Preserve useful compatibility evidence. Prefer capability delivered with a small coherent design over ceremony or speculative infrastructure.

## Constraints and evidence rules

- The goal is bit-exact Excel parity, judged by recorded Excel answers. Do not trade observable semantics for a cleaner implementation without explicitly identifying the divergence and the experiment needed to resolve it. Do not silently bless a documented known difference.
- Owner direction: capability first, no ceremony; extend rather than duplicate. One evaluator, one graph, one canonical authored model, one capture lineage and one verdict path. Typed not-modelled refusals are preferable to invented behavior and count as failures.
- Macro levels are **Off / Safe / Unguarded**, with CalcVBA's **Safe / Full capability policy**. Distinguish project execution grants, worksheet-function restrictions, native-code trust and process isolation. A capability gate is not an OS sandbox. Preserve the values-only OneCalc boundary.
- Rust and wasm are required. VBA execution/callbacks are on their owning thread. Native COM/XLL services are native-only; missing browser facilities must have explicit behavior. The desktop shell uses WebView2; the grid should admit WebGPU-friendly rendering. Do not claim WebGPU is implemented merely because a display-list boundary exists.
- No Microsoft source or disassembly. Public documentation and black-box Excel experiments are permitted. Do not claim to have consulted external sources you cannot access. Mark outside recollection and proposed API behavior as such.
- Use labels **[MEASURED-EXCEL]**, **[CODE]**, **[DESIGN]**, **[EARLIER-REVIEW]**, **[INFERENCE]** and **[PROPOSED]**. Measured results establish only the recorded cases/builds. Code proves what our route does, not what Excel does. Earlier reviews are conclusions at older revisions, not current defects by default.
- Appendix source labels and numbered source lines refer to the pins stated above. Excerpts are trimmed; boundaries can cut functions and do not imply surrounding code is absent. Original line numbers survive redaction. Local locations/machine labels and embedded historical file:line locators are marked as omitted; cite the pinned source label and numbered line, not an obsolete locator inside an old document. Older prose can contain stale claims; reconcile it with current code, and say when a supplied excerpt is insufficient to decide.
- No builds, cargo invocations or Excel runs were performed to assemble this packet. Test/corpus counts below are **reported repository measurements**, not fresh validation of this pinned tree. Keep their populations and provenance separate.

## Topic questions to cover

Review threading and execution architecture **above calc-fml**, including its required capability interface. Map threads/processes and state ownership for engine calculation, host/session, VBA VM3/JIT, native ordinary/#/$/async XLL calls, COM activation/events, RTD, UI and save. Compare desktop with wasm's single-threaded execution and dedicated workers; distinguish code compile-time thread safety from semantic permission to run concurrently.

Assess the current native parallel-level path, its eligibility gates and unlimited-budget requirement, worker-local scratch, concurrent scalar callback path/pool limits, ordering/publication, known dynamic-prerequisite refusal and serial fallback. Explain which Excel MTR observables must match: caller context, callback permissions/statuses, ordinary/#/$ execution, Range.Calculate owner-thread behavior, repeated entries, iteration, volatile draws and cached VBA read visibility. Thread-safe is not pure, deterministic or freely reorderable. Decide how a future faster engine preserves externally observable order without reproducing irrelevant physical timing.

Specify one owner thread for VBA execution, project globals, runtime objects, finalizers, COM apartment references and synchronous events; compilers may parallelize. Address invocation-aware reentry, nested callbacks, ByRef/cancel events, message pumping, deadlocks without held locks, reload/close with retained activations, teardown order and code/state leases. Do not solve affinity by declaring unsafe Send/Sync on thread-bound objects.

Separate yield, dependency wait, external await, semantic reevaluation, supersession, cancellation, interruption and shutdown. Define what a cancel key can interrupt during a resumable evaluation, long kernel, VBA loop or uncooperative native call; what effects remain; what happens to already-published units; and how the next edit proceeds. Include native crash/hang, stuck callback, provider death, late async return, old RTD leases and cancellation during save. A future timeout must not imply safe in-process termination of arbitrary native code.

Design a concrete staged **add-in surrogate** for bead dc-6332.15: per-add-in/session/runtime-group process choices, .NET/Python coexistence and unload constraints, what stays in process, host-owned document checkpoint, bidirectional callbacks with parent invocation IDs, caller/reference descriptors, status-64 unwind, nested xlUDF/WorksheetFunction calls, owning-memory copy/free rules, async tokens with process incarnation, RTD/COM on the surrogate STA and event ordering. State what cannot cross transparently (pointers, COM identity, window discovery, native callback ABI, cached read frontiers). Specify crash recovery without automatic replay of user code/effects and honest user-visible unknown outcomes.

Tie Off/Safe/Unguarded and Safe/Full to execution grants and the surrogate's authority. Safe's HAL gates are not native-code containment. Explain how a callback avoids authority escalation and how policy follows nested calls, worker threads and process messages. Recommend semantic versus publication replay, deterministic nondeterministic-input admission and reproducibility artifacts; a random seed alone is inadequate. Experiments should discriminate Excel's MTR and event behavior and separately fault-inject our owner/worker/surrogate implementation.

## What was already tried

Already tried: the synthesis chose serial finite stepping first, then optional native concurrency with invocation/read/publication contracts intact. Current native code has a gated parallel wave and an XLL worker pool; host code also retains owner-thread and async/RTD routes. Earlier reviews recommended a checked worker capability boundary and runtime-group surrogates. dc-6332.15 is an open design-note task, not evidence of a shipping surrogate. CalcVBA explicitly retains single-owner-thread execution; its capability wrapper explicitly says it is not a sandbox.

## Current measured state and its limits

Reported corpus: **76,903 / 78,264** oracle formula cells, 1,361 failures; bank: **3,895,333 / 3,910,795**, 15,462 failures. Capture batch 1: **130,849 / 134,577 = 97.22%** against live Excel, only 289 of 486 selected files captured. Enron is a separate saved-value comparison: **12,961,186 / 19,666,149** saved-value agreements in compared workbooks; **zero live passes** in that run. These populations overlap in neither meaning nor guarantee; none proves the architecture reviewed here is correct. Source excerpts follow. Existing test bodies are evidence of intended checks, not proof of execution in this preparation.

## Appendix A — Reported measurement provenance

### Tracker record dc-mndh [DESIGN / REPORTED CHECKS]

Selected fields from `.beads/issues.jsonl:455` at the DnaCalc pin; formatted for readability, other fields omitted. [GitHub record](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/.beads/issues.jsonl#L455).

````json
{
  "id": "dc-mndh",
  "status": "closed",
  "close_reason": "C1-C6 fixed in 44e300ed: source/expansion admission and shared concurrency budget; streaming JSONL and incremental summaries; iterative formula-shape scan with typed malformed/work-budget refusals; complete defined-name scans and live/host exclusions; reader_build before serialization; HostState::open_report and classification repair counts. C2 deliberately refuses affected saves because preserved chart/dialog references cannot be transformed safely. Tests written before each finding fix. cargo xtask check passed, and integration check --base 2141cb13 passed after incoming dependency changes: 895 passed, 0 failed, 1 ignored, 1 not run; portable compile passed. Clean d23905a9 baseline and final oracle both 76903/78264 pass, 1361 fail; all 78156 captured cell verdicts unchanged."
}
````

### Tracker record dc-6332.15 [DESIGN / REPORTED CHECKS]

Selected fields from `.beads/issues.jsonl:82` at the DnaCalc pin; formatted for readability, other fields omitted. [GitHub record](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/.beads/issues.jsonl#L82).

````json
{
  "id": "dc-6332.15",
  "title": "Add-in surrogate: boundary design note",
  "description": "Design note, no code (harvest docs/architecture/review-2026-11/pro/harvest/13-harvest.md, N4; answer 13 B.8): session or runtime-group surrogate, what stays in process, bidirectional callbacks with parent IDs, caller descriptors, async incarnation in tokens, RTD/COM in the surrogate's STA, controller document checkpoint, no automatic replay. Written under docs/architecture/. Feeds dc-snz.21 (xlOil safe reload) and dc-snz.19 (AOT). Starts while steps 0-2 (dc-6332.10, .11) are in progress, as the answer advises.\nModel Opus. Read AGENTS.md first.\n",
  "status": "open"
}
````

### Reported full-bank baseline and denominator [MEASURED-EXCEL]

Source: `docs/research/gamma-family-2026-11-12.md:305` (lines 305–312, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/research/gamma-family-2026-11-12.md#L305-L312).

````text
 305 | 
 306 | Measured before and after this patch on the same master calculation code (baseline bce9c5aa;
 307 | the later pulls to 0192ddd0 and 9bfdd1bf changed tracker and documentation only). Full bank: pass
 308 | 3,893,688 -> 3,895,333 of 3,910,795 rows; fail 17,107 -> 15,462, pass rate
 309 | 99.5626% -> 99.6046%. Gains 1,723, losses 78, net +1,645. The four unread and 2,064 excluded
 310 | rows remain failures. The 3,910,791 decoded trace rows have identical input, selection,
 311 | answer, source and environment fields before and after; only calculated results may differ.
 312 | 
````

### Reported oracle corpus and failure accounting [MEASURED-EXCEL]

Source: `docs/research/gamma-family-2026-11-12.md:344` (lines 344–348, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/research/gamma-family-2026-11-12.md#L344-L348).

````text
 344 | Oracle corpus, both runs: 62/62 workbooks compared; pass 76,903, fail 1,361 over 78,264 formula
 345 | cells (98.26%). Fail is mismatched 497 + refused 668 + failed/missing 1 + external links 73 +
 346 | circular held 14 + awaiting capture 108. All 152 retained report files have identical cell
 347 | verdicts before and after: zero gains and zero losses. Counts are over the corpus summary's
 348 | formula-cell population, not the report-file row count.
````

### Partial capture batch 1: live Excel comparison [MEASURED-EXCEL]

Source: `docs/plan/capture-batch-1-results.md:24` (lines 24–39, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/plan/capture-batch-1-results.md#L24-L39).

````text
  24 | ## Comparison (DNA Calc versus live Excel), 289 captured files
  25 | 
  26 | Pass = matched Excel; fail = mismatched + refused + failed or missing.
  27 | 
  28 | | | Cells |
  29 | |---|---:|
  30 | | Formula cells | 134,577 |
  31 | | Pass | 130,849 (97.22%) |
  32 | | Mismatched | 3,081 |
  33 | | Refused | 103 |
  34 | | Failed or missing | 544 |
  35 | 
  36 | The denominator covers only the captured files, so it is not a batch-wide rate. 3,153 of the 3,728 failing
  37 | cells are three capture or compare problems, not calculation: 2,604 cells "capture failure at stage reopen"
  38 | (2 workbooks, dc-ljie), 516 cells of one workbook whose comparison exceeded 600 s (dc-n2st), 33 with no
  39 | evaluator evidence.
````

### Enron: saved-value provenance limits [DESIGN]

Source: `docs/plan/enron-saved-cache-triage.md:8` (lines 8–21, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/plan/enron-saved-cache-triage.md#L8-L21).

````text
   8 | The 15,871 `.xlsx` files of the Enron spreadsheets dataset (figshare 1221767, `spreadsheets.7z`,
   9 | R2 `dnacalc-corpus/sources/enron/20261111/`) were opened and recalculated by DNA Calc, and each
  10 | formula cell's result was compared with the value saved in the file. The saved value is
  11 | `UnknownFreshnessCache` evidence: it can be stale, it carries 17 significant digits, and in a
  12 | file saved in manual calculation mode it may never have been recalculated. **A saved-value
  13 | agreement is not a pass.** The live `pass` count of this run is 0 by rule (no live Excel capture);
  14 | every number below is "saved-value agreement" or "saved-value disagreement".
  15 | 
  16 | Provenance of the saved values: the dataset's own note says its authors converted the original
  17 | `.xls` files to `.xlsx` with Excel 15, and every package here says `Microsoft Excel` 15.x in
  18 | `docProps/app.xml` with a `fileVersion` element. No Excel-save record is retained, so the
  19 | `--saved-excel` manifest (built by a script outside the repository from each package's own
  20 | metadata, keyed by SHA-256) records exactly that basis as its evidence. Treat it as an
  21 | assertion by us, not as retained Excel evidence.
````

### Enron: saved-value totals, never live passes [MEASURED-EXCEL]

Source: `docs/plan/enron-saved-cache-triage.md:35` (lines 35–56, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/plan/enron-saved-cache-triage.md#L35-L56).

````text
  35 | 
  36 | ## Totals (sum of per-workbook summaries)
  37 | 
  38 | | | Count |
  39 | |---|---:|
  40 | | `.xlsx` files | 15,871 (15,869 distinct by hash) |
  41 | | Workbooks compared (formula cells present) | 9,201 |
  42 | | Files with no formula cells | 6,649 in the second run, not compared |
  43 | | Timed out at 120 s (second run) | 19 files (known formula counts kept as failures by the harness) |
  44 | | Formula cells in compared workbooks | 19,666,149 |
  45 | | Live pass | 0 (by rule: no live capture) |
  46 | | Saved-value agreement (matched, DNA value equals saved decimal) | 12,961,186 (65.9%) |
  47 | | Matched from another workbook's link cache | 1,921,404 (9.8%) |
  48 | | Disagree: mismatched | 33,861 (0.17%) |
  49 | | Disagree: refused (typed not-modelled) | 476,695 (2.4%) |
  50 | | No saved answer used (failed or missing: volatile-dirty readers, downstream of refusals, array caches) | 4,273,003 (21.7%) |
  51 | 
  52 | Reading: Everything not matched counts as failing in the
  53 | headline, as the honesty rules require. The 1,288,450 cells excluded as
  54 | `volatile-dirty-or-unclassified-dependency` and 473 array-shape caches are in the 4.27 M.
  55 | I did not break the remaining ~3 M "not run" cells down further; most read a refused cell
  56 | (`MissingInput`, 196,605 listed cells in `offenders.json`, which is capped, so more exist).
````

## Appendix B — Pinned design, prior-review, implementation and test excerpts

The evidence class on each excerpt describes its source. A design document may mix measurements and recommendations; its wording and dates control the narrower claim. Old review defects must be rechecked against the code excerpts.

### Earlier concurrency and invocation-aware reentry direction [DESIGN]

Source: `docs/architecture/SYNTHESIS.md:180` (lines 180–196, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/SYNTHESIS.md#L180-L196).

````text
 180 | ### 1.4 Concurrency, scheduler and asupersync [4.7]
 181 | 
 182 | - `calc-sched` with our serial stepping executor (reference twin, browser driver). DNA Calc owns command demand and
 183 |   ordering, ready queues and interest priority, effect admission, checkpoints inside kernels, invocation identity
 184 |   and stale-return rejection, publication, freshness and replay (RT). Parallel levels accept a finite budget;
 185 |   native pool and block scheduling come after the contract works, without changing read, invocation or publication
 186 |   semantics.
 187 | - Owner-lane mailbox with **invocation-aware reentry**: engine waits on VBA, VBA calls the host, the host request
 188 |   queues behind the waiting activation: a deadlock with no lock held (RT 7). Cached reads, WorksheetFunction
 189 |   callbacks, COM STA reentry and macro operations go through the invocation's frame, not the general inbox.
 190 | - **asupersync [4.7]**: permitted, rider in notices, native-only adapter for task execution, channels, timers,
 191 |   cancellation delivery, resource draining and provider supervision; `Cx`, `Region` and `Outcome` never leak into
 192 |   calc-fml, calc-func or the scheduler interface. Never one region per recalculation that must close before the
 193 |   next edit; never cancel-and-respawn to reprioritise. Spike off the critical path on provider churn (delayed async
 194 |   returns, RTD bursts, reload), pinned version, features and toolchain (the red-team reports a nightly default
 195 |   feature set in 0.5.0 docs; to verify), compared with the serial driver plus a modest pool.
 196 | 
````

### Contract scheduling, budgets and cancellation [DESIGN]

Source: `docs/architecture/RECALC_CONTRACT.md:705` (lines 705–725, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/RECALC_CONTRACT.md#L705-L725).

````text
 705 | ## 9. Scheduling and finite work
 706 | 
 707 | Serial stepping executor: `advance(budget) -> Progress | Yield(checkpoint) | Waiting(reason) | Terminal`; budget checks
 708 | inside long kernels and VM/JIT code, not only between cells (today the budget counts evaluations,
 709 | `calc-engine/src/recalc.rs [historical locator omitted]`). Within a fence segment: interest priority with an age escape (after seven
 710 | interest-preferred quanta the oldest off-interest continuation runs; ties by reference order); promoting a target
 711 | promotes its prerequisite cone, never extra scope. Exact kernels yield only at binary64 store points or with full x87
 712 | state. The browser driver returns to a real worker task between slices (a resolved-promise chain does not).
 713 | 
 714 | Implementation update (dc-hsa5.4, 2026-11-01): root instructions and SEQUENCE kernels retain
 715 | continuations across worker tasks. First-wave interest promotes ready prerequisite cones inside fixed
 716 | callback fence segments, with an eighth-quantum age escape. Pure volatile scalars
 717 | retain their pass and draws when reordered. Volatile arrays, iteration/table sessions and later repair
 718 | waves retain reference order. Competing spills retain semantic reservation order.
 719 | The host requests installation handoffs before a later fence can enter: completed prefix values,
 720 | spill ownership and removals publish together. An unprovable prefix stops before the callback.
 721 | Headless callers enabling those handoffs must drain `publish_ready` between `step` calls.
 722 | The callable runtime now supplies checked slot/generation bindings, invocation identities and pinned
 723 | cached/reference views. Root typed Long/Double and DNA numeric Range calls retain owning VM/JIT activations;
 724 | counted native tests demonstrate suspension and reload without repeating the prefix. Variant adapters
 725 | retain an explicit synchronous interval. Lazy argument blocks, defined names and direct LAMBDA
````

### Prior evaluator review: checked worker boundary [EARLIER-REVIEW]

Source: `docs/architecture/review-2026-11/pro/8-evaluator-design-answer.md:467` (lines 467–487, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-11/pro/8-evaluator-design-answer.md#L467-L487).

````text
 467 | # D. Parallel safety and transcodability
 468 | 
 469 | ## D1. Smallest parallel boundary
 470 | 
 471 | The existing first wave already demonstrates a useful architecture: shared read-only evaluation inputs, local activation state, and deterministic staging by the main thread. Preserve it. Nothing in the excerpts justifies claiming that this wave is unsound. [J.6]
 472 | 
 473 | | Object or facility | Assessment |
 474 | |---|---|
 475 | | Immutable plans and placed references | Natural sharing boundary. Verify the actual compiled types' `Send + Sync` bounds, including target payloads. |
 476 | | `CalcValue`, immutable arrays/text, captured values | Reported `Send + Sync` in `calc-value`; shared immutable representation is appropriate. [A.1] |
 477 | | Operand/local stacks, random stream and evidence builder | Safe when owned by one activation/worker. Sharing a mutable source across workers is a different design. |
 478 | | `SharedReader` and `SharedRandom` using `RefCell` | Local use is compatible with independent parallel evaluations. They should not be shared concurrently. |
 479 | | `Rc<dyn UserFunction>`, `Rc<dyn Any>` read view | Cannot simply be sent/shared as worker objects. Preserve an owner-thread path or provide a checked parallel representation. [I.2, J.6] |
 480 | | `UserFunction`, user activation and kernel-task trait objects | Do not infer mobility from a call-count limit. A task can execute entirely on its creating worker without being migratable. |
 481 | | Host facts, RTD connector, reference handles, observer | Capability-specific. Immutable cuts and stamped snapshots can be shared; mutable host operations need their permitted lane/dispatcher. |
 482 | | `PlanCache` | Mutable compilation infrastructure, not per-formula execution state. No need to share it across evaluator workers merely because plans are shared. |
 483 | 
 484 | Rust's `RefCell<T>` is not `Sync`, but can be `Send` when its contained type is; `Rc<T>` is neither `Send` nor `Sync`. These facts do not require replacing all local `RefCell` values with locks. [Rust-RefCell, Rust-Rc]
 485 | 
 486 | The smallest useful changes are:
 487 | 
````

### Rework plan worker capability boundary and open restart parity [DESIGN]

Source: `docs/architecture/review-2026-11/REWORK_PLAN.md:59` (lines 59–70, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-11/REWORK_PLAN.md#L59-L70).

````text
  59 | ## R7. Worker capability boundary (B7 Pro) and R8. name code identity (B9 Pro, Fable 4)
  60 | Checked worker view of services and concurrent callables; shared immutable name bodies (`NameBodyId`) separate
  61 | from per-use evaluation identity; certified name-result reuse only later and only for eligible bodies.
  62 | 
  63 | ## Open semantic item
  64 | Effect-count parity on the first calculation after entry: Excel restarts a formula on an uncalculated read
  65 | (static references too) and redraws RAND; DNA resumes in place. Values never differ. Decide after R2/R3 whether
  66 | to model Excel's restart (and its calc-chain order) as a named mode or keep it as a documented difference.
  67 | 
  68 | ## Later, bold (not scheduled)
  69 | Certified trace segments; a portable semantic execution format; GPU execution of proven pure regions. Each needs
  70 | R3 (summaries), R4 (verified plans) and R6 (explicit frames) first.
````

### XLL owner lane and scheduler direction [DESIGN]

Source: `docs/architecture/XLL_COM_HOSTING.md:199` (lines 199–218, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/XLL_COM_HOSTING.md#L199-L218).

````text
 199 | ## 5. Threading and the scheduler
 200 | 
 201 | Excel's rules [V MTR page]: one main thread runs xlAutoOpen, XLL and VBA commands, VBA UDFs, XLL functions not marked
 202 | `$`, macro-sheet-equivalent `#` functions, COM add-in calls, CF formulas and **functions inside name definitions**;
 203 | other threads run only thread-safe cells. Thread-safe XLL functions may call `xlCoerce` (not on uncalculated cells),
 204 | `xlFree`, `xlStack`, `xlSheetId`, `xlSheetNm`, `xlAbort`, `xlGetInst`, `xlGetHwnd`, binary names; XLM information
 205 | functions fail `xlretFailed`, `xlfSetName`/unsafe `xlUDF`/unsafe `xlfEvaluate` fail `xlretNotThreadSafe`; no worksheet
 206 | function may call commands. A non-`#` function that coerces an uncalculated cell gets `xlretUncalced` and **is called
 207 | again** once the cell is calculated; a `#` function reads the old value [V xlfRegister page]. Async: only
 208 | `xlAsyncReturn` may be called from non-calculation threads during recalculation; arguments must be deep-copied [V].
 209 | 
 210 | Implications for DNA Calc (connects to SYNTHESIS 1.3-1.4):
 211 | - **One application main lane**: VBA's owner lane, the XLL main thread, the COM STA and the thread owning `XLMAIN` must
 212 |   be the same OS thread (Excel-DNA caches `_mainNativeThreadId` and enumerates that thread's windows). It runs non-`$`
 213 |   XLL UDFs, `#` UDFs, name-definition evaluation that calls them, commands, RTD server calls, events, ribbon callbacks.
 214 | - `$` XLL UDFs run on engine pool threads; each pool thread carries the per-thread XLL context and the permitted
 215 |   callback set. Return values are copied before the next call into the module on that thread (xlw's pool).
 216 | - **Uncalced retry is Excel semantics**: a non-`#` UDF that reads an uncalculated cell through `xlCoerce` aborts and is
 217 |   re-invoked later. Under SYNTHESIS 1.3 that is a **new `InvocationId`** in the same pass, not a scheduler retry, and
 218 |   its side effects can happen twice, as in Excel. It discovers a dependency at run time; the engine records it as a
````

### Measured status-64 and RTD lifecycle boundaries [MEASURED-EXCEL]

Source: `docs/architecture/XLL_COM_HOSTING.md:1263` (lines 1263–1297, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/XLL_COM_HOSTING.md#L1263-L1297).

````text
1263 | ## 30. Status 64 batch x17: xlretUncalced, discarded attempts, `#` and value arguments (dc-6332.11)
1264 | 
1265 | Answer 13's D.1 and D.2 with the X1 fixture's opt-in probes (`calc-xll-fixture/src/fixture/
1266 | uncalced.rs`, marker `uncalced-tests.enable`), `fixture/x17-uncalced-cases.json` (28 cases with
1267 | Excel's answers) and evidence in `crates/calc-xll-reference/traces/2026-11-11-x17-uncalced`.
1268 | Each group calculates `Range("B3,C3")` with the reader B3 before its producer C3 (P03's
1269 | reversed-area order). Two Excel runs agreed on all 28 rows. Measured in Excel 16.0.20527.20150:
1270 | 
1271 | - xlCoerce of a valid reference whose producer is not calculated yet returns 64
1272 |   (`xlretUncalced`) and writes #VALUE! into the result operand. Every later callback of that
1273 |   attempt (a second xlCoerce, xlfCaller) returns 64 and leaves its result operand untouched.
1274 | - The attempt's return is discarded, but a DLLFree result still goes to xlAutoFree12. After the
1275 |   producer the procedure is entered again as a new entry and reads the new value.
1276 | - xlUDF from a parent whose child gets 64 returns 64 with #VALUE!; both are entered again.
1277 | - `QU#` reads the producer's previous value with status 0, in one entry.
1278 | - `QQ`/`QQ$` value arguments are prepared before entry: one entry, after the producer; so is
1279 |   `QUQ` with the cell as both arguments.
1280 | - Range.Calculate enters `QU$` on the main thread, the 64 attempt and the retry alike.
1281 | 
1282 | Host and engine: the engine-neutral seam has a typed read (`UserFunctionContext::read_reference`,
1283 | `read_argument` returning `ReferenceRead::{Value, Pending}`). A registration whose policy is
1284 | `XllUncalcedUnwind` (ordinary and `$` XLL worksheet functions; not `#`, not async) gets `Pending`
1285 | from CalcFml for an admitted, unfinished producer; the read requests the reevaluation and stores
1286 | no refusal. The XLL host answers it as measured above (`frame.uncalced` latch, xlFree still
1287 | served), discards the return (`UNCALCED_RETURN`) after running xlAutoFree12 for a DLLFree
1288 | operand, and does not enter the procedure when a value argument is pending. The engine ends the
1289 | attempt at the call's return and evaluates the formula again after the producer. The policy
1290 | keeps pool parallelism and early execution (callable.rs `keeps_measured_concurrency`; only
1291 | `ProvisionalArguments` makes a callable serial), cached readers may be entered before a direct
1292 | producer in range order, and a range calculation enters native calls on the owner thread.
1293 | DNA Calc's engine matches all 28 rows.
1294 | - Excel-DNA 1.9.0 (`fixture/X17Uncalced.csproj`): `XlCall.TryExcel(xlCoerce)` returns 64; the
1295 |   throwing `XlCall.Excel` raises XlCallException with xlReturn 64, and a function that catches
1296 |   it and returns -999 still gets a fresh entry; -999 is never the cell's value. The engine
1297 |   matches all 12 rows.
````

### Native and wasm execution feature gates [CODE]

Source: `crates/calc-engine/src/recalc.rs:1917` (lines 1917–1948, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L1917-L1948).

````text
1917 |     {
1918 |         work.failure = Some(RecalcFailure::Superseded {
1919 |             revision: work.revision,
1920 |         });
1921 |     }
1922 |     work.local.quantum = if budget == WorkBudget::UNLIMITED {
1923 |         usize::MAX
1924 |     } else {
1925 |         1024
1926 |     };
1927 |     #[cfg(not(target_arch = "wasm32"))]
1928 |     if budget == WorkBudget::UNLIMITED && parallel::eligible(state, inputs, work, &*observer) {
1929 |         parallel::evaluate_levels(state, inputs, work, &mut *observer);
1930 |     }
1931 |     let mut evaluations = 0;
1932 |     while work.failure.is_none() && evaluations < budget.evaluations {
1933 |         if work.publication_fences
1934 |             && work.has_fences
1935 |             && work.segment_scheduling
1936 |             && work.waves == 0
1937 |             && let Some(segment) = active_segment(work)
1938 |             && work.progress.staged_results().any(|(node, result)| {
1939 |                 work.segments.get(&node).is_some_and(|own| *own < segment)
1940 |                     && work.early_published.get(&node) != Some(&result.stamp().cloned())
1941 |             })
1942 |         {
1943 |             work.fence_flush = Some(segment);
1944 |             break;
1945 |         }
1946 |         let Some(node) = next_node(state, inputs, work) else {
1947 |             if queue_certificate_repairs(state, inputs, work) {
1948 |                 continue;
````

### Parallel-wave eligibility and levels [CODE]

Source: `crates/calc-engine/src/recalc.rs:2319` (lines 2319–2344, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L2319-L2344).

````text
2319 |     /// How many formulas a thread takes from a level at a time.
2320 |     const CHUNK: usize = 32;
2321 | 
2322 |     pub(super) fn eligible(
2323 |         state: &EngineState,
2324 |         inputs: &ModelRoot,
2325 |         work: &RecalcWork,
2326 |         observer: &dyn ReadObserver,
2327 |     ) -> bool {
2328 |         state.threads > 1
2329 |             && work.interest.is_empty()
2330 |             && !work.has_fences
2331 |             && work.suspended.is_empty()
2332 |             && work.local.activation.is_none()
2333 |             && work.local.placement.is_none()
2334 |             && observer.is_silent()
2335 |             && work.failure.is_none()
2336 |             && work.next == 0
2337 |             && work.waves == 0
2338 |             && work.ready.is_empty()
2339 |             && work.waits_on.is_empty()
2340 |             && work.order.len() >= state.parallel_minimum.wave
2341 |             && !work.iteration.enabled()
2342 |             && work.cyclic.is_empty()
2343 |             && work.spills.is_empty()
2344 |             && inputs.definitions().data_tables().is_empty()
````

### Worker-local execution and deterministic result slots [CODE]

Source: `crates/calc-engine/src/recalc.rs:2554` (lines 2554–2607, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L2554-L2607).

````text
2554 |         let attempts: Vec<OnceLock<Result<Attempt, RecalcFailure>>> =
2555 |             positions.iter().map(|_| OnceLock::new()).collect();
2556 |         let worker = |local: &mut EvalLocal| {
2557 |             let mut silent = Unobserved;
2558 |             loop {
2559 |                 let from = cursor.fetch_add(CHUNK, Ordering::Relaxed);
2560 |                 if from >= positions.len() {
2561 |                     break;
2562 |                 }
2563 |                 for index in from..(from + CHUNK).min(positions.len()) {
2564 |                     let node = order[positions[index]];
2565 |                     let attempt = evaluate_with(
2566 |                         state,
2567 |                         inputs,
2568 |                         &shared,
2569 |                         local,
2570 |                         node,
2571 |                         None,
2572 |                         None,
2573 |                         None,
2574 |                         &mut silent,
2575 |                     );
2576 |                     // Unfinished speculative work is replayed by process on the owner.
2577 |                     // Its continuation belongs to this node, never the worker's next one.
2578 |                     if !matches!(&attempt, Ok(Attempt::Done(_))) {
2579 |                         *local = EvalLocal::default();
2580 |                     } else {
2581 |                         debug_assert!(local.activation.is_none() && local.placement.is_none());
2582 |                     }
2583 |                     if attempts[index].set(attempt).is_err() {
2584 |                         unreachable!("each formula of a level is taken once");
2585 |                     }
2586 |                 }
2587 |             }
2588 |         };
2589 |         std::thread::scope(|scope| {
2590 |             // Thread-bound callable activations stay on their owner. Pure workers create
2591 |             // their own scratch on entry; no activation is transferred across threads.
2592 |             for _ in 1..locals.len() {
2593 |                 let worker = &worker;
2594 |                 scope.spawn(move || {
2595 |                     let mut local = EvalLocal::default();
2596 |                     worker(&mut local);
2597 |                 });
2598 |             }
2599 |             let own = locals.first_mut().expect("at least one thread");
2600 |             worker(own);
2601 |         });
2602 |         attempts
2603 |     }
2604 | }
2605 | 
2606 | /// Install interested scalars, or the completed prefix before an observation fence.
2607 | /// Prefix spills install values, ownership and removals in the same publication unit.
````

### Concurrent callback wait restriction [CODE]

Source: `crates/calc-engine/src/recalc.rs:2193` (lines 2193–2221, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L2193-L2221).

````text
2193 |         Ok(Attempt::Waits { on, cell }) => {
2194 |             // A prerequisite before callback entry retires the provisional
2195 |             // position too: dynamically discovered SCCs require unique members.
2196 |             finish_preliminary(work, node);
2197 |             if work.parallel_scheduling && parallel_limit(state, node).is_some() {
2198 |                 // Static prerequisites were checked before entry. A callback's
2199 |                 // additional prerequisite cannot consume an unreserved worker,
2200 |                 // be retried as the same invocation, or be labelled circular.
2201 |                 work.local.activation.take();
2202 |                 let done = refused(
2203 |                     work,
2204 |                     NotModeled::Evaluation(EvalFault::NotModelled {
2205 |                         construct: "a dynamic prerequisite inside a concurrent scalar callback",
2206 |                         owner: "dc-snz.23",
2207 |                     }),
2208 |                 );
2209 |                 stage(state, inputs, work, node, done);
2210 |                 return;
2211 |             }
2212 |             observer.deferred(live_anchor(state, node), cell);
2213 |             defer(state, inputs, work, node, on);
2214 |         }
2215 |         Err(failure) => work.failure = Some(failure),
2216 |     }
2217 | }
2218 | 
2219 | /// Retains an executed reader's historical result and releases its pure waiters.
2220 | /// Refusal does not establish SCC membership or satisfy a target requirement.
2221 | fn exclude_range_reader(state: &EngineState, work: &mut RecalcWork, node: NodeId) {
````

### Async acceptance and stale result checks [CODE]

Source: `crates/calc-engine/src/recalc.rs:6513` (lines 6513–6568, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L6513-L6568).

````text
6513 |     pub fn accept_async(
6514 |         &mut self,
6515 |         state: &EngineState,
6516 |         inputs: &ModelRoot,
6517 |         token: &AsyncToken,
6518 |         value: CalcValue,
6519 |     ) -> Result<(), AsyncRejection> {
6520 |         let node = token.target.node;
6521 |         let held = self
6522 |             .async_waits
6523 |             .get(&node)
6524 |             .ok_or(AsyncRejection::UnknownToken)?;
6525 |         if held != token {
6526 |             return Err(AsyncRejection::IdentityMismatch);
6527 |         }
6528 |         if self.failure.is_some()
6529 |             || !self.live_inputs_current(state)
6530 |             || self.generation != state.generation
6531 |             || self.revision != inputs.revision()
6532 |             || self.revision != state.revision()
6533 |             || token.invocation.pass.command != self.admission.command
6534 |         {
6535 |             return Err(AsyncRejection::Revoked);
6536 |         }
6537 |         if token.target.workbook != state.contract.workbook
6538 |             || token.target.formula_generation != state.contract.generation(node)
6539 |             || state
6540 |                 .node(node)
6541 |                 .is_none_or(|n| n.anchor() != token.target.locus)
6542 |         {
6543 |             return Err(AsyncRejection::TargetChanged);
6544 |         }
6545 |         if !self.callables_current(state)
6546 |             || self
6547 |                 .functions
6548 |                 .as_ref()
6549 |                 .and_then(|f| f.resolve(token.provider))
6550 |                 .is_none()
6551 |         {
6552 |             return Err(AsyncRejection::ProviderChanged);
6553 |         }
6554 |         if state.contract.requirement(node).required != token.required
6555 |             || self
6556 |                 .requirements
6557 |                 .get(&node)
6558 |                 .is_none_or(|r| r.required != token.required)
6559 |         {
6560 |             return Err(AsyncRejection::RequirementsChanged);
6561 |         }
6562 |         let local = self
6563 |             .suspended
6564 |             .get_mut(&node)
6565 |             .ok_or(AsyncRejection::ContinuationUnavailable)?;
6566 |         if state.environment != self.environment.map(Into::into)
6567 |             || local.read_stamps.iter().any(|read| {
6568 |                 span_versions_at(state, &self.progress, read.region, Some(&self.requirements))
````

### Native XLL thread pool lifecycle [CODE]

Source: `crates/calc-xll-host/src/worker.rs:1` (lines 1–64, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/worker.rs#L1-L64).

````text
   1 | //! Only owned values and request messages cross this boundary. The evaluator and its
   2 | //! borrowed reader remain on the main lane; native calls and their frees use one worker.
   3 | use crate::native::{Module, Registration, State, XllFunction};
   4 | use calc_func::UserFunctionContext;
   5 | use calc_value::{CalcValue, CellKey, ReferenceLike};
   6 | use std::{
   7 |     cell::{Cell, RefCell},
   8 |     collections::BTreeMap,
   9 |     rc::Rc,
  10 |     sync::{
  11 |         Arc, Mutex,
  12 |         atomic::Ordering,
  13 |         mpsc::{self, Receiver, Sender, TryRecvError},
  14 |     },
  15 |     thread::JoinHandle,
  16 | };
  17 | 
  18 | type Job = Box<dyn FnOnce() + Send>;
  19 | pub(crate) struct Pool {
  20 |     sender: Option<Sender<Job>>,
  21 |     threads: Vec<JoinHandle<()>>,
  22 | }
  23 | impl Pool {
  24 |     pub fn capacity(&self) -> usize {
  25 |         self.threads.len()
  26 |     }
  27 |     pub fn new() -> Self {
  28 |         let (sender, receiver) = mpsc::channel::<Job>();
  29 |         let receiver = Arc::new(Mutex::new(receiver));
  30 |         let count = std::thread::available_parallelism()
  31 |             .map(usize::from)
  32 |             .unwrap_or(1)
  33 |             .clamp(1, 4);
  34 |         let threads = (0..count)
  35 |             .map(|index| {
  36 |                 let receiver = receiver.clone();
  37 |                 std::thread::Builder::new()
  38 |                     .name(format!("xll-calculation-{index}"))
  39 |                     .spawn(move || {
  40 |                         loop {
  41 |                             let job = receiver.lock().unwrap().recv();
  42 |                             match job {
  43 |                                 Ok(job) => job(),
  44 |                                 Err(_) => break,
  45 |                             }
  46 |                         }
  47 |                     })
  48 |                     .expect("create native calculation worker")
  49 |             })
  50 |             .collect();
  51 |         Self {
  52 |             sender: Some(sender),
  53 |             threads,
  54 |         }
  55 |     }
  56 | }
  57 | impl Drop for Pool {
  58 |     fn drop(&mut self) {
  59 |         self.sender.take();
  60 |         for thread in self.threads.drain(..) {
  61 |             let _ = thread.join();
  62 |         }
  63 |     }
  64 | }
````

### Pool context and worker dispatch [CODE]

Source: `crates/calc-xll-host/src/worker.rs:168` (lines 168–213, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/worker.rs#L168-L213).

````text
 168 | pub(crate) fn submit(
 169 |     function: &XllFunction,
 170 |     pool: &Pool,
 171 |     arguments: &[CalcValue],
 172 |     caller: Option<CellKey>,
 173 |     has_context: bool,
 174 | ) -> Result<PoolCall, String> {
 175 |     let (sender, receiver) = mpsc::channel();
 176 |     let info = function.registration.info.clone();
 177 |     let uses = function.registration.uses.get();
 178 |     let (sheets, sheet_names, active_sheet, hwnd, closed, main_thread, allocations) = {
 179 |         let state = function.state.borrow();
 180 |         (
 181 |             state.sheets.clone(),
 182 |             state.sheet_names.clone(),
 183 |             state.active_sheet,
 184 |             state.hwnd,
 185 |             state.closed,
 186 |             state.thread,
 187 |             state.allocations.clone(),
 188 |         )
 189 |     };
 190 |     if main_thread != unsafe { crate::native::GetCurrentThreadId() } {
 191 |         return Err("dc-snz.18: invocation submitted outside owning main lane".into());
 192 |     }
 193 |     let arguments = arguments.to_vec();
 194 |     // The owner keeps both native modules resident from enqueue through the
 195 |     // physical return, including when the evaluator drops its activation.
 196 |     let mut pending = crate::callbacks::owned_pending(&function.state, &function.registration)?;
 197 |     pending.completion.forwarded.store(true, Ordering::Release);
 198 |     let completion = pending.completion.clone();
 199 |     pool.sender
 200 |         .as_ref()
 201 |         .unwrap()
 202 |         .send(Box::new(move || {
 203 |             let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
 204 |                 if !completion.active.load(Ordering::Acquire) {
 205 |                     return Err("dc-snz.23: canceled before native worker entry".into());
 206 |                 }
 207 |                 // Acquire separate Windows module leases on this thread. No Rc, RefCell,
 208 |                 // arena pointer, libffi interface or evaluator borrow was sent from the STA.
 209 |                 let module = Module::loaded(info.module.to_string_lossy().as_ref())?;
 210 |                 let origin = Module::loaded(info.origin.to_string_lossy().as_ref())?;
 211 |                 let state = Rc::new(RefCell::new(State {
 212 |                     modules: Vec::new(),
 213 |                     registrations: BTreeMap::new(),
````

### XLL invocation frame, return and cleanup [CODE]

Source: `crates/calc-xll-host/src/invoke.rs:298` (lines 298–326, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/invoke.rs#L298-L326).

````text
 298 |                 pending.entered = true;
 299 |             }
 300 |             cif.call_return_into(CodePtr(r.info.address as *mut _), &ffi_arguments, output);
 301 |         }
 302 |         let mut auto_free = None;
 303 |         let mut result = if let Some(pending) = pending {
 304 |             Ok(NativeReturn::Pending(pending))
 305 |         } else if frame.uncalced {
 306 |             // A callback answered status 64: this attempt's return is discarded, never converted
 307 |             // as a value (B.5 step 5). Only the measured cleanup runs: an operand the host
 308 |             // allocated (xlbitXLFree) is released, and a DLLFree operand goes back to the add-in's
 309 |             // xlAutoFree12, as Excel 16.0.20527 does after 64 (x17, D.2).
 310 |             if matches!(signature.result, XllType::Oper12 | XllType::Oper12Reference)
 311 |                 && returned.word != 0
 312 |             {
 313 |                 let pointer = returned.word as *mut XlOper12;
 314 |                 let flags = unsafe { pointer.read_unaligned().xltype };
 315 |                 if flags & DLL_FREE != 0 {
 316 |                     auto_free = r
 317 |                         .module
 318 |                         .address("xlAutoFree12")
 319 |                         .ok()
 320 |                         .map(|address| (address, pointer));
 321 |                 } else if flags & XL_FREE != 0 {
 322 |                     callbacks::free(&self.state, &unsafe { pointer.read_unaligned() });
 323 |                 }
 324 |             }
 325 |             Err(UNCALCED_RETURN.into())
 326 |         } else {
````

### Callback thread routing and status latch [CODE]

Source: `crates/calc-xll-host/src/callbacks.rs:305` (lines 305–337, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/callbacks.rs#L305-L337).

````text
 305 |             return async_return(args, result);
 306 |         }
 307 |         FRAME.with(|active| {
 308 |             let ptr = active.get();
 309 |             if ptr.is_null() {
 310 |                 return FAILED;
 311 |             }
 312 |             let frame = &mut *ptr;
 313 |             if frame.kind == FrameKind::FreeOnly && function != XL_FREE_FN {
 314 |                 // Excel 16.0.20527 terminated when xlAutoFree12 called xlStack, xlGetName,
 315 |                 // xlfCaller and SUM (x13 batch, MyFreeFrame(2)); xlFree there returns 0. The host
 316 |                 // cannot crash in Excel's place: it fails the callback and refuses the formula.
 317 |                 return CallbackReply::refused(
 318 |                     FAILED,
 319 |                     format!("dc-6332.13: callback {function} from xlAutoFree12 terminates Excel 16.0.20527"),
 320 |                 )
 321 |                 .apply(Some(frame), result);
 322 |             }
 323 |             if frame.uncalced && function != XL_FREE_FN {
 324 |                 // After status 64 an add-in may only free and return (Microsoft's xlretUncalced
 325 |                 // contract). Excel 16.0.20527 answers anything else with 64 again and leaves the
 326 |                 // result operand untouched (x17 MyUcAgain: a second xlCoerce and xlfCaller).
 327 |                 return CallbackReply::status(UNCALCED).apply(Some(frame), result);
 328 |             }
 329 |             if matches!(function, XLF_CALL | XLF_REGISTER_ID) {
 330 |                 return crate::call::dispatch(frame, function, args, result);
 331 |             }
 332 |             let reply = answer(frame, function, args, result.is_null());
 333 |             reply.apply(Some(frame), result)
 334 |         })
 335 |     }))
 336 |     .unwrap_or(FAILED)
 337 | }
````

### Async callback thread admission [CODE]

Source: `crates/calc-xll-host/src/callbacks.rs:525` (lines 525–556, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-xll-host/src/callbacks.rs#L525-L556).

````text
 525 | unsafe fn async_return(args: &[*mut XlOper12], result: *mut XlOper12) -> i32 {
 526 |     if args.len() != 2 {
 527 |         return INVALID_COUNT;
 528 |     }
 529 |     let handle = unsafe { args[0].read_unaligned() };
 530 |     if handle.xltype & TYPE_MASK != XL_BIG_DATA {
 531 |         return INVALID_ASYNC;
 532 |     }
 533 |     let key = unsafe { handle.value.big_data.data as usize };
 534 |     let completion = asynchronous().lock().unwrap().get(&key).cloned();
 535 |     let Some(completion) = completion else {
 536 |         return INVALID_ASYNC;
 537 |     };
 538 |     let mut lock = completion.result.lock().unwrap();
 539 |     completion.physical_returned.store(true, Ordering::Release);
 540 |     if lock.is_some() || !completion.active.load(Ordering::Acquire) {
 541 |         return INVALID_ASYNC;
 542 |     }
 543 |     // Async completions are values, never references to another thread's sheet context.
 544 |     // A revoked handle never dereferences the supplied result operand.
 545 |     let value = unsafe { from_oper(args[1], &SheetMap::default(), None) };
 546 |     *lock = Some(value);
 547 |     completion.ready.notify_one();
 548 |     // Excel20527 clears all result bytes on successful completion, including
 549 |     // an operand initially containing a number.
 550 |     let reply = CallbackReply {
 551 |         status: SUCCESS,
 552 |         write: ResultWrite::ZeroAll,
 553 |         refusal: None,
 554 |     };
 555 |     unsafe { reply.apply(None, result) }
 556 | }
````

### RTD client owner and poll routes [CODE]

Source: `crates/calc-com-host/src/rtd.rs:150` (lines 150–188, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-com-host/src/rtd.rs#L150-L188).

````text
 150 | pub struct RtdClient {
 151 |     state: RefCell<State>,
 152 | }
 153 | impl RtdClient {
 154 |     pub fn new() -> Result<Self, String> {
 155 |         let pointer = unsafe { dna_rtd_create() };
 156 |         if pointer.is_null() {
 157 |             return Err("dc-snz.23: RTD allocation failed".into());
 158 |         }
 159 |         Ok(Self {
 160 |             state: RefCell::new(State {
 161 |                 pointer,
 162 |                 topics: BTreeMap::new(),
 163 |                 by_key: BTreeMap::new(),
 164 |                 pending_disconnects: Vec::new(),
 165 |                 declarations: BTreeMap::new(),
 166 |                 connector_reads: BTreeMap::new(),
 167 |                 replacement_calls: Vec::new(),
 168 |                 renewed_inputs: BTreeMap::new(),
 169 |                 next: 1,
 170 |                 next_transport: 1,
 171 |                 transports: BTreeMap::new(),
 172 |                 updates: Vec::new(),
 173 |                 initial_values: BTreeMap::new(),
 174 |                 closed: false,
 175 |                 failure: None,
 176 |                 close_failure: None,
 177 |                 activated: false,
 178 |             }),
 179 |         })
 180 |     }
 181 |     /// Owner polls between activations. Inputs still require CalcHost.update_rtd admission.
 182 |     pub fn poll(&self) -> Result<Vec<RtdInput>, String> {
 183 |         self.poll_inputs(true)
 184 |     }
 185 |     /// A calculation can poll signals without destroying a completed native cache
 186 |     /// before its consumer activation has read the newly admitted input.
 187 |     pub fn poll_during_evaluation(&self) -> Result<Vec<RtdInput>, String> {
 188 |         self.poll_inputs(false)
````

### Host cancellation, async and update entry points [CODE]

Source: `crates/calc-host/src/session.rs:568` (lines 568–594, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/session.rs#L568-L594).

````text
 568 |     pub fn complete_async(
 569 |         &mut self,
 570 |         token: &calc_engine::contract::AsyncToken,
 571 |         value: calc_doc_model::value::CalcValue,
 572 |     ) -> calc_engine::contract::AsyncDisposition {
 573 |         self.host.complete_async(token, value)
 574 |     }
 575 | 
 576 |     pub fn cancel_calculation(&mut self) {
 577 |         self.host.revoke_calculation_handles();
 578 |         if let Some(mut work) = self.host.work.take() {
 579 |             self.host.record_calc_events(work.take_events());
 580 |             let receipt = work.completion(
 581 |                 calc_engine::contract::CompletionOutcome::Cancelled,
 582 |                 self.host.engine.result_view(),
 583 |                 None,
 584 |             );
 585 |             self.host
 586 |                 .record_calc_events(vec![calc_engine::CalcEvent::Completed(receipt)]);
 587 |         }
 588 |         self.host.calculate_requested = false;
 589 |         self.host.full_calculate_requested = false;
 590 |         self.host.requested_calculation = None;
 591 |         self.host.transformed_calculation_scope = None;
 592 |         self.host.calculation_source = None;
 593 |         self.host.calculation_cancelled = true;
 594 |     }
````

### Macro execution levels and policy resolution [CODE]

Source: `crates/calc-macro-security/src/lib.rs:27` (lines 27–85, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-macro-security/src/lib.rs#L27-L85).

````text
  27 | /// to it, CalcVBA's `PolicyMode` (dc-10un).
  28 | #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  29 | pub enum VbaPolicy {
  30 |     /// Dangerous operating-system services refused.
  31 |     Safe,
  32 |     /// Full access.
  33 |     Full,
  34 | }
  35 | 
  36 | /// Whether a workbook file's own VBA project may run.
  37 | #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
  38 | pub enum MacroSecurity {
  39 |     /// Held on open; the person enables it (a marked or network file: unblocks it) per file.
  40 |     #[default]
  41 |     Off,
  42 |     /// Runs on open with dangerous operating-system services refused.
  43 |     Safe,
  44 |     /// Runs on open with full access, whatever the file's origin.
  45 |     Unguarded,
  46 | }
  47 | 
  48 | impl MacroSecurity {
  49 |     pub const ALL: [Self; 3] = [Self::Off, Self::Safe, Self::Unguarded];
  50 | 
  51 |     /// The stored and spoken form: `off`, `safe`, `unguarded`.
  52 |     pub fn key(self) -> &'static str {
  53 |         match self {
  54 |             Self::Off => "off",
  55 |             Self::Safe => "safe",
  56 |             Self::Unguarded => "unguarded",
  57 |         }
  58 |     }
  59 | 
  60 |     /// The name the page shows.
  61 |     pub fn label(self) -> &'static str {
  62 |         match self {
  63 |             Self::Off => "Off",
  64 |             Self::Safe => "Safe",
  65 |             Self::Unguarded => "Unguarded",
  66 |         }
  67 |     }
  68 | 
  69 |     /// The level a stored or typed key names (case and surrounding space ignored).
  70 |     pub fn parse(text: &str) -> Option<Self> {
  71 |         let text = text.trim();
  72 |         Self::ALL
  73 |             .into_iter()
  74 |             .find(|level| level.key().eq_ignore_ascii_case(text))
  75 |     }
  76 | 
  77 |     /// The policy the VBA host is asked to enforce when a project runs at this level; `None` when
  78 |     /// nothing runs on open (Off).
  79 |     pub fn vba_policy(self) -> Option<VbaPolicy> {
  80 |         match self {
  81 |             Self::Off => None,
  82 |             Self::Safe => Some(VbaPolicy::Safe),
  83 |             Self::Unguarded => Some(VbaPolicy::Full),
  84 |         }
  85 |     }
````

### Thread-local VBA policy and nesting [CODE]

Source: `crates/calc-vba/src/policy.rs:20` (lines 20–62, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-vba/src/policy.rs#L20-L62).

````text
  20 | struct Active {
  21 |     mode: PolicyMode,
  22 |     audit: AuditHandle,
  23 | }
  24 | 
  25 | thread_local! {
  26 |     static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
  27 | }
  28 | 
  29 | /// Choose the policy for projects loaded on this thread from now on. `None` removes it. The
  30 | /// audit log starts empty.
  31 | pub fn set_thread_policy(mode: Option<PolicyMode>) {
  32 |     ACTIVE.with(|active| {
  33 |         *active.borrow_mut() = mode.map(|mode| Active {
  34 |             mode,
  35 |             audit: AuditHandle::default(),
  36 |         });
  37 |     });
  38 | }
  39 | 
  40 | /// Temporarily select a project's policy, preserving the audit and any stricter outer Safe.
  41 | /// The previous policy is restored even when compilation unwinds.
  42 | pub struct PolicyScope(Option<Active>);
  43 | impl PolicyScope {
  44 |     pub fn enter(mode: PolicyMode) -> Self {
  45 |         ACTIVE.with(|active| {
  46 |             let previous = active.borrow_mut().take();
  47 |             let mode = if previous
  48 |                 .as_ref()
  49 |                 .is_some_and(|p| p.mode == PolicyMode::Safe)
  50 |             {
  51 |                 PolicyMode::Safe
  52 |             } else {
  53 |                 mode
  54 |             };
  55 |             let audit = previous
  56 |                 .as_ref()
  57 |                 .map(|p| p.audit.clone())
  58 |                 .unwrap_or_default();
  59 |             *active.borrow_mut() = Some(Active { mode, audit });
  60 |             Self(previous)
  61 |         })
  62 |     }
````

### Workbook project grant enforcement [CODE]

Source: `crates/calc-host-vba/src/trust.rs:45` (lines 45–85, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host-vba/src/trust.rs#L45-L85).

````text
  45 |         package_sha256: String,
  46 |     },
  47 | }
  48 | 
  49 | impl ProjectGrant {
  50 |     /// The person enabled macros for a file of `origin`; refused for a file whose origin blocks
  51 |     /// them, whatever the person asks.
  52 |     pub fn person_enabled(origin: FileOrigin) -> Result<Self, MacroBlock> {
  53 |         match origin.block() {
  54 |             Some(block) => Err(block),
  55 |             None => Ok(Self {
  56 |                 reason: GrantReason::PersonEnabled,
  57 |                 policy: calc_macro_security::VbaPolicy::Full,
  58 |             }),
  59 |         }
  60 |     }
  61 | 
  62 |     /// The person unblocked `origin`'s file: the one route past a Mark of the Web or network
  63 |     /// block, taken only from the page's Unblock and enable button, behind its warning.
  64 |     pub fn person_unblocked(origin: FileOrigin) -> Self {
  65 |         Self {
  66 |             reason: GrantReason::PersonUnblocked(origin.block()),
  67 |             policy: calc_macro_security::VbaPolicy::Full,
  68 |         }
  69 |     }
  70 | 
  71 |     /// The macro security level Unguarded, or the flag or variable that pins it (`by` says which),
  72 |     /// trusts every file whatever its origin, with the VBA host policy `policy`.
  73 |     pub fn trust_all(by: &str, policy: calc_macro_security::VbaPolicy) -> Self {
  74 |         Self {
  75 |             reason: GrantReason::TrustAll { by: by.to_owned() },
  76 |             policy,
  77 |         }
  78 |     }
  79 | 
  80 |     /// The policy the VBA host enforces for the project this grant lets run.
  81 |     pub fn policy(&self) -> calc_macro_security::VbaPolicy {
  82 |         self.policy
  83 |     }
  84 | 
  85 |     /// An evidence run's trusted-code manifest lists the package. The caller has checked the
````

### VBA host services and native affinity [DESIGN]

Source: `CalcVBA/docs/spec/HOSTING_AND_EARLY_BINDING.md:34` (lines 34–47, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/docs/spec/HOSTING_AND_EARLY_BINDING.md#L34-L47).

````text
  34 | parameter/result/wire types and defaults. Dual calls use the retained native
  35 | vtable plan and do not retry through IDispatch. Pure dispinterfaces use an
  36 | explicit compiler-prebound Invoke plan. Object/Variant calls and untyped
  37 | members resolve dynamically. Both backends use shared marshalling and error
  38 | translation; nominal interfaces and native object results retain COM identity.
  39 | 
  40 | Host tables can supply portable typed objects without native Windows COM.
  41 | HAL host and callback traits do not require Send or Sync. Arc host ownership
  42 | remains for the DNA Calc API; it does not permit cross-thread execution.
  43 | Native interface references remain on the owning apartment. The host controls
  44 | lifecycle and supplies the root objects; it must not pass another thread's raw
  45 | COM interface to a session. The runtime executes on its owning VBA thread.
  46 | 
  47 | ## Events
````

### CalcVBA invocation leases and owner-local scope [CODE]

Source: `CalcVBA/crates/calcvba-host-api/src/invocation.rs:1` (lines 1–73, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-host-api/src/invocation.rs#L1-L73).

````text
   1 | //! Host-owned invocation identity and a resumable, owning typed result contract.
   2 | use crate::{Error, HostObject};
   3 | use std::{any::Any, rc::Rc};
   4 | 
   5 | /// The engine retains this view; only the host interprets its contents.
   6 | #[derive(Clone)]
   7 | pub struct InvocationFrame {
   8 |     pub id: u64,
   9 |     pub parent: Option<u64>,
  10 |     pub read_view: Rc<dyn Any>,
  11 | }
  12 | impl InvocationFrame {
  13 |     pub fn new(id: u64, parent: Option<u64>, read_view: Rc<dyn Any>) -> Self {
  14 |         Self {
  15 |             id,
  16 |             parent,
  17 |             read_view,
  18 |         }
  19 |     }
  20 | }
  21 | 
  22 | pub enum OwnedValue {
  23 |     Long(i32),
  24 |     Double(f64),
  25 |     Object(Option<HostObject>),
  26 | }
  27 | impl std::fmt::Debug for OwnedValue {
  28 |     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
  29 |         match self {
  30 |             Self::Long(v) => f.debug_tuple("Long").field(v).finish(),
  31 |             Self::Double(v) => f.debug_tuple("Double").field(v).finish(),
  32 |             Self::Object(v) => f
  33 |                 .debug_tuple("Object")
  34 |                 .field(&v.as_ref().map(|o| o.argument().as_ptr()))
  35 |                 .finish(),
  36 |         }
  37 |     }
  38 | }
  39 | #[derive(Debug)]
  40 | pub enum InvocationProgress {
  41 |     Suspended,
  42 |     Returned(OwnedValue),
  43 | }
  44 | /// Same-thread, single-use continuation. A slice pauses only between completed
  45 | /// instructions; callbacks are synchronous and cannot be interrupted mid-effect.
  46 | /// Dropping the invocation cancels it and releases its local owners and leases.
  47 | pub trait Invocation {
  48 |     fn frame(&self) -> &InvocationFrame;
  49 |     fn remaining_blocks(&self) -> u64;
  50 |     fn resume(&mut self, operations: u64) -> Result<InvocationProgress, Error>;
  51 | }
  52 | 
  53 | thread_local! {
  54 |     static ACTIVE_FRAME: std::cell::RefCell<Option<Rc<InvocationFrame>>> = const { std::cell::RefCell::new(None) };
  55 | }
  56 | /// Callback-scoped metadata. Retaining it retains the host's read view, not an
  57 | /// engine stack pointer. Nested synchronous calls inherit the active invocation.
  58 | pub fn active_invocation() -> Option<Rc<InvocationFrame>> {
  59 |     ACTIVE_FRAME.with(|slot| slot.borrow().clone())
  60 | }
  61 | pub struct InvocationScope(Option<Rc<InvocationFrame>>);
  62 | impl InvocationScope {
  63 |     pub fn enter(frame: Rc<InvocationFrame>) -> Self {
  64 |         Self(ACTIVE_FRAME.with(|slot| slot.replace(Some(frame))))
  65 |     }
  66 | }
  67 | impl Drop for InvocationScope {
  68 |     fn drop(&mut self) {
  69 |         ACTIVE_FRAME.with(|slot| {
  70 |             slot.replace(self.0.take());
  71 |         });
  72 |     }
  73 | }
````

### Safe/Full capabilities are gates, not a sandbox [CODE]

Source: `CalcVBA/crates/calcvba-hal/src/capability.rs:1` (lines 1–24, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-hal/src/capability.rs#L1-L24).

````text
   1 | //! Capability policy for the host layer.
   2 | //!
   3 | //! [`PolicyHost`] wraps any [`HostServices`] and gates the operations that reach the
   4 | //! operating system (files, processes, environment, settings, `Declare`, COM activation,
   5 | //! `SendKeys`/`AppActivate`) behind eight [`Capability`] values. Both execution backends
   6 | //! reach those services only through the host they are given, so one wrapper covers VM3 and
   7 | //! the JIT. In Safe mode a denied operation raises the nearest real VBA run-time error
   8 | //! (70, 53 or 429) with its own `Err.Description`, and the [`HalError`] keeps the stable code
   9 | //! `HAL-E-POLICY-DENIED` so a harness can tell a policy denial from the real error. Every
  10 | //! request, allowed or denied, goes into a bounded audit log that the harness drains per case.
  11 | //!
  12 | //! This is a policy gate, not a sandbox. Design record:
  13 | //! `docs/design/capability-policy-review-2026-11-11.md`.
  14 | //!
  15 | //! Gated and logged: `Open` (read or write by mode), `Kill`/`MkDir`/`RmDir`/`Name`/`FileCopy`/
  16 | //! `SetAttr`/`ChDir`/`ChDrive`, `Dir`/`FileLen`/`GetAttr`/`FileDateTime`/`CurDir`, `Shell`,
  17 | //! `Environ`/`Command`, the four settings functions, every `Declare` call, `SendKeys`/
  18 | //! `AppActivate`, and COM activation (`CreateObject`, `GetObject`, `New` of an external class)
  19 | //! through [`HostServices::authorize_com_activation`]. Handle operations (`Print #`, `Get`,
  20 | //! `EOF`), `MsgBox`/`InputBox`, `Now`/`Timer`, `Debug.Print` and `DoEvents` pass through
  21 | //! unlogged: after a denied open no handle exists. Native type-library loading and COM member
  22 | //! dispatch are also gated while the owning host is compiling or executing.
  23 | 
  24 | use std::cell::RefCell;
````

### Capability policy constructors and local ownership [CODE]

Source: `CalcVBA/crates/calcvba-hal/src/capability.rs:379` (lines 379–406, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-hal/src/capability.rs#L379-L406).

````text
 379 | /// The policy choice an application passes: Safe denies and audits every capability, Full
 380 | /// passes everything through (still audited). "Not at all" is the embedder's choice not to run.
 381 | #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
 382 | pub enum PolicyMode {
 383 |     #[default]
 384 |     Safe,
 385 |     Full,
 386 | }
 387 | 
 388 | impl PolicyMode {
 389 |     pub const fn name(self) -> &'static str {
 390 |         match self {
 391 |             PolicyMode::Safe => "safe",
 392 |             PolicyMode::Full => "full",
 393 |         }
 394 |     }
 395 |     pub fn from_name(name: &str) -> Option<Self> {
 396 |         match name {
 397 |             "safe" => Some(PolicyMode::Safe),
 398 |             "full" => Some(PolicyMode::Full),
 399 |             _ => None,
 400 |         }
 401 |     }
 402 | }
 403 | 
 404 | /// Wraps a [`HostServices`] with the capability policy and the audit log.
 405 | pub struct PolicyHost {
 406 |     inner: Arc<dyn HostServices>,
````

### Browser worker handoff [CODE]

Source: `apps/dna-calc-worker/src/worker.rs:123` (lines 123–170, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-worker/src/worker.rs#L123-L170).

````text
 123 |     }
 124 |     schedule_calculation();
 125 | }
 126 | 
 127 | /// A timer queues a worker task. A chain of resolved promises would keep input behind the
 128 | /// calculation's microtasks and would not provide this handoff.
 129 | #[cfg(target_arch = "wasm32")]
 130 | fn schedule_calculation() {
 131 |     use wasm_bindgen::{JsCast, prelude::Closure};
 132 |     let pending = SERVER.with(|server| {
 133 |         server
 134 |             .borrow()
 135 |             .as_ref()
 136 |             .is_some_and(|server| server.has_pending_calculation())
 137 |     });
 138 |     if !pending || DRIVE_PENDING.with(|scheduled| scheduled.replace(true)) {
 139 |         return;
 140 |     }
 141 |     let callback = Closure::once_into_js(|| {
 142 |         DRIVE_PENDING.with(|scheduled| scheduled.set(false));
 143 |         let events = SERVER.with(|server| {
 144 |             let mut server = server.borrow_mut();
 145 |             let Some(server) = server.as_mut() else {
 146 |                 return Vec::new();
 147 |             };
 148 |             server.advance_calculation();
 149 |             server.take_events()
 150 |         });
 151 |         for event in events {
 152 |             if !post_event(event) {
 153 |                 return;
 154 |             }
 155 |         }
 156 |         schedule_calculation();
 157 |     });
 158 |     if scope()
 159 |         .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 0)
 160 |         .is_err()
 161 |     {
 162 |         DRIVE_PENDING.with(|scheduled| scheduled.set(false));
 163 |         post(&FromServer::Failed {
 164 |             id: None,
 165 |             message: "could not schedule the calculation continuation".into(),
 166 |         });
 167 |     }
 168 | }
 169 | 
 170 | /// WEBSERVICE and IMAGE through the worker's `fetch`. A cross-origin URL answers only when its
````

### Prior add-in review: session/runtime-group surrogate, callback ownership and no replay [EARLIER-REVIEW]

Source: `docs/architecture/review-2026-11/pro/13-addin-hosting-answer.md:376` (lines 376–397, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-11/pro/13-addin-hosting-answer.md#L376-L397).

````text
 376 | ### B.8 Process isolation
 377 | 
 378 | **[inference] Yes: add process isolation, but preserve an in-process conformance mode.** Make the unit a calculation session or a compatible add-in/runtime group, not each individual DLL. Keep xlOil core, plugins, Python and their factories together; likewise an Excel-DNA XLL, its payload and generated RTD/COM infrastructure. Split groups only where their interactions can be represented faithfully.
 379 | 
 380 | Start OneCalc with one native hosting surrogate per session. For DNA Calc, consider co-locating the calculation session and add-in host in that surrogate through application composition; that avoids turning every reference read into IPC. A later multi-runtime arrangement can use the same services remotely. Neither arrangement introduces an XLL dependency into `calc-engine`.
 381 | 
 382 | The surrogate must itself provide the main-image export, private shim, STA message pump, matching hidden window hierarchy, local COM projection, stable workers, allocation stores and module lifecycle. Providing the EXCEL7 window only in the UI process does not satisfy frameworks that enumerate their own process/thread windows. **[inference]**
 383 | 
 384 | | Boundary | What stays local / what crosses | Failure of a value-only RPC design |
 385 | |---|---|---|
 386 | | Native ABI | Pointers, handles, libffi entry, export snapshots and free callbacks stay local | Calling/freeing pointers in another process is invalid |
 387 | | References | Logical workbook/sheet/areas plus invocation/read-view identity cross if needed | Eagerly converting everything to values loses lazy reads, cached observations and64 |
 388 | | Callbacks | Bidirectional nested requests carrying parent ID, sequence and permission context | A single blocking request/reply cannot service a callback during the UDF |
 389 | | Caller | Tagged caller descriptor, translated to process-local operands | A synthetic A1 caller changes results and RTD identities |
 390 | | Async | Token includes surrogate incarnation, request ID and document generation | Restart can otherwise accept an old completion as a new request |
 391 | | RTD | COM server/update object/STA together; logical topic admission sent to document owner | Duplicate topics, wrong apartments and extra native entries |
 392 | | COM | Local facades hold stable logical object IDs; permitted document operations cross | Raw IDispatch pointer forwarding is invalid; arbitrary Office interface marshalling needs its own contract |
 393 | | UI | Explicit command/dialog/ribbon/task-pane bridge | HWND subclassing, modality, UI Automation and embedded ActiveX cannot be transparently reduced to data |
 394 | 
 395 | **[inference] A timeout cancels admission; it does not stop native code.** A crash/hang supervisor may terminate the surrogate, but must not silently replay an invocation that may have performed external side effects. Keep the committed document/edit journal or a recoverable checkpoint in the controller even if the calculation session runs in the surrogate. Otherwise the crash can destroy both computation and the only document copy. Restore committed edits independently of native invocation replay. In-process SEH can improve fault reporting; continuing normal operation after arbitrary AV is not a general recovery design.
 396 | 
 397 | **[inference] Your current lease condition is insufficient for physical unloading.** The last async completion can return to add-in code that still frees resources or exits its worker thread. “Late return observed” does not prove that no instruction pointer remains in the module. Queued Windows messages, timers, class factories, COM references, event subscriptions and plugin thunks are additional leases. For modules without a proven quiescence protocol, logical deactivation plus residency until process exit is the defensible policy. Native AOT already requires that residency under the documented runtime contract. [M12]
````

### Existing multithreaded test checks serial values and certificates [CODE]

Source: `crates/calc-host/tests/threads.rs:105` (lines 105–139, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/tests/threads.rs#L105-L139).

````text
 105 |         at: at(row),
 106 |         input: interpret_entry(text).unwrap().unwrap(),
 107 |     };
 108 |     let edits: Vec<_> = (1..=70_000)
 109 |         .map(|row| {
 110 |             edit(
 111 |                 row,
 112 |                 match row {
 113 |                     2 => "=INDIRECT(\"A70000\")+1",
 114 |                     69_999 => "=INDIRECT(\"A1\")+2",
 115 |                     _ => "=ROW()+1",
 116 |                 },
 117 |             )
 118 |         })
 119 |         .collect();
 120 |     for host in [&mut one, &mut several] {
 121 |         let base = host.revision();
 122 |         dispatch(
 123 |             host,
 124 |             HostCommand {
 125 |                 base,
 126 |                 edits: edits.clone(),
 127 |             },
 128 |         )
 129 |         .unwrap();
 130 |     }
 131 |     let cells: Vec<_> = (1..=70_000).map(at).collect();
 132 |     for phase in 0..3 {
 133 |         if phase == 1 {
 134 |             calculate_full(&mut one);
 135 |             calculate_full(&mut several);
 136 |         } else if phase == 2 {
 137 |             for host in [&mut one, &mut several] {
 138 |                 let base = host.revision();
 139 |                 dispatch(
````
