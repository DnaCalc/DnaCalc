# Question for GPT-6 Pro: one coherent authored model across engine, host and document persistence

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

Review the boundary among calc-doc-model, calc-doc, calc-engine and calc-host. Decide which owns cells, formula text/form/dialect/templates, names and binding identity, tables, styles and geometry, cached values/provenance, preserved ZIP parts and relationships. Show a state ownership table and distinguish the original LoadedWorkbook/source package, current immutable authored ModelRoot, derived EngineState/results, session history and save snapshot. Detect authoritative-state duplication and define the invariants that make edits and save agree.

Trace opening a package, formula binding and cache admission; scalar/formula/name/style edits; row/column insert/delete; sheet insert/delete/rename/move; cell cut/move versus copy/fill; table/name rewrites; undo/redo; concurrent calculation; and save/reopen. References, relative defined names, sheet-scoped names, 3-D spans, shared formula membership, array/spill metadata, rules, charts and object-model Range handles must transform under the same authored operation or have an honest refusal. What does a failed prepare, failed engine commit, failed save or cancellation leave visible? Undo creates a new revision and must not replay external effects or resurrect stale async results.

Assess save fidelity precisely: a byte-identical untouched ZIP, byte-preserved unchanged entries in an edited ZIP, semantic equality after reserialization and Excel repair-free reopen are distinct promises. Review SavePin/model/result frontiers, CalculateBeforeSave false/true, dirty/file caches, fullCalcOnLoad/forceFullCalc/calcChain, before-save events and edits arriving after a pin. Specify whether a successful edit may later prove unsavable and how the host surfaces that honestly.

Include xlsx/xlsm/xlsb, original-binary preservation versus authored XLSB writing/conversion limits, vbaProject.bin, signatures and source write-back boundaries with CalcVBA. Preserve macro/chart/dialog sheets without claiming they are modeled calculation or UI surfaces. Examine why rename/removal can corrupt opaque chart references even when bytes are preserved. Commits 8081154a and f0f58188 and the later dc-mndh fix are evidence: reader repairs/tolerance, opaque sheet preservation and conservative save refusals. Reconcile the October synthesis's XLSB limitation with today's code rather than repeating it.

Review repairs(): what Excel opens silently versus repairs, normalized model versus original bytes, id stability, report delivery through HostState and the UI, and whether an untouched save should preserve or normalize a repaired input. Public files with a cached number are not live Excel answers. Keep the Enron saved-value comparison distinct from capture batch 1.

For large workbooks, identify full scans, rebuilds, Arc copy costs, formula-source sharing, sparse pages, snapshot/undo retention, source loading, package expansion/memory budgets and streaming opportunities. Recommend measurements before storage rewrites. Experiments should distinguish copy/move/structural transform rules, formula dialect serialization, chart/dialog/macro preservation on edits/rename, tolerated malformed inputs, save cache truth tables and xlsb/xlsm fidelity.

## What was already tried

Already tried: calc-doc gained shared/array formula and structural edit support, source-preserving saves and explicit not-modelled routes. f0f58188 let chart/dialog sheets open while preserving opaque parts and refusing unsafe changes. 8081154a matched selected reader tolerance/repair behavior against Excel. dc-mndh then fixed the dangerous rename/preserved-reference case by refusal and exposed repairs through HostState. The earlier red-team warned about atomic structural transactions and pinned save result views; current code must be checked before treating those warnings as open defects.

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

### Earlier persistence design and scope limits [DESIGN]

