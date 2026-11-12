# Question for GPT-6 Pro: dependency discovery, invalidation and Excel-compatible recalculation end to end

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

Review the **one dependency graph** from formula binding through runtime reads, invalidation, dirty closure, ordering, evaluation, publication and future edits. Structural dependencies include cells, ranges and whole axes, defined names and namespace misses/shadowing, tables/structured references, 3-D sheet spans, spills/shape/blank occupancy, LET and LAMBDA bodies. Runtime dependencies include OFFSET, INDIRECT, reference-valued INDEX/CHOOSE/names, CELL and host facts, UDF/XLL reads and volatile events. Separate structural potential dependencies, observed reads and deliberately cached macro observations. Specify the calc-fml boundary needed for each; do not redesign its evaluator internals.

Assess reverse-index granularity and complexity, dynamic-edge replacement after successful/waiting/abandoned/failed evaluation, obsolete dynamic cycles, name rebinds, changing spill footprints and fact invalidation. Explain what dirties a cell when a precedent's value stays equal, how structural references to untaken branches differ from executed reads, and how names with caller-relative references or stateful functions are represented without a second graph. Show the invariants connecting requirement/invalidation versions and result versions to read certificates.

Cover dirty marking, smart/partial calculation, ready queues and learned order/calcChain, first calculation versus steady state, visible interest versus demand, F9/Shift+F9/Ctrl+Alt+F9/full rebuild, workbook/sheet/range scope, Manual/Automatic/Automatic Except Tables and out-of-scope caches. Distinguish a completed command from a globally fresh workbook. Review SCC detection, disabled/enabled iteration, seeds, sweep order, convergence and dynamic topology; separate a real cycle from a temporarily missing producer.

Connect RTD UpdateNotify/RefreshData and topic leases, initial ConnectData, xlAsyncReturn and stale invocation tokens, ordinary/$ XLL status 64 unwind, # cached reads and value arguments, external link/data refresh and table/pivot refresh to dirtying and command admission. Give traces for one RTD batch, one async completion, one abandoned status-64 attempt and one refresh. Do not treat these as interchangeable authored cell edits or restart all formulas indiscriminately.

Use VOLATILITY's measured ordered-event model rather than assuming a sticky boolean or 'built-in executed'. Reconcile its findings with the pinned code: syntactic tokens, per-completed-evaluation events, name uses, SUMIF shape, spill obstruction, ! and # registration defaults, false requests, lifted UDF calls, dependents and file ca bits. Separate a scheduler suspension retaining progress from Excel's measured abandonment/reentry and XLL's explicit semantic reevaluation; final-value agreement alone does not prove invocation-count parity.

Ask which current data structures and algorithmic choices should change before performance work, with memory/time measurements for 1M formulas, whole-column/range fanout, name-heavy workbooks and rapidly changing dynamic targets. Experiments should discriminate branch dependency versus read dependency, dynamic-edge retirement, unchanged-value propagation, name shadowing, status-64 effects, Range.Calculate post-state and volatile bare-reference dependent quirks.

## What was already tried

Already tried: October's red-team separated execution, read visibility and publication; RECALC_CONTRACT captured the first end-to-end slice. The suspension study measured Excel restarting even static-reference formulas in some initial chain orders, while DNA retains evaluator progress. Status-64 XLL work added explicit unwind/reevaluation; RTD work adopted initial ConnectData inputs without a duplicate entry. VOLATILITY measured ordered overwrites and named open cases. The evaluator rework plan proposes evidence/summary/worker boundaries; do not assume those proposals are all implemented.

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

### Contract decisions and corrected evidence [DESIGN]

