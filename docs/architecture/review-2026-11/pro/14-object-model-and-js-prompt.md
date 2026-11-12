# Question for GPT-6 Pro: one Excel object model projected through VBA, COM/XLL and JavaScript

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

Design the COM-style Excel application object model—Application, Workbooks/Workbook, Worksheets/Worksheet, Range, Names and events—and a JavaScript add-in projection over the **same core**. Decide whether that core belongs inside calc-host, above it in a full application host, or across a deliberately small boundary. It must serve CalcVBA early-bound and late-bound calls, external COM add-ins and XLL callbacks without spreadsheet semantics migrating into CalcVBA. Keep OneCalc's values-only profile explicit.

Cover stable workbook incarnations, sheet/name identities, Range identity versus an address at a revision, multi-area references and collections, equality, lifetime/release, objects retained in VBA globals, structural edits, close/reopen, undo and stale proxies. Specify typed portable host tables versus native IUnknown/IDispatch/dual vtables, type libraries, DISPIDs versus slots, hidden LCID/result parameters, ByRef copyback, enumerators, defaults, optional arguments and error translation. Do not assume the current local Book1 COM shell already projects the real HostSession. Reconcile the earlier own-GUID recommendation with current use of Excel IIDs for selected compatibility slots.

For JS, design an Office.js-like Excel.run, RequestContext, proxy objects, object paths, load/sync batching, tracked objects, custom functions and events. Give an example batch containing a read, a write and a calculation, and define when it observes state and publishes errors. Decide atomicity, partial failure, ordering, invalidated paths, cancellation, batching across awaits and permission changes. Distinguish actual Office.js compatibility from a familiar syntax we merely propose. No supplied source establishes a working JS object model. Explain how one command/query/event core can support synchronous COM/VBA and asynchronous JS without creating a second workbook, second graph or second identity namespace.

Trace macro writes through host edits, dependency invalidation, recalculation and skin publication. Distinguish UDF callbacks from macros/commands; cached Range.Value/Value2 observations from dependency reads; Formula versus Formula2 setters; WorksheetFunction from Evaluate; Calculate, Change/Calculate/AfterCalculate events and event-handler writes. Address owner-thread reentry, STA message pumping, nested calls, DoEvents, synchronous cancel events, EnableEvents, event suppression and deadlocks while the engine awaits VBA.

Explain Safe/Full policy enforcement at compile/bind/dispatch time, safe host-owned objects versus arbitrary COM activation, worksheet write permissions, nested policies and audit propagation. A Full grant cannot by itself make a worksheet UDF write legal. Review what survives of the current read-only SpreadsheetContext, SpreadsheetHost object table, event objects and COM projection.

The pseudo object model added in commits ecd1d610 and b28341ab was deliberately removed under dc-svqh. Appendix history at this pin records its Range metadata/Caller/Union experiment and the hidden-LCID ABI crash. Recover useful evidence without restoring a fake workbook in OneCalc. Propose the smallest real full-host route and experiments for retained ranges, name scopes, callback reads, event writes and matching early-/late-bound errors.

## What was already tried

Already tried: October's VBA review separated procedure calls from host objects and recommended stable identities, common semantics with platform ABIs and generated adapters. November's limited COM Range/Union/Caller experiment validated selected metadata and exposed an LCID ABI defect; owner policy then removed that pseudo model from OneCalc. The current read-only VBA projection and local native COM shell remain separate surfaces to evaluate. These are historical conclusions and code facts, not evidence that a complete Excel or JS object model exists.

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

### Tracker record dc-svqh [DESIGN / REPORTED CHECKS]