Source: `docs/architecture/SYNTHESIS.md:14` (lines 14–28, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/SYNTHESIS.md#L14-L28).

````text
  14 | ## 0. What Excel persists (summary; appendix A)
  15 | 
  16 | - **xlsx**: cell formulas as A1 text; `t` = normal | array | dataTable | shared. A shared group's master holds the
  17 |   text and a bounding `ref`; membership is per cell (`si`). Dynamic arrays are marked by `cm=` metadata. Prefixes
  18 |   (`_xlfn.`, `_xlpm.`, `_xleta.`, ANCHORARRAY, SINGLE) and dynamic-array storage follow measured per-function
  19 |   tables (`calc-doc/src/save/file_form.rs [historical locator omitted]`). Defined names carry `localSheetId`; their relative references are
  20 |   stored from A1 and wrap at the grid edge (measured, `calc-fml/src/reference/mod.rs [historical locator omitted]`).
  21 | - **xlsb**: parsed token streams; shared formulas as BrtShrFmla with PtgRefN offsets, members as PtgExp; names by
  22 |   index. The codec converts xlsb to SpreadsheetML text before loading and **saves as .xlsx**
  23 |   (`calc-doc/src/xlsb/mod.rs [historical locator omitted]`); there is no same-format xlsb writer (decision 12).
  24 | - **A1 vs R1C1** differ observably in INDIRECT's `a1` flag, names that look like references, and off-grid
  25 |   relative references (names and INDIRECT-R1C1 wrap, measured; cell copy/fill routes unmeasured).
  26 | - **Consequence for 4.8**: Excel persists templates with relative offsets over areas; memory holds templates over
  27 |   blocks with plain coordinates, explicit membership and a semantic dialect, A1 at the edges, no row/column keys,
  28 |   optional indexes only where measurement shows a bad case.
````

### Earlier red-team: atomic structural edits [EARLIER-REVIEW]

Source: `docs/architecture/review-2026-10/pro/3-architecture-redteam-review.md:159` (lines 159–175, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-10/pro/3-architecture-redteam-review.md#L159-L175).

````text
 159 | ### 3. Structural changes need atomic authored transactions and revision-qualified identities
 160 | 
 161 | **Target: A §1.2, S2, U1, C2/C3 and the saved-Range proposal.**
 162 | 
 163 | An address at a revision and (BlockId, offset) are storage locations. Neither alone authenticates a late result.
 164 | 
 165 | Construct this trace:
 166 | 
 167 | 1. B10=SlowAsync(A10) begins at revision 20.
 168 | 2. Insert row 5; the formula moves to B11.
 169 | 3. Put an unrelated formula at B10.
 170 | 4. Return the original asynchronous result.
 171 | 5. Undo the insertion while another old return is pending.
 172 | 
 173 | The host needs to decide whether each original activation transfers through the edit or becomes invalid. Looking up its old address, reused slot or old block offset is unsafe.
 174 | 
 175 | ~~~rust
````

### Pinned save contract [DESIGN]

Source: `docs/architecture/RECALC_CONTRACT.md:670` (lines 670–692, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/RECALC_CONTRACT.md#L670-L692).

````text
 670 | ## 8. Save
 671 | 
 672 | `SaveRequest { id, workbook, model: ModelPin, base_results, inputs, cache_policy, format }` ->
 673 | `SaveImage { model, results: ResultViewPin, cache_manifest }`; the serializer never walks a mutable result map.
 674 | `CachePolicy::{PreservePermittedCaches, RequireCalculatedScope(scope), DropStaleCaches}`.
 675 | 
 676 | - `DropStaleCaches` remains the explicit current-only policy: a formula not in the stale set is written with its calculated value,
 677 |   every other formula without `<v>` (`calc-host/src/save.rs [historical locator omitted]`), plus `fullCalcOnLoad` when an array lacks one
 678 |   (`calc-doc/src/save/author.rs [historical locator omitted]`). Excel recalculates those on open (1). Honest, but it makes Excel call a UDF
 679 |   on open that Excel itself might not have called.
 680 | - `PreservePermittedCaches` is selected by P06 (O, 20527 safe mode) for ordinary Manual Save(false): old
 681 |   caches of the same formula instance survive with ca=1, calcCompleted=0 and calcOnSave=0. Do not infer freshness
 682 |   from absent flags: disabled-sheet cases can keep stale caches without ca or calcCompleted. Flags follow the
 683 |   measured file profile; they do not create a producing command or certificate.
 684 | - `CalculateBeforeSave` is a `CalculationDomain` setting (xlsb conversion at `calc-doc/src/xlsb/workbook.rs [historical locator omitted]`,
 685 |   and save writes `calcOnSave` from `calculate_before_save`, `calc-doc/src/save/parts.rs [historical locator omitted]`, supplied by
 686 |   `calc-host/src/save.rs [historical locator omitted]`). True: admit a `PinnedSave` calculation of the workbook's dirty/volatile demand,
 687 |   pin its final view, serialize with the original model pin; waiting on pending async; cancel or failure gives an
 688 |   explicit save outcome, never a silent downgrade. A live in-flight effectful invocation the save needs is waited for,
 689 |   not shared; if it is superseded the save is `Incomplete`.
 690 | - P06 requires these normalization qualifications: Save/SaveAs admits BeforeSave edits before the serialization
 691 |   model pin; SaveCopyAs skipped those handlers. Ordinary Manual true updated live caches in the saved workbook
 692 |   and a dirty peer, so Excel's observed calculation scope exceeds one workbook. With the active sheet disabled,
````

### Reader tolerance measured against Excel (8081154a) [MEASURED-EXCEL]

Source: `crates/calc-doc/tests/fixtures/reader_tolerance/excel-answers.md:10` (lines 10–48, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/tests/fixtures/reader_tolerance/excel-answers.md#L10-L48).

````text
  10 | 
  11 | - `open-in-excel-job.ps1`: the session host's owned automation Excel (`Workbooks.Open`, read
  12 |   only), once with the default load and once with `CorruptLoad = xlRepairFile`. Output:
  13 |   `excel-answers/automation-normal-load.json`, `automation-repair-load.json`,
  14 |   `automation-repair-load-stale-array.json`.
  15 | - `open-by-ui-job.ps1`: a plain Excel process of our own started with the file on its command
  16 |   line, which is what File > Open does. Every dialog is recorded with UI Automation and dismissed
  17 |   through its Yes button; the workbook is then read through that process's own object model
  18 |   (process ID checked) and Excel's own save of it is kept (`SaveCopyAs`). Output:
  19 |   `excel-answers/file-open-ui3.json`, `-ui4.json`, `-ui5.json`, and the sheet lists of Excel's
  20 |   saved copies in `sheets-of-the-workbooks-excel-saved-after-opening.json`.
  21 | - Every run was on the private desktop (`tools/excel-session/on-private-desktop.ps1`); each guard
  22 |   file recorded 0 windows and 0 foreground hits on the owner's desktop. Only Excel processes these
  23 |   scripts started were touched; no Trust Center, macro or registry setting.
  24 | - Limit: the second dialog Excel shows after a repair ("Repairs to ..." with the removed
  25 |   records) is an Office dialog whose text UI Automation does not expose; its title is recorded,
  26 |   its text is not. The repaired workbook is read instead.
  27 | 
  28 | ## Answers
  29 | 
  30 | | Defect (fixture) | Automation default load | File > Open | What the opened workbook shows |
  31 | |---|---|---|---|
  32 | | `r:id=""` on a sheet, with no part (`empty-rid-veryhidden`, `empty-rid-visible`; the shape of 687 sheets in 400 Enron files, VBA modules saved by Excel) | opens | opens, no prompt, title not "Repaired" | the sheet is there, empty, with its own state (a veryHidden sheet stays veryHidden, a visible one stays visible) |
  33 | | `r:id="rId9"`, no such relationship (`dangling-rid`) | refused (0x800A03EC) | repair prompt "We found a problem with some content in '...'. Do you want us to try to recover as much as we can? If you trust the source of this workbook, click Yes." (Yes, No), then a "Repairs to" dialog; title "Repaired" | the sheet is kept, empty; Excel's save writes it with its sheetId and a part |
  34 | | two sheets with one `sheetId` (`duplicate-sheetid`, `-gap`, `-lower`, `-triple`, `-two-pairs`; 108 Enron files) | refused | the same repair prompt | both sheets keep all their cells; Excel's save gives each later repeat the next id above the highest declared: 1,109,109 gives 1,109,110; 1,1,5 gives 1,6,5; 1,1,1 gives 1,2,3; 2,2,3,3 gives 2,4,3,5 |
  35 | | `<f ca="1"/>` (no text) on a member of a multi-cell array's area (`array-inner-empty-f`; what Excel itself writes, in 7 Enron files) | opens | opens, no prompt | the cell is a member of the array, like the control |
  36 | | a formula with text in a legacy array's area (`array-inner-formula`, `-stale`) | refused | the repair prompt | the formula is dropped; the cell shows the array's result (also with a wrong cached value in the area: Excel recalculates it) |
  37 | | a worksheet cut off inside a row (`watcher-control-malformed-sheet`) | refused, also with xlRepairFile | the repair prompt | sheet Two is empty; this file is the positive control for the dialog watcher |
  38 | 
  39 | Corpus note: the 7 Enron array files carry `<f ca="1"/>`, the member marker. The reader accepted
  40 | that marker only inside a dynamic array's area; Excel opens it inside a legacy array's area too.
  41 | The plain formula with text inside an array area is not in the corpus; it is measured here because
  42 | it is the same reader check.
  43 | 
  44 | ## The reader
  45 | 
  46 | - `r:id=""`: an empty sheet that keeps its name and state; no repair is reported. It is carried
  47 |   as a preserved sheet of kind "sheet without a part": a save that leaves it alone writes the
  48 |   package as it was; an edit of that sheet is refused (`SaveRefusal::PreservedSheetChanged`).
````

### Open/load top-level API [CODE]

Source: `crates/calc-doc/src/lib.rs:136` (lines 136–177, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/lib.rs#L136-L177).

````text
 136 | pub struct OpenedDocument {
 137 |     package: Arc<opc::Package>,
 138 | }
 139 | 
 140 | impl OpenedDocument {
 141 |     pub fn package(&self) -> &Package {
 142 |         &self.package
 143 |     }
 144 | 
 145 |     pub(crate) fn package_arc(&self) -> &Arc<Package> {
 146 |         &self.package
 147 |     }
 148 | 
 149 |     /// An immutable view of the document as opened, which a save is prepared from.
 150 |     pub fn snapshot(&self) -> DocumentSnapshot {
 151 |         DocumentSnapshot::new(Arc::clone(&self.package))
 152 |     }
 153 | }
 154 | 
 155 | /// Opens the package in `source` under `policy`. The document keeps `source` and reads part
 156 | /// payloads from it lazily and when it is saved.
 157 | pub fn open(
 158 |     source: Arc<dyn PackageRead>,
 159 |     policy: &OpenPolicy,
 160 | ) -> Result<OpenedDocument, OpenError> {
 161 |     let package = opc::Package::open(source, policy)?;
 162 |     // An Excel binary workbook is read as the SpreadsheetML package it stands for.
 163 |     if let Some(workbook) = xlsb::binary_workbook(&package) {
 164 |         let converted = xlsb::convert(&package, &workbook)?;
 165 |         let original = Arc::new(package);
 166 |         let package = opc::Package::open(Arc::new(MemorySource::new(converted)), policy)?
 167 |             .with_original_binary(original);
 168 |         return Ok(OpenedDocument {
 169 |             package: Arc::new(package),
 170 |         });
 171 |     }
 172 |     Ok(OpenedDocument {
 173 |         package: Arc::new(package),
 174 |     })
 175 | }
 176 | 
 177 | /// The SpreadsheetML package (the bytes of an .xlsx or .xlsm archive) an Excel binary workbook
````

### ModelRoot storage fields [CODE]

Source: `crates/calc-doc-model/src/root.rs:248` (lines 248–287, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc-model/src/root.rs#L248-L287).

````text
 248 | /// such as another load of the same file. Equality compares the revision, the sheets, the cells
 249 | /// and the layout, not the lineage.
 250 | #[derive(Clone, Debug)]
 251 | pub struct ModelRoot {
 252 |     revision: AuthoredRevision,
 253 |     sheets: Vec<SheetEntry>,
 254 |     /// The cells of the sheet at the same position of `sheets`.
 255 |     cells: Vec<SheetPages>,
 256 |     layout: Arc<SkinLayout>,
 257 |     definitions: Arc<Definitions>,
 258 |     lineage: u64,
 259 | }
 260 | 
 261 | impl PartialEq for ModelRoot {
 262 |     fn eq(&self, other: &Self) -> bool {
 263 |         self.revision == other.revision
 264 |             && self.sheets == other.sheets
 265 |             && self.cells == other.cells
 266 |             && self.layout == other.layout
 267 |             && self.definitions == other.definitions
 268 |     }
 269 | }
 270 | 
 271 | impl Eq for ModelRoot {}
 272 | 
 273 | impl ModelRoot {
 274 |     /// Retain this exact authored root when the caller holds a borrowed view. Pages,
 275 |     /// definitions and layout stay shared; no revision or lineage is invented.
 276 |     pub fn pin(&self) -> Arc<Self> {
 277 |         Arc::new(Self {
 278 |             revision: self.revision,
 279 |             sheets: self.sheets.clone(),
 280 |             cells: self.cells.clone(),
 281 |             layout: self.layout.clone(),
 282 |             definitions: self.definitions.clone(),
 283 |             lineage: self.lineage,
 284 |         })
 285 |     }
 286 | 
 287 |     /// The same authored revision after a successful Save As. File identity is session
````

### LoadedWorkbook ownership and repair types [CODE]

Source: `crates/calc-doc/src/load/mod.rs:397` (lines 397–440, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/load/mod.rs#L397-L440).

````text
 397 | /// A defect Excel repairs when it opens the file, which prompts "We found a problem with some
 398 | /// content" and then shows the repaired workbook. The load reads the same result and lists the
 399 | /// repair ([`LoadedWorkbook::repairs`]). Measured against Excel in our own session on authored
 400 | /// files (`tests/fixtures/reader_tolerance`); a defect Excel refuses is a [`LoadError`].
 401 | #[derive(Clone, Debug, PartialEq, Eq)]
 402 | pub enum LoadRepair {
 403 |     /// A sheet's `r:id` names no relationship of the workbook part: Excel keeps the sheet, empty,
 404 |     /// with its name and visibility, and so does the load. (An empty `r:id=""` is not a repair:
 405 |     /// Excel opens it without a prompt as the same empty sheet.)
 406 |     SheetRelationshipMissing { sheet: SheetId, id: String },
 407 |     /// A plain formula inside the area of an array formula: Excel drops it and shows the array's
 408 |     /// result in that cell, and so does the load (the cell is a member of the array).
 409 |     FormulaInsideArray { sheet: SheetId, row: u32, column: u32 },
 410 |     /// A later sheet repeats an earlier sheet's `sheetId` (`id`): Excel keeps both sheets and gives
 411 |     /// the later one a new id (`now`), the next above the highest the workbook part declares.
 412 |     DuplicateSheetId { sheet: SheetId, id: u32, now: u32 },
 413 | }
 414 | 
 415 | /// A loaded workbook: the model root of its sheets, their cells and its skin layout at the
 416 | /// document's first revision, each sheet's records beside it, and the package they were read
 417 | /// from.
 418 | #[derive(Clone, Debug)]
 419 | pub struct LoadedWorkbook {
 420 |     package: Arc<Package>,
 421 |     model: Arc<ModelRoot>,
 422 |     workbook_part: String,
 423 |     sheets: Arc<[LoadedSheet]>,
 424 |     pivot_caches: Arc<pivot::PivotCaches>,
 425 |     cell_formats: u32,
 426 |     styles: Arc<StyleTable>,
 427 |     theme_colors: Arc<[String]>,
 428 |     date_system: DateSystem,
 429 |     compatibility_version: u32,
 430 |     calc_mode: CalcMode,
 431 |     /// The sheet `<workbookView activeTab>` names (dc-uar).
 432 |     active_sheet: Option<SheetId>,
 433 |     defined_names_not_loaded: usize,
 434 |     manifest: ManifestState,
 435 |     layout_warnings: Vec<LayoutWarning>,
 436 |     repairs: Vec<LoadRepair>,
 437 |     /// The workbook-scoped defined names no layout save removes.
 438 |     fixed_names: Vec<String>,
 439 | }
 440 | 
````

### Original model, format and VBA APIs [CODE]

Source: `crates/calc-doc/src/load/mod.rs:929` (lines 929–948, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/load/mod.rs#L929-L948).

````text
 929 |     pub fn model(&self) -> &Arc<ModelRoot> {
 930 |         &self.model
 931 |     }
 932 | 
 933 |     /// The file kind declared by the workbook part, independent of its file name.
 934 |     pub fn spreadsheet_format(&self) -> crate::SpreadsheetFormat {
 935 |         crate::SpreadsheetFormat::from_content_type(
 936 |             self.package
 937 |                 .part(&self.workbook_part)
 938 |                 .and_then(|p| p.content_type())
 939 |                 .unwrap_or(""),
 940 |         )
 941 |         .expect("load checked the workbook content type")
 942 |     }
 943 | 
 944 |     /// The workbook part the package's relationships name.
 945 |     pub fn workbook_part(&self) -> &str {
 946 |         &self.workbook_part
 947 |     }
 948 | 
````

### Repairs reporting and snapshot construction [CODE]

Source: `crates/calc-doc/src/load/mod.rs:1084` (lines 1084–1093, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/load/mod.rs#L1084-L1093).

````text
1084 |     pub fn layout_warnings(&self) -> &[LayoutWarning] {
1085 |         &self.layout_warnings
1086 |     }
1087 | 
1088 |     /// The defects Excel would have repaired on opening the file, which the load repaired the
1089 |     /// way Excel does ([`LoadRepair`]). Empty for a file Excel opens without a prompt.
1090 |     pub fn repairs(&self) -> &[LoadRepair] {
1091 |         &self.repairs
1092 |     }
1093 | 
````

### Tolerant load and preserved-sheet admission [CODE]

Source: `crates/calc-doc/src/load/mod.rs:1881` (lines 1881–1919, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/load/mod.rs#L1881-L1919).

````text
1881 |     let mut repairs = Vec::new();
1882 |     for (index, (declared, &id)) in declared.iter().zip(&issued).enumerate() {
1883 |         if let Some(was) = declared.repeated_key {
1884 |             repairs.push(LoadRepair::DuplicateSheetId {
1885 |                 sheet: id,
1886 |                 id: was,
1887 |                 now: declared.key.get(),
1888 |             });
1889 |         }
1890 |         let loaded = if declared.no_part.is_some() {
1891 |             if let Some(NoPart::Missing(missing)) = &declared.no_part {
1892 |                 repairs.push(LoadRepair::SheetRelationshipMissing {
1893 |                     sheet: id,
1894 |                     id: missing.clone(),
1895 |                 });
1896 |             }
1897 |             let name = format!("(sheet {} has no part)", index + 1);
1898 |             Ok((sheet::partless_sheet(&name), Vec::new()))
1899 |         } else if declared.preserved.is_some() {
1900 |             sheet::preserved_sheet(package, &declared.part).map(|loaded| (loaded, Vec::new()))
1901 |         } else {
1902 |             sheet::load_sheet(package, &context, id, &declared.part, declared.macro_sheet)
1903 |                 .and_then(|loaded| Ok((loaded, tables::read_tables(package, &declared.part, id)?)))
1904 |         };
1905 |         match loaded {
1906 |             Ok((loaded, sheet_tables)) => {
1907 |                 if !loaded.conditional_sources.is_empty() {
1908 |                     conditional_sources.push((id, loaded.conditional_sources));
1909 |                 }
1910 |                 cells.extend(loaded.inputs);
1911 |                 repairs.extend(loaded.dropped_array_formulas.iter().map(|&(row, column)| {
1912 |                     LoadRepair::FormulaInsideArray {
1913 |                         sheet: id,
1914 |                         row,
1915 |                         column,
1916 |                     }
1917 |                 }));
1918 |                 arrays.extend(loaded.arrays);
1919 |                 data_tables.extend(loaded.data_tables.iter().copied());
````

### Host state ownership and open report [CODE]

Source: `crates/calc-host/src/state.rs:95` (lines 95–148, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/state.rs#L95-L148).

````text
  95 | /// counts without treating a repaired workbook as a clean open.
  96 | #[derive(Clone, Debug, PartialEq, Eq)]
  97 | pub struct OpenReport<'a> {
  98 |     pub repairs: &'a [calc_doc::LoadRepair],
  99 |     pub repair_counts: std::collections::BTreeMap<&'static str, u64>,
 100 | }
 101 | 
 102 | /// One open workbook: the workbook calc-doc loaded from a package (which keeps the package for a
 103 | /// later save), the current authored revision of its model, and the engine state at that
 104 | /// revision. The host is the one owner that changes either: [`crate::dispatch`] commits an edit
 105 | /// to both, [`crate::step`] drives the engine's recalculation, and [`crate::project`] reads both.
 106 | /// The authored revision and the engine state are always at one revision.
 107 | #[derive(Debug)]
 108 | pub struct HostState {
 109 |     /// Identifies this open host for revision-bound prepared commands.
 110 |     pub(crate) command_scope: u64,
 111 |     pub(crate) sheet_ids: IdAllocator<SheetId>,
 112 |     pub(crate) sheet_identities: std::collections::BTreeMap<calc_doc_model::SheetKey, SheetId>,
 113 |     /// The workbook as calc-doc loaded it. Its model is the loaded revision, never the current
 114 |     /// one, which is `model`.
 115 |     pub(crate) loaded: LoadedWorkbook,
 116 |     pub(crate) model: Arc<ModelRoot>,
 117 |     pub(crate) engine: EngineState,
 118 |     pub(crate) options: HostOptions,
 119 |     /// Regional meanings of undeclared built-ins; authored style records retain their IDs.
 120 |     pub(crate) builtin_formats: std::collections::BTreeMap<u32, String>,
 121 |     /// The recalculation in progress, between two [`crate::step`] calls.
 122 |     pub(crate) work: Option<RecalcWork>,
 123 |     pub(crate) async_records: calc_engine::AsyncReturns,
 124 |     pub(crate) last_calculation_completion: Option<calc_engine::CompletionReceipt>,
 125 |     pub(crate) pinned_calculations: std::rc::Rc<std::cell::Cell<usize>>,
 126 |     /// Revokes owner-held continuations even when no cell calculation is active.
 127 |     pub(crate) calculation_authority: u64,
 128 |     pub(crate) current_facts: Option<Arc<crate::facts::HostFacts>>,
 129 |     pub(crate) admitted_model_snapshots:
 130 |         std::cell::RefCell<Vec<std::sync::Weak<calc_doc_model::DataModel>>>,
 131 |     pub(crate) cube_cache:
 132 |         std::cell::RefCell<Option<(Arc<calc_doc_model::DataModel>, calc_func::CubeLookup)>>,
 133 |     pub(crate) functions: calc_fml::CallableTable,
 134 |     /// The host's way to fetch a URL a workbook names; none gives Excel's failed-fetch results.
 135 |     pub(crate) fetch_route: Option<std::rc::Rc<dyn calc_func::FetchRoute>>,
 136 |     /// The host's way to read a file IMPORTCSV and IMPORTTEXT name; none gives Excel's unreadable-file result.
 137 |     pub(crate) import_route: Option<std::rc::Rc<dyn calc_func::ImportRoute>>,
 138 |     pub(crate) rtd_inputs: std::rc::Rc<calc_fml::rtd::RtdLedger>,
 139 |     /// The owner's capability to join or start RTD topics while this host evaluates, and the
 140 |     /// provisional values it returned that this host must admit before publishing.
 141 |     pub(crate) rtd_connector: Option<std::rc::Rc<dyn calc_fml::RtdConnector>>,
 142 |     /// The sheet and cell the person has active (the session's selection), which a VBA
 143 |     /// function's `ActiveSheet` and `Selection` answer (dc-hsa5.12). `None` until a session
 144 |     /// selects; the workbook's active sheet and its A1 stand in until then.
 145 |     pub(crate) user_view: Option<calc_func::UserView>,
 146 |     pub(crate) rtd_connected: crate::recalc::ConnectedQueue,
 147 |     /// Every RTD value admitted since the owner last asked, in admission order.
 148 |     pub(crate) rtd_admissions: Vec<crate::RtdAdmission>,
````

### Document edit preparation and structural rewrite interface [CODE]

Source: `crates/calc-doc/src/edit.rs:169` (lines 169–196, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/edit.rs#L169-L196).

````text
 169 | pub fn prepare_edit(model: &ModelRoot, batch: &OpBatch) -> Result<PreparedEdit, EditError> {
 170 |     check_arrays(model, batch)?;
 171 |     let edited = model.apply(batch).map_err(EditError::Model)?;
 172 |     for (operation, op) in batch.operations.iter().enumerate() {
 173 |         if let WorkbookOp::SetCell {
 174 |             input: CellInput::Error(code),
 175 |             ..
 176 |         } = op
 177 |             && !error_constant_writable(*code)
 178 |         {
 179 |             return Err(EditError::ErrorConstant {
 180 |                 operation,
 181 |                 code: *code,
 182 |             });
 183 |         }
 184 |         let (WorkbookOp::SetCell {
 185 |             input: CellInput::Formula(formula),
 186 |             ..
 187 |         }
 188 |         | WorkbookOp::SetArray {
 189 |             input: CellInput::Formula(formula),
 190 |             ..
 191 |         }) = op
 192 |         else {
 193 |             continue;
 194 |         };
 195 |         check_formula(formula).map_err(|check| match check {
 196 |             FormulaCheck::TooLong(units) => EditError::FormulaTooLong { operation, units },
````

### Structural edit document route [CODE]

Source: `crates/calc-doc/src/edit.rs:257` (lines 257–285, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/edit.rs#L257-L285).

````text
 257 | pub fn prepare_structural_edit(
 258 |     model: &ModelRoot,
 259 |     batch: &OpBatch,
 260 |     rewriter: &dyn FormulaRewriter,
 261 | ) -> Result<PreparedEdit, EditError> {
 262 |     let mut current: Option<Arc<ModelRoot>> = None;
 263 |     for (operation, op) in batch.operations.iter().enumerate() {
 264 |         let WorkbookOp::EditAxis(edit) = op else {
 265 |             return Err(EditError::NotAxisEdit { operation });
 266 |         };
 267 |         let next = {
 268 |             let root: &ModelRoot = current.as_deref().unwrap_or(model);
 269 |             check_axis_edit(root, operation, *edit)?;
 270 |             let single = OpBatch {
 271 |                 base: root.revision(),
 272 |                 operations: vec![op.clone()],
 273 |             };
 274 |             root.apply_structural(&single, rewriter, &mut no_sheet)
 275 |                 .map_err(EditError::Model)?
 276 |         };
 277 |         current = Some(next);
 278 |     }
 279 |     let edited = model
 280 |         .apply_structural(batch, rewriter, &mut no_sheet)
 281 |         .map_err(EditError::Model)?;
 282 |     Ok(PreparedEdit { model: edited })
 283 | }
 284 | 
 285 | /// The first and last positions of an area on an axis.
````

### Host validates base revision before edit [CODE]

Source: `crates/calc-host/src/command.rs:323` (lines 323–354, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/command.rs#L323-L354).

````text
 323 | pub fn dispatch(
 324 |     host: &mut HostState,
 325 |     command: HostCommand,
 326 | ) -> Result<CommandReceipt, CommandError> {
 327 |     dispatch_slice(host, command.base, &command.edits)
 328 | }
 329 | 
 330 | pub fn dispatch_slice(
 331 |     host: &mut HostState,
 332 |     base: AuthoredRevision,
 333 |     edits: &[CellEdit],
 334 | ) -> Result<CommandReceipt, CommandError> {
 335 |     if edits.is_empty() {
 336 |         return Err(CommandError::NoEdits);
 337 |     }
 338 |     let current = host.model.revision();
 339 |     if base != current {
 340 |         return Err(CommandError::StaleBase {
 341 |             command: base,
 342 |             current,
 343 |         });
 344 |     }
 345 |     let batch = OpBatch {
 346 |         base,
 347 |         operations: edits.iter().map(CellEdit::to_operation).collect(),
 348 |     };
 349 | 
 350 |     let pivot_change = batch
 351 |         .operations
 352 |         .iter()
 353 |         .any(|op| matches!(op, WorkbookOp::SetPivots(_)));
 354 |     let refuse = |message: &str| {
````

### Prepare both document and engine, then commit [CODE]

Source: `crates/calc-host/src/command.rs:920` (lines 920–956, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/command.rs#L920-L956).

````text
 920 |             {
 921 |                 return Err(CommandError::Document(EditError::SheetOperation(
 922 |                     "Renaming or deleting sheets with layout, table, rule, chart or other package references is not modelled (dc-971p); nothing changed"
 923 |                         .to_owned(),
 924 |                 )));
 925 |             }
 926 |         }
 927 |         let identity = if let calc_doc_model::SheetEdit::Insert { key, .. } = edit {
 928 |             Some(*host.sheet_identities.entry(*key).or_insert_with(|| host.sheet_ids.issue().expect("sheet identity space")))
 929 |         } else { None };
 930 |         calc_doc::prepare_sheet_edit(&host.model, &batch, &calc_engine::ReferenceRewriter,
 931 |             &mut || identity.expect("only an insertion issues a sheet"))
 932 |     } else if axis_edits.is_empty() {
 933 |         calc_doc::prepare_edit(&host.model, &batch)
 934 |     } else {
 935 |         calc_doc::prepare_structural_edit(&host.model, &batch, &calc_engine::ReferenceRewriter)
 936 |     }
 937 |     .map_err(CommandError::Document)?;
 938 | 
 939 |     let formula_cells_before = host.engine.node_count();
 940 |     let pending_before = host.engine.pending().count();
 941 |     let engine = calc_engine::prepare_edit(&mut host.engine, &host.model, &batch)
 942 |         .map_err(CommandError::Engine)?;
 943 | 
 944 |     let tables_changed = batch.operations.iter().any(|op| match op {
 945 |         WorkbookOp::SetTables(next) => {
 946 |             let old = host.model.definitions().tables();
 947 |             old.len() != next.len()
 948 |                 || old.iter().zip(next).any(|(a, b)| {
 949 |                     a.name != b.name
 950 |                         || a.display_name != b.display_name
 951 |                         || a.sheet != b.sheet
 952 |                         || a.area != b.area
 953 |                         || a.header_rows != b.header_rows
 954 |                         || a.totals_rows != b.totals_rows
 955 |                         || a.columns != b.columns
 956 |                         || a.filter != b.filter
````

### Reference rewriting for structural operations [CODE]

Source: `crates/calc-engine/src/structural.rs:1` (lines 1–82, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-engine/src/structural.rs#L1-L82).

````text
   1 | //! Structural edits (L3, dc-m3u): the formula rewriter the model applies them with, and the
   2 | //! engine's share of them.
   3 | //!
   4 | //! [`ReferenceRewriter`] is the [`calc_doc_model::FormulaRewriter`] that
   5 | //! [`calc_doc_model::ModelRoot::apply_structural`] rewrites every formula and defined name with:
   6 | //! CalcFml's [`calc_fml::reference::transform_formula`] over the sheets of the root before the
   7 | //! edit. [`crate::prepare_edit`] stages a structural batch as a move of every formula node's
   8 | //! anchor (or its removal, when the edit deletes its cell or sheet), and [`crate::commit_edit`]
   9 | //! moves the nodes and their published results, keeps their IDs, and leaves every node unbound
  10 | //! and pending: the next recalculation binds each formula's rewritten text and calculates it.
  11 | //! Nothing of the graph is patched in place; that refinement (ceng-m4.1's) is not done.
  12 | //!
  13 | //! [`relocate_formula`] is the text a copy of a formula cell writes in another cell, and
  14 | //! [`move_references`] the text a move (cut and paste) of cells leaves a formula with (dc-09v).
  15 | 
  16 | use crate::read::SnapshotCatalog;
  17 | use calc_doc_model::{
  18 |     Axis, AxisChange, FormulaRewriter, FormulaText, ModelRoot, NameScope, StructuralChange,
  19 | };
  20 | use calc_fml::reference::{
  21 |     EditAxis, ReferenceNotation, StructuralEdit, convert_notation, transform_formula,
  22 |     transform_name,
  23 | };
  24 | use calc_value::{CellKey, ExcelText, GridArea};
  25 | 
  26 | /// The formula rewriter of structural edits: CalcFml's reference transform over the root before
  27 | /// the edit.
  28 | #[derive(Clone, Copy, Debug, Default)]
  29 | pub struct ReferenceRewriter;
  30 | 
  31 | fn fml_edit(root: &ModelRoot, change: &StructuralChange) -> StructuralEdit {
  32 |     match change {
  33 |         StructuralChange::Axis(edit) => {
  34 |             let axis = match edit.axis() {
  35 |                 Axis::Rows => EditAxis::Rows,
  36 |                 Axis::Columns => EditAxis::Columns,
  37 |             };
  38 |             match edit.change() {
  39 |                 AxisChange::Insert => StructuralEdit::Insert {
  40 |                     sheet: edit.sheet(),
  41 |                     axis,
  42 |                     first: edit.first(),
  43 |                     count: edit.count(),
  44 |                 },
  45 |                 AxisChange::Delete => StructuralEdit::Delete {
  46 |                     sheet: edit.sheet(),
  47 |                     axis,
  48 |                     first: edit.first(),
  49 |                     count: edit.count(),
  50 |                 },
  51 |             }
  52 |         }
  53 |         StructuralChange::RenameSheet { sheet, name } => StructuralEdit::RenameSheet {
  54 |             sheet: *sheet,
  55 |             name: name.text().to_string_lossy(),
  56 |         },
  57 |         StructuralChange::RemoveSheet { sheet } => StructuralEdit::RemoveSheet {
  58 |             sheet: *sheet,
  59 |             order: root
  60 |                 .sheets()
  61 |                 .iter()
  62 |                 .map(|entry| (entry.id(), entry.name().text().to_string_lossy()))
  63 |                 .collect(),
  64 |         },
  65 |     }
  66 | }
  67 | 
  68 | impl FormulaRewriter for ReferenceRewriter {
  69 |     fn relocate(
  70 |         &self,
  71 |         root: &ModelRoot,
  72 |         formula: &FormulaText,
  73 |         from: CellKey,
  74 |         to: CellKey,
  75 |     ) -> Option<FormulaText> {
  76 |         relocate_formula(root, formula, from, to)
  77 |     }
  78 | 
  79 |     fn formula(
  80 |         &self,
  81 |         root: &ModelRoot,
  82 |         formula: &FormulaText,
````

### Undo metadata and coordinate transformations [CODE]

Source: `crates/calc-host/src/history.rs:52` (lines 52–77, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/history.rs#L52-L77).

````text
  52 | /// What undoes a step.
  53 | #[derive(Clone, Debug)]
  54 | pub(crate) enum Inverse {
  55 |     /// The edits that restore every cell the step touched.
  56 |     Edits(Vec<CellEdit>),
  57 |     /// The revision before the step, brought back whole ([`crate::command::restore`]), with the
  58 |     /// axis edits that led to it.
  59 |     Restore {
  60 |         model: Arc<ModelRoot>,
  61 |         axis_edits: Vec<AxisEdit>,
  62 |         source: Option<calc_doc::LoadedWorkbook>,
  63 |     },
  64 | }
  65 | 
  66 | /// One undo step: a committed edit, the edits that undo it, and the selection before and after.
  67 | #[derive(Clone, Debug)]
  68 | pub(crate) struct Step {
  69 |     pub(crate) forward: Vec<CellEdit>,
  70 |     pub(crate) coordinates: Option<calc_fml::reference::StructuralEdit>,
  71 |     pub(crate) imported: Option<calc_doc::LoadedWorkbook>,
  72 |     pub(crate) inverse: Result<Inverse, Irreversible>,
  73 |     pub(crate) before: Selection,
  74 |     pub(crate) after: Selection,
  75 | }
  76 | 
  77 | /// Undo follows the surviving locations back; it never restores a deleted scope.
````

### Undo/redo stack retention and failed restoration [CODE]

Source: `crates/calc-host/src/history.rs:425` (lines 425–460, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/history.rs#L425-L460).

````text
 425 |     /// Records a committed edit: it becomes the step an undo takes back, the oldest step is
 426 |     /// dropped beyond the limit, and nothing is left to redo.
 427 |     pub(crate) fn record(&mut self, step: Step) {
 428 |         self.undo.push(step);
 429 |         if self.undo.len() > self.limit {
 430 |             self.undo.remove(0);
 431 |         }
 432 |         self.redo.clear();
 433 |     }
 434 | 
 435 |     pub(crate) fn pop_undo(&mut self) -> Option<Step> {
 436 |         self.undo.pop()
 437 |     }
 438 | 
 439 |     pub(crate) fn pop_redo(&mut self) -> Option<Step> {
 440 |         self.redo.pop()
 441 |     }
 442 | 
 443 |     /// Puts back a step an undo did not commit.
 444 |     pub(crate) fn restore_undo(&mut self, step: Step) {
 445 |         self.undo.push(step);
 446 |     }
 447 | 
 448 |     /// Puts back a step a redo did not commit.
 449 |     pub(crate) fn restore_redo(&mut self, step: Step) {
 450 |         self.redo.push(step);
 451 |     }
 452 | 
 453 |     /// A step an undo committed: a redo commits it again.
 454 |     pub(crate) fn undone(&mut self, step: Step) {
 455 |         self.redo.push(step);
 456 |     }
 457 | 
 458 |     /// A step a redo committed: an undo takes it back again.
 459 |     pub(crate) fn redone(&mut self, step: Step) {
 460 |         self.undo.push(step);
````

### Save frontier and prepare API [CODE]

Source: `crates/calc-host/src/save.rs:134` (lines 134–160, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/save.rs#L134-L160).

````text
 134 | pub struct SavePin {
 135 |     pub destination: calc_engine::contract::ResultDestination,
 136 |     pub workbook: calc_engine::contract::WorkbookInstanceId,
 137 |     pub model: calc_doc_model::AuthoredRevision,
 138 |     pub results: calc_engine::contract::ReadViewId,
 139 |     pub active_calculation: Option<calc_engine::contract::CalcCommandId>,
 140 |     pub cache_policy: SaveCachePolicy,
 141 | }
 142 | 
 143 | /// Owns the prepared package bytes and their source pin. Writing consumes only this plan,
 144 | /// so later edits, calculations, cancellation or replacement of the live host cannot mix views.
 145 | pub struct PreparedSave {
 146 |     pin: SavePin,
 147 |     plan: calc_doc::SavePlan,
 148 |     calculation: Option<calc_engine::contract::CompletionReceipt>,
 149 | }
 150 | impl PreparedSave {
 151 |     pub fn calculation(&self) -> Option<&calc_engine::contract::CompletionReceipt> {
 152 |         self.calculation.as_ref()
 153 |     }
 154 |     pub fn pin(&self) -> &SavePin {
 155 |         &self.pin
 156 |     }
 157 |     pub fn write(self, out: &mut dyn PackageWrite) -> Result<SaveReceipt, SaveFailed> {
 158 |         calc_doc::write(self.plan, out).map_err(SaveFailed::Write)
 159 |     }
 160 | }
````

### Save snapshots and cache provenance [CODE]

Source: `crates/calc-host/src/save.rs:308` (lines 308–347, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/save.rs#L308-L347).

````text
 308 |     let stale = engine
 309 |         .stale(model)
 310 |         .expect("the host keeps its authored revision and its engine state at one revision");
 311 |     let mut values = BTreeMap::new();
 312 |     let mut dirty_caches = BTreeSet::new();
 313 |     let mut spills = BTreeMap::new();
 314 |     let definitions = model.definitions();
 315 |     for sheet in model.sheets() {
 316 |         for (at, input) in model.sheet_cells(sheet.id()).into_iter().flatten() {
 317 |             if !matches!(input, CellInput::Formula(_)) {
 318 |                 continue;
 319 |             }
 320 |             let dirty = stale.binary_search(&at).is_ok();
 321 |             if cache_policy == SaveCachePolicy::PreservePermittedCaches
 322 |                 && definitions.array(at).is_none()
 323 |                 && engine.spill(at).is_none()
 324 |             {
 325 |                 if let Some(result) = engine.permitted_cache(at) {
 326 |                     let cache = match result {
 327 |                         FormulaResult::Calculated { value, .. } => cached(value),
 328 |                         FormulaResult::FileCached(cache) => Some(cache.value().clone()),
 329 |                         FormulaResult::NotModeled { .. } => None,
 330 |                     };
 331 |                     if let Some(cache) = cache {
 332 |                         values.insert(at, cache);
 333 |                         if dirty
 334 |                             || engine.calculation_required_at(at)
 335 |                             || matches!(result, FormulaResult::FileCached(_))
 336 |                         {
 337 |                             dirty_caches.insert(at);
 338 |                         }
 339 |                         continue;
 340 |                     }
 341 |                 }
 342 |             }
 343 |             if dirty {
 344 |                 continue;
 345 |             }
 346 |             let Some(FormulaResult::Calculated { value, .. }) = engine.result(at) else {
 347 |                 continue;
````

### Document snapshot source, authored model and result overlays [CODE]

Source: `crates/calc-doc/src/save/mod.rs:68` (lines 68–108, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/save/mod.rs#L68-L108).

````text
  68 | pub struct DocumentSnapshot {
  69 |     package: Arc<Package>,
  70 |     edit: Option<Edited>,
  71 | }
  72 | 
  73 | /// Cache metadata and the selected save-calculation policy. Reopen imports caches unverified.
  74 | #[derive(Clone)]
  75 | pub(crate) struct CalculationCacheState {
  76 |     pub(crate) dirty: BTreeSet<CellKey>,
  77 |     pub(crate) completed: bool,
  78 |     pub(crate) mode: crate::CalcMode,
  79 |     pub(crate) calculate_before_save: bool,
  80 | }
  81 | 
  82 | /// A revision of a loaded workbook's model and what the save rewrites it against.
  83 | #[derive(Clone)]
  84 | pub(crate) struct Edited {
  85 |     workbook_part: String,
  86 |     sheets: Arc<[LoadedSheet]>,
  87 |     /// Whether and where the loaded package's DNA manifest was read.
  88 |     manifest: ManifestState,
  89 |     /// The model as loaded.
  90 |     base: Arc<ModelRoot>,
  91 |     /// The revision to save.
  92 |     model: Arc<ModelRoot>,
  93 |     /// Supplied cached values selected by the caller's policy; these grant no calculation or
  94 |     /// freshness proof. `None` saves without them.
  95 |     values: Option<Arc<BTreeMap<CellKey, CachedValue>>>,
  96 |     /// The area each array formula's result fills, by anchor
  97 |     /// ([`crate::LoadedWorkbook::snapshot_with_results`]).
  98 |     spills: Arc<BTreeMap<CellKey, GridArea>>,
  99 |     /// The rows and columns inserted and deleted since the load, which the save moves the
 100 |     /// package's rows, columns and what lies on them by ([`DocumentSnapshot::with_structural_edits`]).
 101 |     structural: Option<StructuralEdits>,
 102 |     cache_state: Option<CalculationCacheState>,
 103 |     pivot_outputs: Arc<BTreeMap<CellKey, calc_doc_model::value::CalcValue>>,
 104 |     /// Custom XML items the save writes (added, or replacing the item of the same namespace).
 105 |     custom_xml: Vec<crate::CustomXmlItem>,
 106 | }
 107 | 
 108 | impl DocumentSnapshot {
````

### Save formats, policies and refusals [CODE]

Source: `crates/calc-doc/src/save/mod.rs:267` (lines 267–310, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/save/mod.rs#L267-L310).

````text
 267 | pub enum SaveFormat {
 268 |     /// SpreadsheetML (.xlsx/.xlsm), including conversion of XLSB inputs.
 269 |     #[default]
 270 |     Xml,
 271 |     /// Explicit workbook/template conversion; package content is retained.
 272 |     Spreadsheet(SpreadsheetFormat),
 273 |     /// Preserve an existing XLSB source when the snapshot needs no part rewrites.
 274 |     /// Binary authoring remains owned by dc-snz.14.
 275 |     Xlsb,
 276 | }
 277 | 
 278 | /// The format and bounds a save is planned under.
 279 | #[derive(Clone, Debug, PartialEq, Eq)]
 280 | pub struct SavePolicy {
 281 |     /// Immutable first-window state captured by the application for this save only.
 282 |     pub views: Option<Arc<SaveViews>>,
 283 |     pub format: SaveFormat,
 284 |     /// The longest package written, in bytes. A package past 4 GiB - 1 byte, the most a ZIP
 285 |     /// archive without ZIP64 records addresses, is refused whatever this says, since this writer
 286 |     /// writes no ZIP64 records.
 287 |     pub max_package_bytes: u64,
 288 | }
 289 | 
 290 | impl Default for SavePolicy {
 291 |     fn default() -> Self {
 292 |         SavePolicy {
 293 |             views: None,
 294 |             format: SaveFormat::Xml,
 295 |             max_package_bytes: u64::from(u32::MAX),
 296 |         }
 297 |     }
 298 | }
 299 | 
 300 | #[derive(Clone, Debug, PartialEq, Eq)]
 301 | pub struct SaveViews {
 302 |     pub revision: u64,
 303 |     pub active_sheet: Option<calc_doc_model::value::SheetId>,
 304 |     pub sheets: BTreeMap<calc_doc_model::value::SheetId, crate::SheetView>,
 305 | }
 306 | 
 307 | /// Why [`prepare_save`] refused a snapshot. A refusal is decided before any byte is written: no
 308 | /// plan exists, the package the document was opened from is unchanged, and so is the model
 309 | /// revision the caller holds.
 310 | #[derive(Debug, PartialEq, Eq)]
````

### Save prepare: unchanged source versus authored rewrite [CODE]

Source: `crates/calc-doc/src/save/mod.rs:926` (lines 926–1010, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/save/mod.rs#L926-L1010).

````text
 926 | pub fn prepare_save(
 927 |     snapshot: &DocumentSnapshot,
 928 |     policy: &SavePolicy,
 929 | ) -> Result<SavePlan, SaveRefusal> {
 930 |     let converted = &snapshot.package;
 931 |     let mut changes = match &snapshot.edit {
 932 |         Some(edit) => author::plan(converted, edit, policy.views.as_deref())?,
 933 |         None if policy.views.is_some() => {
 934 |             return Err(SaveRefusal::WindowView {
 935 |                 detail: "Window saving needs a loaded model".into(),
 936 |                 owner: "dc-971p.16",
 937 |             });
 938 |         }
 939 |         None => author::Changes::default(),
 940 |     };
 941 |     if let SaveFormat::Spreadsheet(format) = policy.format {
 942 |         let relationship = converted
 943 |             .relationships_of(None)
 944 |             .and_then(|r| {
 945 |                 r.items()
 946 |                     .iter()
 947 |                     .find(|r| r.kind.ends_with("/officeDocument"))
 948 |             })
 949 |             .and_then(|r| r.resolved.as_deref())
 950 |             .ok_or_else(|| SaveRefusal::InvalidPart {
 951 |                 part: "_rels/.rels".into(),
 952 |                 detail: "missing workbook relationship".into(),
 953 |             })?;
 954 |         if !format.macro_enabled()
 955 |             && (crate::vba::has_project(converted, relationship)
 956 |                 || converted.parts().iter().any(|p| {
 957 |                     p.content_type().is_some_and(|t| {
 958 |                         t.to_ascii_lowercase().contains("macrosheet")
 959 |                             || t.to_ascii_lowercase().contains("vbaproject")
 960 |                     })
 961 |                 }))
 962 |         {
 963 |             return Err(SaveRefusal::MacroLoss { owner: "dc-bctq" });
 964 |         }
 965 |         let xml = match changes.content_types.take() {
 966 |             Some(bytes) => crate::xml::XmlPart::new(
 967 |                 crate::opc::CONTENT_TYPES_ENTRY.into(),
 968 |                 bytes,
 969 |                 crate::xml::XmlLimits::default(),
 970 |             ),
 971 |             None => converted
 972 |                 .content_types_xml()
 973 |                 .map_err(|error| SaveRefusal::Part {
 974 |                     part: crate::opc::CONTENT_TYPES_ENTRY.into(),
 975 |                     error,
 976 |                 })?,
 977 |         };
 978 |         changes.content_types = Some(parts::content_types(
 979 |             &xml,
 980 |             Some(relationship),
 981 |             &[(relationship, format.content_type())],
 982 |         )?);
 983 |     }
 984 |     let package = match policy.format {
 985 |         SaveFormat::Xml | SaveFormat::Spreadsheet(_) => converted,
 986 |         SaveFormat::Xlsb => {
 987 |             let original = converted
 988 |                 .original_binary()
 989 |                 .ok_or(SaveRefusal::BinarySourceRequired)?;
 990 |             if !changes.rewritten.is_empty()
 991 |                 || changes.content_types.is_some()
 992 |                 || !changes.removed.is_empty()
 993 |                 || !changes.added.is_empty()
 994 |             {
 995 |                 return Err(SaveRefusal::BinaryChangesNotModelled { owner: "dc-snz.14" });
 996 |             }
 997 |             original
 998 |         }
 999 |     };
1000 |     let archive = package.archive();
1001 |     let mut new_content: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
1002 |     for (name, content) in changes.rewritten {
1003 |         let part = package
1004 |             .part(&name)
1005 |             .expect("a rewritten part is a part of the package");
1006 |         new_content.insert(part.entry(), content);
1007 |     }
1008 |     if let Some(content) = changes.content_types {
1009 |         new_content.insert(package.content_types_entry(), content);
1010 |     }
````

### dc-mndh: opaque chart/dialog references force refusal on rename [CODE]

Source: `crates/calc-doc/src/save/author.rs:387` (lines 387–416, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/save/author.rs#L387-L416).

````text
 387 |     // A macro sheet's formulas are not read, so references they hold cannot be moved: a
 388 |     // structural edit of a workbook with a macro sheet is refused (dc-xuw).
 389 |     if structural.is_some()
 390 |         && let Some(sheet) = sheets.iter().find(|sheet| sheet.is_macro_sheet())
 391 |     {
 392 |         return Err(SaveRefusal::MacroSheetChanged {
 393 |             part: sheet.part().name().to_owned(),
 394 |         });
 395 |     }
 396 |     // These parts can refer to any sheet by name (for example chart series). We cannot
 397 |     // transform their opaque references, so preserving their bytes after a rename/removal
 398 |     // would silently corrupt the saved workbook. Reordering/visibility changes remain safe.
 399 |     let sheet_names_changed = base.sheets().iter().any(|before| {
 400 |         model.sheet_by_id(before.id()).is_none_or(|after| after.name() != before.name())
 401 |     });
 402 |     if (structural.is_some() || sheet_names_changed)
 403 |         && let Some(sheet) = original_sheets.iter().find(|sheet| sheet.preserved_kind().is_some())
 404 |     {
 405 |         return Err(SaveRefusal::PreservedSheetChanged {
 406 |             part: sheet.part().name().to_owned(),
 407 |             kind: sheet.preserved_kind().unwrap_or_default(),
 408 |         });
 409 |     }
 410 |     let mut moved = match structural {
 411 |         Some(structural) => Some(structural::moved(
 412 |             package,
 413 |             original_sheets,
 414 |             base,
 415 |             structural,
 416 |         )?),
````

### Binary workbook format and preservation boundary [CODE]

Source: `crates/calc-doc/src/xlsb/mod.rs:1` (lines 1–69, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/xlsb/mod.rs#L1-L69).

````text
   1 | //! Excel binary workbooks (.xlsb, MS-XLSB; dc-5sr): a package whose workbook, worksheets, macro
   2 | //! sheets (dc-xuw), styles, shared strings, tables, external links, comments and metadata are
   3 | //! parts of binary records
   4 | //! instead of SpreadsheetML. [`crate::open`] reads such a package by converting it, before
   5 | //! anything else reads it, into the SpreadsheetML package it stands for: each binary part becomes
   6 | //! the XML part of the same content (`xl/workbook.bin` becomes `xl/workbook.xml`), with its
   7 | //! relationships and content types; every formula, stored as parsed tokens, is printed back as the
   8 | //! text Excel writes for it ([`formula`]). Parts that are the same in both forms (themes,
   9 | //! drawings, VML, images, printer settings, a VBA project, custom XML, document properties) are
  10 | //! carried over as they are; the binary index of each sheet, the calculation chain and the
  11 | //! volatile dependencies, which only speed up Excel's reading, are left out. The loader then reads
  12 | //! the converted package as it reads any, so the model of an .xlsb is the model of the .xlsx Excel
  13 | //! saves for it, and a save writes an .xlsx (an .xlsm for a workbook with a VBA project or a macro
  14 | //! sheet).
  15 | //!
  16 | //! A part of a kind this conversion does not read (a pivot table or cache, a query table, a
  17 | //! connection list, a slicer) refuses the workbook with a typed reason
  18 | //! ([`crate::OpenError::BinaryWorkbook`]), as does a record of a read kind or a formula token it
  19 | //! cannot read. The records a converted part leaves out are listed with it ([`sheet`],
  20 | //! [`styles`], [`parts`]); of them the loader reads only a data bar's Excel 2010 properties
  21 | //! (its lengths and fill), so a data bar converts with SpreadsheetML 2006's.
  22 | 
  23 | mod commands;
  24 | mod formula;
  25 | mod parts;
  26 | mod records;
  27 | mod sheet;
  28 | mod styles;
  29 | mod workbook;
  30 | 
  31 | use crate::OpenError;
  32 | use crate::opc::{Package, TargetMode};
  33 | use crate::save::text::{attribute_value, xstring};
  34 | use std::collections::BTreeMap;
  35 | 
  36 | pub(crate) use records::Malformed;
  37 | 
  38 | /// The content type of an Excel binary workbook's workbook part.
  39 | const BINARY_MAIN: &str = "application/vnd.ms-excel.sheet.binary.macroEnabled.main";
  40 | const OFFICE_DOCUMENT: &str =
  41 |     "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
  42 | pub(crate) const MAIN_NAMESPACE: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
  43 | pub(crate) const RELATIONSHIPS_NAMESPACE: &str =
  44 |     "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
  45 | 
  46 | /// The workbook part of an Excel binary workbook, when the package is one.
  47 | pub(crate) fn binary_workbook(package: &Package) -> Option<String> {
  48 |     let relationships = package.relationships_of(None)?;
  49 |     relationships
  50 |         .items()
  51 |         .iter()
  52 |         .filter(|item| item.kind == OFFICE_DOCUMENT && item.mode == TargetMode::Internal)
  53 |         .filter_map(|item| item.resolved.clone())
  54 |         .find(|part| {
  55 |             package
  56 |                 .part(part)
  57 |                 .and_then(|p| p.content_type())
  58 |                 .is_some_and(|ct| ct == BINARY_MAIN)
  59 |         })
  60 | }
  61 | 
  62 | /// Why a binary workbook was not converted: the part and what in it.
  63 | pub(crate) fn refusal(part: &str, detail: impl Into<String>) -> OpenError {
  64 |     OpenError::BinaryWorkbook {
  65 |         part: part.to_owned(),
  66 |         detail: detail.into(),
  67 |     }
  68 | }
  69 | 
````

### Source part and package retention [CODE]

Source: `crates/calc-doc/src/source.rs:62` (lines 62–91, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-doc/src/source.rs#L62-L91).

````text
  62 | pub trait PackageRead: Send + Sync {
  63 |     /// The source's length in bytes.
  64 |     fn size(&self) -> u64;
  65 | 
  66 |     /// Fills `buf` with the bytes starting at `offset`, or fails without a partial result.
  67 |     fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), SourceError>;
  68 | 
  69 |     /// The file name the package is known by (`Book1.xlsx`), without its folder, when the source
  70 |     /// knows it: Excel names the workbook so in reference text (`[Book1.xlsx]Sheet1!A1`). `None`
  71 |     /// by default.
  72 |     fn name(&self) -> Option<&str> {
  73 |         None
  74 |     }
  75 | }
  76 | 
  77 | /// The sink a saved package is written to, front to back.
  78 | pub trait PackageWrite {
  79 |     /// Appends `bytes`.
  80 |     fn write_all(&mut self, bytes: &[u8]) -> Result<(), SinkError>;
  81 | }
  82 | 
  83 | /// A package held in memory, as the browser receives a file.
  84 | #[derive(Clone, Debug)]
  85 | pub struct MemorySource {
  86 |     bytes: Arc<[u8]>,
  87 |     name: Option<Arc<str>>,
  88 | }
  89 | 
  90 | impl MemorySource {
  91 |     pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
````

### Commit publishes engine and authored root together [CODE]

Source: `crates/calc-host/src/command.rs:1021` (lines 1021–1050, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/command.rs#L1021-L1050).

````text
1021 |     let committed = calc_engine::commit_edit(engine);
1022 |     host.model = document.into_model();
1023 |     if tables_changed {
1024 |         host.engine = host.engine.rebuilt(&host.model);
1025 |     }
1026 |     for edit in &axis_edits {
1027 |         host.references.apply(&axis_transform(*edit));
1028 |     }
1029 |     if let Some(calc_doc_model::SheetEdit::Remove { sheet }) = sheet_edit {
1030 |         host.references
1031 |             .apply(&calc_fml::reference::StructuralEdit::RemoveSheet {
1032 |                 sheet: *sheet,
1033 |                 order: Vec::new(),
1034 |             });
1035 |     }
1036 |     if transformed_scope.is_some() {
1037 |         host.transformed_calculation_scope = transformed_scope;
1038 |     }
1039 |     host.calculation_cancelled = false;
1040 |     host.calc_events.push(calc_engine::CalcEvent::Committed {
1041 |         revision: host.model.revision(),
1042 |     });
1043 |     let superseded_recalc = host.work.as_ref().map(|work| work.tick());
1044 |     if changes_inputs
1045 |         || !axis_edits.is_empty()
1046 |         || sheet_edit
1047 |             .is_some_and(|edit| !matches!(edit, calc_doc_model::SheetEdit::SetVisibility { .. }))
1048 |     {
1049 |         host.inputs_changed_since_open = true;
1050 |     }
````

### Host exposes reader repairs to consumers [CODE]

Source: `crates/calc-host/src/state.rs:413` (lines 413–426, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/state.rs#L413-L426).

````text
 413 | 
 414 |     pub fn loaded(&self) -> &LoadedWorkbook {
 415 |         &self.loaded
 416 |     }
 417 | 
 418 |     /// The successful open's repair diagnostics, retained for the life of this host.
 419 |     pub fn open_report(&self) -> OpenReport<'_> {
 420 |         let repairs = self.loaded.repairs();
 421 |         let mut repair_counts = std::collections::BTreeMap::new();
 422 |         for repair in repairs {
 423 |             let kind = match repair {
 424 |                 calc_doc::LoadRepair::SheetRelationshipMissing { .. } => {
 425 |                     "sheet_relationship_missing"
 426 |                 }
````

### Round-trip tests cover undo, style state and authored values [CODE]

Source: `crates/calc-host/tests/round_trip.rs:74` (lines 74–101, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/tests/round_trip.rs#L74-L101).

````text
  74 |         assert_eq!(book.host().model().cell(b).cloned(), values[1]);
  75 |         let after = book.host().styles().clone();
  76 |         assert!(after.fonts.starts_with(&original.fonts));
  77 |         assert!(after.cell_formats.starts_with(&original.cell_formats));
  78 |         let authored = [book.host().cell_style(a), book.host().cell_style(b)];
  79 |         assert!(book.host().cell_alignment(a).unwrap().shrink_to_fit);
  80 |         assert!(book.dispatch(SkinIntent::Undo).accepted());
  81 |         assert_eq!(book.host().styles(), &original);
  82 |         assert_eq!(book.host().cell_style(a), old_a);
  83 |         assert_eq!(book.host().cell_style(b), old_b);
  84 |         assert!(book.dispatch(SkinIntent::Redo).accepted());
  85 |         assert_eq!(book.host().styles(), &after);
  86 |         let revision = book.host().revision();
  87 |         let refusal = book.dispatch(SkinIntent::Command(WorkbookCommand::FormatCells {
  88 |             areas,
  89 |             patch: FormatPatch {
  90 |                 font_size: Some("NaN".into()),
  91 |                 ..Default::default()
  92 |             },
  93 |         }));
  94 |         assert!(!refusal.accepted());
  95 |         assert_eq!(book.host().revision(), revision);
  96 |         assert_eq!(book.host().styles(), &after);
  97 |         let mut saved = Vec::new();
  98 |         book.save(&SavePolicy::default(), &mut saved).unwrap();
  99 |         let reopened = open(&saved);
 100 |         let r = reopened.sheets()[0].id();
 101 |         assert_eq!(reopened.styles().fonts, after.fonts);
````
