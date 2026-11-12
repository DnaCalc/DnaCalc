# Question for GPT-6 Pro: host commands, worker protocol and shared skin IR for responsive workbook views

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

Review the path from calc-host to the skin IR and shell: HostSession intents/subscriptions/deltas, dna-calc-worker's server/publication layer, dna-calc-protocol controls/binary tiles, dna-calc-model's DOM-free state/commands/projections, and the browser/WebView2 adapters and skins. Define semantic state versus presentation state and make the smallest boundary that supports another skin without duplicating commands, recalculation or workbook ownership.

Audit incremental granularity: authored operations, changed cells/areas, spill values/shape/removals, styles/geometry, name definition/evaluation, calculation status and freshness, viewport interests, tile dictionaries, fragments, admission/completion/acks, continuity/reset and slow-subscriber recovery. A publication spanning tiles must become visible atomically to rendering, editor and accessibility readers. Specify how a reply relates to earlier events and how stale replies, reused subscriptions or replaced workbooks are rejected. Estimate copies, retained bytes, message count and per-change amplification before recommending a new wire format.

For the grid, cover viewport virtualisation, frozen panes, merged cells, overflow text, variable geometry, styles/conditional formats, precise hit-testing, editing/IME/focus and accessibility. Today's Canvas2D and DOM display-list paths are evidence; WebGPU-friendly rendering is a target, not an implementation claim. Propose the reusable render IR needed for GPU buffers/atlas/text shaping with a semantic accessibility projection, and name what stays local versus comes from host calculation/presentation facts.

For the notebook, cover cell-backed and name-backed blocks, identity versus placement/order/layout, passive name inspection versus explicit evaluation, lazy paging, live edits and formula/source navigation. For the proposed linked-boxes skin, use defined names (e.g. MyCell.Input1/MyCell.Result), wire edits as normal name definitions and custom XML for layout. Decide its place in the skin IR. Design the **formula-name presenter/interpreter** over syntax/binding rather than string substitution: present/interpret round-trip, local aliases, scope and collisions with LET/LAMBDA locals, functions, cell references, quoted strings and sheet-qualified names. Define diagnostics/spans, completion and rename behavior, reverse import of dotted names, arrays and cycles. This is a proposal; the idea document does not establish working code.

Support multiple skins over one session; distinguish today's switch-one-mounted-skin behavior from simultaneous views/windows. Specify per-view selection/interest/focus/editor ownership and shared undo/commands, selection-sensitive macro facts, subscriptions and lifecycle. Compare web dedicated workers with the desktop native owner/STA worker and WebView2 shell; maintain equivalent command semantics and refusal reporting.

Propose explicit latency and memory budgets with workloads: edit-to-ack, edit-to-first-visible value, edit-to-current viewport, scroll-to-paint, frame cost, cancellation, slow foreign calls, 100k+ names and dense/spilled areas. Label budgets as proposals unless an appendix measures them. Experiments include slow subscriber/reset, multi-tile spill, rapid skin switching with pending replies, malformed batches and linked-name parsing; Excel experiments should focus on source/name/save semantics, not claim Excel has our skins.

## What was already tried

Already tried: October's risk audit found skin-specific commands and a DOM-coupled controller; the synthesis requested a neutral command/model layer, binary tiles with transaction closure and semantic accessibility. Current code now has a model package, transport trait, bounded tiles/mirror, display list and registry; verify the route instead of repeating the old audit as fact. The linked-boxes note is explicitly an undecided idea. The evaluator/host work introduced finite slices and scoped publications that this UI boundary must preserve.

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

### Synthesis command, wire and semantic accessibility boundary [DESIGN]