Selected fields from `.beads/issues.jsonl:524` at the DnaCalc pin; formatted for readability, other fields omitted. [GitHub record](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/.beads/issues.jsonl#L524).

````json
{
  "id": "dc-svqh",
  "title": "OneCalc values-only function host: refuse As Range, Caller and ThisCell; remove the pseudo object model",
  "description": "Owner 2026-11-11 policy: OneCalc hosts functions, DNA Calc hosts workbooks. One implementation of the Excel contract in the shared host crates, two profiles: OneCalc's values-only profile refuses what needs a workbook with a typed 'needs a workbook: open in DNA Calc' refusal; DNA Calc gets the full profile (its real COM object model is built higher in the stack later). For OneCalc: (1) VBA functions with As Range parameters are refused at registration; (2) Application.Caller and ThisCell refused (start here; not a permanent rule: a fixed fake value may come later; do not build a cult of tests, one test each is enough); (3) Application.Volatile and xlfVolatile stay (OneCalc has a volatile flag; F9 recalcs by state); Application.WorksheetFunction stays/maps to our library; (4) XLL: reference parameter types (R/U) and macro-sheet-equivalent registrations (# suffix) stay allowed: OneCalc simply never passes a reference; (5) remove the pseudo object model added for OneCalc in dc-6332.12 (ecd1d610, b28341ab: Application.Range/Union/Caller, name-only Range, type info) from OneCalc; git history keeps it for DNA Calc's object model; (6) OneCalc acceptance: the three generated-add-in cases that need references or Caller (Native_ReferenceKind, Native_OptionalReference, Native_CallerRow) become DNA Calc-only cases (wait for dc-0f1k.23), reported as such, not as failures; the 109-case simple-UDF set stays. Record the policy in dc-8ovd's doc when that is written. Read AGENTS.md first.",
  "status": "closed",
  "close_reason": "Done: values-only profile, pseudo object model removed, DNA Calc-only cases reported; cross-built m4 350/350 + 3, m5 simple 109/109 both routes"
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

### Earlier core layering, commands and VBA boundary [DESIGN]

Source: `docs/architecture/SYNTHESIS.md:215` (lines 215–249, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/SYNTHESIS.md#L215-L249).

````text
 215 | ### 1.6 CalcVBA hosting and early binding (owner 4.5; VBA-A, RT 7)
 216 | 
 217 | - **State at OxVba 0ad2ad28**: `ComCallEarly` carries the compiler descriptor to `ComHal::invoke_early_bound` in VM3
 218 |   and JIT; native x64 dual calls use vtables; remaining cost is per-call Variant vectors and a `HashMap`
 219 |   (`oxvba-vm3/src/lib.rs [historical locator omitted]`), libffi, a JIT helper and Variant-backed JIT frames (CORE-7).
 220 | - **Four tightenings (VBA-A)**: two call contracts (module procedures as typed callable entries with explicit state
 221 |   ownership; host objects as typed interfaces), sharing type descriptions but never making procedures COM objects;
 222 |   early binding is not low overhead; common semantics with explicit per-target ABIs (Windows COM, Linux native
 223 |   projection, wasm handles and generated adapters; `#[repr(C)]` fixes layout only); lifetime and observable state
 224 |   are part of the contract.
 225 | - **Ownership objects**: `ProjectImage`, `ProjectSession`, `BoundProcedure`, `Invocation` (appendix F). Entry
 226 |   profiles: typed native entry `(session, invocation, typed args, *result) -> CvStatus` reaching typed execution;
 227 |   bound typed-frame entry for the interpreter; a separate explicit Variant adapter. Genuine Variant signatures
 228 |   (`Range.Value2`, `WorksheetFunction.Sum`) stay Variant.
 229 | - **`CallableLease { project, generation, signature, code: CodeLease, state: ProjectStateLease }`** (RT 7): an old
 230 |   activation never runs against reset globals; replacement publishes at an execution boundary; leases may cover a
 231 |   batch. **`InvocationFrame { id, parent, view: ReadView, lease, mode, reentry: ReentryPolicy }`** carries
 232 |   continuations and permitted nesting.
 233 | - **Typed descriptors** cover ByRef aliasing and writeback, omitted/Empty/Null/Error, array bounds and element types,
 234 |   string and object ownership, error propagation (`Success`(may be `CVErr`), `UnhandledVbaError`,
 235 |   `ForeignCallFailure`, `Cancelled`, `BudgetExceeded`, `StaleBinding`, `HostExecutionAbort`), permitted callbacks
 236 |   and suspension. Performance promise: allocation-free steady-state **scalar** calls; objects, strings and arrays
 237 |   measured separately.
 238 | - **Range handles**: workbook instance and incarnation, sheet identity and generation, `AreaSet` (multi-area, not a
 239 |   bounding rectangle), structural-tracking policy; resolved against the invocation's read view; an index of live
 240 |   handles, not per-cell cost.
 241 | - **Object library** in `calc-object-model`, generated from one descriptor model into IDL/`.tlb` (MIDL or
 242 |   `CreateTypeLib2`), native vtables, portable adapters, interpreter adapters and JIT lowering, round-tripped through
 243 |   the importer. **GUIDs** (VBA-A, RT): DNA's own LIBID, CLSIDs, ProgIDs and IIDs with an explicit Excel
 244 |   source-compatibility mapping; an Excel IID is reused only by a facade implementing that interface's complete
 245 |   contract; type-library identity, activation identity and coexistence with Excel stay separate (decision 9).
 246 | - **Composition**: calc-* libraries depend only on `calcvba-host-api`; each app binary composes the runtime provider.
 247 | - **Security** (RT 7): trusted native vs sandboxed profiles with granted capabilities, memory/work limits and effect
 248 |   policy fixed at registration; a "pure" annotation proves nothing; a hard stop needs an isolation boundary.
 249 | 
````

### Earlier VBA answer: host objects, persistent ranges and generated projections [EARLIER-REVIEW]

Source: `docs/architecture/review-2026-10/pro/2-vba-hosting-answer.md:461` (lines 461–509, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-10/pro/2-vba-hosting-answer.md#L461-L509).

````text
 461 | ### 4.3 Persistent Range objects require stable workbook identity
 462 | 
 463 | Appendix D’s `(SheetId, area)` descriptor is a useful starting point, but insufficient as the complete contract.
 464 | 
 465 | A persistent reference needs enough identity to distinguish:
 466 | 
 467 | ```text
 468 | workbook identity and incarnation
 469 | worksheet identity and incarnation
 470 | reference/area identity
 471 | structural-update policy
 472 | ```
 473 | 
 474 | The current invocation supplies access to an authorized reader or mutation service. It must not supply a replacement identity for whichever workbook happens to be active.
 475 | 
 476 | A saved Range must never retain a borrowed evaluation reader. It should retain stable reference identity and resolve that identity against an appropriate current host context.
 477 | 
 478 | Specify behavior for row/column insertion, sheet deletion, workbook closure, reopened workbooks and recycled IDs. The correct Excel behavior for those cases belongs in the black-box corpus.
 479 | 
 480 | COM identity adds a separate requirement: interfaces belonging to one object must preserve the required `IUnknown` identity and stable interface support. Equal coordinates do not establish that two Range acquisitions must be the same COM object. [40]
 481 | 
 482 | Reference counting can remove the arbitrary lifetime cap. It does not prove that every Range-returning operation is allocation-free, eliminate every possible object table, or prevent ownership cycles.
 483 | 
 484 | The appropriate initial performance promise is **allocation-free steady-state scalar invocation**. Escaping objects, strings and arrays have legitimate allocation requirements.
 485 | 
 486 | ### 4.4 Generate the library and adapters from one model
 487 | 
 488 | The canonical object-library descriptor should generate:
 489 | 
 490 | - Windows IDL and `.tlb`.
 491 | - Native interface declarations and vtables.
 492 | - Portable interface declarations and adapters.
 493 | - Interpreter call adapters.
 494 | - JIT ABI-lowering descriptions.
 495 | - Dispatch metadata for supported dynamic paths.
 496 | - Layout and signature conformance manifests.
 497 | 
 498 | MIDL generates a type library from its library definitions; `CreateTypeLib2` provides a programmatic alternative with an explicit target `SYSKIND`. [41] [42]
 499 | 
 500 | Include default/source interfaces, appobject/global exposure, default members, accessor kinds, DISPIDs, logical slots, optional/default values, ownership and HRESULT/retval lowering in that model.
 501 | 
 502 | Round-trip the generated `.tlb` through the importer and compare it against the canonical descriptor. This tests the actual generator/importer boundary.
 503 | 
 504 | For DNA Calc’s known interfaces, generated adapters can meet the no-libffi hot-path target. Retaining a generic, prebound native marshaller for broader third-party COM signatures is compatible with early binding; it is a separate performance tier.
 505 | 
 506 | ### 4.5 Excel GUIDs: distinguish source compatibility from binary compatibility
 507 | 
 508 | My recommendation is:
 509 | 
````

### Added then removed: ecd1d610, b28341ab, dc-svqh [DESIGN]

Source: `docs/architecture/XLL_COM_HOSTING.md:1333` (lines 1333–1364, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/XLL_COM_HOSTING.md#L1333-L1364).

````text
1333 | ## 32. OneCalc COM0/COM2: Application.Range, Union, Caller (dc-6332.12), removed by dc-svqh
1334 | 
1335 | Superseded (owner policy 2026-11-11): OneCalc hosts functions, DNA Calc hosts workbooks. The pseudo
1336 | object model described here (commits ecd1d610 and b28341ab; the text below is kept as the record of
1337 | what was measured) is removed from calc-com-host. `Application.Caller` and `Application.ThisCell`
1338 | now answer error 1004 "needs a workbook: open in DNA Calc"; there is no `Application.Range` or
1339 | `Union`. The measurements stand as input for DNA Calc's object model.
1340 | 
1341 | 
1342 | Evidence: `crates/calc-xll-reference/traces/2026-11-11-x19-com`. The projection's `Application` has
1343 | `Range(Cell1, [Cell2])` (DISPID 0xc5, early-bound slot 40), `Union` (0x30b, slot 50) and `Caller`
1344 | (0x13d, slot 74); a Range carries its areas and answers `Address`, `Row`, `Column`, `Count`,
1345 | `Rows`, `Columns` (a Rows or Columns view counts the first area's dimension, as Excel does), and
1346 | `GetTypeInfo(0)` names it `Range`: a private dispinterface made in memory with CreateTypeLib2,
1347 | which is what VBA's `TypeName` of a late-bound object reads (the first host run without it
1348 | answered "IDispatch::<invoke-result>:1"). An address behind another sheet, a whole row or column
1349 | or malformed is error 1004 with its EXCEPINFO. `Caller` is the cell of the native call running on
1350 | the thread (`calc_xll_host::running_caller`), the #REF! error outside a call; CSE, spill and name
1351 | callers and the Index argument are refused. No `Value`, `Value2`, `Cells`, `Item`, `Areas`,
1352 | `Intersect` or `ActiveCell`: OneCalc carries no full object model and `Range.Value` reads stay
1353 | typed refusals (ONECALC_ADDINS.md lines 51, 122, 156; the answer's command and `Range.Value2`
1354 | suggestions are not implemented). The three calcvba-udf page failures pass: page acceptance 353 of
1355 | 353 (350 before), the 20 calcvba-udf cases 20 of 20, x13 65/68, x13-open 1/1, x14 128/145, x15
1356 | 19/21, x17 28/28 and the framework host route 333/333 unchanged. A real Excel-DNA client
1357 | (`fixture/X19Com.csproj`) through the primary interop assembly (early-bound vtable) and through
1358 | `dynamic` gets Excel's strings byte for byte (Version, Caller shape, Range metadata, Union, error
1359 | 1004 for another sheet); the PIA run found that `Caller` and `Union` take an `[lcid]` before the
1360 | return value (the first host build crashed in `get_Caller`), and `Range.Value2` answers
1361 | DISP_E_MEMBERNOTFOUND where Excel returns an array. QueueAsMacro's transport
1362 | (`Application.Run` of `SyncMacro_<guid>`, returning R8 0) is the existing one and was not changed:
1363 | OneCalc offers no commands (line 51), so an edit-aware command entry and `PermissionWrites`
1364 | belong to DNA Calc (dc-6332.13).
````

### Values-only profile and current native COM scope [DESIGN]

Source: `crates/calc-com-host/README.md:29` (lines 29–41, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-com-host/README.md#L29-L41).

````text
  29 | 
  30 | X3 adds the observed Window.Parent workbook, Workbook.Application/Name/FullName/Path,
  31 | Application.Workbooks/Path/EnableEvents and Workbooks.Count/Item. The measured
  32 | Workbook dual vtable has 289 slots and Workbooks has 26. These objects describe
  33 | one local Book1 shell; they do not enumerate or project arbitrary application
  34 | documents. xlOil's original native and Python plugins use this route in the
  35 | [X3 captures](../calc-xll-reference/traces/2026-11-01-x3/).
  36 | 
  37 | Values-only profile (dc-svqh, owner policy 2026-11-11): OneCalc hosts functions, DNA Calc hosts
  38 | workbooks. `Application.Caller` (DISPID 0x13d) and `Application.ThisCell` (0x7aa) answer a typed
  39 | refusal, Excel's error 1004 with the text "needs a workbook: open in DNA Calc"; there is no
  40 | `Application.Range`, `Union` or Range object with type information. The earlier pseudo object model
  41 | (dc-6332.12, commits ecd1d610 and b28341ab) stays in git history for DNA Calc's own object model.
````

### Native COM ownership and identity state [CODE]

Source: `crates/calc-com-host/src/projection.cpp:29` (lines 29–90, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-com-host/src/projection.cpp#L29-L90).

````text
  29 | struct State {
  30 |     HWND hwnd;
  31 |     DWORD thread;
  32 |     const void* context;
  33 |     Run run;
  34 |     Qualifier qualifier = nullptr;
  35 |     OwnsModule owns_module = nullptr;
  36 |     bool connected = true;
  37 |     bool enable_events = true;
  38 |     Object* workbook = nullptr;
  39 |     Object* workbooks = nullptr;
  40 |     std::wstring workbook_name = L"Book1";
  41 |     Object* worksheet = nullptr;
  42 |     std::wstring worksheet_name;
  43 |     DWORD next_cookie = 1;
  44 |     std::vector<std::pair<DWORD,IUnknown*>> sinks;
  45 |     Object* addins = nullptr;
  46 |     Object* bars = nullptr;
  47 |     HRESULT office_failure = S_OK;
  48 |     bool office_closing = false;
  49 | };
  50 | struct Object {
  51 |     void** vtable;
  52 |     volatile LONG references = 1;
  53 |     std::shared_ptr<State> state;
  54 |     Object* application;
  55 |     bool is_application;
  56 |     Object* parent = nullptr;
  57 |     bool is_workbook = false;
  58 |     bool is_collection = false;
  59 |     bool is_worksheet = false;
  60 |     bool is_range = false;
  61 |     unsigned row = 0, column = 0;
  62 |     OfficeRole office = OfficeRole::None;
  63 |     std::vector<Object*> children;
  64 |     Object* controls = nullptr;
  65 |     std::wstring caption, action;
  66 |     bool deleted = false;
  67 |     std::shared_ptr<ComAddinState> com_addin;
  68 | };
  69 | bool retain_office_owner(Object*);
  70 | ULONG WINAPI add_ref(Object* object) { return InterlockedIncrement(&object->references); }
  71 | ULONG WINAPI release(Object* object) {
  72 |     LONG count = InterlockedDecrement(&object->references);
  73 |     if (!count) {
  74 |         if(retain_office_owner(object))return InterlockedIncrement(&object->references);
  75 |         for(auto child:object->children) release(child);
  76 |         if(object->controls) release(object->controls);
  77 |         if (!object->is_application) {
  78 |             if(object->parent && object->parent!=object->application) release(object->parent);
  79 |             release(object->application);
  80 |         }
  81 |         delete object;
  82 |     }
  83 |     return count;
  84 | }
  85 | HRESULT valid(Object* object) {
  86 |     if (!object->state->connected) return CO_E_OBJNOTCONNECTED;
  87 |     if(object->office!=OfficeRole::None&&object->deleted)return CO_E_OBJNOTCONNECTED;
  88 |     if (GetCurrentThreadId() != object->state->thread) return RPC_E_WRONG_THREAD;
  89 |     return S_OK;
  90 | }
````

### COM names and dispatch routing including needs-workbook refusal [CODE]

Source: `crates/calc-com-host/src/projection.cpp:382` (lines 382–421, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-com-host/src/projection.cpp#L382-L421).

````text
 382 | HRESULT needs_workbook(EXCEPINFO* error) {
 383 |     if(!error) return DISP_E_MEMBERNOTFOUND;
 384 |     *error=EXCEPINFO{};
 385 |     error->wCode=1004;
 386 |     error->scode=static_cast<SCODE>(0x800A03ECu);
 387 |     error->bstrSource=SysAllocString(L"DNA OneCalc");
 388 |     error->bstrDescription=SysAllocString(L"needs a workbook: open in DNA Calc");
 389 |     return DISP_E_EXCEPTION;
 390 | }
 391 | HRESULT WINAPI invoke(Object* object,DISPID id,REFIID iid,LCID,WORD flags,DISPPARAMS* params,VARIANT* result,EXCEPINFO* error,UINT*) {
 392 |     if(result) VariantInit(result);
 393 |     if(iid!=IID_NULL) return DISP_E_UNKNOWNINTERFACE;
 394 |     if(!params || (params->cArgs && !params->rgvarg)) return E_POINTER;
 395 |     HRESULT hr=valid(object); if(FAILED(hr)) return hr;
 396 |     if(object->office!=OfficeRole::None) {
 397 |         try{return office_invoke(object,id,flags,params,result);}catch(const std::bad_alloc&){return E_OUTOFMEMORY;}catch(...){return E_FAIL;}
 398 |     }
 399 |     if(object->is_application && (id==0x704 || id==0x59f) && (flags&DISPATCH_PROPERTYGET)) {
 400 |         if(params->cArgs) return DISP_E_BADPARAMCOUNT;
 401 |         IDispatch* value=nullptr;hr=id==0x704?get_addins(object,&value):get_bars(object,&value);
 402 |         if(SUCCEEDED(hr)) {if(result){result->vt=VT_DISPATCH;result->pdispVal=value;}else value->Release();} return hr;
 403 |     }
 404 |     if(object->is_application && id==0x795) {
 405 |         if(flags&DISPATCH_PROPERTYGET) {if(params->cArgs)return DISP_E_BADPARAMCOUNT;LONG value;hr=get_automation_security(object,&value);if(SUCCEEDED(hr)&&result){result->vt=VT_I4;result->lVal=value;}return hr;}
 406 |         return E_NOTIMPL;
 407 |     }
 408 |     if(object->is_application && id==0x103 && (flags&DISPATCH_METHOD)) return run(object,params,result);
 409 |     if(object->is_application && (id==0x13d || id==0x7aa) && (flags&DISPATCH_PROPERTYGET)) return needs_workbook(error);
 410 |     if(object->is_collection && (id==0xaa || id==0) && (flags&(DISPATCH_METHOD|DISPATCH_PROPERTYGET))) {
 411 |         if(params->cArgs!=1) return DISP_E_BADPARAMCOUNT;
 412 |         IDispatch* workbook=nullptr; hr=get_item(object,params->rgvarg[0],&workbook);
 413 |         if(SUCCEEDED(hr)) { if(result) { result->vt=VT_DISPATCH; result->pdispVal=workbook; } else workbook->Release(); } return hr;
 414 |     }
 415 |     if(object->is_application && id==0x4bc && (flags&DISPATCH_PROPERTYPUT)) {
 416 |         if(params->cArgs!=1) return DISP_E_BADPARAMCOUNT;
 417 |         VARIANT value{}; hr=VariantChangeType(&value,&params->rgvarg[0],0,VT_BOOL);
 418 |         if(SUCCEEDED(hr)) hr=put_enable_events(object,value.boolVal);
 419 |         VariantClear(&value); return hr;
 420 |     }
 421 |     if(!(flags&DISPATCH_PROPERTYGET)) return DISP_E_MEMBERNOTFOUND;
````

### Native application events and event argument construction [CODE]

Source: `crates/calc-com-host/src/projection.cpp:600` (lines 600–617, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-com-host/src/projection.cpp#L600-L617).

````text
 600 | extern "C" HRESULT dna_com_enable_events(void* pointer,bool enabled) noexcept {
 601 |     return put_enable_events(static_cast<Projection*>(pointer)->app,enabled?VARIANT_TRUE:VARIANT_FALSE);
 602 | }
 603 | extern "C" HRESULT dna_com_sheet_change(void* pointer,unsigned row,unsigned column) noexcept {
 604 |     auto projection=static_cast<Projection*>(pointer);
 605 |     if(row>=1048576 || column>=16384) return E_INVALIDARG;
 606 |     auto range=new(std::nothrow) Object{window_vtable.data(),1,projection->app->state,projection->app,false,projection->worksheet,false,false,false,true,row,column};
 607 |     if(!range) return E_OUTOFMEMORY;
 608 |     add_ref(projection->app); add_ref(projection->worksheet);
 609 |     VARIANT args[2]{}; args[0].vt=VT_DISPATCH; args[0].pdispVal=reinterpret_cast<IDispatch*>(projection->worksheet);
 610 |     args[1].vt=VT_DISPATCH; args[1].pdispVal=reinterpret_cast<IDispatch*>(range);
 611 |     HRESULT hr=fire(projection->app,0x61c,args,2); release(range); return hr;
 612 | }
 613 | extern "C" HRESULT dna_com_after_calculate(void* pointer) noexcept {
 614 |     return fire(static_cast<Projection*>(pointer)->app,0xa34,nullptr,0);
 615 | }
 616 | // Exercises source lifetime and reentrant subscription removal independently of
 617 | // xlOil. The application route is separately measured with the original plugin.
````

### Current read-only VBA host context [CODE]

Source: `crates/calc-vba/src/runtime/spreadsheet.rs:58` (lines 58–104, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-vba/src/runtime/spreadsheet.rs#L58-L104).

````text
  58 | pub trait SpreadsheetContext {
  59 |     /// A reentered worksheet UDF cannot use a surrounding document event Target.
  60 |     fn event_view(&self) -> bool {
  61 |         false
  62 |     }
  63 |     fn caller(&self) -> Option<Range>;
  64 |     /// The name of the sheet the person has active (`ActiveSheet.Name`), or why the host does not
  65 |     /// model one.
  66 |     fn active_sheet_name(&self) -> Result<String, ContextError> {
  67 |         Err(ContextError::Refused(
  68 |             "ActiveSheet (the host does not model the user's active sheet)".into(),
  69 |         ))
  70 |     }
  71 |     /// A range of the sheet the person has active, by A1 address (`ActiveSheet.Range("B2")`).
  72 |     fn active_range(&self, _address: &str) -> Result<Range, ContextError> {
  73 |         Err(ContextError::Refused(
  74 |             "ActiveSheet (the host does not model the user's active sheet)".into(),
  75 |         ))
  76 |     }
  77 |     /// The range the person has selected (`Selection`).
  78 |     fn selection(&self) -> Result<Range, ContextError> {
  79 |         Err(ContextError::Refused(
  80 |             "Selection (the host does not model the user's selection)".into(),
  81 |         ))
  82 |     }
  83 |     /// Record a capability refusal in the owning evaluator, independently of VBA On Error.
  84 |     fn refuse(&self, reason: &'static str) -> String {
  85 |         reason.into()
  86 |     }
  87 |     fn metadata(&self, range: Range) -> Result<RangeInfo, ContextError>;
  88 |     fn external_address(&self, _range: Range) -> Result<String, ContextError> {
  89 |         Err(self
  90 |             .refuse("reference names without an admitted metadata view")
  91 |             .into())
  92 |     }
  93 |     fn cell(&self, range: Range, row: u32, column: u32) -> Result<Range, ContextError>;
  94 |     fn area(&self, range: Range, index: u32) -> Result<Range, ContextError>;
  95 |     fn read_range(&self, range: Range) -> Result<Value, ContextError>;
  96 |     fn read_argument_range(&self, range: Range) -> Result<Value, ContextError> {
  97 |         self.read_range(range)
  98 |     }
  99 |     fn worksheet_function(&self, name: &str, args: &[Value]) -> Result<Value, ContextError>;
 100 |     fn set_volatile(&self, value: bool) -> Result<(), ContextError>;
 101 | }
 102 | 
 103 | scoped_tls_hkt::scoped_thread_local!(static CONTEXT: for<'a> (
 104 |     &'a (dyn SpreadsheetContext + 'a), &'a RefCell<Option<String>>, bool
````

### VBA object ownership and typed range table [CODE]

Source: `crates/calc-vba/src/runtime/spreadsheet.rs:194` (lines 194–234, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-vba/src/runtime/spreadsheet.rs#L194-L234).

````text
 194 | pub(crate) struct SpreadsheetHost {
 195 |     base: Arc<dyn HostServices>,
 196 |     objects: Mutex<ObjectTable>,
 197 |     native_events: bool,
 198 |     /// Platform COM for the objects of the project's type-library references (early-bound
 199 |     /// through their registered type libraries). DNA's own objects never reach it.
 200 |     com: Option<Arc<dyn HostServices>>,
 201 |     /// What the project's own source activates late-bound (refused).
 202 |     late: super::late_binding::LateBinding,
 203 | }
 204 | 
 205 | #[derive(Default)]
 206 | struct ObjectTable {
 207 |     by_id: std::collections::BTreeMap<i32, Object>,
 208 |     by_object: std::collections::BTreeMap<Object, i32>,
 209 |     selections: std::collections::BTreeMap<Range, calcvba_host_api::HostObject>,
 210 | }
 211 | 
 212 | const SELECTION_IID: calcvba_host_api::InterfaceId = *b"DnaCalcSelect001";
 213 | struct SelectionProjection {
 214 |     range: Range,
 215 |     // Retains the original projection's canonical identity for the native view.
 216 |     _object: ObjectRef,
 217 | }
 218 | const FIRST_OBJECT: i32 = 0x1000_0000;
 219 | const MAX_OBJECTS: usize = 4096;
 220 | const MAX_VALUE_CELLS: u64 = 1_048_576;
 221 | 
 222 | /// Range members the projection declares so that real add-in code compiles; reading them is a
 223 | /// typed refusal (their date and currency forms need the cell's number format), writing them is
 224 | /// what Excel forbids a worksheet function.
 225 | const RANGE_VALUE: i32 = 100;
 226 | 
 227 | /// The reason an explicit `CreateObject` or `GetObject` is refused.
 228 | const CREATE_OBJECT_REFUSAL: &str = "CreateObject or GetObject (late-bound COM activation; DNA OneCalc binds COM early, through imported type libraries)";
 229 | 
 230 | /// The reason a late-bound call on a COM object is refused (owner rule: early-bound COM through
 231 | /// imported type libraries first, never late binding as a start).
 232 | const LATE_BOUND_REFUSAL: &str =
 233 |     "a late-bound COM call (DNA OneCalc binds COM early, through imported type libraries)";
 234 | const RANGE_FORMULA: i32 = 101;
````

### Object interning, ownership and limit [CODE]

Source: `crates/calc-vba/src/runtime/spreadsheet.rs:353` (lines 353–396, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-vba/src/runtime/spreadsheet.rs#L353-L396).

````text
 353 |     fn foreign(&self, object: &ObjectRef) -> Option<&dyn ComHal> {
 354 |         match &self.com {
 355 |             Some(com) if !self.owns(object) => Some(com.com()),
 356 |             _ => None,
 357 |         }
 358 |     }
 359 | 
 360 |     fn object(&self, object: Object) -> HalResult<Variant> {
 361 |         let mut objects = self
 362 |             .objects
 363 |             .lock()
 364 |             .map_err(|_| fault(5, "object table is unavailable"))?;
 365 |         let id = match objects.by_object.get(&object) {
 366 |             Some(id) => *id,
 367 |             None => {
 368 |                 if objects.by_id.len() == MAX_OBJECTS {
 369 |                     return self.unsupported("spreadsheet object limit exceeded");
 370 |                 }
 371 |                 static NEXT: std::sync::atomic::AtomicI32 =
 372 |                     std::sync::atomic::AtomicI32::new(FIRST_OBJECT);
 373 |                 let id = NEXT
 374 |                     .fetch_update(
 375 |                         std::sync::atomic::Ordering::Relaxed,
 376 |                         std::sync::atomic::Ordering::Relaxed,
 377 |                         |id| id.checked_add(1),
 378 |                     )
 379 |                     .map_err(|_| fault(5, "spreadsheet object identities exhausted (dc-hsa5.4)"))?;
 380 |                 objects.by_id.insert(id, object);
 381 |                 objects.by_object.insert(object, id);
 382 |                 id
 383 |             }
 384 |         };
 385 |         Ok(Variant::from_object_ref(ObjectRef::from_compat_identity(
 386 |             id,
 387 |         )))
 388 |     }
 389 |     fn lookup(&self, object: &ObjectRef) -> HalResult<Object> {
 390 |         if let Some(range) = self.selection_range(object) {
 391 |             return Ok(Object::Selection(range));
 392 |         }
 393 |         // raw() is only defined for compatibility carriers, never a native IUnknown.
 394 |         if !object.is_compat_object() {
 395 |             return self
 396 |                 .unsupported("a native spreadsheet object outside the supported projection");
````

### CalcVBA adapter HostServices and native COM delegation [CODE]

Source: `crates/calc-vba/src/runtime/spreadsheet.rs:822` (lines 822–841, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-vba/src/runtime/spreadsheet.rs#L822-L841).

````text
 822 | impl HostServices for SpreadsheetHost {
 823 |     fn is_host_class(&self, target: &str) -> bool {
 824 |         [
 825 |             "DnaCalc.Application",
 826 |             "DnaCalc.ActiveSheet",
 827 |             "DnaCalc.Selection",
 828 |         ]
 829 |         .iter()
 830 |         .any(|class| target.eq_ignore_ascii_case(class))
 831 |     }
 832 |     fn profile(&self) -> HalProfileId {
 833 |         HalProfileId::Null
 834 |     }
 835 |     fn descriptor(&self) -> HalDescriptor {
 836 |         let mut descriptor = self.base.descriptor();
 837 |         for capability in &mut descriptor.capabilities {
 838 |             if capability.id == CapabilityId::ComActivationDispatch {
 839 |                 capability.supported = true;
 840 |             }
 841 |         }
````

### Document execution grants and bound policy [CODE]

Source: `crates/calc-host-vba/src/trust.rs:18` (lines 18–85, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host-vba/src/trust.rs#L18-L85).

````text
  18 | pub use calc_macro_security::{FileOrigin, MacroBlock};
  19 | 
  20 | /// Permission to compile a workbook file's own VBA project and raise its events. Only the
  21 | /// constructors below make one; nothing derives it from opening a file.
  22 | #[derive(Clone, Debug, PartialEq, Eq)]
  23 | pub struct ProjectGrant {
  24 |     reason: GrantReason,
  25 |     /// The policy bound to every runtime host prepared under this grant.
  26 |     policy: calc_macro_security::VbaPolicy,
  27 | }
  28 | 
  29 | /// Why a [`ProjectGrant`] was given.
  30 | #[derive(Clone, Debug, PartialEq, Eq)]
  31 | pub enum GrantReason {
  32 |     /// The person enabled macros for this file (the page's bar, or the desktop harness's
  33 |     /// `--enable-macros`).
  34 |     PersonEnabled,
  35 |     /// The person unblocked a marked or network file and enabled its macros, after the page's
  36 |     /// short warning (dc-15dk).
  37 |     PersonUnblocked(Option<MacroBlock>),
  38 |     /// The macro security level Unguarded, or `--trust-macros` or `DNACALC_TRUST_ALL_MACROS=1`,
  39 |     /// trusts every file; `by` names which (dc-15dk).
  40 |     TrustAll { by: String },
  41 |     /// An evidence run's trusted-code manifest (its SHA-256) lists the package (its SHA-256)
  42 |     /// with its VBA project enabled.
  43 |     EvidenceManifest {
  44 |         manifest_sha256: String,
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

### Macro execution levels versus capability policy [CODE]

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

### Policy nesting on owning thread [CODE]

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

### Host event command interface [CODE]

Source: `crates/calc-host/src/events.rs:11` (lines 11–47, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/events.rs#L11-L47).

````text
  11 | pub enum WorkbookEvent {
  12 |     Open,
  13 |     WorksheetChange {
  14 |         sheet: SheetId,
  15 |         target: Vec<GridArea>,
  16 |     },
  17 |     WorksheetCalculate {
  18 |         sheet: SheetId,
  19 |     },
  20 |     BeforeSave {
  21 |         operation: SaveOperation,
  22 |     },
  23 | }
  24 | 
  25 | /// One registered owner. It supplies runtime-wired document sources, not handler names.
  26 | /// The session lends exclusive authority during each synchronous callback. A nested
  27 | /// command completes its transaction, calculation and events before the caller resumes.
  28 | /// BeforeSave may also return an authored batch for the existing pre-pin route.
  29 | pub trait WorkbookEventHandler {
  30 |     fn file_identity_changed(&self, _name: &str) {}
  31 |     fn workbook_open(&self, _session: &mut HostSession) -> Result<(), String> {
  32 |         Ok(())
  33 |     }
  34 |     fn worksheet_change(
  35 |         &self,
  36 |         session: &mut HostSession,
  37 |         sheet: SheetId,
  38 |         target: &[GridArea],
  39 |     ) -> Result<(), String>;
  40 |     fn worksheet_calculate(&self, session: &mut HostSession, sheet: SheetId) -> Result<(), String>;
  41 |     fn before_save(
  42 |         &self,
  43 |         session: &mut HostSession,
  44 |         operation: SaveOperation,
  45 |     ) -> Result<BeforeSaveOutcome, String>;
  46 | }
  47 | 
````

### VBA event host implementation: reads and writes [CODE]

Source: `crates/calc-host-vba/src/lib.rs:455` (lines 455–553, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host-vba/src/lib.rs#L455-L553).

````text
 455 |     fn write_reference(&self, reference: &ReferenceLike, value: CalcValue) -> Result<(), String> {
 456 |         if reference.area_count() != 1 {
 457 |             return Err("multi-area Range.Value2 writes are not modelled (dc-hsa5.12)".into());
 458 |         }
 459 |         let area = reference.areas()[0];
 460 |         if area.cell_count() > 1_048_576 {
 461 |             return Err("Range.Value2 write exceeds its materialization limit (dc-hsa5.12)".into());
 462 |         }
 463 |         let array = value.as_array();
 464 |         if let Some(array) = array
 465 |             && (array.shape().rows != area.row_count() as usize
 466 |                 || array.shape().cols != area.column_count() as usize)
 467 |         {
 468 |             return Err(
 469 |                 "Range.Value2 array/target shape differences are not modelled (dc-hsa5.12)".into(),
 470 |             );
 471 |         }
 472 |         // Validate every element before dispatch. One Automation edit owns the
 473 |         // complete rectangle, its calculation and its nested Change event.
 474 |         let scalar = array.is_none().then(|| value_input(&value)).transpose()?;
 475 |         let edits = if scalar == Some(None) {
 476 |             vec![calc_host::CellEdit::Clear { area }]
 477 |         } else {
 478 |             let mut edits = Vec::with_capacity(area.cell_count() as usize);
 479 |             for row in 0..area.row_count() {
 480 |                 for column in 0..area.column_count() {
 481 |                     let input = if let Some(array) = array {
 482 |                         value_input(
 483 |                             array
 484 |                                 .get(row as usize, column as usize)
 485 |                                 .expect("validated rectangle"),
 486 |                         )?
 487 |                     } else {
 488 |                         scalar.as_ref().expect("scalar input").clone()
 489 |                     };
 490 |                     let at = calc_value::CellKey::new(
 491 |                         area.sheet(),
 492 |                         area.first().row() + row,
 493 |                         area.first().column() + column,
 494 |                     )
 495 |                     .map_err(|e| e.to_string())?;
 496 |                     edits.push(match input {
 497 |                         Some(input) => calc_host::CellEdit::Set { at, input },
 498 |                         None => calc_host::CellEdit::Clear {
 499 |                             area: GridArea::cell(at),
 500 |                         },
 501 |                     });
 502 |                 }
 503 |             }
 504 |             edits
 505 |         };
 506 |         let receipt = self.session.borrow_mut().dispatch_from(
 507 |             calc_host::CommandIngress::Automation,
 508 |             calc_host::SkinIntent::Edit { edits },
 509 |         );
 510 |         match receipt.recalc {
 511 |             Some(calc_host::HostProgress::Failed(error)) => {
 512 |                 return Err(format!(
 513 |                     "calculation after a macro write failed: {error:?} (dc-hsa5.12)"
 514 |                 ));
 515 |             }
 516 |             Some(calc_host::HostProgress::Waiting { .. }) => {
 517 |                 return Err(
 518 |                     "calculation after a macro write is waiting for a provider (dc-hsa5.12)".into(),
 519 |                 );
 520 |             }
 521 |             Some(calc_host::HostProgress::Cancelled) => {
 522 |                 return Err("calculation after a macro write was cancelled (dc-hsa5.12)".into());
 523 |             }
 524 |             _ => {}
 525 |         }
 526 |         if self.session.borrow().has_unfinished_calculation() {
 527 |             return Err("calculation after a macro write is unfinished (dc-hsa5.12)".into());
 528 |         }
 529 |         match receipt.error {
 530 |             Some(error) => Err(error.to_string()),
 531 |             None => Ok(()),
 532 |         }
 533 |     }
 534 |     fn calculate(&self, command: calc_vba::events::CalculationCommand) -> Result<(), String> {
 535 |         use calc_host::{CalcScope, EvalSelection, SkinIntent};
 536 |         use calc_vba::events::CalculationCommand as C;
 537 |         let intent = match command {
 538 |             C::Dirty => SkinIntent::Recalculate,
 539 |             C::Full => SkinIntent::Calculate {
 540 |                 scope: CalcScope::Workbook,
 541 |                 selection: EvalSelection::ForceAll,
 542 |             },
 543 |             C::Rebuild => SkinIntent::Rebuild,
 544 |             C::Range(reference) => SkinIntent::CalculateRange {
 545 |                 areas: reference.areas().to_vec(),
 546 |             },
 547 |         };
 548 |         let mut session = self.session.borrow_mut();
 549 |         let receipt = session.dispatch_from(calc_host::CommandIngress::Automation, intent);
 550 |         if let Some(error) = receipt.error {
 551 |             return Err(error.to_string());
 552 |         }
 553 |         let mut progress = receipt.recalc;
````

### Host session command and delta projection shapes [CODE]

Source: `crates/calc-host/src/session.rs:83` (lines 83–132, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/session.rs#L83-L132).

````text
  83 | pub enum DeltaCause {
  84 |     ExternalInputsChanged {
  85 |         revision: AuthoredRevision,
  86 |     },
  87 |     CallablesChanged {
  88 |         revision: AuthoredRevision,
  89 |     },
  90 |     /// The subscription was registered: its interest's cells as they stand.
  91 |     Subscribed,
  92 |     /// The subscription's interest moved (a new area, or a new selection for a selection
  93 |     /// interest): the new interest's cells as they stand.
  94 |     InterestMoved,
  95 |     /// A transaction committed this revision.
  96 |     Committed {
  97 |         revision: AuthoredRevision,
  98 |     },
  99 |     /// A recalculation of this revision published a generation.
 100 |     Recalculated {
 101 |         publication: calc_engine::contract::PublicationUnitId,
 102 |         revision: AuthoredRevision,
 103 |         tick: CalcTick,
 104 |     },
 105 |     /// A recalculation of this revision published nothing; the results it would have replaced
 106 |     /// are stale ([`crate::StaleCause::RecalcFailed`]).
 107 |     RecalcFailed {
 108 |         revision: AuthoredRevision,
 109 |     },
 110 | }
 111 | 
 112 | /// The cells of a subscription's interest that changed.
 113 | #[derive(Clone, Debug, PartialEq)]
 114 | pub struct HostDelta {
 115 |     pub subscription: SubscriptionId,
 116 |     /// Counts the deltas of this subscription from 1, so a mirror can see a gap.
 117 |     pub seq: u64,
 118 |     pub cause: DeltaCause,
 119 |     /// The authored revision the cells were projected at.
 120 |     pub authored: AuthoredRevision,
 121 |     /// The cells are the whole interest as it stands: every cell not among them is empty.
 122 |     pub reset: bool,
 123 |     /// The changed cells (or, on a reset, every cell of the interest that is not empty), in cell
 124 |     /// order. A cell that became empty is here with no input and [`ProjectedValue::Empty`].
 125 |     pub cells: Vec<ProjectedCell>,
 126 | }
 127 | 
 128 | /// Where a subscription's deltas go: any closure, so a skin of any toolkit can forward them.
 129 | pub type DeltaSink = Box<dyn FnMut(&HostDelta)>;
 130 | 
 131 | /// What a skin asks of the session.
 132 | #[derive(Clone, Debug, PartialEq, Eq)]
````

### Spreadsheet services belong to the embedder [DESIGN]

Source: `CalcVBA/docs/spec/HOSTING_AND_EARLY_BINDING.md:30` (lines 30–61, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/docs/spec/HOSTING_AND_EARLY_BINDING.md#L30-L61).

````text
  30 | 
  31 | ## Native objects and calls
  32 | 
  33 | Imported type libraries provide IIDs, member kinds, DISPIDs, vtable slots,
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
  48 | 
  49 | Compiler metadata selects source interfaces and private event handlers.
  50 | Hosts attach document identities by module name and supply lifecycle timing.
  51 | Owning sessions wire WithEvents and document sources through retained native
  52 | connection points. Native sinks retain the same-thread Rc callback directly,
  53 | without a TLS callback-ID registry. Supported synchronous events enter the same VM3/JIT session;
  54 | tested Boolean ByRef arguments copy back before returning to the native caller.
  55 | Idle delivery needs an STA message pump. Reset/reload/detach reject stale
  56 | callbacks and release subscriptions. Unhandled handler errors abort the
  57 | triggering call; take_event_error exposes idle errors to the host.
  58 | 
  59 | General structural/custom event signatures, the older package-session event
  60 | path, custom-vtable sources, broader cross-process transport and JIT-backed COM
  61 | serving remain incomplete. Ordinary out-of-process COM events are in scope;
````

### HostServices: no Send/Sync requirement and host-class authorization [CODE]

Source: `CalcVBA/crates/calcvba-hal/src/traits.rs:92` (lines 92–128, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-hal/src/traits.rs#L92-L128).

````text
  92 | pub trait HostServices {
  93 |     fn profile(&self) -> HalProfileId;
  94 |     fn descriptor(&self) -> HalDescriptor;
  95 |     fn policy(&self) -> &HostPolicy;
  96 | 
  97 |     fn console(&self) -> &dyn ConsoleHal;
  98 |     fn ui(&self) -> &dyn UiInteractionHal;
  99 |     fn events(&self) -> &dyn EventPumpHal;
 100 |     fn fs(&self) -> &dyn FileSystemHal;
 101 |     fn process(&self) -> &dyn ProcessEnvHal;
 102 |     fn com(&self) -> &dyn ComHal;
 103 |     fn time_locale(&self) -> &dyn TimeLocaleHal;
 104 |     fn dynlink(&self) -> &dyn DynamicLinkHal;
 105 |     fn diag(&self) -> &dyn DiagnosticsHal;
 106 |     /// Install this host's policy while compiling or executing native COM routes.
 107 |     fn enter_com_policy(&self) -> calcvba_com::ComPolicyScope {
 108 |         calcvba_com::ComPolicyScope::unrestricted()
 109 |     }
 110 |     /// True only for an exact class resolved internally, without native COM fallback.
 111 |     /// Never applies to GetObject.
 112 |     fn is_host_class(&self, _target: &str) -> bool {
 113 |         false
 114 |     }
 115 |     /// Policy hook before CreateObject/GetObject or New reaches the COM facet.
 116 |     /// The default permits it; PolicyHost denies or audits native activation.
 117 |     fn authorize_com_activation(&self, _op: &'static str, _target: &str) -> HalResult<()> {
 118 |         Ok(())
 119 |     }
 120 |     fn project_catalog(&self) -> Option<&dyn ProjectCatalogHal> {
 121 |         None
 122 |     }
 123 |     fn project_references(&self) -> Option<&dyn ProjectReferenceHal> {
 124 |         None
 125 |     }
 126 |     fn project_mutation(&self) -> Option<&dyn ProjectMutationHal> {
 127 |         None
 128 |     }
````

### ComHal: creation and identity transport [CODE]

Source: `CalcVBA/crates/calcvba-hal/src/traits.rs:381` (lines 381–397, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-hal/src/traits.rs#L381-L397).

````text
 381 | pub trait ComHal {
 382 |     fn create_object_variant(&self, _prog_id: Variant) -> HalResult<Variant> {
 383 |         variant_companion_not_overridden(
 384 |             CapabilityId::ComActivationDispatch,
 385 |             "create_object_variant",
 386 |         )
 387 |     }
 388 |     /// VBA `GetObject([pathname], [class])`. Three modes by the shape of the arguments:
 389 |     /// an omitted `pathname` (an `Empty`/`Null` variant) binds to the currently-running
 390 |     /// instance of `class` (`GetActiveObject`); a zero-length `pathname` creates a new
 391 |     /// instance of `class` (like `CreateObject`); a non-empty `pathname` binds to the
 392 |     /// object that file names (`CoGetObject`). Activation failures carry the VBA error
 393 |     /// number for their shape (429 for running/create activation, 432 for file bind);
 394 |     /// native HRESULT detail is preserved in the fault message.
 395 |     fn get_object_variant(&self, _pathname: Variant, _class: Variant) -> HalResult<Variant> {
 396 |         variant_companion_not_overridden(CapabilityId::ComActivationDispatch, "get_object_variant")
 397 |     }
````

### ComHal: late dispatch and writebacks [CODE]

Source: `CalcVBA/crates/calcvba-hal/src/traits.rs:507` (lines 507–549, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-hal/src/traits.rs#L507-L549).

````text
 507 |     /// without a real COM transport (null/wasm/replay) return a capability error
 508 |     /// by default, which the VM surfaces through normal VBA error handling.
 509 |     fn enumerate_object(&self, _object: ObjectRef) -> HalResult<Vec<Variant>> {
 510 |         variant_companion_not_overridden(CapabilityId::ComActivationDispatch, "enumerate_object")
 511 |     }
 512 |     /// The class/type name VBA's `TypeName` reports for a COM object — e.g.
 513 |     /// `"Dictionary"` for a `Scripting.Dictionary`. `None` when the adapter
 514 |     /// cannot determine it (no COM transport, or the object exposes no usable
 515 |     /// type identity), in which case the caller keeps the generic `"Object"`.
 516 |     fn object_type_name(&self, _object: ObjectRef) -> HalResult<Option<String>> {
 517 |         Ok(None)
 518 |     }
 519 |     /// A reflected zero-argument default value, when this adapter supplies one.
 520 |     /// Information built-ins such as VarType inspect its type without replacing the
 521 |     /// original object argument. None leaves an object without this reflection intact.
 522 |     fn object_default_value(&self, _object: ObjectRef) -> HalResult<Option<Variant>> {
 523 |         Ok(None)
 524 |     }
 525 |     fn dispatch_invoke_variant(&self, _request: &ComInvokeRequest) -> HalResult<Variant> {
 526 |         variant_companion_not_overridden(
 527 |             CapabilityId::ComActivationDispatch,
 528 |             "dispatch_invoke_variant",
 529 |         )
 530 |     }
 531 |     fn dispatch_invoke_dynamic_variant(&self, _request: &DynamicCallRequest) -> HalResult<Variant> {
 532 |         variant_companion_not_overridden(
 533 |             CapabilityId::ComActivationDispatch,
 534 |             "dispatch_invoke_dynamic_variant",
 535 |         )
 536 |     }
 537 |     /// Dynamic dispatch that also returns per-argument write-back values for
 538 |     /// `[out]`/`[in,out]` COM parameters — `Some(value)` for an argument the
 539 |     /// callee modified, aligned to `request.args`. The default returns no
 540 |     /// write-backs (value-only); a host with COM out-param support overrides this.
 541 |     /// The caller (vm3) applies a write-back only to a `ByRef` call-site slot.
 542 |     fn dispatch_invoke_dynamic_variant_with_writebacks(
 543 |         &self,
 544 |         request: &DynamicCallRequest,
 545 |     ) -> HalResult<(Variant, Vec<Option<Variant>>)> {
 546 |         Ok((self.dispatch_invoke_dynamic_variant(request)?, Vec::new()))
 547 |     }
 548 |     /// Invoke the compiler-selected interface member. Native Windows adapters must
 549 |     /// use its IID, slot and wire signature, without name lookup or Invoke fallback
````

### Portable typed object ABI and ownership [CODE]

Source: `CalcVBA/crates/calcvba-host-api/src/object.rs:29` (lines 29–48, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-host-api/src/object.rs#L29-L48).

````text
  29 | pub struct NativeSlot {
  30 |     pub entry: *const u8,
  31 |     pub data: *const (),
  32 |     pub signature: u32,
  33 | }
  34 | #[repr(C)]
  35 | pub struct ObjectHeader {
  36 |     /// Exact runtime IUnknown prefix; the typed table is a separate, versioned
  37 |     /// host ABI and never masquerades as an Automation/dual COM vtable.
  38 |     pub unknown: RawRuntimeIUnknown,
  39 |     pub iid: InterfaceId,
  40 |     pub slots: *const NativeSlot,
  41 |     pub slot_count: usize,
  42 | }
  43 | 
  44 | type Double0 = dyn Fn(&mut TypedContext) -> Result<f64, i32>;
  45 | type DoubleLong = dyn Fn(&mut TypedContext, i32) -> Result<f64, i32>;
  46 | enum Callback {
  47 |     Double0(Box<Double0>),
  48 |     DoubleLong(Box<DoubleLong>),
````

### Capability gates and their stated limits [CODE]

Source: `CalcVBA/crates/calcvba-hal/src/capability.rs:1` (lines 1–54, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-hal/src/capability.rs#L1-L54).

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
  25 | use std::collections::BTreeMap;
  26 | use std::rc::Rc;
  27 | use std::sync::Arc;
  28 | 
  29 | use calcvba_runtime::{BindingHandle, DynLinkSymbol, RuntimeCallArgument, Variant};
  30 | 
  31 | use crate::error::{HalError, HalResult, HostErrorInfo};
  32 | use crate::model::{CapabilityId, HalDescriptor, HalProfileId, HostPolicy};
  33 | use crate::traits::{
  34 |     ComHal, ConsoleHal, DiagnosticsHal, DynLinkDescriptorView, DynamicLinkHal, EventPumpHal,
  35 |     FileSystemHal, HostServices, ProcessEnvHal, ProjectCatalogHal, ProjectMutationHal,
  36 |     ProjectReferenceHal, TimeLocaleHal, UiInteractionHal,
  37 | };
  38 | 
  39 | /// The stable code carried by every policy denial.
  40 | pub const POLICY_DENIED_CODE: &str = "HAL-E-POLICY-DENIED";
  41 | 
  42 | /// What a gated operation needs. One capability per family of operations.
  43 | #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
  44 | pub enum Capability {
  45 |     FsRead,
  46 |     FsWrite,
  47 |     Process,
  48 |     Env,
  49 |     Settings,
  50 |     Ffi,
  51 |     ComActivate,
  52 |     UiKeys,
  53 | }
  54 | 
````

### Safe/Full construction and host-owned class exemption [CODE]

Source: `CalcVBA/crates/calcvba-hal/src/capability.rs:379` (lines 379–429, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-hal/src/capability.rs#L379-L429).

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
 407 |     deny: CapabilitySet,
 408 |     audit: AuditHandle,
 409 | }
 410 | 
 411 | impl PolicyHost {
 412 |     pub fn with_mode(inner: Arc<dyn HostServices>, mode: PolicyMode) -> Self {
 413 |         match mode {
 414 |             PolicyMode::Safe => Self::safe(inner),
 415 |             PolicyMode::Full => Self::full(inner),
 416 |         }
 417 |     }
 418 | 
 419 |     /// Safe mode: all eight capabilities denied (and logged).
 420 |     pub fn safe(inner: Arc<dyn HostServices>) -> Self {
 421 |         Self::with_denied(inner, CapabilitySet::ALL)
 422 |     }
 423 | 
 424 |     /// Full mode: everything passes through, and is logged.
 425 |     pub fn full(inner: Arc<dyn HostServices>) -> Self {
 426 |         Self::with_denied(inner, CapabilitySet::EMPTY)
 427 |     }
 428 | 
 429 |     pub fn with_denied(inner: Arc<dyn HostServices>, deny: CapabilitySet) -> Self {
````

### COM authorization gate: host objects and external activation [CODE]

Source: `CalcVBA/crates/calcvba-hal/src/capability.rs:565` (lines 565–591, CalcVBA pin). [GitHub excerpt](https://github.com/DnaCalc/CalcVBA/blob/8115aa78135114c1dea79f26cbc12c8a3b00a4cf/crates/calcvba-hal/src/capability.rs#L565-L591).

````text
 565 |             } else {
 566 |                 Ok(())
 567 |             }
 568 |         })
 569 |         .with_parent(parent)
 570 |     }
 571 |     fn authorize_com_activation(&self, op: &'static str, target: &str) -> HalResult<()> {
 572 |         // Only CreateObject of a class the inner host resolves internally is exempt.
 573 |         // GetObject may bind an arbitrary moniker regardless of its class argument.
 574 |         if op == "create_object" && self.inner.is_host_class(target) {
 575 |             return self.inner.authorize_com_activation(op, target);
 576 |         }
 577 |         self.check(Capability::ComActivate, op, target.to_string())?;
 578 |         self.inner.authorize_com_activation(op, target)
 579 |     }
 580 | }
 581 | 
 582 | impl FileSystemHal for PolicyHost {
 583 |     fn open_variant(&self, path: Variant, mode: Variant) -> HalResult<Variant> {
 584 |         self.check_open("open", &path, &mode)?;
 585 |         self.inner.fs().open_variant(path, mode)
 586 |     }
 587 | 
 588 |     fn open_with_record_len(
 589 |         &self,
 590 |         path: Variant,
 591 |         mode: Variant,
````