Source: `docs/architecture/RECALC_CONTRACT.md:10` (lines 10–31, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/RECALC_CONTRACT.md#L10-L31).

````text
  10 | ## 0. Decisions
  11 | 
  12 | 1. **Two receipts.** `AdmissionReceipt(K)` precedes every value K causes; `CompletionReceipt(K)` follows every
  13 |    installation K requires. S's "the command receipt precedes its values" means the admission receipt.
  14 | 2. **Fixed admitted frontier.** A calculation command pins model revision and external-input cut at admission. An edit
  15 |    creates a successor command (new pass, predecessor link); validated pure work carries over. Today a commit discards
  16 |    the whole recalculation (`calc-host/src/command.rs [historical locator omitted]`, `calc-engine/src/recalc.rs [historical locator omitted]`).
  17 | 3. **Semantic evaluation vs physical attempt.** `InvocationId` carries an evaluation ordinal. Yield and physical retry
  18 |    keep it; a new semantic evaluation needs a typed `ReevaluationReason` (2.3), including the documented XLL
  19 |    `xlretUncalced` unwind that A7 omits (A4 2.10).
  20 | 4. **Dependencies vs observations.** A certificate holds dependency reads (validated, reverse edges installed) apart from
  21 |    cached observations (validated against their pinned view, no reverse edge, no demand).
  22 | 5. **Qualified freshness only.** `FreshFor(K, T)` (3.3); no result, receipt or projection carries an unqualified
  23 |    workbook-wide "settled". Status-only publications exist.
  24 | 6. **Effects are at-most-once per InvocationId.** Execution state and result disposition are separate; cancel and
  25 |    supersede never undo or repeat an effect.
  26 | 7. **VBA cached reads read a pinned root** fixed at invocation admission; SliceV1 uses serial observation fences.
  27 | 8. **Save pins one model revision and one result view**; the cache policy is explicit. P06 selects
  28 |    `PreservePermittedCaches` for ordinary Manual Save(false). Save/SaveAs pins include admitted BeforeSave edits (8).
  29 | 9. **Strict blank reads need an absence proof**; a spill footprint installs as one unit.
  30 | 10. **No value-based early cutoff in SliceV1**; all admitted dirty or forced work is discharged.
  31 | 
````

### Read policies and certificates [DESIGN]

Source: `docs/architecture/RECALC_CONTRACT.md:495` (lines 495–521, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/RECALC_CONTRACT.md#L495-L521).

````text
 495 | ## 4. Reads and freshness
 496 | 
 497 | `RequirementState { value_version, required, satisfied }`: locally clean iff `satisfied == required`; every
 498 | invalidation advances `required` even if already dirty; a result satisfies only the version it validated. Today a node
 499 | is pending or not (`calc-engine/src/state.rs [historical locator omitted]`), so a second invalidation during a run cannot be told apart.
 500 | 
 501 | | Read | Value selected | Validation, invalidation |
 502 | |---|---|---|
 503 | | In-scope formula dependency | authored input or result certified for the required work | value, requirement, satisfaction, facets; reverse edge |
 504 | | Out-of-scope formula dependency | boundary cache pinned at pass start | historical entry validated, qualification carried; reverse edge kept |
 505 | | Cached VBA observation | cache/model at the invocation's pinned view | against that view; evidence only; no edge, no demand |
 506 | | Diagnostic snapshot | requested pin | no demand, effects or edges |
 507 | 
 508 | Constants are read from the pinned model even out of scope. `RegisteredWorksheetCallback` answers an uncalculated read
 509 | by the registration's capability (refuse, defer, or the XLL unwind of 3.3), never with VBA's privilege. A4's
 510 | `ExplicitFreshSnapshot` is not a read policy here: it is the save/export policy `RequireCalculatedScope` (8).
 511 | 
 512 | **FreshFor(K, T)** holds iff: T is in K's scope with its certified target identity; K's demand on T is met (a reusable
 513 | cache cannot meet a new force); every read obeyed K's policy and validates; every strict occupancy question in scope
 514 | is resolved; result and topology were installed atomically; the claim names K, pass, frontier and qualifications. It
 515 | does not claim precedents or the workbook current. Projections say e.g. "calculated for K7; cached input B1 dirty".
 516 | 
 517 | **Install** (single owner, no authored commit in between): check command and target authority; environment and
 518 | callable versions; dependency reads against the required view; cached reads against their pins; bindings, facts,
 519 | absences; spill reservation; requirement version still matching; then install value, status, topology and
 520 | invalidations atomically. A dependent may cite a producer proof in the same unit, else the producer must already be
 521 | installed. Failed validation installs nothing; an effectful attempt is never restarted by this path. Facts declare
````

### Volatility: instrument, ordered overwrites and token rules [MEASURED-EXCEL]

Source: `docs/architecture/VOLATILITY.md:24` (lines 24–76, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/VOLATILITY.md#L24-L76).

````text
  24 | ## 2. The rule that holds: one flag, set by the formula's token, overwritten by ordered events (F2)
  25 | 
  26 | Per evaluation of a formula cell: the flag starts at S (the formula text contains a token-writing function) and
  27 | each event overwrites it in execution order; the value after the last event is the cell's `ca` bit, and the
  28 | cell stays volatile exactly while that value is true. DNA's current rule (sticky OR of S and the last request,
  29 | F1, `recalc.rs [historical locator omitted]`) is wrong; F3 (flag set when a built-in runs) is wrong too.
  30 | 
  31 | | Evidence (P2, `p2`, `p2h`, `p6`) | F1 | F2 | F3 | Excel |
  32 | |---|---|---|---|---|
  33 | | `RAND()+VFalse()` (token, then request false) | vol | not | not | **not** |
  34 | | `VFalse()+RAND()` | vol | not | vol | **not** |
  35 | | `IF(TRUE,VFalse(),RAND())`, `IF(TRUE,RAND(),VFalse())` | vol, vol | not, vol | not, vol | not, vol |
  36 | | `NOW()+VTF()` / `NOW()+VFT()` (T,F / F,T in one call) | vol | not / vol | not / vol | not / vol |
  37 | | `VTrue()+VFalse()` / `VFalse()+VTrue()` | not / vol | not / vol | not / vol | not / vol |
  38 | | `VArgF(RAND())` (request inside a formula holding a token) | vol | not | not | not |
  39 | 
  40 | Also measured, all consistent with F2: `SUM(RAND(),VFalse())`, `VFalse()+IF(TRUE,1,RAND())`, a helper that
  41 | requests true on the caller's behalf (`VFalse()+VHelper()` vol, `VHelper()+VFalse()` not), three requests in
  42 | one formula (`T,F,T` vol; `T,F,F`, `F,T,T,F` not), and the same with XLL `#` functions (`HashFalse()+RAND()`,
  43 | `HFT()+NOW()`, `HashTrue()+VFalse()` and the reverse; `p2h`).
  44 | 
  45 | Evaluations start afresh: a function that requests true once (call 1) and nothing afterwards leaves the cell
  46 | volatile only until its next evaluation, which starts from S, sees no request and ends nonvolatile (cal1,
  47 | X1). Only a completed evaluation's events count (X4, section 6).
  48 | 
  49 | ## 3. What sets S (the token): P1
  50 | 
  51 | Token present, syntactic, regardless of branch or use: RAND, RANDBETWEEN, RANDARRAY, NOW, TODAY, OFFSET,
  52 | INDIRECT, **CELL** (all 12 info types, literal or from a cell, with or without the reference, unknown type
  53 | text, range or reference-returning arguments: `p1ca`..`p1cf`), **INFO** (all 10 types, literal or from a cell),
  54 | **FORMULATEXT, SHEET, SHEETS** (`p1a`; the catalogue does not list these three). Not volatile: ROW, COLUMN,
  55 | ROWS, COLUMNS, AREAS, INDEX in every form tried (`INDEX(r,0)`, `r:INDEX(..)`, `INDEX(..):r`), ISREF, ISFORMULA,
  56 | SUBTOTAL, AGGREGATE, N, T, ADDRESS, CHOOSE, MATCH, VLOOKUP, XLOOKUP, HYPERLINK, TEXT, date functions,
  57 | SUMPRODUCT, LOOKUP, STDEV, TRANSPOSE. WEBSERVICE was not measured (it needs the network).
  58 | 
  59 | The token covers the whole formula text, executed or not (`p1b`, 34 formulas): `IF(TRUE,1,RAND())`,
  60 | `IF(FALSE,RAND(),1)`, `CHOOSE(1,1,NOW())`, `IFERROR(1,RAND())`, `IFNA`, `IFS`, `SWITCH`, `AND(FALSE,RAND()>.5)`,
  61 | `LET(x,RAND(),1)` with `x` unused, `LET(f,LAMBDA(RAND()),1)` (lambda defined, never called),
  62 | `(LAMBDA(x,1))(RAND())`, `MAP(.., LAMBDA(x,x*RAND()))`, `IF(C1=2,TODAY(),0)*0` with a false condition. A LAMBDA
  63 | body without a volatile function is not volatile. The token is not written for `!`/`#` registered functions,
  64 | for the run-time decisions below, or through defined names (section 5).
  65 | 
  66 | Run-time decisions, no token, `ca` set while the condition holds (`p1d`, `p1e`, `p1f`):
  67 | - **SUMIF and AVERAGEIF** when `sum_range` is smaller than `range` (`D1:D2`, `D1`, `D1:D1`, a range built by
  68 |   `D1:INDEX(D:D,n)` with n < 3 against a 3-cell range). Same shape, a larger sum range (`D1:E3`), a single-cell
  69 |   range, SUMIFS, COUNTIF(S), MAXIFS, MINIFS are not volatile. Decided per evaluation from the shapes the
  70 |   executed call gets: with `n` toggled 2, 3, 2 the cell was volatile, nonvolatile, volatile again; an untaken
  71 |   branch holding a mismatched SUMIF is nonvolatile.
  72 | - **A spill blocked by another cell** (#SPILL!): `ca` set, no token (`TRANSPOSE`, `SEQUENCE`, `TEXTSPLIT` into
  73 |   an occupied range). Free spills and `INDEX(r,0)`, `SORT`, `FILTER`, `EXPAND` are not volatile.
  74 | Both behave as events at execution (section 4): `SUMIF(mismatch)+VFalse()` is not volatile,
  75 | `VFalse()+SUMIF(mismatch)` is (`p6`).
  76 | 
````

### Volatility: runtime events, names, commands and open cases [MEASURED-EXCEL]

Source: `docs/architecture/VOLATILITY.md:77` (lines 77–146, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/VOLATILITY.md#L77-L146).

````text
  77 | ## 4. Events, in execution order
  78 | 
  79 | 1. **Registration default `!`**: true, applied when the call starts. A later request overwrites it: `BangHashClear()`
  80 |    (`Q!#`, requests false on every call) and `BangHuF(x)` are nonvolatile for every argument; `Bang()+VFalse()`
  81 |    is nonvolatile, `VFalse()+Bang()` volatile (`p3`, `p2h`). Not in the token; an untaken `IF(TRUE,1,Bang())` is
  82 |    nonvolatile. It also applies to `>EX!` async registrations.
  83 | 2. **Registration default `#` with R/U parameter**: true, at call start, **only when a reference is actually passed
  84 |    in an R or U parameter** (`p4`). `Hu(C1)`, `Hu(C1:C2)`, `Hu(INDEX(..))`, `Hu(empty cell)`, `Hr(..)` are volatile;
  85 |    `Hu(1)` and `Hu(C1+0)` (a value) are not. Any R/U position counts (`Hub(C1,1)` yes, `Hub(1,C1)` no because
  86 |    position 2 is `B`; `Hbu(1,C1)` yes; `Hrr(1,C2)` yes). Q, P and B parameters never count; `Q#` with no
  87 |    parameter is nonvolatile; coercing the argument changes nothing. Same event order as 1: `HuF(C1)` (requests
  88 |    false) is nonvolatile for every argument, `HuT(1)` volatile, `HuTF(1)` not, `HuFT(1)` volatile.
  89 | 3. **Requests**: `xlfVolatile(TRUE/FALSE)` and `Application.Volatile True/False`, any number per call; a call with
  90 |    a different argument per invocation behaves per invocation. `Application.Volatile` with no argument is true.
  91 |    Helper functions' requests count for the formula's cell.
  92 | 4. **Name use** (section 5), **SUMIF shape**, **spill block**: events at execution that set true (name use: the
  93 |    name body's own flag, see 5).
  94 | No event clears a flag except a request of false. An untaken branch contributes no event.
  95 | 
  96 | **`xlfVolatile` is function 237 and Excel12 accepts it only in some contexts** (`p2x`, `p3`, `p4`, `x3`, `x5x`):
  97 | status 0 from a function registered with `#` called from a cell, and from any registration called outside a cell
  98 | (`Application.Run`, no effect on any cell); status 2 (xlretInvXlfn) from a cell call of an ordinary, `!` or `$`
  99 | registration, and from `>EX`/`>EX!`/`>EX$` entries; status 32 from the worker thread after the async return. So
 100 | `BangClear` (`Q!`, requests false) stays volatile: the request is refused, not obeyed. `$` with `#` was not
 101 | measured (DNA refuses the pair). DNA's `callbacks.rs` now answers status 2 from any non-`#` registration
 102 | (checked before the `$` rule) and 0 from a `#` cell call; lifecycle and command frames have no cell, so 0 and no
 103 | effect.
 104 | 
 105 | ## 5. Names (P5, `p5`, `p6`)
 106 | 
 107 | A defined name's body has its own token (`RAND()`, `IF(TRUE,1,RAND())`, `INDIRECT`, `LAMBDA(RAND())` yes; a body
 108 | calling a user function or `=1` no). A cell using a name gets no token from it. Executing a name use is an event:
 109 | it sets true when the body's flag is true. Hence `=NmRand` and `NmRand*0` are volatile; `IF(TRUE,1,NmRand)`,
 110 | `IF(FALSE,NmRand,1)`, `IF(TRUE,1,NmVba)` are not; a name whose body is `VFalse()` leaves the cell nonvolatile;
 111 | `NmVF+RAND()` is nonvolatile (token first, the name's request later); `VFalse()+NmRand` volatile,
 112 | `NmRand+VFalse()` not; `NmInd+VFalse()` not, `VFalse()+NmInd` volatile. **`LET(f,NmLam,1)` is volatile although `f`
 113 | is never called**: binding the name evaluates it. Redefining the names without touching the cells
 114 | (`NmRand` to `=1`, back to `=RAND()`) changed the cells' volatility at the next Calculate, and the dependents
 115 | recalculated once on the redefinition: names are late bound, the fact belongs to the body, joined at each
 116 | executed use.
 117 | 
 118 | ## 6. Persistence, commands, order, lifting, files
 119 | 
 120 | - **Calculate kinds (X1)**: a nonvolatile cell is not recalculated by Calculate or `Worksheet.Calculate`;
 121 |   `Range.Calculate` recalculates the target and leaves it dirty (target and dependents run again at the next
 122 |   Calculate); Full and Full-rebuild recalculate everything, requests are re-evaluated then (a function
 123 |   that now requests true becomes volatile only after one of these; one that now requests false ends the next
 124 |   Calculate). In Automatic mode volatile cells recalculate on an unrelated edit.
 125 | - **Entry and first calculation (X4, cal1)**: every formula is called once when entered and once more at the
 126 |   first Calculate; the events of the first Calculate decide the state (a request at that evaluation is
 127 |   honoured).
 128 | - **Abandoned attempt (X4, 3 runs)**: a reader of a three-level producer chain is evaluated twice in the first
 129 |   Calculate; a request made only in the abandoned attempt is lost, one made in the completed attempt counts.
 130 |   Only the completed evaluation's events are kept; never the abandoned one's.
 131 | - **Lifted calls (X2)**: with a Double-parameter user function lifted over a multi-cell range or array
 132 |   (`SUM(VElemD(C1:C3))`, `{1,2,3}`, 1x3 and 3x1, requests true and false mixed, all true) **no request
 133 |   is honoured: the cell is nonvolatile**; over a 1x1 reference the request is honoured; inside
 134 |   MAP/BYROW lambdas each call's request counts and elements run in ascending order, last wins (`MAP(C1:C3,..)`
 135 |   with requests F,T,T volatile, `MAP(E1:E3,..)` with T,T,F not). Repeated calls in one formula: last wins.
 136 | - **Outside a worksheet call (X5)**: `Application.Volatile` from a Sub, `Application.Run` or `Evaluate`
 137 |   raises no error and changes no cell; `Evaluate("VTrue()")` runs the function with no effect on cells.
 138 | - **Threads and async (X3)**: see section 4; `$` registered functions and async entries cannot clear or set
 139 |   by request; `>EX!` volatile; `AsyncLate` status 32.
 140 | - **File round trip (X6)**: saved, `ca` is written for requested, `!`, `#`+reference, SUMIF-shape and token
 141 |   cells alike (no token for the run-time ones). On open in Manual mode nothing is called; the first Calculate
 142 |   calls exactly the cells with `ca` (the reopened user-function cells included), none of the others. Automatic mode
 143 |   on open was not measured.
 144 | - **Dependents (X7)**: dependents of a volatile cell are recalculated every Calculate and carry `ca` in the
 145 |   file, to any depth, through ranges (`SUM(A1:A2)`); the bits clear when the source stops being volatile.
 146 |   Exception: the bare-reference user-function dependent of section 1.
````

### Excel restart versus retained evaluation [MEASURED-EXCEL]

Source: `docs/architecture/EVALUATOR_SUSPENSION.md:23` (lines 23–97, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/EVALUATOR_SUSPENSION.md#L23-L97).

````text
  23 | ## 2. What Excel does (measured)
  24 | 
  25 | Excel 16.0.20527 x64 (CalculationVersion 191029, multi-threaded calculation on, 16 threads),
  26 | owned hidden instances on a private desktop, 2026-11-09. A VBA UDF `Tk(tag, v)` records each call
  27 | in order and returns `v`. Each trial: a new workbook, Manual, formulas entered with `Formula2` in
  28 | the listed order, then C1 := 6 and `Application.Calculate` (or `CalculateFull`), then C1 := 7 and
  29 | Calculate, then a clean Calculate. Ten trials per case under Calculate and five under CalculateFull,
  30 | 41 cases, 615 workbooks. Probe, method and raw results:
  31 | `crates/calc-engine/tests/fixtures/excel-calc-probes/suspension/`.
  32 | 
  33 | With B1 `=C1*2`, B2 `=B1+1`, B3 `=B2+1` entered first and A1 last:
  34 | 
  35 | | A1 | Calls in the first calculation (all trials) |
  36 | |---|---|
  37 | | `=Tk("pre",1)+INDIRECT("B3")+Tk("post",2)` | pre, pre, post |
  38 | | same with `B3` (static), or `OFFSET(B1,2,0)` | pre, pre, post |
  39 | | `=Tk("outer",Tk("pre",1)+INDIRECT("B3"))+Tk("post",2)` | pre, pre, outer, post |
  40 | | `=SUM(Tk("pre",1),INDIRECT("B3"),Tk("post",2))` | pre, post, pre, post |
  41 | | `=IF(Tk("cond",1)>0,INDIRECT("B3"),0)+Tk("post",2)` | cond, cond, post |
  42 | | `=LET(x,Tk("pre",1),x+INDIRECT("B3")+Tk("post",2))` | pre, pre, post |
  43 | | `=Tk("pre",1)+Tk("arg",INDIRECT("B3")+0)` | pre, pre, arg |
  44 | | `...INDIRECT("B3")+Tk("mid",2)+INDIRECT("E3")...` (two chains) | pre, pre, mid, post |
  45 | | B1..B8 chain, `INDIRECT("B8")` | pre, pre, post |
  46 | | `=Tk("pre",RAND())+INDIRECT("B3")+Tk("post",RAND())` | pre(r1), pre(r2), post(r3); A1 = B3 + r2 + r3 |
  47 | | `NOW()` in place of RAND | NOW read again; equal or one tick later |
  48 | 
  49 | So Excel abandons the formula at the read, keeps nothing of the attempt (the second attempt draws
  50 | RAND again and the value uses the second draws), calculates the formula again from its start once
  51 | the chain is done, and the second attempt does not abandon again. A UDF before the read is entered
  52 | twice, its effects stay. The read happens where the reference becomes a value: at `+`, at SUM's
  53 | own read of its reference argument (after all its arguments ran), at IF's result. The later
  54 | calculations ran each UDF once; a clean Calculate entered the UDFs again only in formulas with
  55 | INDIRECT or OFFSET (volatile). The abandonment depends on Excel's calc chain, not on the reference
  56 | kind: the static `B3` form abandons too, while with the producer one level from C1 (B1 `=C1*2`,
  57 | 26 cases in either entry order, through INDIRECT, OFFSET, RAND, NOW, SUM, IF, LET) every UDF was
  58 | entered once.
  59 | 
  60 | ## 3. The model DNA Calc keeps: wait at the reading instruction
  61 | 
  62 | An instruction that may read a cell (`waits_for_inputs`: an eager call, a lazy call, a call of a
  63 | function value) and faults with `MissingInput` for a pending producer, with no user-function call
  64 | since it started, **waits**: the cursor stays on it, the evaluation returns
  65 | `EvalStep::Suspended` with `EvalContinuation::waiting_input()` naming the cell, and after the
  66 | producer is done the same instruction runs again from its start. Nothing else in the formula runs
  67 | again.
  68 | 
  69 | Guarantees to the engine above:
  70 | 
  71 | - **What a suspended formula leaves behind:** an `EvalContinuation` holding the plan, the cursor
  72 |   (instruction position, retained child cursors of lazy arguments, LAMBDA bodies and names), the
  73 |   operand stack below the waiting instruction exactly as it was when the instruction started, its
  74 |   locals, and the user-function effects and random stream position so far. No result, no
  75 |   observer outcome, no published value.
  76 | - **What is re-executed:** only the waiting instruction: its argument admission and its kernel,
  77 |   on the same operands. A lazy call keeps the arguments it already evaluated. Earlier
  78 |   instructions, their calls, draws and user functions are never re-entered.
  79 | - **Effect accounting:** user-function calls and RAND/NOW reads happen once per occurrence, as
  80 |   without the wait; the evaluation keeps its `InvocationId` (contract 0.3, a physical retry). A
  81 |   wait after a user-function call inside the same instruction is not taken: the fault ends the
  82 |   evaluation and the engine either refuses it (`an effectful callable requiring a dependency
  83 |   continuation`, dc-hsa5.4) or takes a typed `ReevaluationReason` (P03).
  84 | - **Fallback:** a pending read that cannot wait in place (an instruction that is not a waiting
  85 |   one, or operands not where the instruction found them) ends the evaluation with
  86 |   `MissingInput`; with no user-function call the engine evaluates the formula again from its
  87 |   start once the producer is done (`Attempt::Waits`), which is Excel's shape and still pure.
  88 | - **Cost:** a wait costs the cursor position and two counts. No operand is copied when nothing
  89 |   waits.
  90 | 
  91 | How the code makes it explicit: `ResumePoint` (stack depth and user-call count when an instruction
  92 | starts) is the whole rewind state, and the waiting instructions are **operand-transactional**:
  93 | they read their operands where they lie and pop them only when they complete. An eager call admits
  94 | its arguments into its own argument list (`Run::admit_arguments`) and replaces its operands with
  95 | its result in `Run::complete_call`; a function value is called in place under its callee; a lazy
  96 | call consumes nothing from the stack. A debug assertion checks the depth on every wait.
  97 | 
````

### Status 64 and RTD corrections from real Excel [MEASURED-EXCEL]

Source: `docs/architecture/XLL_COM_HOSTING.md:1263` (lines 1263–1319, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/XLL_COM_HOSTING.md#L1263-L1319).

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
1298 | 
1299 | ## 31. Exact RTD protocol: numeric topics and no second entry (dc-6332.12)
1300 | 
1301 | Evidence: `crates/calc-xll-reference/traces/2026-11-11-x18-rtd` (README, predictions, the Excel
1302 | measurement, the x5call verification). Excel 16.0.20527.20150; every guard 0 windows, 0
1303 | foreground hits.
1304 | 
1305 | - **One topic rule, numbers included.** `calc_func::read_rtd_call` is the one rule behind the typed
1306 |   `=RTD()` and an add-in's `xlfRtd` callback: arguments coerce to text as CONCAT does, the topic
1307 |   key is `(ProgID folded, "", strings)`, and the read goes through the connector. The callback
1308 |   used to refuse a non-text topic. Measured through a native UDF that forwards its operands to
1309 |   `xlfRtd` (`X5.RtdCall`): the number 1, the text "1", TRUE and "TRUE", 0.1+0.2 and "0.3", a
1310 |   reference to a cell holding 1 are each one topic with the typed route's (one ConnectData);
1311 |   an error topic, a numeric or unknown ProgID connect nothing and show #N/A with status 0; an
1312 |   omitted topic connects one. The x5call comparison (`verify.py`) matches Excel: client protocol
1313 |   6 of 6.
1314 | - **ConnectData's value is not an update.** Excel returns it to the xlfRtd call that connected
1315 |   and enters nothing again for it. The host admitted it after the step and superseded the work,
1316 |   so every connecting function was entered twice (1, 1, 42 for the X5 UDF; a, a, b, a, b for a
1317 |   formula with two connecting calls). The work now adopts that admission in place
1318 |   (`RecalcWork::adopt_connected_inputs`; a read of a provisional value the connector reports as
1319 |   pending, `RtdConnector::provisional_pending`, passes the certificate repair), and the
````

### Graph prerequisite taxonomy [CODE]

Source: `crates/calc-engine/src/graph.rs:106` (lines 106–132, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/graph.rs#L106-L132).

````text
 106 | pub enum Prerequisite {
 107 |     /// A bound defined-name participant, independent of its caller-relative evaluation.
 108 |     Name(NameId),
 109 |     /// The value of one cell. Its binding from a direct reference is ceng-m1.2's.
 110 |     Cell(CellKey),
 111 |     /// The values of every cell of a rectangle. Its binding from a range is ceng-m1.2's.
 112 |     Area(GridArea),
 113 |     /// The values of every cell of whole rows or whole columns. Its binding from a whole-row or
 114 |     /// whole-column reference comes with ceng-m1.2; its other facets are dc-m3u's.
 115 |     AxisValue(AxisInterval),
 116 |     /// The values of a rectangle on every sheet of a span. Its binding from a 3D reference and
 117 |     /// its calculation are dc-m3u's.
 118 |     SheetSpan(SheetSpan),
 119 | }
 120 | 
 121 | impl Prerequisite {
 122 |     /// The sheets this prerequisite names: one, or a span's two endpoints.
 123 |     /// Whether the cell `at` is one this prerequisite reads (a span's middle sheets aside: a span
 124 |     /// covers `at` only on one of its endpoints here).
 125 |     pub(crate) fn covers(self, at: CellKey) -> bool {
 126 |         match self {
 127 |             Prerequisite::Name(_) => false,
 128 |             Prerequisite::Cell(cell) => cell == at,
 129 |             Prerequisite::Area(area) => area.contains(at),
 130 |             Prerequisite::AxisValue(interval) => {
 131 |                 let position = match interval.axis() {
 132 |                     Axis::Rows => at.row(),
````

### Reverse index including namespace misses and range blocks [CODE]

Source: `crates/calc-engine/src/graph.rs:343` (lines 343–402, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/graph.rs#L343-L402).

````text
 343 | /// The reverse indexes of the one graph: from an input to the nodes whose prerequisites read it.
 344 | /// Cell edges are keyed by cell; rectangle edges are filed in the blocks they meet at the level of
 345 | /// blocks of their own size, with the nodes reading one rectangle kept together, and checked for
 346 | /// containment (a cell's query looks in one block per level); axis-value edges are kept per sheet and axis;
 347 | /// span edges are kept once each and resolved against the snapshot's sheet order.
 348 | #[derive(Clone, Debug, Default)]
 349 | pub(crate) struct ReverseIndex {
 350 |     cells: BTreeMap<CellKey, NodeSet>,
 351 |     names: BTreeMap<NameId, NodeSet>,
 352 |     namespace: BTreeMap<NameKey, NodeSet>,
 353 |     name_bindings: BTreeMap<NodeId, Vec<calc_fml::reference::NameBindingProof>>,
 354 |     name_edges: BTreeMap<(NameId, NameId), NodeSet>,
 355 |     /// Each block's rectangles, each with the nodes that read it. A rectangle is filed in every
 356 |     /// block of its level it meets (at most four).
 357 |     rect_blocks: BlockMap<BTreeMap<Rect, NodeSet>>,
 358 |     /// The levels at which a rectangle has been filed (bit `l` for level `l`), so a cell's query
 359 |     /// looks only in those; a level whose last rectangle was removed keeps its bit.
 360 |     rect_levels: u8,
 361 |     /// How many times an edge has been filed or removed: the readers a query found stand while
 362 |     /// this is unchanged (dc-10d9, [`crate::recalc::ReadersCache`]).
 363 |     epoch: u64,
 364 |     rects_by_sheet: BTreeMap<SheetId, BTreeSet<RectEdge>>,
 365 |     row_values: BTreeMap<SheetId, BTreeSet<AxisEdge>>,
 366 |     column_values: BTreeMap<SheetId, BTreeSet<AxisEdge>>,
 367 |     spans: BTreeSet<SpanEdge>,
 368 | }
 369 | 
 370 | impl ReverseIndex {
 371 |     /// Rebuild discards every derived edge. Retire the index in one traversal,
 372 |     /// retaining a changed epoch so an existing readers cache cannot match it.
 373 |     pub(crate) fn clear(&mut self) {
 374 |         let epoch = self.epoch.checked_add(1).expect("graph epoch exhausted");
 375 |         *self = Self {
 376 |             epoch,
 377 |             ..Self::default()
 378 |         };
 379 |     }
 380 | 
 381 |     pub(crate) fn name_bindings(&self, node: NodeId) -> &[calc_fml::reference::NameBindingProof] {
 382 |         self.name_bindings.get(&node).map_or(&[], Vec::as_slice)
 383 |     }
 384 | 
 385 |     pub(crate) fn set_name_bindings(
 386 |         &mut self,
 387 |         node: NodeId,
 388 |         proofs: Vec<calc_fml::reference::NameBindingProof>,
 389 |     ) {
 390 |         if let Some(old) = self.name_bindings.remove(&node) {
 391 |             for proof in old {
 392 |                 for probe in proof.probes {
 393 |                     update(&mut self.namespace, probe.key, node, false);
 394 |                 }
 395 |                 if let (Some(parent), Some(child)) = (proof.parent, proof.resolved) {
 396 |                     update(&mut self.name_edges, (parent, child.id), node, false);
 397 |                 }
 398 |             }
 399 |         }
 400 |         for proof in &proofs {
 401 |             for probe in &proof.probes {
 402 |                 update(&mut self.namespace, probe.key.clone(), node, true);
````

### Node structural versus runtime edges [CODE]

Source: `crates/calc-engine/src/state.rs:21` (lines 21–58, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/state.rs#L21-L58).

````text
  21 | pub struct FormulaNode {
  22 |     id: NodeId,
  23 |     anchor: CellKey,
  24 |     prerequisites: Option<Vec<Prerequisite>>,
  25 |     dynamic: Vec<Prerequisite>,
  26 | }
  27 | 
  28 | impl FormulaNode {
  29 |     pub fn id(&self) -> NodeId {
  30 |         self.id
  31 |     }
  32 | 
  33 |     /// The cell whose formula this node is.
  34 |     pub fn anchor(&self) -> CellKey {
  35 |         self.anchor
  36 |     }
  37 | 
  38 |     /// What the node reads, or `None` while its formula's plan is not bound: a node adopted from
  39 |     /// a snapshot, or added or replaced by an edit, has no prerequisites until
  40 |     /// [`EngineState::set_prerequisites`] states them, which [`crate::begin_recalc`] does when it
  41 |     /// binds the node's formula. A node whose formula CalcFml refused to compile keeps any
  42 |     /// inspected definitions and namespace probes, so a name edit can make it bind again.
  43 |     pub fn prerequisites(&self) -> Option<&[Prerequisite]> {
  44 |         self.prerequisites.as_deref()
  45 |     }
  46 | 
  47 |     /// What the node's last calculated evaluation read beyond its placed references: the areas a
  48 |     /// reference computed as it ran (INDIRECT's text, OFFSET's result, a spilled range `#` names)
  49 |     /// read, and, for an array formula, the cells beside its anchor its result covers or would
  50 |     /// cover. They are edges of the one graph like the placed references, so a change to one of
  51 |     /// these cells makes the node stale. A node with no published calculation, and one whose
  52 |     /// formula an edit replaced, has none.
  53 |     pub fn dynamic_prerequisites(&self) -> &[Prerequisite] {
  54 |         &self.dynamic
  55 |     }
  56 | }
  57 | 
  58 | /// Why [`EngineState::set_prerequisites`] refused.
````

### Runtime overlay replacement and prerequisite installation [CODE]

Source: `crates/calc-engine/src/state.rs:1390` (lines 1390–1416, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/state.rs#L1390-L1416).

````text
1390 |     pub(crate) fn set_dynamic(&mut self, node: NodeId, dynamic: Vec<Prerequisite>) {
1391 |         if let Some(record) = self
1392 |             .nodes
1393 |             .get_mut(node.slot() as usize)
1394 |             .and_then(Option::as_mut)
1395 |             .filter(|record| record.id == node)
1396 |         {
1397 |             // Most evaluations read nothing beyond their placed references, so the edges are
1398 |             // unchanged and the index is left as it is (dc-10d9).
1399 |             if record.dynamic == dynamic {
1400 |                 return;
1401 |             }
1402 |             let old = std::mem::replace(&mut record.dynamic, dynamic);
1403 |             self.index.remove(node, &old);
1404 |             self.index.insert(node, &record.dynamic);
1405 |         }
1406 |     }
1407 | 
1408 |     /// States what a live node reads, replacing what it read before, and files each prerequisite
1409 |     /// in the graph's reverse indexes. Its product caller is [`crate::begin_recalc`], which binds
1410 |     /// each unbound node's formula through CalcFml and states the cells, rectangles and whole
1411 |     /// rows or columns its references name; sheet spans have no product caller yet (dc-m3u's
1412 |     /// binding and calculation). Every sheet named must be in `inputs`, the snapshot at
1413 |     /// this state's revision.
1414 |     ///
1415 |     /// It is public because ceng-m1.1's acceptance cases state prerequisites with it by hand, the
1416 |     /// spans included. Stating prerequisites other than a bound node's placed references leaves
````

### RTD invalidation uses published readers [CODE]

Source: `crates/calc-engine/src/state.rs:671` (lines 671–718, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/state.rs#L671-L718).

````text
 671 |     /// An RTD event advances the next command's input cut. It is neither an authored cell edit
 672 |     /// nor an async return, and never supplies a formula result directly. Only the formulas whose
 673 |     /// published stamp read `topic` are dirtied (dc-snz.23.2): the reverse index names them, and
 674 |     /// each is checked against its current stamp before it counts.
 675 |     pub fn invalidate_rtd_topic(
 676 |         &mut self,
 677 |         inputs: &ModelRoot,
 678 |         topic: &calc_fml::TopicKey,
 679 |         lease: calc_fml::RtdLease,
 680 |         sequence: u64,
 681 |     ) {
 682 |         let readers = self.rtd_readers.get(topic).cloned().unwrap_or_default();
 683 |         let (live, gone): (BTreeSet<_>, BTreeSet<_>) = readers.into_iter().partition(|node| {
 684 |             self.results
 685 |                 .of(*node)
 686 |                 .and_then(FormulaResult::stamp)
 687 |                 .is_some_and(|stamp| {
 688 |                     stamp
 689 |                         .reads
 690 |                         .external_inputs
 691 |                         .iter()
 692 |                         .any(|read| &read.topic == topic)
 693 |                 })
 694 |         });
 695 |         if !gone.is_empty() {
 696 |             if let Some(held) = self.rtd_readers.get_mut(topic) {
 697 |                 held.retain(|node| !gone.contains(node));
 698 |                 if held.is_empty() {
 699 |                     self.rtd_readers.remove(topic);
 700 |                 }
 701 |             }
 702 |         }
 703 |         let roots = live;
 704 |         let dirty = crate::recalc::dirty_closure(self, inputs, roots.clone(), &|n| {
 705 |             crate::recalc::invalidation_areas(self, inputs, n)
 706 |         })
 707 |         .members()
 708 |         .to_vec();
 709 |         self.pending.extend(roots);
 710 |         self.contract.invalidate(dirty);
 711 |         self.contract
 712 |             .external_versions
 713 |             .insert(topic.clone(), (lease, sequence));
 714 |         self.contract.external_inputs = self
 715 |             .contract
 716 |             .external_inputs
 717 |             .checked_add(1)
 718 |             .expect("external input epoch exhausted");
````

### Scheduling with stale dynamic cycles and SCC analysis [CODE]

Source: `crates/calc-engine/src/recalc.rs:3567` (lines 3567–3621, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L3567-L3621).

````text
3567 |                 |node| live_anchor(state, node),
3568 |                 |node| live_anchor(state, node),
3569 |             );
3570 |             (closure, ordered)
3571 |         }
3572 |     };
3573 |     match ordered {
3574 |         Ok(order) => acyclic(order),
3575 |         Err(_) => {
3576 |             let readers_of = closure.to_map();
3577 |             // Keep only the edges a placed reference makes: a dynamic prerequisite may be stale
3578 |             // (it is what the last evaluation read), and a formula that reads a cell evaluated
3579 |             // later waits for it at evaluation.
3580 |             let placed: BTreeMap<NodeId, Vec<NodeId>> = readers_of
3581 |                 .iter()
3582 |                 .map(|(node, readers)| {
3583 |                     let anchor = live_anchor(state, *node);
3584 |                     let node_areas = areas(*node);
3585 |                     let kept = readers
3586 |                         .iter()
3587 |                         .copied()
3588 |                         .filter(|reader| {
3589 |                             state
3590 |                                 .node(*reader)
3591 |                                 .and_then(|record| record.prerequisites())
3592 |                                 .is_some_and(|placed| {
3593 |                                     placed.iter().any(|prerequisite| {
3594 |                                         prerequisite.covers(anchor)
3595 |                                             || node_areas
3596 |                                                 .iter()
3597 |                                                 .any(|area| prerequisite.meets_area(*area))
3598 |                                     })
3599 |                                 })
3600 |                         })
3601 |                         .collect();
3602 |                     (*node, kept)
3603 |                 })
3604 |                 .collect();
3605 |             match order_map(state, &placed) {
3606 |                 Ok(order) => acyclic(order),
3607 |                 Err(_) => order_with_cycles(state, inputs, &placed),
3608 |             }
3609 |         }
3610 |     }
3611 | }
3612 | 
3613 | /// The same graph's temporary integer indices for Tarjan, used by initial
3614 | /// ordering and by a dynamic edge that joins an existing cyclic component.
3615 | fn indexed_readers(readers_of: &BTreeMap<NodeId, Vec<NodeId>>) -> (Vec<NodeId>, Vec<Vec<usize>>) {
3616 |     let nodes: Vec<NodeId> = readers_of.keys().copied().collect();
3617 |     let index_of: BTreeMap<NodeId, usize> =
3618 |         nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
3619 |     let edges: Vec<Vec<usize>> = nodes
3620 |         .iter()
3621 |         .map(|node| readers_of[node].iter().map(|r| index_of[r]).collect())
````

### Cycle order measured by sheet name [CODE]

Source: `crates/calc-engine/src/recalc.rs:3680` (lines 3680–3721, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L3680-L3721).

````text
3680 |             if waiting[*reader] == 0 {
3681 |                 ready.insert((first_anchor(*reader), *reader));
3682 |             }
3683 |         }
3684 |     }
3685 |     debug_assert_eq!(order.len(), nodes.len(), "the components form no cycle");
3686 |     Scheduled { order, cyclic }
3687 | }
3688 | 
3689 | /// Sorts the members of a cycle into the order Excel evaluates them in (dc-86f, Excel 16.0.20527,
3690 | /// 2026-10-30, tests/fixtures/dc_86f): their sheets by name, letter case ignored (a sheet named
3691 | /// b before one named C, and A before b, whatever their tab order, sheet IDs or the order they
3692 | /// were made in), then by row, then by column. The order the formulas were entered in and the
3693 | /// order of the file's saved calc chain play no part. Names that fold equal keep their workbook
3694 | /// order.
3695 | fn sort_members(state: &EngineState, inputs: &ModelRoot, members: &mut [NodeId]) {
3696 |     let mut sheets: Vec<(String, usize, calc_value::SheetId)> = inputs
3697 |         .sheets()
3698 |         .iter()
3699 |         .enumerate()
3700 |         .map(|(position, sheet)| {
3701 |             let name = sheet.name().text().to_string_lossy();
3702 |             (calc_value::fold_workbook_name(&name), position, sheet.id())
3703 |         })
3704 |         .collect();
3705 |     sheets.sort();
3706 |     let rank: BTreeMap<calc_value::SheetId, usize> = sheets
3707 |         .iter()
3708 |         .enumerate()
3709 |         .map(|(rank, (_, _, id))| (*id, rank))
3710 |         .collect();
3711 |     members.sort_by_key(|member| {
3712 |         let at = live_anchor(state, *member);
3713 |         (
3714 |             rank.get(&at.sheet()).copied().unwrap_or(usize::MAX),
3715 |             at.row(),
3716 |             at.column(),
3717 |         )
3718 |     });
3719 | }
3720 | 
3721 | /// The strongly connected component of each vertex of `edges` (vertex `i` reaches each of
````

### Attempt outcomes: reevaluation versus dependency wait [CODE]

Source: `crates/calc-engine/src/recalc.rs:2157` (lines 2157–2192, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L2157-L2192).

````text
2157 |             stage(state, inputs, work, node, done);
2158 |             if work.async_resumed.remove(&node) {
2159 |                 work.open_pass(PassReason::AsyncDependents);
2160 |             }
2161 |         }
2162 |         Ok(Attempt::Reevaluate { on, cell, reason }) => {
2163 |             finish_preliminary(work, node);
2164 |             record_reevaluation(state, work, node, reason);
2165 |             work.semantic_waits.insert(node);
2166 |             // The previous invocation has ended. Its next evaluation and known consumers
2167 |             // follow the unresolved producer and the surviving observation fences. Install
2168 |             // that prefix before admitting the new cached view; never certify staged inputs
2169 |             // against an older installed frontier.
2170 |             let moved: BTreeSet<_> = dirty_closure(state, inputs, [node].into(), &|n| {
2171 |                 invalidation_areas(state, inputs, n)
2172 |             })
2173 |             .members()
2174 |             .iter()
2175 |             .copied()
2176 |             .collect();
2177 |             let mut ranks: BTreeMap<_, usize> = work
2178 |                 .segments
2179 |                 .iter()
2180 |                 .map(|(node, segment)| ((moved.contains(node), *segment), 0))
2181 |                 .collect();
2182 |             for (rank, value) in ranks.values_mut().enumerate() {
2183 |                 *value = rank;
2184 |             }
2185 |             for (node, segment) in work.segments.iter_mut() {
2186 |                 *segment = ranks[&(moved.contains(node), *segment)];
2187 |             }
2188 |             observer.deferred(live_anchor(state, node), cell);
2189 |             defer(state, inputs, work, node, on);
2190 |         }
2191 |         Ok(Attempt::Kept) => keep(state, inputs, work, node),
2192 |         Ok(Attempt::RangeRefused) => exclude_range_reader(state, work, node),
````

### Recalc command normalization [CODE]

Source: `crates/calc-host/src/recalc.rs:139` (lines 139–185, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/recalc.rs#L139-L185).

````text
 139 |                     }
 140 |                 }
 141 |                 Err(error) => return failed(host, RecalcFailed::Refused(error)),
 142 |             }
 143 |         }
 144 |         Some(work) => work,
 145 |         None => {
 146 |             let current = host.model.revision();
 147 |             let requested = std::mem::take(&mut host.calculate_requested);
 148 |             // L3 (dc-m3u): automatic except tables leaves data tables until a recalculation is
 149 |             // asked for.
 150 |             let deferred = host.calc_mode() == CalcMode::AutomaticExceptTables && !requested;
 151 |             host.engine.set_tables_deferred(deferred);
 152 |             if !requested {
 153 |                 if host.calc_mode() == CalcMode::Manual {
 154 |                     return HostProgress::Manual;
 155 |                 }
 156 |                 if host.published == Some(current) && !host.engine.needs_recalc(&host.model) {
 157 |                     return HostProgress::Current;
 158 |                 }
 159 |             }
 160 |             // dc-uar: the facts CELL and INFO read, as of this revision.
 161 |             host.renew_workbook_facts();
 162 |             let started = if let Some((scope, selection)) = host.requested_calculation.take() {
 163 |                 let scope = host.transformed_calculation_scope.take().unwrap_or(scope);
 164 |                 let poststate = if host.calc_mode() == CalcMode::Manual
 165 |                     && matches!(scope, calc_engine::CalcScope::Range(_))
 166 |                     && selection == calc_engine::EvalSelection::ForceAll
 167 |                 {
 168 |                     calc_engine::contract::RangePostState::StillRequired
 169 |                 } else {
 170 |                     calc_engine::contract::RangePostState::Satisfied
 171 |                 };
 172 |                 calc_engine::begin_recalc_scoped_with_poststate(
 173 |                     &mut host.engine,
 174 |                     &host.model,
 175 |                     host.options.numeric.clone(),
 176 |                     scope,
 177 |                     selection,
 178 |                     poststate,
 179 |                 )
 180 |             } else {
 181 |                 calc_engine::begin_recalc(
 182 |                     &mut host.engine,
 183 |                     &host.model,
 184 |                     host.options.numeric.clone(),
 185 |                 )
````

### External input updates, async completion and finite stepping [CODE]

Source: `crates/calc-host/src/session.rs:549` (lines 549–611, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/session.rs#L549-L611).

````text
 549 |     pub fn update_rtd(
 550 |         &mut self,
 551 |         topic: calc_fml::TopicKey,
 552 |         lease: calc_fml::RtdLease,
 553 |         sequence: u64,
 554 |         value: calc_doc_model::value::CalcValue,
 555 |     ) -> crate::RtdDisposition {
 556 |         let disposition = self.host.update_rtd(topic, lease, sequence, value);
 557 |         if disposition == crate::RtdDisposition::Accepted {
 558 |             self.deliver_changes(
 559 |                 DeltaCause::ExternalInputsChanged {
 560 |                     revision: self.host.revision(),
 561 |                 },
 562 |                 false,
 563 |             );
 564 |         }
 565 |         disposition
 566 |     }
 567 | 
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
 595 | 
 596 |     /// Continues an already authorized calculation. It creates no authored command or history
 597 |     /// entry and never widens its scope.
 598 |     pub fn advance_calculation(&mut self) -> HostProgress {
 599 |         let mut receipt = IntentReceipt {
 600 |             ingress: CommandIngress::UserInterface,
 601 |             seq: self.next_seq,
 602 |             parent: self.active_command.get(),
 603 |             error: None,
 604 |             committed: None,
 605 |             recalc: None,
 606 |             revision: self.host.revision(),
 607 |             deltas: 0,
 608 |         };
 609 |         self.recalculate(&mut receipt);
 610 |         receipt.recalc.expect("one step always reports progress")
 611 |     }
````

### Callback status latch and registration permissions [CODE]

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

### xlAsyncReturn admission and typed reference Pending [CODE]

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

### Return disposal for abandoned native invocation [CODE]

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

### Current evaluator interface evidence: runtime overlay review [EARLIER-REVIEW]

Source: `docs/architecture/review-2026-11/pro/8-evaluator-design-answer.md:693` (lines 693–710, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-11/pro/8-evaluator-design-answer.md#L693-L710).

````text
 693 | # E. Runtime dependency overlay
 694 | 
 695 | ## E1. What can be exact
 696 | 
 697 | Let `S` be structural compatibility dependencies, `R` be the typed observations of a completed semantic evaluation, and `D` be the runtime dependencies projected from `R` that are not already covered by `S`. The existing graph represents `S ∪ D`, with additional engine-derived publication requirements where applicable.
 698 | 
 699 | There are several different exactness claims:
 700 | 
 701 | 1. Every relevant query is reported with the correct policy, facet, context and outcome.
 702 | 2. The certificate captures enough evidence to validate reuse.
 703 | 3. The compatibility graph represents the intended invalidation policy.
 704 | 4. Scheduling waits occur only for producers whose completion the admitted read actually requires.
 705 | 5. The observations describe exactly the cells whose values an implementation consumed.
 706 | 
 707 | The excerpts establish useful mechanisms for the first four, with gaps to audit. They do not establish the fifth. Nor should the structural graph be expected to equal the fifth: it deliberately includes untaken branches and latent bodies. [J.1–J.3]
 708 | 
 709 | The current footprint mixes requested areas, fact subjects, spill anchors and resolved references. Keep that conservative behavior initially, but name its meaning. A future optimisation must not treat an untyped area footprint as proof that every value in that area was consumed.
 710 | 
````

### Dirty closure traverses current reverse readers [CODE]

Source: `crates/calc-engine/src/recalc.rs:3454` (lines 3454–3508, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L3454-L3508).

````text
3454 | fn closure_with(
3455 |     state: &EngineState,
3456 |     inputs: &ModelRoot,
3457 |     roots: &BTreeSet<NodeId>,
3458 |     mut cache: Option<&mut ReadersCache>,
3459 |     areas: &dyn Fn(NodeId) -> Vec<GridArea>,
3460 | ) -> Closure {
3461 |     let slots = state.nodes.len();
3462 |     let mut member = crate::slots::Slots::new(slots, roots.len());
3463 |     let mut readers = crate::slots::Slots::new(slots, roots.len());
3464 |     let mut edges: Vec<NodeId> = Vec::new();
3465 |     let mut members: Vec<NodeId> = Vec::with_capacity(roots.len());
3466 |     for node in roots {
3467 |         member.insert(node.slot(), ());
3468 |         members.push(*node);
3469 |     }
3470 |     let mut stack = members.clone();
3471 |     let mut found = Vec::new();
3472 |     while let Some(node) = stack.pop() {
3473 |         let anchor = live_anchor(state, node);
3474 |         match cache.as_deref().and_then(|cache| cache.get(node, anchor)) {
3475 |             Some(cached) => {
3476 |                 found.clear();
3477 |                 found.extend_from_slice(cached);
3478 |                 #[cfg(debug_assertions)]
3479 |                 {
3480 |                     let mut fresh = Vec::new();
3481 |                     readers_into(state, inputs, node, &areas(node), &mut fresh);
3482 |                     debug_assert_eq!(found, fresh, "the cached readers of {node:?} stand");
3483 |                 }
3484 |             }
3485 |             None => {
3486 |                 readers_into(state, inputs, node, &areas(node), &mut found);
3487 |                 if let Some(cache) = cache.as_deref_mut() {
3488 |                     cache.insert(node, anchor, &found);
3489 |                 }
3490 |             }
3491 |         }
3492 |         for &reader in &found {
3493 |             let slot = reader.slot();
3494 |             if member.get(slot).is_none() {
3495 |                 member.insert(slot, ());
3496 |                 members.push(reader);
3497 |                 stack.push(reader);
3498 |             }
3499 |         }
3500 |         let start = u32::try_from(edges.len()).expect("fewer than 2^32 edges in a closure");
3501 |         let count = u32::try_from(found.len()).expect("fewer than 2^32 readers of a node");
3502 |         readers.insert(node.slot(), (start, count));
3503 |         edges.extend_from_slice(&found);
3504 |     }
3505 |     members.sort_unstable();
3506 |     Closure {
3507 |         slots,
3508 |         members,
````

### Current publication volatility still combines UDF request and plan flag [CODE]

Source: `crates/calc-engine/src/recalc.rs:2722` (lines 2722–2737, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/recalc.rs#L2722-L2737).

````text
2722 |         work.early_published.insert(node, stamp.cloned());
2723 |         state.cached_arrays.remove(&node);
2724 |         if let Some(volatile) = result
2725 |             .stamp()
2726 |             .and_then(|stamp| stamp.reads.udf_volatility)
2727 |             .map(calc_fml::UdfVolatility::requests_volatile)
2728 |         {
2729 |             let built_in = matches!(state.bindings.get(&node),Some(Binding::Placed(instance,_)) if instance.plan().volatile());
2730 |             if volatile || built_in {
2731 |                 state.volatile.insert(node);
2732 |             } else {
2733 |                 state.volatile.remove(&node);
2734 |             }
2735 |         }
2736 |         state.publish_result(node, at, result);
2737 |         state.pending.remove(&node);
````