Source: `docs/architecture/SYNTHESIS.md:197` (lines 197–214, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/SYNTHESIS.md#L197-L214).

````text
 197 | ### 1.5 Commands, skins and wire (owner 4.2)
 198 | 
 199 | - Host commands in calc-host; the actor inbox separates `ActorMessage::{Command(CommandEnvelope), External,
 200 |   Interest, TransportAck}` (RT 11). `CommandEnvelope { id, base_revision, resolved_target, source, undo_group }`;
 201 |   availability is revalidated at execution. A UI adapter owns focus and browser-authorised effects; an old response
 202 |   never focuses a newly mounted skin or pastes into a moved target.
 203 | - UI model DOM-free (registry, neutral `Target` including `Name`, `EditSession`, `ViewStateStore`, focus registry,
 204 |   `view.reveal`); shell keymap as data; skin contract `mount`, `focus_target`, gestures, `view_state_schema`.
 205 | - **Notebook over cells or names [4.8]**: switching changes the projection only; names shown through `NameView`.
 206 | - **Wire**: control envelope apart from binary tiles; `TileFrame { base, next, revision, dictionary, transaction:
 207 |   PublicationUnitId, final_fragment }`, so a multi-tile spill applies atomically; dictionary definitions precede
 208 |   references; removals explicit; geometry and style versions related to value tiles; per-subscription continuity or
 209 |   explicit coverage; bounded queues with coalescing and atomic reset. A transport trait keeps the in-process server
 210 |   out of the page (`apps/dna-calc/src/port.rs [historical locator omitted]`).
 211 | - **Accessibility** is a semantic projection from committed units (cell identity, coordinates and totals, headers,
 212 |   spans, selection, editability, pending/refused text), independent of painted tiles; real-browser focus, keyboard,
 213 |   IME and screen-reader checks stay (RT 11).
 214 | 
````

### Earlier tile review: transaction closure and backpressure [EARLIER-REVIEW]

Source: `docs/architecture/review-2026-10/pro/3-architecture-redteam-review.md:472` (lines 472–489, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/review-2026-10/pro/3-architecture-redteam-review.md#L472-L489).

````text
 472 | ### 11. Binary tiles require transaction closure, bounded queues and a separate semantic projection
 473 | 
 474 | **Target: A §1.6 and U1–U3.**
 475 | 
 476 | Revision, publication sequence and dictionary generation are necessary, but do not specify the atomicity of a multi-tile result. A spill can occupy two tiles; installing only the first exposes half a committed spill to the renderer, hit testing or accessibility.
 477 | 
 478 | Choose either complete frames or a transaction marker:
 479 | 
 480 | ~~~rust
 481 | struct TileFrame {
 482 |     base: PublicationSeq,
 483 |     next: PublicationSeq,
 484 |     revision: Revision,
 485 |     dictionary: DictionaryGeneration,
 486 |     transaction: PublicationUnitId,
 487 |     final_fragment: bool,
 488 | }
 489 | ~~~
````

### Linked boxes and formula-name presenter/interpreter proposal [DESIGN]

Source: `docs/architecture/ideas/LINKED_BOXES_SKIN.md:1` (lines 1–48, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/docs/architecture/ideas/LINKED_BOXES_SKIN.md#L1-L48).

````text
   1 | # Idea: a linked-boxes skin over defined names (for discussion)
   2 | 
   3 | Owner idea, 2026-11-11. Status: queued for discussion, not scheduled. Nothing here is decided.
   4 | 
   5 | ## The idea
   6 | 
   7 | A free-form canvas skin for DNA Calc. Instead of a grid, the user sees **boxes** wired together. A box is a named
   8 | calculation unit with **inputs** and **outputs**. Nothing new is stored in the file: every box, input and output is
   9 | a **defined name**, so the workbook stays an ordinary Excel workbook that Excel can open and calculate.
  10 | 
  11 | - Box `MyCell` with input `Input1` and output `Result` maps to the defined names `MyCell.Input1` and `MyCell.Result`.
  12 |   The canvas shows `Input1` and `Result` on the `MyCell` box; showing the box prefix is optional.
  13 | - **Wiring** `OtherCell.Output` to `MyCell.Input1` sets the definition of the name `MyCell.Input1` to
  14 |   `=OtherCell.Output`. Unwiring sets it back to a literal or leaves it empty.
  15 | - **An output** is a name whose definition is a formula over the box's inputs, such as
  16 |   `MyCell.Result = MyCell.Input1 * 2`. Shown inside the box as `= Input1 * 2`, with the prefix stripped.
  17 | - **Layout**, meaning box positions, sizes and colours, is presentation only. It lives in a skin part of the file
  18 |   (custom XML), never in the calculation model, so Excel ignores it safely.
  19 | 
  20 | ## Why it fits what we have
  21 | 
  22 | - Defined names are already first-class in calc-doc and the engine, including scopes, `LET`/`LAMBDA` bodies and
  23 |   name calculation (the dc-hsa5 work).
  24 | - Excel allows dots in names (`MyCell.Input1` is a legal name), so the mapping needs no escaping in the common case.
  25 | - It is a skin, not a new engine: the same recalculation and the same Excel parity apply.
  26 | 
  27 | ## The formula-name presenter and interpreter (a skin IR piece)
  28 | 
  29 | The canvas, and the notebook skin, both need to show formulas **in the vocabulary of the skin** rather than raw
  30 | workbook text. Proposed: a small layer in the skin IR with two directions.
  31 | - **Present:** take a formula and a context (which box is being shown) and rewrite name references for display:
  32 |   `MyCell.Input1 * 2` becomes `Input1 * 2` inside `MyCell`, and `OtherCell.Output` stays qualified.
  33 | - **Interpret:** take what the user typed in that context and resolve short names back to full defined names, with
  34 |   the same rules, before the formula reaches the engine.
  35 | 
  36 | It works on the parsed formula (calc-fml's syntax tree), never on text, so ranges, strings and functions with
  37 | similar names are not touched. The notebook skin can reuse the same layer: a notebook cell named `Revenue`
  38 | referring to `Costs` and `Price` is the same idea without the canvas.
  39 | 
  40 | ## Questions for the discussion
  41 | 
  42 | 1. Box scope: workbook-scoped names only, or also sheet-scoped (`Sheet1!MyCell.Input1`)?
  43 | 2. Arrays and spills: may an input or output hold a whole array, shown as a small table on the box?
  44 | 3. Cycles: the canvas makes cycles easy to draw. Refuse the wire, or allow iterative calculation as Excel does?
  45 | 4. Grid interplay: may a box output be placed into a grid cell (`=MyCell.Result`), and a grid range be an input?
  46 | 5. Excel round trip: workbooks saved from the canvas must open unchanged in Excel. Do we also need a reverse
  47 |    import, which builds boxes from an existing workbook's dotted names?
  48 | 6. Where it sits: a DNA Calc skin only, or also in OneCalc as a playground for a few named formulas?
````

### Host interests, deltas and intents [CODE]

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

### Host subscription identity and dispatch ingress [CODE]

Source: `crates/calc-host/src/session.rs:861` (lines 861–900, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/crates/calc-host/src/session.rs#L861-L900).

````text
 861 |     pub fn subscribe(
 862 |         &mut self,
 863 |         interest: Interest,
 864 |         sink: DeltaSink,
 865 |     ) -> Result<SubscriptionId, IntentError> {
 866 |         self.check_interest(&interest)?;
 867 |         let id = SubscriptionId(self.next_subscription);
 868 |         self.next_subscription += 1;
 869 |         self.subscriptions.insert(
 870 |             id,
 871 |             Subscription {
 872 |                 interest,
 873 |                 sink,
 874 |                 seq: 0,
 875 |                 cells: BTreeMap::new(),
 876 |             },
 877 |         );
 878 |         self.deliver_reset(id, DeltaCause::Subscribed);
 879 |         Ok(id)
 880 |     }
 881 | 
 882 |     /// Removes a subscription; `false` when the session has none with this identity.
 883 |     pub fn unsubscribe(&mut self, id: SubscriptionId) -> bool {
 884 |         self.subscriptions.remove(&id).is_some()
 885 |     }
 886 | 
 887 |     /// Carries out `intent` and logs it.
 888 |     pub fn dispatch(&mut self, intent: SkinIntent) -> IntentReceipt {
 889 |         self.dispatch_from(CommandIngress::UserInterface, intent)
 890 |     }
 891 | 
 892 |     /// Automation uses the same authored transaction and evaluator as the UI.
 893 |     /// Its synchronous automatic calculation precedes Change (P22, 20527 COM).
 894 |     pub fn dispatch_from(&mut self, ingress: CommandIngress, intent: SkinIntent) -> IntentReceipt {
 895 |         let seq = self.next_seq;
 896 |         self.next_seq = self
 897 |             .next_seq
 898 |             .checked_add(1)
 899 |             .expect("host command identity exhausted");
 900 |         let parent = self.active_command.replace(Some(seq));
````

### Protocol boundary and binary codec entry [CODE]

Source: `apps/dna-calc-protocol/src/lib.rs:1` (lines 1–14, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-protocol/src/lib.rs#L1-L14).

````text
   1 | //! JSON controls and bounded binary tile records shared by page and worker.
   2 | pub mod tiles;
   3 | pub use dna_calc_model::projection::*;
   4 | use serde::{Serialize, de::DeserializeOwned};
   5 | pub use tiles::*;
   6 | /// A message as the JSON text `postMessage` carries.
   7 | pub fn encode<T: Serialize>(message: &T) -> String {
   8 |     serde_json::to_string(message).expect("a wire message always encodes")
   9 | }
  10 | 
  11 | /// A message from its JSON text.
  12 | pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, String> {
  13 |     serde_json::from_str(text).map_err(|e| format!("not a message: {e}"))
  14 | }
````

### Binary tile encoder and presentation dictionary [CODE]

Source: `apps/dna-calc-protocol/src/tiles.rs:1` (lines 1–69, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-protocol/src/tiles.rs#L1-L69).

````text
   1 | //! Tile wire v2: little-endian header and address/index records, preceded by a bounded JSON
   2 | //! dictionary of presentation atoms and geometry. Controls use the separate JSON envelope.
   3 | //! Dictionaries never reference a previous fragment, so reset does not leave dangling ids.
   4 | use crate::{decode, encode};
   5 | use dna_calc_model::projection::{Cell, Geometry, Key, Sheet};
   6 | pub use dna_calc_model::tiles::*;
   7 | use std::collections::BTreeMap;
   8 | 
   9 | pub const TILE_TAG: &str = "dnacalc-tiles-v2";
  10 | const MAGIC: &[u8; 8] = b"DNATILE2";
  11 | 
  12 | pub fn encode_tiles(batch: &TileBatch) -> Result<Vec<u8>, String> {
  13 |     if batch.cells.len() + batch.removed.len() > MAX_TILE_RECORDS
  14 |         || batch.retained_bytes() > MAX_TILE_BYTES
  15 |     {
  16 |         return Err("The tile fragment exceeds its bounded budget".into());
  17 |     }
  18 |     let mut out = MAGIC.to_vec();
  19 |     let s = batch.stamp;
  20 |     for n in [
  21 |         s.id,
  22 |         s.workbook,
  23 |         s.transaction,
  24 |         s.base,
  25 |         s.next,
  26 |         s.revision,
  27 |         s.geometry_version,
  28 |         s.style_version,
  29 |         s.dictionary,
  30 |     ] {
  31 |         out.extend_from_slice(&n.to_le_bytes());
  32 |     }
  33 |     out.push(u8::from(s.calculation_through.is_some()));
  34 |     if let Some(unit) = s.calculation_through {
  35 |         for n in [unit.domain, unit.command, unit.sequence] {
  36 |             out.extend_from_slice(&n.to_le_bytes());
  37 |         }
  38 |     }
  39 |     out.extend_from_slice(&batch.fragment.to_le_bytes());
  40 |     out.push(u8::from(s.reset) | (u8::from(batch.final_fragment) << 1));
  41 |     let mut dictionary = Vec::new();
  42 |     let mut indexes = BTreeMap::new();
  43 |     let mut records = Vec::new();
  44 |     for cell in &batch.cells {
  45 |         let mut atom = cell.clone();
  46 |         atom.at = Key(0, 0, 1, 1);
  47 |         let text = encode(&atom);
  48 |         let next = dictionary.len() as u32;
  49 |         let index = *indexes.entry(text.clone()).or_insert_with(|| {
  50 |             dictionary.push(text);
  51 |             next
  52 |         });
  53 |         records.push((cell.at, index));
  54 |     }
  55 |     // Definitions precede every use. Lengths are checked before decoder allocation.
  56 |     write_u32(&mut out, dictionary.len());
  57 |     for atom in dictionary {
  58 |         write_text(&mut out, &atom);
  59 |     }
  60 |     write_text(&mut out, &encode(&batch.geometry));
  61 |     write_u32(&mut out, records.len());
  62 |     for (at, index) in records {
  63 |         write_key(&mut out, at);
  64 |         out.extend_from_slice(&index.to_le_bytes());
  65 |     }
  66 |     write_u32(&mut out, batch.removed.len());
  67 |     for &at in &batch.removed {
  68 |         write_key(&mut out, at);
  69 |     }
````

### Tile bounds, stamps and publication identity [CODE]

Source: `apps/dna-calc-model/src/tiles.rs:1` (lines 1–74, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/tiles.rs#L1-L74).

````text
   1 | //! Subscription publications, independent of a painter and of calculation completion.
   2 | //!
   3 | //! These receipts close a projection of already installed host results. They are not engine
   4 | //! AdmissionReceipt/CompletionReceipt or a claim that a workbook is fresh. Engine publication
   5 | //! identities travel alongside stream identities without upgrading individual cell provenance.
   6 | use crate::projection::{Cell, Geometry, Key, Sheet};
   7 | use serde::{Deserialize, Serialize};
   8 | 
   9 | pub const MAX_SUBSCRIPTIONS: usize = 32;
  10 | pub const MAX_TILE_CELLS: usize = 65_536;
  11 | /// A delta can remove the old root and install a disjoint new root.
  12 | pub const MAX_PUBLICATION_RECORDS: usize = MAX_TILE_CELLS * 2;
  13 | /// Sized so that MAX_TILE_CELLS ordinary cells fit: each is charged CELL_BYTES plus its text.
  14 | pub const MAX_PUBLICATION_BYTES: usize = 80 * 1024 * 1024;
  15 | /// What a Cell holds inline, rounded up (896 bytes on 64-bit native). Heap text is added on top.
  16 | pub const CELL_BYTES: usize = 1024;
  17 | pub const MAX_TILE_BYTES: usize = 512 * 1024;
  18 | pub const MAX_TILE_RECORDS: usize = 256;
  19 | pub const MAX_PENDING_REQUESTS: usize = 256;
  20 | 
  21 | /// A stream-local publication identity. `workbook` changes on replacement. `next` counts
  22 | /// publications for this subscription, independently of host deltas coalesced under pressure.
  23 | #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  24 | pub struct TileStamp {
  25 |     /// Latest engine publication incorporated by this root, possibly followed by authored
  26 |     /// edits. This correlates delivery; cell provenance still determines freshness.
  27 |     #[serde(default)]
  28 |     pub calculation_through: Option<crate::projection::Publication>,
  29 |     pub id: u64,
  30 |     pub workbook: u64,
  31 |     pub transaction: u64,
  32 |     pub base: u64,
  33 |     pub next: u64,
  34 |     pub revision: u64,
  35 |     /// Geometry and styles share this publication's root epoch. They can change without an
  36 |     /// authored revision changing, for example when conditional formatting recalculates.
  37 |     pub geometry_version: u64,
  38 |     pub style_version: u64,
  39 |     /// Dictionaries are local to a publication and are released with it.
  40 |     pub dictionary: u64,
  41 |     pub reset: bool,
  42 | }
  43 | 
  44 | /// One bounded binary fragment. Dictionary definitions precede its address/index records on
  45 | /// the wire. Removals are explicit. No fragment is observable until the transaction closes.
  46 | #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  47 | pub struct TileBatch {
  48 |     pub stamp: TileStamp,
  49 |     pub fragment: u32,
  50 |     pub final_fragment: bool,
  51 |     /// Geometry travels in the first fragment and commits with its cells.
  52 |     pub geometry: Vec<(Sheet, Geometry)>,
  53 |     pub cells: Vec<Cell>,
  54 |     pub removed: Vec<Key>,
  55 | }
  56 | 
  57 | #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  58 | pub struct TileBarrier {
  59 |     pub id: u64,
  60 |     pub workbook: u64,
  61 |     pub transaction: u64,
  62 |     pub through: u64,
  63 | }
  64 | impl TileStamp {
  65 |     pub fn barrier(self) -> TileBarrier {
  66 |         TileBarrier {
  67 |             id: self.id,
  68 |             workbook: self.workbook,
  69 |             transaction: self.transaction,
  70 |             through: self.next,
  71 |         }
  72 |     }
  73 | }
  74 | 
````

### Mirror admission, continuity and invisible staging [CODE]

Source: `apps/dna-calc-model/src/tile_mirror.rs:68` (lines 68–116, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/tile_mirror.rs#L68-L116).

````text
  68 | 
  69 |     pub fn admit(&mut self, stamp: TileStamp) -> Result<TileApply, TileMirrorError> {
  70 |         // Old controls, including old resets, cannot roll back an applied root.
  71 |         if self
  72 |             .stamp
  73 |             .is_some_and(|s| s.workbook == stamp.workbook && stamp.next <= s.next)
  74 |         {
  75 |             return Ok(TileApply::Ignored);
  76 |         }
  77 |         let result = self.admit_inner(stamp);
  78 |         if result.is_err() {
  79 |             self.invalidate();
  80 |         }
  81 |         result
  82 |     }
  83 |     fn admit_inner(&mut self, stamp: TileStamp) -> Result<TileApply, TileMirrorError> {
  84 |         if stamp.next == 0 || stamp.base.checked_add(1) != Some(stamp.next) {
  85 |             return Err(TileMirrorError::Continuity);
  86 |         }
  87 |         if stamp.geometry_version != stamp.transaction
  88 |             || stamp.style_version != stamp.transaction
  89 |             || stamp.dictionary != stamp.transaction
  90 |         {
  91 |             return Err(TileMirrorError::Version);
  92 |         }
  93 |         if let Some(current) = self.stamp {
  94 |             if stamp.id != current.id
  95 |                 || stamp.workbook != current.workbook
  96 |                 || stamp.transaction <= current.transaction
  97 |             {
  98 |                 return Err(TileMirrorError::Identity);
  99 |             }
 100 |             if stamp.revision < current.revision {
 101 |                 return Err(TileMirrorError::Version);
 102 |             }
 103 |             if !stamp.reset && (self.failed || stamp.base != current.next) {
 104 |                 return Err(TileMirrorError::Continuity);
 105 |             }
 106 |         } else if !stamp.reset {
 107 |             return Err(TileMirrorError::Continuity);
 108 |         }
 109 |         if let Some(pending) = &self.staging {
 110 |             if stamp.id != pending.stamp.id || stamp.workbook != pending.stamp.workbook {
 111 |                 return Err(TileMirrorError::Identity);
 112 |             }
 113 |             if pending.stamp == stamp {
 114 |                 return Ok(TileApply::Ignored);
 115 |             }
 116 |             // Only a newer explicit reset can retire a broken or abandoned transaction.
````

### Worker projection stream and backpressure [CODE]

Source: `apps/dna-calc-worker/src/publication.rs:67` (lines 67–112, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-worker/src/publication.rs#L67-L112).

````text
  67 |                 return Self::reset();
  68 |             }
  69 |         }
  70 |         Self {
  71 |             reset: false,
  72 |             cells: changed.into_values().collect(),
  73 |             removed: emptied.into_iter().collect(),
  74 |         }
  75 |     }
  76 | }
  77 | impl TileStream {
  78 |     pub fn update(
  79 |         &mut self,
  80 |         reset: bool,
  81 |         cells: Vec<Cell>,
  82 |         removed: Vec<Key>,
  83 |         revision: u64,
  84 |         geometry: Vec<(Sheet, Geometry)>,
  85 |         calculation: Option<crate::wire::Publication>,
  86 |     ) -> Result<(), String> {
  87 |         if self.failed && !reset {
  88 |             return Err("This projection needs a narrower subscription".into());
  89 |         }
  90 |         let incoming = cells.iter().fold(removed.len().saturating_mul(16), |n, c| {
  91 |             n.saturating_add(c.retained_bytes())
  92 |         });
  93 |         if cells.len() > MAX_TILE_CELLS
  94 |             || cells.len().saturating_add(removed.len()) > MAX_PUBLICATION_RECORDS
  95 |             || incoming > MAX_PUBLICATION_BYTES - MAX_TILE_BYTES
  96 |         {
  97 |             self.reject();
  98 |             return Err(
  99 |                 "This projection delta exceeds the tile budget; narrow its visible area".into(),
 100 |             );
 101 |         }
 102 |         if reset {
 103 |             self.failed = false;
 104 |             self.cells.clear();
 105 |         }
 106 |         self.revision = revision;
 107 |         let mut retained = self
 108 |             .cells
 109 |             .values()
 110 |             .fold(0usize, |n, cell| n.saturating_add(cell.retained_bytes()));
 111 |         for at in &removed {
 112 |             if let Some(old) = self.cells.remove(at) {
````

### Worker deltas to tile publications [CODE]

Source: `apps/dna-calc-worker/src/server.rs:3776` (lines 3776–3829, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-worker/src/server.rs#L3776-L3829).

````text
3776 |             let publication = match &delta.cause {
3777 |                 calc_host::DeltaCause::Recalculated { publication, .. } => {
3778 |                     Some(wire::Publication {
3779 |                         domain: publication.command.domain_incarnation,
3780 |                         command: publication.command.sequence,
3781 |                         sequence: publication.sequence,
3782 |                     })
3783 |                 }
3784 |                 _ => None,
3785 |             };
3786 |             if let Some(stream) = self.tile_streams.get_mut(&id) {
3787 |                 if let Err(reason) = stream.update(
3788 |                     delta.reset,
3789 |                     cells,
3790 |                     removed,
3791 |                     revision,
3792 |                     geometry.get(&id).cloned().unwrap_or_default(),
3793 |                     publication,
3794 |                 ) {
3795 |                     self.events.push(Event::ProjectionFailed { id, reason });
3796 |                 }
3797 |             }
3798 |         }
3799 |         for (&id, stream) in &mut self.tile_streams {
3800 |             if stream.revision != revision {
3801 |                 if let Err(reason) = stream.update(
3802 |                     false,
3803 |                     Vec::new(),
3804 |                     Vec::new(),
3805 |                     revision,
3806 |                     geometry.get(&id).cloned().unwrap_or_default(),
3807 |                     None,
3808 |                 ) {
3809 |                     self.events.push(Event::ProjectionFailed { id, reason });
3810 |                 }
3811 |             }
3812 |             let transaction = self.next_publication;
3813 |             self.next_publication = self
3814 |                 .next_publication
3815 |                 .checked_add(1)
3816 |                 .expect("Tile transaction identities exhausted");
3817 |             match stream.publish(id, self.workbook, transaction) {
3818 |                 Ok(events) => self.events.extend(events),
3819 |                 Err(reason) => self.events.push(Event::ProjectionFailed { id, reason }),
3820 |             }
3821 |         }
3822 |         for event in terminal {
3823 |             self.push_calculation_event(event, true);
3824 |         }
3825 |         if self
3826 |             .tile_streams
3827 |             .values()
3828 |             .all(|stream| !stream.has_pending())
3829 |         {
````

### Control protocol: command envelopes [CODE]

Source: `apps/dna-calc-model/src/projection.rs:663` (lines 663–682, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/projection.rs#L663-L682).

````text
 663 | pub struct CommandEnvelope {
 664 |     pub session: String,
 665 |     pub id: u64,
 666 |     pub workbook: u64,
 667 |     pub base_revision: u64,
 668 |     pub source: String,
 669 |     pub undo_group: Option<u64>,
 670 |     pub resolved_selection: Option<(Key, Vec<Area>)>,
 671 |     pub request: Box<Request>,
 672 | }
 673 | 
 674 | /// The page to the server.
 675 | #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
 676 | pub struct ToServer {
 677 |     pub id: u64,
 678 |     pub request: Request,
 679 | }
 680 | 
 681 | /// Commands carry neutral criteria; the host validates and owns their authored representation.
 682 | #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
````

### Control protocol: subscriptions and requests [CODE]

Source: `apps/dna-calc-model/src/projection.rs:801` (lines 801–858, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/projection.rs#L801-L858).

````text
 801 | pub enum Request {
 802 |     /// The picture a cell holds: [`Reply::CellImage`], with its bytes beside the reply.
 803 |     CellImage {
 804 |         at: Key,
 805 |     },
 806 |     PageSetup(Sheet),
 807 |     PrintPreview {
 808 |         sheet: Sheet,
 809 |         revision: u64,
 810 |     },
 811 |     InspectTemplate,
 812 |     OpenTemplate {
 813 |         name: String,
 814 |     },
 815 |     InsertTemplate {
 816 |         name: Option<String>,
 817 |         revision: u64,
 818 |     },
 819 |     FilterValues {
 820 |         sheet: Sheet,
 821 |         table: Option<String>,
 822 |         column: u32,
 823 |     },
 824 |     Names {
 825 |         query: String,
 826 |         page: u64,
 827 |     },
 828 |     EvaluateName {
 829 |         id: (u32, u32),
 830 |         version: u64,
 831 |         anchor: Key,
 832 |     },
 833 |     /// F9 on a part of the formula being edited at `anchor` (dc-0f1k.7): the value of the part of
 834 |     /// `formula` from `start` for `len` UTF-16 code units, as that cell would read it.
 835 |     EvaluatePart {
 836 |         formula: String,
 837 |         anchor: Key,
 838 |         start: usize,
 839 |         len: usize,
 840 |     },
 841 |     /// The evaluation trace of the formula being edited at `anchor`: every part with its value.
 842 |     TraceFormula {
 843 |         formula: String,
 844 |         anchor: Key,
 845 |     },
 846 |     /// The registered functions the workbook can call (add-in registrations and VBA), for the
 847 |     /// formula field's completion.
 848 |     UserFunctions,
 849 |     /// The add-ins the host has loaded (dc-0f1k.23). A browser worker loads none and says so.
 850 |     Addins,
 851 |     /// The native host shows its file dialog and loads the .xll or .xlam add-in the person chooses.
 852 |     LoadAddinNative,
 853 |     /// The native host loads the .xll or .xlam add-in at `path`.
 854 |     LoadAddin {
 855 |         path: String,
 856 |     },
 857 |     /// The native host unloads the add-in it loaded from `path`.
 858 |     UnloadAddin {
````

### Events and server replies [CODE]

Source: `apps/dna-calc-model/src/projection.rs:1770` (lines 1770–1817, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/projection.rs#L1770-L1817).

````text
1770 | pub enum Event {
1771 |     WorkbookEventFailed {
1772 |         workbook: u64,
1773 |         sequence: u64,
1774 |         event: String,
1775 |         reason: String,
1776 |         omitted: u64,
1777 |     },
1778 |     SaveReady {
1779 |         workbook: u64,
1780 |         token: u64,
1781 |     },
1782 |     TileAdmission(crate::tiles::TileStamp),
1783 |     Tiles(crate::tiles::TileBatch),
1784 |     TileCompletion(crate::tiles::TileBarrier),
1785 |     ProjectionFailed {
1786 |         id: u64,
1787 |         reason: String,
1788 |     },
1789 |     Calculation {
1790 |         workbook: u64,
1791 |         receipt: CalculationEvent,
1792 |         summary: Option<Summary>,
1793 |         #[serde(default)]
1794 |         failed_projections: Vec<u64>,
1795 |     },
1796 | }
1797 | 
1798 | /// The server to the page. A message lives only until it is encoded or applied, so its
1799 | /// variants' sizes are left as they are.
1800 | #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
1801 | #[allow(clippy::large_enum_variant)]
1802 | pub enum FromServer {
1803 |     /// The worker is loaded and listening.
1804 |     Started,
1805 |     /// The answer to the request `id`, and whether bytes travel beside it.
1806 |     Reply {
1807 |         id: u64,
1808 |         reply: Reply,
1809 |     },
1810 |     /// The request `id` (or, with no id, a message that was not a request) could not be served.
1811 |     Failed {
1812 |         id: Option<u64>,
1813 |         message: String,
1814 |     },
1815 |     Event(Event),
1816 | }
1817 | 
````

### Notebook blocks and publication IDs [CODE]

Source: `apps/dna-calc-model/src/projection.rs:1871` (lines 1871–1907, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/projection.rs#L1871-L1907).

````text
1871 | pub struct Notebook {
1872 |     pub total: u64,
1873 |     pub page: u64,
1874 |     pub omitted: u64,
1875 |     pub blocks: Vec<NotebookBlock>,
1876 | }
1877 | #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
1878 | pub struct NotebookBlock {
1879 |     pub placement: Option<String>,
1880 |     pub anchor: Option<Area>,
1881 |     pub prose: bool,
1882 |     pub cell: Option<Cell>,
1883 | }
1884 | #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
1885 | pub struct BlockDetail {
1886 |     pub summary: String,
1887 |     pub anchor: Option<Key>,
1888 |     pub cells: Vec<Cell>,
1889 |     pub blockers: Vec<Area>,
1890 | }
1891 | 
1892 | /// One sheet as the skins list it.
1893 | #[derive(Clone, Debug, PartialEq, Eq)]
1894 | pub struct SheetTab {
1895 |     pub id: SheetId,
1896 |     pub name: String,
1897 | }
1898 | 
1899 | /// Initial notebook projection page size.
1900 | pub const NOTEBOOK_PAGE: usize = 40;
1901 | #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
1902 | pub struct Publication {
1903 |     pub domain: u64,
1904 |     pub command: u64,
1905 |     pub sequence: u64,
1906 | }
1907 | 
````

### Shared model commands and skin switching [CODE]

Source: `apps/dna-calc-model/src/command.rs:1` (lines 1–43, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/command.rs#L1-L43).

````text
   1 | //! Named commands, availability and the shared shell keymap. A skin submits a command;
   2 | //! the application adapter executes the returned effect against its single transport.
   3 | use crate::{FindMode, Model, SkinId};
   4 | use calc_value::{CellKey, GridArea, NameId, SheetId};
   5 | 
   6 | #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
   7 | pub enum CommandId {
   8 |     PageSetup,
   9 |     PrintPreview,
  10 |     PasteSpecial,
  11 |     FlashFill,
  12 |     FormatCells,
  13 |     NameManager,
  14 |     Undo,
  15 |     Redo,
  16 |     Find,
  17 |     Replace,
  18 |     GoTo,
  19 |     Recalculate,
  20 |     Reveal,
  21 |     SwitchSkin,
  22 | }
  23 | impl CommandId {
  24 |     pub const ALL: [Self; 14] = [
  25 |         Self::PageSetup,
  26 |         Self::PrintPreview,
  27 |         Self::PasteSpecial,
  28 |         Self::FlashFill,
  29 |         Self::FormatCells,
  30 |         Self::NameManager,
  31 |         Self::Undo,
  32 |         Self::Redo,
  33 |         Self::Find,
  34 |         Self::Replace,
  35 |         Self::GoTo,
  36 |         Self::Recalculate,
  37 |         Self::Reveal,
  38 |         Self::SwitchSkin,
  39 |     ];
  40 |     pub const fn stable_id(self) -> &'static str {
  41 |         match self {
  42 |             Self::PageSetup => "page.setup",
  43 |             Self::PrintPreview => "page.print-preview",
````

### DOM-free render plan types [CODE]

Source: `apps/dna-calc-model/src/render_plan.rs:55` (lines 55–91, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/render_plan.rs#L55-L91).

````text
  55 | 
  56 | /// One cell of a [`RenderPlan`]: its 1-based address, its viewport-local rectangle and what it
  57 | /// shows.
  58 | #[derive(Debug, Clone, PartialEq, serde::Serialize)]
  59 | pub struct PlannedCell {
  60 |     /// Uniform physical sides, with varying merged sides represented by boundary strokes.
  61 |     pub borders: [Option<crate::projection::BorderShown>; 4],
  62 |     pub boundary: Vec<BorderSegment>,
  63 |     pub row_span: u32,
  64 |     pub col_span: u32,
  65 |     pub editability: Editability,
  66 |     /// Advance of the source shown text before fitting, in CSS pixels.
  67 |     pub text_advance: f64,
  68 |     pub text_measured: bool,
  69 |     /// Display font size in CSS pixels before zoom. Authored style size stays in `shown`.
  70 |     pub paint_pixels: f64,
  71 |     /// Extra bottom inset from the largest measured descent in this row (CSS pixels).
  72 |     pub baseline_inset: f64,
  73 |     /// Viewport-local clipping of this fragment (a merge may cross frozen panes).
  74 |     pub clip: Option<CellRect>,
  75 |     pub address: String,
  76 |     pub row: u32,
  77 |     pub col: u32,
  78 |     pub rect: CellRect,
  79 |     /// Physical cell before text extends over blank neighbours.
  80 |     pub anchor: CellRect,
  81 |     #[serde(serialize_with = "serialize_shown")]
  82 |     pub shown: CellShown,
  83 | }
  84 | 
  85 | #[derive(Debug, Clone, PartialEq, serde::Serialize)]
  86 | pub struct BorderSegment {
  87 |     pub side: u8,
  88 |     pub x1: f64,
  89 |     pub y1: f64,
  90 |     pub x2: f64,
  91 |     pub y2: f64,
````

### Skin registry and mounted-view contract [CODE]

Source: `apps/dna-calc/src/skin.rs:1` (lines 1–84, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc/src/skin.rs#L1-L84).

````text
   1 | //! The skin framework: the contract every DNA Calc skin meets, the registry of skins, and which
   2 | //! skin is shown.
   3 | //!
   4 | //! A skin is one way of looking at and editing the one open workbook: the Excel grid, the
   5 | //! notebook, the atlas, the tree, the canvas and the ledger. The application shows exactly one
   6 | //! skin at a time.
   7 | //!
   8 | //! - **Switching re-projects, it never reloads.** [`switch_skin`] dispatches the model's skin
   9 | //!   command. A live edit transfers to the shared formula bar; workbook state and history stay
  10 | //!   intact. Mounted views release and renew their subscriptions.
  11 | //! - **A skin reaches the workbook only through its [`SkinContext`]**: the controller, whose every
  12 | //!   read is a neutral cell projection or a subscription's delta and whose every change is a
  13 | //!   [`crate::wire::Intent`].
  14 | //! - **A skin that cannot be shown says so**: [`resolve_active_skin`] gives
  15 | //!   [`SkinResolution::Unavailable`] for an identity the registry does not hold, which the
  16 | //!   application shows as a card naming it, never a blank or another skin.
  17 | //!
  18 | //! U1's current authoring surface:
  19 | //! - Implement `Skin` with a stable `SkinId::new("your-id")`, `mount` and a focus target.
  20 | //!   Declare the module and register it together in `app.rs`; an additional skin does not extend
  21 | //!   a central identity enum or the controller's state/reset list.
  22 | //! - Register pure, workbook-relative view data with `Controller::view_field` and a `ViewState`
  23 | //!   schema. The model retains it across mounts and resets it on workbook replacement.
  24 | //! - Submit `dna_calc_model::Command` for common actions and `Controller::reveal` for source
  25 | //!   navigation. The shell supplies Undo/Redo, Find/Replace/Go to and F9 to every mounted skin.
  26 | //! - For a shared formula editor, call `Controller::register_editor_surface` with an open
  27 | //!   `SurfaceDescriptor` and its `FormulaAssist`. Declare whether later keys retain the target,
  28 | //!   follow the active selection, or continue through the skin's navigation surface.
  29 | //!   The browser adapter binds each mount and queues its input without Grid or formula-bar DOM
  30 | //!   markers. Give simultaneously mounted logical editors distinct surface ids. Native controls
  31 | //!   retain their own keys.
  32 | //! - Declare the navigation surface's clipboard ownership and optional input target when it
  33 | //!   differs from the readiness-aware focus target. The model stores those surface roles.
  34 | //! - Reuse existing projections through the controller. Scoped CSS under `web/css` is included
  35 | //!   automatically. A new projection family extends the shared contract and worker, while an additional skin
  36 | //!   over existing projections needs only its module and its declaration/registration in app.rs.
  37 | //! - Give `Controller::focus_projection` the target and submission focus ticket when a projection
  38 | //!   refresh needs focus. The browser adapter owns timing, retries and stale-response refusal.
  39 | //!
  40 | //! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-shell/src/stage.rs` (`StageId`,
  41 | //! `StageSurface`, `StageRegistry`, `StageResolution`, `resolve_active_stage`, `switch_stage`,
  42 | //! with their tests) and the registration of `src/dnacalc-app/src/app.rs`. Left behind: the
  43 | //! profiles that decided which stages a product showed, the shared skin state and its audit log,
  44 | //! the continuity halo and the preview service.
  45 | 
  46 | use crate::controller::Controller;
  47 | use leptos::prelude::*;
  48 | use std::sync::Arc;
  49 | 
  50 | pub use dna_calc_model::SkinId;
  51 | 
  52 | /// What a skin is given when it is shown.
  53 | #[derive(Clone, Copy)]
  54 | pub struct SkinContext {
  55 |     pub controller: Controller,
  56 | }
  57 | 
  58 | /// The contract of a skin.
  59 | pub trait Skin: Send + Sync + 'static {
  60 |     fn id(&self) -> SkinId;
  61 |     fn title(&self) -> &'static str;
  62 |     /// Input surface while worker-backed content is still mounting. Defaults to the focus target.
  63 |     fn input_target(&self) -> &'static str {
  64 |         self.focus_target()
  65 |     }
  66 |     /// Clipboard routing for this skin's navigation surface; registered editors retain native text.
  67 |     fn clipboard_owner(&self) -> dna_calc_model::ClipboardOwner {
  68 |         dna_calc_model::ClipboardOwner::Native
  69 |     }
  70 |     /// A focusable surface belonging to this skin. The shell owns focus timing.
  71 |     fn focus_target(&self) -> &'static str {
  72 |         "#dc-workspace"
  73 |     }
  74 |     /// The skin's view over the workbook. It is dropped when another skin is shown, so a skin
  75 |     /// releases what it holds (its subscriptions) in `on_cleanup`.
  76 |     fn mount(&self, ctx: SkinContext) -> AnyView;
  77 | }
  78 | 
  79 | /// The skins, in the switcher's order: the order they were registered in. A keyboard slot (1 to
  80 | /// 9) is a skin's place in that order.
  81 | #[derive(Clone, Default)]
  82 | pub struct SkinRegistry {
  83 |     skins: Vec<Arc<dyn Skin>>,
  84 | }
````

### Notebook current skin route [CODE]

Source: `apps/dna-calc/src/notebook.rs:1` (lines 1–43, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc/src/notebook.rs#L1-L43).

````text
   1 | //! A bounded notebook of worksheet sources and saved links. Input uses the shared edit session;
   2 | //! block order is document layout metadata, independent of worksheet dependency order.
   3 | //!
   4 | //! Extends DnaTreeCalc bf6dfd7 `src/dnacalc-stage-notebook/src/lib.rs` and `src/model.rs`.
   5 | 
   6 | use crate::cells::{self, Provenance};
   7 | use crate::controller::Controller;
   8 | use crate::skin::{Skin, SkinContext, SkinId};
   9 | use crate::wire;
  10 | use leptos::prelude::*;
  11 | 
  12 | /// The blocks one page shows.
  13 | pub use dna_calc_model::projection::NOTEBOOK_PAGE as PAGE;
  14 | 
  15 | #[derive(Clone, Copy, Debug, Default)]
  16 | pub struct NotebookSkin;
  17 | 
  18 | impl Skin for NotebookSkin {
  19 |     fn id(&self) -> SkinId {
  20 |         SkinId::Notebook
  21 |     }
  22 | 
  23 |     fn title(&self) -> &'static str {
  24 |         "Notebook"
  25 |     }
  26 | 
  27 |     fn mount(&self, ctx: SkinContext) -> AnyView {
  28 |         notebook_view(ctx.controller).into_any()
  29 |     }
  30 | }
  31 | 
  32 | /// What a block holds.
  33 | pub fn block_kind(shown: &cells::CellShown, has_input: bool) -> &'static str {
  34 |     match (shown.formula, has_input) {
  35 |         (true, _) => "formula",
  36 |         (false, true) => "value",
  37 |         (false, false) => "spilled value",
  38 |     }
  39 | }
  40 | 
  41 | /// Pages are session state; cell edits and layout have workbook history.
  42 | #[derive(Clone, Copy)]
  43 | pub struct ViewState {
````

### Name projections and explicit evaluation request [CODE]

Source: `apps/dna-calc-model/src/names.rs:1` (lines 1–34, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/names.rs#L1-L34).

````text
   1 | //! Name definitions are passive projections. A value is always a qualified, explicit evaluation.
   2 | use crate::projection::{Area, Key, Sheet};
   3 | use serde::{Deserialize, Serialize};
   4 | 
   5 | #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
   6 | pub struct NameRecord {
   7 |     pub id: (u32, u32),
   8 |     pub version: u64,
   9 |     pub name: String,
  10 |     pub scope: Option<Sheet>,
  11 |     pub formula: String,
  12 |     pub hidden: bool,
  13 |     pub comment: Option<String>,
  14 | }
  15 | #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
  16 | pub struct NamePage {
  17 |     pub revision: u64,
  18 |     pub rows: Vec<NameRecord>,
  19 |     pub page: u64,
  20 |     pub total: usize,
  21 | }
  22 | /// Bounded execution metadata. Body indices are local to this diagnostic command.
  23 | #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  24 | pub struct NameBodyEvaluation {
  25 |     pub ordinal: u64,
  26 |     pub parent: Option<u64>,
  27 |     pub id: (u32, u32),
  28 |     pub version: u64,
  29 |     pub scope: Option<Sheet>,
  30 |     pub caller: Key,
  31 |     pub definition_sheet: Option<Sheet>,
  32 |     pub body: usize,
  33 |     pub result: NameBodyResult,
  34 | }
````

### Dedicated wasm worker posting and slice admission [CODE]

Source: `apps/dna-calc-worker/src/worker.rs:94` (lines 94–143, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-worker/src/worker.rs#L94-L143).

````text
  94 |         let mut server = server.borrow_mut();
  95 |         let server = server.get_or_insert_with(|| {
  96 |             let mut server = crate::server::Server::new();
  97 |             server.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
  98 |             server.set_fetch_route(std::rc::Rc::new(BrowserFetch::default()));
  99 |             server
 100 |         });
 101 |         let (reply, out) = crate::server::answer_text(server, &text, bytes);
 102 |         (reply, out, server.take_events())
 103 |     });
 104 |     // The events first, as the in-process transport delivers them: a reply finds its
 105 |     // subscriptions' cells already applied.
 106 |     for event in events {
 107 |         if !post_event(event) {
 108 |             return;
 109 |         }
 110 |     }
 111 |     let message = wasm_bindgen::JsValue::from_str(&reply);
 112 |     match out {
 113 |         None => {
 114 |             let _ = scope().post_message(&message);
 115 |         }
 116 |         Some(bytes) => {
 117 |             // A saved package travels beside the reply, its buffer transferred.
 118 |             let array = js_sys::Uint8Array::from(bytes.as_slice());
 119 |             let buffer = array.buffer();
 120 |             let pair = js_sys::Array::of2(&message, &buffer);
 121 |             let _ = scope().post_message_with_transfer(&pair, &js_sys::Array::of1(&buffer));
 122 |         }
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
````

### Browser versus local transport selection [CODE]

Source: `apps/dna-calc/src/port.rs:107` (lines 107–137, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc/src/port.rs#L107-L137).

````text
 107 | pub fn start() {
 108 |     let started = PORT.with(|port| port.borrow().transport.is_some());
 109 |     if started {
 110 |         return;
 111 |     }
 112 |     #[cfg(target_arch = "wasm32")]
 113 |     {
 114 |         match start_worker() {
 115 |             Ok(link) => {
 116 |                 PORT.with(|port| port.borrow_mut().transport = Some(Transport::Worker(link)));
 117 |                 // A first round trip, timed, so the page can show the worker answers.
 118 |                 let asked = crate::controller::now_ms();
 119 |                 call(Request::Ping, None, move |reply, _| match reply {
 120 |                     Ok(Reply::Pong { .. }) => {
 121 |                         mark(
 122 |                             "data-dnacalc-worker-ms",
 123 |                             &format!("{:.1}", crate::controller::now_ms() - asked),
 124 |                         );
 125 |                         mark("data-dnacalc-worker", "ready");
 126 |                         notify_health(Some("Worker ready".to_owned()));
 127 |                     }
 128 |                     Ok(_) => mark(
 129 |                         "data-dnacalc-worker",
 130 |                         "failed: a ping answered with something else",
 131 |                     ),
 132 |                     Err(e) => mark("data-dnacalc-worker", &format!("failed: {e}")),
 133 |                 });
 134 |                 return;
 135 |             }
 136 |             Err(why) => {
 137 |                 fail(why);
````

### Current Canvas2D painter over committed plans [CODE]

Source: `apps/dna-calc/src/grid/painter.rs:1` (lines 1–37, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc/src/grid/painter.rs#L1-L37).

````text
   1 | //! Canvas2D paints the same committed plan as DOM. Interaction, semantic rows, headers,
   2 | //! charts, review controls and the live editor stay in the shared grid adapter.
   3 | use super::{
   4 |     alignment, autofit,
   5 |     geometry::GridMetrics,
   6 |     panes::{self, Panes},
   7 |     render_plan::{PlannedCell, RenderPlan},
   8 | };
   9 | use crate::cells::Provenance;
  10 | use leptos::prelude::*;
  11 | use wasm_bindgen::JsCast;
  12 | use web_sys::{CanvasRenderingContext2d as Context, HtmlCanvasElement};
  13 | 
  14 | const STORAGE: &str = "dna-grid-painter";
  15 | pub(super) fn initial_canvas() -> bool {
  16 |     let query = window().location().search().unwrap_or_default();
  17 |     if let Some(value) = query
  18 |         .trim_start_matches('?')
  19 |         .split('&')
  20 |         .find_map(|part| part.strip_prefix("renderer="))
  21 |     {
  22 |         return value == "canvas";
  23 |     }
  24 |     window()
  25 |         .session_storage()
  26 |         .ok()
  27 |         .flatten()
  28 |         .and_then(|s| s.get_item(STORAGE).ok().flatten())
  29 |         .as_deref()
  30 |         == Some("canvas")
  31 | }
  32 | pub(super) fn remember(canvas: bool) {
  33 |     if let Ok(Some(storage)) = window().session_storage() {
  34 |         let _ = storage.set_item(STORAGE, if canvas { "canvas" } else { "dom" });
  35 |     }
  36 | }
  37 | fn font(cell: &PlannedCell, scale: f64) -> String {
````

### Desktop WebView2 composition entry [CODE]

Source: `apps/dna-calc-desktop/src/main.rs:1` (lines 1–34, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-desktop/src/main.rs#L1-L34).

````text
   1 | //! DNA Calc on the Windows desktop: the workbook composition over CalcHost, with the add-ins
   2 | //! loaded through `calc-addin-host` and the window chain and WebView2 host from
   3 | //! `calc-platform-desktop`. Two modes:
   4 | //! - `--app`: the DNA Calc page in WebView2 over the native workbook worker (`app.rs`);
   5 | //! - `--engine`: the command-line harness through CalcHost's checked route (`engine.rs`), whose
   6 | //!   case files are the same ones the OneCalc harness runs (`dna-onecalc-desktop`).
   7 | //!
   8 | //! This executable does not depend on `dna-onecalc`; OneCalc's harness is the other executable.
   9 | //! Each exports its own `MdCallBack12` and carries its own version resource.
  10 | #[cfg(target_arch = "wasm32")]
  11 | compile_error!("dna-calc-desktop is native-only");
  12 | #[cfg(windows)]
  13 | mod app;
  14 | #[cfg(windows)]
  15 | mod embedded;
  16 | #[cfg(windows)]
  17 | mod engine;
  18 | #[cfg(windows)]
  19 | mod fetch;
  20 | mod file_import;
  21 | mod origin;
  22 | 
  23 | #[cfg(windows)]
  24 | #[unsafe(no_mangle)]
  25 | pub unsafe extern "system" fn MdCallBack12(
  26 |     function: i32,
  27 |     count: i32,
  28 |     args: *const *mut calc_xll_abi::XlOper12,
  29 |     result: *mut calc_xll_abi::XlOper12,
  30 | ) -> i32 {
  31 |     unsafe { calc_xll_host::dispatch(function, count, args, result) }
  32 | }
  33 | 
  34 | /// Loads one add-in file into `set`: an `.xlam` (a VBA add-in: its project's public functions,
````

### Worker notebook name/cell projection [CODE]

Source: `apps/dna-calc-worker/src/server.rs:3861` (lines 3861–3928, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-worker/src/server.rs#L3861-L3928).

````text
3861 | 
3862 |     fn notebook(
3863 |         &self,
3864 |         sheet: Sheet,
3865 |         requested: usize,
3866 |         reveal: Option<CellKey>,
3867 |         placement: Option<&str>,
3868 |     ) -> wire::Notebook {
3869 |         use calc_host::{BlockKind, ViewKind};
3870 |         let host = self.book.host();
3871 |         let layout = host.model().layout();
3872 |         let mut linked = Vec::new();
3873 |         let mut placed = Vec::new();
3874 |         for placement in layout.view(ViewKind::Notebook) {
3875 |             if let Some(block) = layout.block(placement.block()) {
3876 |                 let anchor = block.anchor();
3877 |                 if anchor.is_none_or(|a| a.sheet() == sheet.id()) {
3878 |                     if let Some(a) = anchor {
3879 |                         placed.push(a);
3880 |                     }
3881 |                     linked.push((
3882 |                         Some(placement.id().to_string()),
3883 |                         anchor,
3884 |                         block.kind() == BlockKind::Prose,
3885 |                     ));
3886 |                 }
3887 |             }
3888 |         }
3889 |         let reveal = reveal.filter(|at| {
3890 |             host.occupied_areas(sheet.id())
3891 |                 .iter()
3892 |                 .any(|a| a.contains(*at))
3893 |         });
3894 |         let (keys, omitted) =
3895 |             crate::tree::outline_keys(host, sheet.id(), wire::NOTEBOOK_PAGE, reveal);
3896 |         linked.extend(
3897 |             keys.into_iter()
3898 |                 .filter(|at| !placed.iter().any(|a| a.contains(*at)))
3899 |                 .map(|at| (None, Some(GridArea::cell(at)), false)),
3900 |         );
3901 |         let total = linked.len();
3902 |         let pages = total.div_ceil(wire::NOTEBOOK_PAGE).max(1);
3903 |         let page = placement
3904 |             .and_then(|id| linked.iter().position(|(p, _, _)| p.as_deref() == Some(id)))
3905 |             .or_else(|| {
3906 |                 reveal.and_then(|at| {
3907 |                     linked
3908 |                         .iter()
3909 |                         .position(|(_, a, _)| a.is_some_and(|a| a.contains(at)))
3910 |                 })
3911 |             })
3912 |             .map(|i| i / wire::NOTEBOOK_PAGE)
3913 |             .unwrap_or(requested)
3914 |             .min(pages - 1);
3915 |         let blocks = linked
3916 |             .into_iter()
3917 |             .skip(page * wire::NOTEBOOK_PAGE)
3918 |             .take(wire::NOTEBOOK_PAGE)
3919 |             .map(|(placement, anchor, prose)| {
3920 |                 let cell = anchor.and_then(|a| self.book.cell(a.first())).map(|p| {
3921 |                     wire::cell_of(
3922 |                         &self.book.shown(&p),
3923 |                         cells::input_text(p.input()),
3924 |                         cells::provenance_text(host, &p),
3925 |                         &p,
3926 |                     )
3927 |                 });
3928 |                 wire::NotebookBlock {
````

### Projected cells preserve provenance and qualified display state [CODE]

Source: `apps/dna-calc-model/src/projection.rs:9` (lines 9–59, DnaCalc pin). [GitHub excerpt](https://github.com/DnaCalc/DnaCalc/blob/3b5aa86b3ba47cbd216daec70c5d6586ad520deb/apps/dna-calc-model/src/projection.rs#L9-L59).

````text
   9 | pub enum Provenance {
  10 |     /// The cell holds nothing.
  11 |     Empty,
  12 |     /// The cell's input is a literal, which is its value.
  13 |     Literal,
  14 |     /// The engine calculated the value, and it still stands.
  15 |     Calculated,
  16 |     /// The value the file stored for the formula; nothing here calculated it.
  17 |     FileCache,
  18 |     /// The engine refused the formula: it has no value.
  19 |     Refused,
  20 |     /// The formula has no value that stands at the current revision.
  21 |     Stale,
  22 | }
  23 | 
  24 | impl Provenance {
  25 |     /// The name the page carries in `data-provenance` and the class suffix.
  26 |     pub const fn name(self) -> &'static str {
  27 |         match self {
  28 |             Provenance::Empty => "empty",
  29 |             Provenance::Literal => "literal",
  30 |             Provenance::Calculated => "calculated",
  31 |             Provenance::FileCache => "file-cache",
  32 |             Provenance::Refused => "refused",
  33 |             Provenance::Stale => "stale",
  34 |         }
  35 |     }
  36 | }
  37 | 
  38 | /// One cell as a skin shows it.
  39 | #[derive(Clone, Debug, PartialEq)]
  40 | pub struct CellShown {
  41 |     pub style: TextStyle,
  42 |     pub at: CellKey,
  43 |     /// The text in the cell: the value as a cell of standard width shows it, `refused` for a
  44 |     /// refusal, a stale cell's last value, or nothing.
  45 |     pub text: String,
  46 |     /// The value's type: `number`, `text`, `logical`, `error`, `array`, or empty.
  47 |     pub kind: &'static str,
  48 |     pub provenance: Provenance,
  49 |     /// Whether the cell's input is a formula.
  50 |     pub formula: bool,
  51 |     /// How the cell's number format applied.
  52 |     pub format: FormatShown,
  53 |     /// The color the number format gives the text (`[Red]`, `[Color10]`), as `#rrggbb`.
  54 |     pub color: Option<String>,
  55 |     /// What the sheet's conditional formats give the cell (dc-6yw), its colours resolved.
  56 |     pub conditional: Option<ConditionalShown>,
  57 | }
  58 | 
  59 | /// A cell's look under its conditional formats, as the grid draws it: colours as `#rrggbb`.
````
