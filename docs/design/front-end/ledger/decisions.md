# DNA Calc Front-End Design Ledger — Stage 3 Decisions

Every conflict across all 1,814 idea cards in [ideas.jsonl](ideas.jsonl) and 207 clusters in [clusters.md](clusters.md) is resolved below. **Part 1 (`D-01`–`D-14`)** records the authoritative owner and coordinator rulings from [DECISIONS_20261126.md](../DECISIONS_20261126.md), which override any disagreeing source. **Part 2 (`D-15`–`D-93`)** records the decider rulings on every remaining conflict, naming the options, supporting sources, the chosen design, what is not taken (for Design Book Appendix Z), and the reason.

No decisions are left open to the owner.

## Summary Counts

- **Idea cards covered:** 1814 / 1814 (0 deleted)
- **Clusters:** 207 across 16 parts
- **Symmetric `same-as` pairs:** 1107 (1092 cards linked across 399 equivalence components)
- **Symmetric `conflicts-with` pairs:** 1117 (698 cards linked; 100% resolved below)
- **Canonical decisions:** 93 (`14` from `DECISIONS_20261126.md` + `79` decider rulings consolidating all 163 per-topic conflicts and cross-topic conflicts; `0` deferred to owner)

## Part 1: Owner and Coordinator Rulings (`DECISIONS_20261126.md`, `D-01`–`D-14`)

| ID | Authority | Topic | Ruling Summary | Clusters |
|---|---|---|---|---|
| [D-01](#d-01) | DECISIONS_20261126 #1 | I. Principles | Ledger-first merge method and single coherent design | CL-01-13, CL-16-01 |
| [D-02](#d-02) | DECISIONS_20261126 #2 | IX. Formula Explorer | Standalone Formula Explorer ships early as a public teaser | CL-09-01, CL-15-10, CL-16-03 |
| [D-03](#d-03) | DECISIONS_20261126 #3 | VII. Model map | Ticked models (RTD clock + macro-ticked Excel models) and circular reference != time delay | CL-01-01, CL-01-03, CL-07-06, CL-07-07, CL-07-08 (+4) |
| [D-04](#d-04) | DECISIONS_20261126 #4 | XIV. Automation | Undo across a macro run: design and try it, do not promise it in v1 | CL-02-10, CL-02-20, CL-04-30, CL-10-16, CL-14-04 (+1) |
| [D-05](#d-05) | DECISIONS_20261126 #5 | XIII. Persistence | Notes and findings live in customXml now; Excel-visible mirror is optional | CL-05-12, CL-13-03 |
| [D-06](#d-06) | DECISIONS_20261126 #6 | VI. Notebook | Notebook works exactly like Jupyter; no separate read-mode presentation (reader presentation -> App) | CL-06-01, CL-10-17, CL-13-06, CL-16-04 |
| [D-07](#d-07) | DECISIONS_20261126 #7 | XII. Customisation and the skin API | Stay on WebView2/Leptos baseline; FerroUI not at this stage | CL-12-02 |
| [D-08](#d-08) | DECISIONS_20261126 #8 | I. Principles | Drop legacy website tagline and unify app<->website storytelling | CL-01-13, CL-03-06 |
| [D-09](#d-09) | DECISIONS_20261126 #9 | XII. Customisation and the skin API | Skin architecture is central: merge independent architecture designs before building UI | CL-12-01, CL-16-03 |
| [D-10](#d-10) | DECISIONS_20261126 #10 | XII. Customisation and the skin API | Write new UI fresh against clean skin interface; discard legacy skin code | CL-12-02 |
| [D-11](#d-11) | DECISIONS_20261126 #11 | IV. Classic | Classic uses Excel's exact key-tip letters and keyboard bindings | CL-04-04 |
| [D-12](#d-12) | DECISIONS_20261126 #12 | III. Shell | View-switching keys: Ctrl+Alt+1..4; Ctrl+Alt+5 preserved for Excel floating shapes -> App via Alt+Q / Alt+Shift+5 | CL-02-06, CL-03-04, CL-04-01, CL-04-05, CL-04-28 (+7) |
| [D-13](#d-13) | DECISIONS_20261126 #13 | XIII. Persistence | Never add hidden defined names as anchors; store all anchors in customXml | CL-13-01, CL-13-02 |
| [D-14](#d-14) | DECISIONS_20261126 #14 | XIII. Persistence | Persist compact structured agent change trail in workbook customXml (ON by default, toggleable) | CL-02-17, CL-13-03, CL-15-05, CL-16-04 |

### D-01: Ledger-first merge method and single coherent design

- **Authority:** DECISIONS_20261126 #1 | **Primary topic:** I. Principles | **Clusters:** `CL-01-13`, `CL-16-01`
- **Options & sources:**
  - **Ledger-first merge with explicit Appendix Z (Adopted — Owner Ruling #1)** (FRAMING; `FRAMING-0164`, `FRAMING-0166`, `FRAMING-0058`): Extract all atomic cards first, cluster and link same-as/conflicts-with without deleting, resolve every conflict in decisions.md, write one coherent Design Book citing card IDs, and place every rejected/parked idea in Appendix Z with its reason.
- **Decision:** Execute the ledger-first merge method from MERGE_PROMPT_PROVISIONAL.md. Produce one coherent design across all parts of the Design Book; never merge conflicting options into an overlapping pile. Every idea not taken goes to Appendix Z with its decision ID and reason.
- **Not taken (Appendix Z):** None — this ruling establishes the merge process itself and supersedes ad-hoc single-pass summarisation.
- **Reason:** Owner ruling (2026-11-26 #1 and Aim). Makes dropping or deferring an idea a visible, reasoned act rather than an accidental loss across >100,000 words of source designs.

### D-02: Standalone Formula Explorer ships early as a public teaser

- **Authority:** DECISIONS_20261126 #2 | **Primary topic:** IX. Formula Explorer | **Clusters:** `CL-09-01`, `CL-15-10`, `CL-16-03`
- **Options & sources:**
  - **Ship standalone `DNA Calc - Formula Explorer` early as a public teaser** [CL-09-01] (FRAMING; `FRAMING-0167`): Deliver standalone Formula Explorer early to showcase DNA Calc's Excel function parity, formatting, CF, and add-in hosting before full workbook UI lands.
  - **Ship embedded Explorer inside DNA Calc first; package standalone as a thin late milestone** [CL-09-01] (P19-NEON; `P19-NEON-0106`): Avoid splitting delivery focus by deferring standalone packaging until after full workbook views ship.
  - **Ship Standalone Formula Explorer early in Milestone 1 as a teaser** [CL-16-03] (P19-CARBON; `P19-CARBON-0123`): Deliver standalone `DNA Calc - Formula Explorer` (with local add-in loading and hot-reload) in M1 alongside the Strand shell and Classic grid.
  - **Defer Standalone Formula Explorer to final Milestone 6** [CL-16-03] (P19-NEON; `P19-NEON-0124`): Ship standalone Formula Explorer last, bundled with Macro View API hooks after all other views are finished.
  - **Additional governing / aligned cards across topics** (FRAMING): `FRAMING-0051`
  - **Additional conflicting / superseded cards across topics** (P19-GPT): `P19-GPT-0101`
- **Decision:** Ship standalone 'DNA Calc - Formula Explorer' (Skin 0, strict engine-free Level-0 host) early in Milestone 1 as a public teaser right after the M0 Skin Architecture foundation.
- **Not taken (Appendix Z):** Deferring standalone Formula Explorer to a final packaging milestone (P19-NEON-0106, P19-NEON-0124) or M2 (P19-GPT-0101) — superseded by Owner Ruling #2; decouple M6 Macro View API hooks from standalone packaging.
- **Reason:** Owner ruling (2026-11-26 #2). Standalone Formula Explorer lets people try DNA Calc's Excel-exact function engine, formatting, CF, and add-in workbench early while mechanically proving the Level-0 host boundary.

### D-03: Ticked models (RTD clock + macro-ticked Excel models) and circular reference != time delay

- **Authority:** DECISIONS_20261126 #3 | **Primary topic:** VII. Model map | **Clusters:** `CL-01-01`, `CL-01-03`, `CL-07-06`, `CL-07-07`, `CL-07-08`, `CL-07-09`, `CL-10-12`, `CL-15-05`, `CL-16-04`
- **Options & sources:**
  - **Option A: Present Model map cycles as z^-1 delay loops** [CL-01-01] (P19-NEON; `P19-NEON-0003`): Describe Model map generally as 'blocks, wires, z^-1 loops', implying Excel circular references act as discrete-time unit delays.
  - **Option B: Reserve delay/tick notation for explicit RTD/macro ticks or verified last-pass iterative reads** [CL-01-01] (FRAMING, P19-GPT; `FRAMING-0001`, `FRAMING-0048`, `P19-GPT-0001`): Keep Excel-exact semantics: an Excel circular reference is not a discrete time delay; Model map shows dependency cycles honestly and reserves tick/delay semantics for explicit RTD/macro clocks or verified iterative last-pass reads.
  - **Option A: Draw a delay mark only where the system reports an iterative last-pass read** [CL-01-03] (P19-FABLE; `P19-FABLE-0028`): Allow a delay/previous-pass indicator strictly when the engine reports an actual iterative last-pass read rather than inferring a delay from any circular reference.
  - **Option B: Treat circular references strictly as dependency cycles; reserve 'delay' for explicit clock/tick models** [CL-01-03] (FRAMING; `FRAMING-0068`): Never label an iterative circular reference a 'delay'; show it as an iterative cycle (with convergence status) and reserve time-stepped 'tick/delay' semantics for explicit RTD clocks or macro-driven ticks.
  - **Offer 'Ticks Director (Max=1)' on loop closure and automatically draw a `[z⁻¹]` delay arc** [CL-07-06] (P19-CARBON, P19-NEON; `P19-CARBON-0037`, `P19-CARBON-0194`, `P19-NEON-0034`, `P19-NEON-0218`): Present a one-click `Ticks (Max=1)` option in the cycle Preview Seam that immediately renders a `[z⁻¹]` delay arc on the circular edge.
  - **Label as iterative circular group by default; claim a delay mark only with confirmed evidence** [CL-07-06] (P19-FABLE, P19-GPT; `P19-FABLE-0077`, `P19-FABLE-0462`, `P19-GPT-0022`): Enable Excel iteration with explicit scope review (`MaxIterations`, `MaxChange`), draw a circular-iteration enclosure, and apply a delay mark only when backed by confirmed clock/prior-period/last-pass evidence.
  - **Infer ticked vs solved cycles directly from file evidence (clock RTD topic, iteration settings, self-ref)** [CL-07-08] (FRAMING; `FRAMING-0141`): Detect `DnaCalc.Clock` RTD topic + `MaxIterations=1` + self-reference pattern from the workbook to classify a cycle as a per-tick delay.
  - **Treat file pattern as 'candidate clocked' only; require confirmed host/provider profile for guarded tick guarantees** [CL-07-08] (P21; `P21-0069`, `P21-0070`): Recognize that Excel's default 2000 ms `RTD.ThrottleInterval`, edit-triggered recalculations, and Application-scoped `MaxIterations` mean file formulas alone cannot guarantee immutable once-only tick delivery; require an explicit host/provider profile (or macro-ticked runner) and disclose limits.
  - **Front-end 'Repeat F9' timer as default tick driver** [CL-07-09] (P19-FABLE, P19-NEON; `P19-FABLE-0080`, `P19-NEON-0116`): Drive Run/Step via front-end F9 recalculation loops by default, keeping RTD clock opt-in.
  - **RTD clock for example ticked models; macro/Calculate ticking for real Excel models** [CL-07-09] (P19-CARBON, P19-FABLE; `P19-CARBON-0017`, `P19-FABLE-0159`): Keep explicit RTD/event markers and honest Excel recalculation semantics without inventing a UI-only timer that doesn't exist when opened in Excel.
  - **Option A: Built-in view simulator controls (Seed / Run 100 / Pause / Reset)** [CL-10-12] (P19-CARBON; `P19-CARBON-0237`): Allow the view to set a simulation seed and drive multi-step Run 100 / Play / Pause loops directly.
  - **Option B: Strictly Excel-native recalculation and ticking triggers** [CL-10-12] (P19-FABLE, P19-GPT, P19-NEON; `P19-FABLE-0586`, `P19-GPT-0344`, `P19-NEON-0279`): Forbid view-side RNGs, synthetic reseeding, or invented execution clocks; 'Draw again' / 'Re-run' triggers real F9 recalculation, an existing {=TABLE} sweep, an RTD clock, or an authorized workbook macro.
  - **RTD clock (or valid macro tick) for ticked models + explicit separation of `INDEX` time-stepped delay from iterative convergence** [CL-15-05] (P19-GPT; `P19-GPT-0084`): Test wiring a precise operand, distinguishing day-2->day-5 `INDEX` delay (`H15->C18`) from same-period circular iteration (`B8`), and driving ticked simulation via the RTD clock (with macro-driven ticking also recognised as valid Excel).
  - **Model delay via circular reference/iteration stepped on F9 or bespoke Ticks-on-iteration** [CL-15-05] (P19-CARBON, P19-FABLE, P19-NEON; `P19-CARBON-0106`, `P19-FABLE-0212`, `P19-NEON-0097`): Use a circular reference to represent a multi-step delay and step it via `F9` / iteration passes.
  - **Additional governing / aligned cards across topics** (FRAMING, P19-GPT, P21): `FRAMING-0168`, `FRAMING-0053`, `P19-GPT-0088`, `P21-0064`
  - **Additional conflicting / superseded cards across topics** (P19-CARBON): `P19-CARBON-0127`
- **Decision:** Strictly separate three distinct structures: (1) explicit period-row lags (e.g. H15->C18), (2) within-calculation iterative circular enclosures (e.g. B8, labelled 'iterative cycle · Max N', never 'time delay'), and (3) verified per-tick state delays (`z^-1`) driven by an explicit clock. Use the RTD clock (`DnaCalc.Clock`) for DNA Calc's example ticked models, while also accepting real Excel models ticked from a macro engine (via value changes, volatile inputs, and Calculate calls). Never invent view-side RNG/reseeding, never auto-tag arbitrary circular loops as `z^-1`, never claim file structure alone proves once-only RTD delivery outside a confirmed host profile, and never claim per-sheet `MaxIterations`.
- **Not taken (Appendix Z):** Auto-labelling circular references as `z^-1` time delays (P19-NEON-0003, P19-FABLE-0028, P19-CARBON-0037, P19-CARBON-0194, P19-CARBON-0127, P19-NEON-0034, P19-NEON-0218); UI-only 'Repeat F9' loops advertised as workbook simulation contracts (P19-FABLE-0080, P19-NEON-0116); view-side RNG/reseeding controls (P19-CARBON-0237); older SB4 scripts treating circular refs as F9 delays (P19-CARBON-0106, P19-FABLE-0212, P19-NEON-0097) — all parked in Appendix Z.
- **Reason:** Owner ruling (2026-11-26 #3) and owner-accepted Pro 19 Push-back #1 (README.md, SKIN_DISCUSSION.md #6). Preserves mathematical honesty and Excel's real Application-scoped iteration and RTD/macro semantics.

### D-04: Undo across a macro run: design and try it, do not promise it in v1

- **Authority:** DECISIONS_20261126 #4 | **Primary topic:** XIV. Automation | **Clusters:** `CL-02-10`, `CL-02-20`, `CL-04-30`, `CL-10-16`, `CL-14-04`, `CL-16-04`
- **Options & sources:**
  - **Option A: Mandate macro runs as single reversible undo steps in v1** [CL-02-10] (P19-FABLE; `P19-FABLE-0132`, `P19-FABLE-0500`): Require every VBA/macro run to be a single reversible undo step from first release.
  - **Option B: Design checkpoints for macro undo, label external macro side effects non-reversible, and do not promise macro undo in v1** [CL-02-10] (P19-GPT; `P19-GPT-0038`): Architect the undo checkpoint journal to capture workbook-state macro runs as single steps over time, honestly flag external/COM/file-system macro side effects as non-undoable, and do not promise macro undo in the first release.
  - **Option A: Require full macro-run checkpoint rollback immediately in v1** [CL-02-20] (P19-CARBON; `P19-CARBON-0223`): Mandate atomic reversible checkpoints for VBA macro runs in the initial checkpoint delivery.
  - **Option B: Support named compound checkpoints with honest non-reversible tagging in v1; architect for macro-run rollback post-v1** [CL-02-20] (P19-GPT; `P19-GPT-0312`): Design the checkpoint schema so macro transactions can plug into it cleanly, surface non-reversible macro side effects honestly, and do not block v1 release on macro-run undo.
  - **Guarantee Undo survives macro runs (Fable D-1)** [CL-04-30] (P19-FABLE; `P19-FABLE-0394`): Promise as a deliberate difference from Excel that per-workbook Undo survives macro execution as well as saves.
  - **Undo survives Save & view switches; design for macro-run Undo without promising it in v1 (GPT + Ruling #4)** [CL-04-30] (P19-GPT; `P19-GPT-0205`, `P19-GPT-0207`): Guarantee per-workbook Undo across Saves and view switches; architect the transaction log to capture in-workbook macro edits eventually, while making zero promise of macro-run Undo in v1 and never claiming external side effects are undoable (see P19-GPT-0208).
  - **Design architecture for macro undo & pre-run checkpoints; do NOT promise universal macro undo in v1** [CL-14-04] (FRAMING, P19-GPT; `FRAMING-0169`, `P19-GPT-0079`, `P19-GPT-0296`): Group capturable document changes where feasible, offer a recoverable pre-run workbook checkpoint before non-undoable runs, distinguish external side effects clearly, and treat full macro undo as a deep post-v1 goal.
  - **Mandate copy-on-write snapshot Ctrl+Z single-step macro undo in v1** [CL-14-04] (P19-CARBON, P19-FABLE; `P19-CARBON-0120`, `P19-CARBON-0206`, `P19-FABLE-0207`, `P19-FABLE-0515`): Require every pure workbook-mutating VBA macro run to push one named Ctrl+Z undo checkpoint in v1.
  - **Additional conflicting / superseded cards across topics** (P19-CARBON): `P19-CARBON-0128`, `P19-CARBON-0241`
- **Decision:** Guarantee per-workbook named Undo across view switches, saves, and interactive/bulk/agent edits in v1. Design explicit engine run boundaries and copy-on-write workbook checkpoints so pure workbook-mutating VBA macro runs can gain single-step Ctrl+Z undo as runtime coverage matures, but DO NOT promise macro-run Undo in release 1. Before any non-undoable macro/provider run, disclose the limit, offer a recoverable pre-run checkpoint, and keep external side effects strictly outside document Undo.
- **Not taken (Appendix Z):** Promising universal single-step Ctrl+Z undo across VBA macro runs as a v1 requirement (P19-FABLE-0132 macro clause, P19-FABLE-0207, P19-FABLE-0394 macro clause, P19-FABLE-0500, P19-FABLE-0515, P19-CARBON-0120, P19-CARBON-0128, P19-CARBON-0206, P19-CARBON-0223, P19-CARBON-0241) — deferred post-v1 per Owner Ruling #4.
- **Reason:** Owner ruling (2026-11-26 #4). Macro-run undo is a deep non-trivial engineering challenge; architecting the checkpoint seam early without over-promising v1 keeps honesty and schedule intact.

### D-05: Notes and findings live in customXml now; Excel-visible mirror is optional

- **Authority:** DECISIONS_20261126 #5 | **Primary topic:** XIII. Persistence | **Clusters:** `CL-05-12`, `CL-13-03`
- **Options & sources:**
  - **Option A: Store notes, findings, and agent/cell change trail in customXml (optional visible-sheet mirror)** [CL-05-12] (P19-FABLE; `P19-FABLE-0114`): Persist audit findings, notes, and the toggleable agent/cell change trail in DNA Calc's customXml part, with an optional (not always-on) visible-sheet mirror and explicit export to Excel comments only on user request.
  - **Option B: Record findings directly as standard Excel cell comments (Ctrl+Shift+F2)** [CL-05-12] (P19-FABLE; `P19-FABLE-0070`): Write every Inspector finding directly into the worksheet as a standard Excel comment.
  - **Store notes and findings in customXml now; optional Excel-visible mirror (not always on)** [CL-13-03] (FRAMING; `FRAMING-0170`): Keep notes and findings in customXml by default without altering worksheet cells; treat an Excel-visible mirror as an optional capability and iterate with experience.
  - **Promote an Excel-visible mirror (cell notes / generated notes sheet) as protection against customXml stripping** [CL-13-03] (P19-FABLE; `P19-FABLE-0219`): Push back against customXml-only findings/prose because Document Inspector can strip customXml; offer cell-note and generated-sheet mirrors and disclose fragility at first save.
- **Decision:** Store Inspector findings, user notes, noted overrides, and Notebook prose in DNA Calc's workbook CustomXML part by default (keeping user worksheets untouched). Disclose clearly on save how Excel sees them, and offer an optional, off-by-default 'Mirror to Excel' action (Excel cell notes for findings, optional generated summary sheet for prose) plus explicit 'Add as Excel comment…' export; expect to improve the storage model with real-world experience.
- **Not taken (Appendix Z):** Storing Inspector findings as native Excel cell comments by default (P19-FABLE-0070) or requiring an always-on Excel-visible mirror sheet (P19-FABLE-0219 default-mirror variant) — superseded by Owner Ruling #5 (kept strictly as opt-in).
- **Reason:** Owner ruling (2026-11-26 #5). Keeps ordinary Excel worksheets clean and unmodified during audits while letting teams opt into Excel-visible mirrors when collaborating with plain-Excel reviewers.

### D-06: Notebook works exactly like Jupyter; no separate read-mode presentation (reader presentation -> App)

- **Authority:** DECISIONS_20261126 #6 | **Primary topic:** VI. Notebook | **Clusters:** `CL-06-01`, `CL-10-17`, `CL-13-06`, `CL-16-04`
- **Options & sources:**
  - **Option A: Single Jupyter-style computation chain; no Read mode (Owner Ruling #6)** [CL-06-01] (FRAMING, P21; `FRAMING-0009`, `FRAMING-0054`, `FRAMING-0056`, `FRAMING-0145`, `FRAMING-0171`, `P21-0014`): Notebook has one unified Jupyter-like surface where written Excel formulas sit visibly above their live results with prose cells between links. Read-only is purely a file-open lock. Reader-style director presentations that hide formulas belong exclusively to App (Easy Forms Maker).
  - **Option B: Dual Author/Read presentation toggle keeping formulas visible (Pro 21 intermediate)** [CL-06-01] (FRAMING, P21; `FRAMING-0153`, `P21-0015`): Maintain separate Author and Read layouts over the same cells, even when both display formulas above outputs.
  - **Option C: Narrative-first Reading memo / formula-hiding 'Reading density' (Pro 19)** [CL-06-01] (P19-CARBON, P19-FABLE, P19-GPT, P21; `P19-CARBON-0032`, `P19-FABLE-0059`, `P19-FABLE-0458`, `P19-GPT-0017`, `P19-GPT-0042`, `P21-0037`): Treat Notebook primarily as an executive reading memo or provide a 'Reading density' that hides/folds formulas into prose in 68-ch serif typography.
  - **Option A: Auto-scaffold Notebook headline narrative and auto-populate App Outcome Cards** [CL-10-17] (P19-CARBON, P19-NEON; `P19-CARBON-0242`, `P19-NEON-0284`): Treat high-fan-in/zero-fan-out sink sheets as High-confidence Reports that automatically scaffold executive narrative in Notebook and Outcome Cards in App.
  - **Option B: 'Suggested' report role, Jupyter-chain Notebook, and author-confirmed App/Reader surface** [CL-10-17] (P19-FABLE, P19-GPT; `P19-FABLE-0591`, `P19-GPT-0349`, `P19-GPT-0053`): Grade report intent as Suggested, require author confirmation for reading order and App input/output contract, and keep reader-style executive presentation in App while Notebook stays a Jupyter-style computation chain.
  - **Additional conflicting / superseded cards across topics** (P19-FABLE): `P19-FABLE-0060`, `P19-FABLE-0240`
- **Decision:** Notebook works strictly like Jupyter: a single live computation chain showing each input formula visibly above its evaluated output, with the next input referencing it and optional Markdown prose/control cells between links (plus horizontal Pipeline arrangement). Read-only is purely a file-open lock (file cannot be edited), NEVER a separate read-mode presentation or formula-hiding 'Reading density'. Reader-style executive write-ups and formula-hidden views belong exclusively to the App (easy forms maker) view.
- **Not taken (Appendix Z):** Separate Author vs Read presentation modes in Notebook (P21-0015, P21-0037, FRAMING-0153); formula-hiding 'Reading density' Notebook mode (P19-FABLE-0059, P19-FABLE-0060, P19-FABLE-0240, P19-FABLE-0458); narrative-first report/memo Notebook modes that hide formulas by default or auto-scaffold executive prose from sink sheets (P19-CARBON-0032, P19-CARBON-0242, P19-GPT-0017, P19-GPT-0042, P19-NEON-0284) — all routed to Appendix Z or moved to App per Owner Ruling #6.
- **Reason:** Owner ruling (2026-11-26 #6) and SKIN_DISCUSSION.md #9–#10. Eliminates the confusing overlap between Notebook and App: Notebook is for computing transparently in a chain; App is for calm forms and executive reading.

### D-07: Stay on WebView2/Leptos baseline; FerroUI not at this stage

- **Authority:** DECISIONS_20261126 #7 | **Primary topic:** XII. Customisation and the skin API | **Clusters:** `CL-12-02`
- **Options & sources:**
  - **Stay on WebView2/Leptos baseline** [CL-12-02] (FRAMING; `FRAMING-0172`): Keep WebView2/Leptos for desktop and web parity, closing `dc-etcxb.3` as not now.
  - **Pursue FerroUI native toolkit (`dc-etcxb.2` prototype)** [CL-12-02] (FRAMING; `FRAMING-0031`): Continue evaluating/migrating to FerroUI 0.5.0 as an alternative GUI stack.
- **Decision:** Keep the WebView2/Leptos desktop and browser baseline for this implementation wave and close dc-etcxb.3 (FerroUI Windows measurements) as not now. All performance budgets and UI architecture plans target WebView2/Leptos (with WebGPU-capable grid canvas where warranted).
- **Not taken (Appendix Z):** Migrating the front end to FerroUI native Rust toolkit now (FerroUI path referenced in FRAMING-0031 and dc-etcxb.3) — parked in Appendix Z per Owner Ruling #7.
- **Reason:** Owner ruling (2026-11-26 #7). Avoids a toolkit rewrite while we reshape the skin architecture and build the fresh UI across desktop and browser from one codebase.

### D-08: Drop legacy website tagline and unify app<->website storytelling

- **Authority:** DECISIONS_20261126 #8 | **Primary topic:** I. Principles | **Clusters:** `CL-01-13`, `CL-03-06`
- **Options & sources:**
  - **New hook + bidirectional app<->website storytelling (Adopted — Owner Ruling #8)** (FRAMING; `FRAMING-0173`, `FRAMING-0165`): Drop 'spreadsheet for your workbooks'; propose a sharper hook in the Design Book; foreground automation, engine exactness, cross-platform reach, and agentic tweaking alongside the UI, with two-way links between app and website.
- **Decision:** Retire the tagline 'spreadsheet for your workbooks'. The Design Book proposes a sharper hook centering Excel-exact calculation + modern multi-view presentation, foregrounds automation, engine use, cross-platform reach, and agentic customisation on the website, and specifies two-way links sharing the same workbook stories between the app and website.
- **Not taken (Appendix Z):** Retaining 'spreadsheet for your workbooks' as the headline hook — retired per Owner Ruling #8.
- **Reason:** Owner ruling (2026-11-26 #8 and Aim). The UI makes the first impression, but DNA Calc's differentiation spans the exact Excel engine, automation, cross-platform support, and agentic customisation.

### D-09: Skin architecture is central: merge independent architecture designs before building UI

- **Authority:** DECISIONS_20261126 #9 | **Primary topic:** XII. Customisation and the skin API | **Clusters:** `CL-12-01`, `CL-16-03`
- **Options & sources:**
  - **M0 Merged Skin Architecture & Strand v0.2 foundation -> M1 Fresh One Shell, Classic & Standalone Explorer** [CL-16-03] (P19-CARBON, P19-NEON; `P19-NEON-0118`, `P19-CARBON-0123`): Complete the merged skin architecture first, then build fresh Strand v0.2 UI surfaces cleanly against Contract v2.
  - **Incremental 5-step refactor from `FRONT_END_SCOPE` (putting Skin SDK before Notebook/Map/Inspector)** [CL-16-03] (FRAMING; `FRAMING-0024`): Refactor existing controllers incrementally before polishing Classic and building the Skin SDK.
  - **Additional governing / aligned cards across topics** (FRAMING): `FRAMING-0174`
- **Decision:** Treat the skin architecture (merged from Fable's and Opus's independent proposals into SKIN_ARCHITECTURE.md) as the central Milestone 0 prerequisite that reshapes the codebase before fresh view implementation begins.
- **Not taken (Appendix Z):** FRONT_END_SCOPE's earlier incremental refactor order that kept legacy skins alive across contract milestones (FRAMING-0024) — superseded by Owner Rulings #9 and #10.
- **Reason:** Owner ruling (2026-11-26 #9). Getting the skin interface and host layering clean first makes building and iterating on all five views plus Formula Explorer fast and coherent.

### D-10: Write new UI fresh against clean skin interface; discard legacy skin code

- **Authority:** DECISIONS_20261126 #10 | **Primary topic:** XII. Customisation and the skin API | **Clusters:** `CL-12-02`
- **Options & sources:**
  - **Fresh UI rewrite discarding superseded skin code** [CL-12-02] (FRAMING; `FRAMING-0175`): Land the merged skin architecture first, then build the new UI cleanly against the skin interface and throw away large chunks of legacy experimental skin code.
  - **Micro-step migration keeping all six legacy skins working at every commit** [CL-12-02] (FRAMING; `FRAMING-0028`): Preserve all six experimental skins continuously across incremental refactoring commits.
- **Decision:** After the Design Book and merged skin architecture land, create implementation beads and write the new UI fresh against the clean skin interface, discarding large chunks of superseded legacy skin code. Publish early for feedback and iterate.
- **Not taken (Appendix Z):** Micro-step migration keeping all six legacy skins running during incremental refactors (FRAMING-0028's migration clause) — superseded by Owner Ruling #10 (keeping only layering checks and clean host contracts).
- **Reason:** Owner ruling (2026-11-26 #10). Writing fresh UI over a well-shaped skin interface is cleaner and faster than nursing legacy skin code through dozens of transitional adapters.

### D-11: Classic uses Excel's exact key-tip letters and keyboard bindings

- **Authority:** DECISIONS_20261126 #11 | **Primary topic:** IV. Classic | **Clusters:** `CL-04-04`
- **Options & sources:**
  - **Remapped mnemonic initials I=Insert, V=View, L=Lens (Carbon)** [CL-04-04] (P19-CARBON; `P19-CARBON-0130`): Reassign top-level key tips to I (Insert), V (View), and L (Lens) to match English initials.
  - **Exact Excel key-tip letters F/H/N/P/M/A/R/W/L/Q (Fable, GPT, Neon, Ruling #11)** [CL-04-04] (FRAMING, P19-FABLE, P19-GPT, P19-NEON; `FRAMING-0176`, `P19-FABLE-0044`, `P19-FABLE-0236`, `P19-FABLE-0384`, `P19-GPT-0109`, `P19-NEON-0126`): Use Excel's exact top-level letters (F File, H Home, N Insert, P Page Layout, M Formulas, A Data, R Review, W View, L Developer, Q Search/Palette) stored in a layout-independent key-tip table.
  - **Additional governing / aligned cards across topics** (P19-FABLE): `P19-FABLE-0435`
- **Decision:** Classic uses Microsoft Excel's exact Alt key-tip letters (F File, H Home, N Insert, P Page Layout, M Formulas, A Data, R Review, W View, L Developer, Q Search) stored in a layout-independent key-tip path table so visual Deck changes never break muscle memory. Bit-matching Excel applies to Classic's keyboard bindings as well as calculation.
- **Not taken (Appendix Z):** Remapping Excel key-tip letters to first-letter mnemonics such as I=Insert, V=View, L=Layout (P19-CARBON-0130) — rejected per Decider Ruling #11.
- **Reason:** Coordinator ruling (2026-11-26 #11). Key-tip chords like Alt,H,V,V and Alt,A,T are deeply ingrained analyst muscle memory; breaking them fails the Excel-compatibility promise.

### D-12: View-switching keys: Ctrl+Alt+1..4; Ctrl+Alt+5 preserved for Excel floating shapes -> App via Alt+Q / Alt+Shift+5

- **Authority:** DECISIONS_20261126 #12 | **Primary topic:** III. Shell | **Clusters:** `CL-02-06`, `CL-03-04`, `CL-04-01`, `CL-04-05`, `CL-04-28`, `CL-05-01`, `CL-06-02`, `CL-07-09`, `CL-07-10`, `CL-08-01`, `CL-11-06`, `CL-12-07`
- **Options & sources:**
  - **Option A: `Ctrl+1..5`** [CL-02-06] (P19-CARBON; `P19-CARBON-0049`): Bind view switching to `Ctrl+1` through `Ctrl+5`.
  - **Option B: `Alt+Shift+1..5` and `Alt+Shift+0`** [CL-02-06] (P19-FABLE; `P19-FABLE-0428`, `P19-FABLE-0430`): Bind view switching to `Alt+Shift+1..5` and companion Classic grid to `Alt+Shift+0`.
  - **Option C: `Ctrl+Alt+1..5` (and `Ctrl+Alt+0` companion grid) per DECISIONS_20261126 #12** [CL-02-06] (P19-NEON; `P19-NEON-0196`): Bind view switching to `Ctrl+Alt+1..5` (checking for Excel collisions) with `Alt+Q` command search and the Mast view switcher as fallbacks.
  - **Ctrl+Alt+1..5 primary with Alt+Shift+1..5 AltGr-safe alias and Command Search fallback** [CL-03-04] (FRAMING, P19-NEON; `FRAMING-0177`, `P19-NEON-0113`): Use Ctrl+Alt+1..5 for the five views (confirmed zero collisions in Excel), keep Alt+Shift+1..5 as an AltGr-safe alias for European keyboards, and fall back to Command Search.
  - **Alt+Shift+1..5 exclusively for views** [CL-03-04] (P19-FABLE; `P19-FABLE-0026`, `P19-FABLE-0420`): Place all ten DNA Calc shortcuts, including views 1–5, exclusively on the Alt+Shift layer.
  - **Ctrl+1..Ctrl+5 for views** [CL-03-04] (P19-CARBON; `P19-CARBON-0054`): Use Ctrl+1..Ctrl+5 to switch views.
  - **Ctrl+1..5 (Carbon)** [CL-04-01] (P19-CARBON; `P19-CARBON-0055`): Bind Ctrl+1..5 to switch views, displacing Excel's Ctrl+1 (Format Cells) and Ctrl+5 (Strikethrough).
  - **Alt+Shift+1..5 (Fable)** [CL-04-01] (P19-FABLE; `P19-FABLE-0151`): Place all view switches on DNA Calc's Alt+Shift+1..5 layer so zero Ctrl-digit Excel bindings are touched.
  - **Ctrl+Alt+1..5 with Excel collision check (Neon + Ruling #12)** [CL-04-01] (P19-NEON; `P19-NEON-0010`, `P19-NEON-0048`): Use Ctrl+Alt+1..5 unless Excel binds the combination; command search (Alt+Q) is the fallback.
  - **Move Format Cells to Ctrl+Shift+F with dismissible hint when pressing Ctrl+1 (Carbon)** [CL-04-05] (P19-CARBON; `P19-CARBON-0159`): Keep Ctrl+1..5 as view switchers and show a one-time hint offering a keymap swap when users press Ctrl+1 inside Classic.
  - **Keep Ctrl+1 strictly as Format Cells and Ctrl+5 as Strikethrough (Fable, GPT, Neon)** [CL-04-05] (P19-FABLE; `P19-FABLE-0388`): Preserve Ctrl+1 for Format Cells and Ctrl+5 for Strikethrough unconditionally; view switching lives on Ctrl+Alt+1..4 (Ruling #12).
  - **Option A: Ctrl+Alt+2 (with command search fallback)** [CL-05-01] (P19-NEON; `P19-NEON-0050`): Use Ctrl+Alt+1..5 across all views (Ctrl+Alt+2 for Inspector) unless Excel already binds the key combination.
  - **Option B: Ctrl+2 or Alt+Shift+2** [CL-05-01] (P19-CARBON, P19-FABLE; `P19-CARBON-0057`, `P19-FABLE-0068`, `P19-FABLE-0152`): Use Ctrl+2 (which collides with Excel Bold!) or Alt+Shift+2.
  - **Option A: Ctrl+Alt+3 view switch; '/' block menu and '=' live value chip in prose** [CL-06-02] (P19-NEON; `P19-NEON-0028`, `P19-NEON-0052`): Switch to Notebook with Ctrl+Alt+3, open block insertion with '/', and insert live cell quotes in prose with '='.
  - **Option B: Ctrl+3 or Alt+Shift+3 view switch; '@' live chip trigger** [CL-06-02] (P19-CARBON, P19-FABLE; `P19-CARBON-0059`, `P19-FABLE-0153`): Use Ctrl+3 (collides with Excel Italic!) or Alt+Shift+3, and '@' for live chips.
  - **`Ctrl+5` for App** [CL-08-01] (P19-CARBON; `P19-CARBON-0063`): Use `Ctrl+1..5` to switch views.
  - **`Alt+Shift+5` for App** [CL-08-01] (P19-FABLE; `P19-FABLE-0155`): Use `Alt+Shift+1..5` to avoid colliding with Excel's `Ctrl+5` strikethrough binding.
  - **Additional governing / aligned cards across topics** (P19-GPT): `P19-GPT-0218`
  - **Additional conflicting / superseded cards across topics** (P19-CARBON, P19-FABLE): `P19-CARBON-0061`, `P19-CARBON-0084`, `P19-FABLE-0154`, `P19-FABLE-0510`
- **Decision:** Per Coordinator Ruling #12 and completed Microsoft 365 Excel collision audit: bind Ctrl+Alt+1 (Classic), Ctrl+Alt+2 (Inspector), Ctrl+Alt+3 (Notebook), and Ctrl+Alt+4 (Model map) as primary view-switching shortcuts (all 4 verified free in desktop Excel), and Ctrl+Alt+0 for the companion Classic grid. Because Microsoft 365 Excel binds Ctrl+Alt+5 to floating-object navigation (A-111, P19-GPT-0218), preserve Ctrl+Alt+5 for floating shapes in Classic and reach View 5 (App) via Command Search (Alt+Q -> App), the Mast view switcher, or the uniform AltGr-safe secondary layer Alt+Shift+1..5 (with Alt+Shift+0 companion grid). Never bind Ctrl+1..5 (Excel Format Cells, Bold, Italic, Underline, Strikethrough).
- **Not taken (Appendix Z):** Hijacking Ctrl+1..5 for views with Ctrl+Shift+F Format Cells workaround (P19-CARBON-0049, P19-CARBON-0054, P19-CARBON-0055, P19-CARBON-0057, P19-CARBON-0059, P19-CARBON-0061, P19-CARBON-0063, P19-CARBON-0084, P19-CARBON-0159); overwriting Excel's Ctrl+Alt+5 floating-object shortcut (P19-NEON-0113, P19-NEON-0196 unconditional Ctrl+Alt+5); making Alt+Shift+1..5 the sole primary view layer instead of secondary alias (P19-FABLE-0026, P19-FABLE-0151..0155, P19-FABLE-0420, P19-FABLE-0428, P19-FABLE-0430, P19-FABLE-0510).
- **Reason:** Coordinator ruling (2026-11-26 #12) explicitly required checking Ctrl+Alt+1..5 for Excel collisions and falling back to Command Search when Excel binds a combination. P19-GPT-0218 confirmed Ctrl+Alt+5 is bound in Excel; keeping Alt+Shift+1..5 as an AltGr-safe alias also solves European keyboards where Ctrl+Alt emits symbols.

### D-13: Never add hidden defined names as anchors; store all anchors in customXml

- **Authority:** DECISIONS_20261126 #13 | **Primary topic:** XIII. Persistence | **Clusters:** `CL-13-01`, `CL-13-02`
- **Options & sources:**
  - **CustomXML-only anchors referencing user's existing visible names/tables + structural fingerprints (optional user-initiated 'Pin with a name')** [CL-13-02] (FRAMING, P19-FABLE; `FRAMING-0178`, `P19-FABLE-0232`): Never add hidden defined names or synthetic _dna.* names; store anchors solely in customXml, optionally letting the user explicitly create a normal visible Excel Defined Name via 'Pin with a name'.
  - **Dual anchors with opt-in synthetic _dna.* Defined Names and fixed xl/customXml/dnaCalcViews.xml part** [CL-13-02] (P19-CARBON; `P19-CARBON-0119`, `P19-CARBON-0203`): Default to structural fingerprints in xl/customXml/dnaCalcViews.xml, with opt-in synthetic _dna.kpi_* Defined Names.
  - **Additional conflicting / superseded cards across topics** (FRAMING): `FRAMING-0013`, `FRAMING-0037`
- **Decision:** Never create hidden Excel defined names or synthetic `_dna.*` defined names as view anchors. Store every view anchor exclusively inside DNA Calc's standard CustomXML part (`urn:dnacalc:views:v1`), keyed to `(sheetId, sheetName, user's own visible Defined Name or Table if present, A1 range, header/label fingerprint, R1C1 formula hash)`. Offer an explicit user-initiated 'Pin with a name' helper that creates an ordinary visible user-named Excel Defined Name when the author wants Excel-native reference tracking.
- **Not taken (Appendix Z):** Hidden defined-name anchors (`_dna.*` / hidden Name Manager entries) proposed in FRONT_END_SCOPE (FRAMING-0013), FRONT_END_IMPLEMENTATION_BRIEF (FRAMING-0037), and Carbon's optional auto-hidden-name fallback (P19-CARBON-0119, P19-CARBON-0203) — rejected per Decider Ruling #13.
- **Reason:** Coordinator ruling (2026-11-26 #13, adopting Fable's argument). Hidden names pollute Excel's Name Manager, confuse workbook auditors, and break the rule of leaving Excel workbook structures untouched unless the user explicitly edits them.

### D-14: Persist compact structured agent change trail in workbook customXml (ON by default, toggleable)

- **Authority:** DECISIONS_20261126 #14 | **Primary topic:** XIII. Persistence | **Clusters:** `CL-02-17`, `CL-13-03`, `CL-15-05`, `CL-16-04`
- **Options & sources:**
  - **Option A: Keep agent audit trail session-only or strictly opt-in export** [CL-02-17] (P19-CARBON; `P19-CARBON-0045`): Treat customXml session provenance as optional/ephemeral rather than stored in the workbook by default.
  - **Option B: Persist the agent's change trail in the workbook's customXml part alongside notes & findings, on by default with an opt-out toggle (DECISIONS_20261126 #14)** [CL-02-17] (P19-FABLE, P21; `P19-FABLE-0497`, `P21-0113`): Store the agent change trail in the same Excel-preserved CustomXML part as notes and findings, enabled by default with a clear setting to turn it off, plus copy/export support.
  - **Persist structured agent change trail in customXml by default, with a clear opt-out toggle** [CL-13-03] (FRAMING, P19-GPT; `FRAMING-0179`, `P19-GPT-0098`): Keep accepted-change provenance inside the workbook's customXml part alongside notes and findings by default, allowing users to turn it off or strip it for privacy.
  - **Keep agent change trail session-only (OFF by default), opt-in per workbook** [CL-13-03] (P19-FABLE; `P19-FABLE-0141`, `P19-FABLE-0233`): Do not persist agent proposals in the file unless the workbook owner explicitly opts in.
  - **Additional conflicting / superseded cards across topics** (P19-NEON): `P19-NEON-0100`, `P19-NEON-0123`
- **Decision:** Persist the compact structured agent change trail (actor, timestamp, proposal summary, affected cells/before-after—never raw chat transcripts) inside the same workbook CustomXML part as notes and findings. Enable it by default with an explicit per-workbook opt-out toggle and 'Clear/Strip agent trail' command, refining the schema with field experience.
- **Not taken (Appendix Z):** Session-only agent audit logs requiring manual Markdown/JSON export (P19-CARBON-0045, P19-NEON-0100, P19-NEON-0123) and OFF-by-default opt-in file logging (P19-FABLE-0141, P19-FABLE-0233) — superseded by Decider Ruling #14.
- **Reason:** Coordinator ruling (2026-11-26 #14). Reviewers opening an agent-edited workbook need an auditable provenance trail inside the file by default, with a clear privacy/size opt-out.

## Part 2: Decider Rulings on Remaining Conflicts (`D-15`–`D-93`)

| ID | Topic | Conflict & Decision | Clusters |
|---|---|---|---|
| [D-15](#d-15) | I. Principles | Per-view quality priority matrix under an inviolable Honesty constraint | CL-01-04 |
| [D-16](#d-16) | I. Principles | Strand v0.2 unified paper ground, petrol chrome, and contrast-hardened semantic inks | CL-01-05, CL-12-03 |
| [D-17](#d-17) | I. Principles | Unified collision-free non-color signal vocabulary and dual-stroke focus ring | CL-01-06, CL-04-06, CL-05-06 |
| [D-18](#d-18) | I. Principles | Quiet steady-state Strip surface with explicit orthogonal abnormal-state readouts | CL-01-08, CL-02-12 |
| [D-19](#d-19) | I. Principles | View density and typography roles across Working, Notebook prose, and App surfaces | CL-01-09 |
| [D-20](#d-20) | I. Principles | Motion budget: 400 ms dependency-propagation ripple and static dashed copy outline default | CL-01-03, CL-01-10, CL-04-20 |
| [D-21](#d-21) | I. Principles | WCAG 2.2 AA accessibility baseline and phased touch/pen ergonomics | CL-01-11, CL-01-12 |
| [D-22](#d-22) | II. Conceptual model | Unified canonical address grammar for Excel targets, inferred blocks, objects, and view items | CL-02-03 |
| [D-23](#d-23) | II. Conceptual model | Separate Read-Only Formula Lens from Scoped-Name Interpreter; no automatic LET refactoring | CL-02-04, CL-05-06, CL-06-09, CL-09-11 |
| [D-24](#d-24) | II. Conceptual model | Cross-view selection continuity without silent substitution and minimal-scroll camera stability | CL-02-05 |
| [D-25](#d-25) | II. Conceptual model | Preview Seam trigger scope: zero-ceremony Classic scalar entry vs mandatory seam for structural and non-grid edits | CL-02-07, CL-04-08 |
| [D-26](#d-26) | II. Conceptual model | Safe Delete on Model map and exception-preserving formula-run edits | CL-02-09 |
| [D-27](#d-27) | II. Conceptual model | Hybrid Undo: one shared Model timeline + scoped per-view Presentation timelines unified in History | CL-02-10 |
| [D-28](#d-28) | II. Conceptual model | Cross-view semantic clipboard matrix preventing silent model duplication or external links | CL-02-11, CL-06-12 |
| [D-29](#d-29) | II. Conceptual model | Uncommitted agent proposal visual grammar (dotted teal-600 + 8% hatch, never amber or dashed) | CL-02-14 |
| [D-30](#d-30) | II. Conceptual model | Agent autonomy modes and Excel-safe invocation shortcuts | CL-02-14, CL-02-15 |
| [D-31](#d-31) | II. Conceptual model | Agent review, subset evaluation, Undo audit trail, and checked out-of-order reversal | CL-02-15, CL-02-17 |
| [D-32](#d-32) | III. Shell | Compact single-row Strand Command Deck with Excel tabs and exact Alt key tips | CL-03-02, CL-04-04 |
| [D-33](#d-33) | III. Shell | Dedicated Stage-bottom sheet-tab bar visible across workbook views | CL-03-02 |
| [D-34](#d-34) | III. Shell | Unified Excel-safe Command Palette (Alt+Q) and Alt+Shift+<key> DNA Calc tool shortcut layer | CL-03-02, CL-03-04, CL-04-05, CL-04-27 (+3) |
| [D-35](#d-35) | III. Shell | Save format downgrades: refuse silent loss on Save; require explicit confirmation on Export | CL-03-07 |
| [D-36](#d-36) | III. Shell | Single non-blocking startup Trust & Notice bar (never stealing Alt+Q or shrinking grid) | CL-03-08 |
| [D-37](#d-37) | III. Shell | Frozen calculation snapshot for Print Preview with explicit Refresh and blocked-calc honesty | CL-03-10 |
| [D-38](#d-38) | IV. Classic | Pinned Microsoft 365 Windows desktop Excel oracle with T1/T2 parity tiering | CL-04-02 |
| [D-39](#d-39) | IV. Classic | Browser-tab keyboard fallbacks documented in live Shortcut Differences table | CL-04-03 |
| [D-40](#d-40) | IV. Classic | Classic Provenance Overlay strictly OFF by default (author owns the paper) | CL-04-06 |
| [D-41](#d-41) | IV. Classic | Desktop Excel parity on Ctrl+F2 (Print Preview) and double-click with in-cell editing off | CL-04-07, CL-04-31 |
| [D-42](#d-42) | IV. Classic | Editing-only 5-step cool Formula Reference Palette reserving amber strictly for active caret | CL-04-09 |
| [D-43](#d-43) | IV. Classic | F9 in-formula sub-expression evaluation with non-blocking warning and Ctrl+Z un-bake | CL-04-12 |
| [D-44](#d-44) | IV. Classic | Classic live tick and Form Control ScrollBar bounded recalculation | CL-04-12 |
| [D-45](#d-45) | IV. Classic | Non-modal inline #SPILL! card with explicit undoable remediation actions | CL-04-14 |
| [D-46](#d-46) | IV. Classic | Non-modal Find & Replace docked in top Bridge with full Excel Options drawer | CL-04-17 |
| [D-47](#d-47) | IV. Classic | Excel-observed double-click Fill Handle rules and border-drag overwrite preference | CL-04-18, CL-04-19 |
| [D-48](#d-48) | IV. Classic | Classic 10%-400% worksheet zoom range with Semantic Zoom block labels below 40% | CL-04-23, CL-11-07 |
| [D-49](#d-49) | IV. Classic | Form Control non-operating selection supports Ctrl+click, right-click, and Selection panel | CL-04-28 |
| [D-50](#d-50) | V. Inspector | Dockable Inspector instrument panels enlarging to a 5-tab full-stage Inspector | CL-05-01 |
| [D-51](#d-51) | V. Inspector | Count-first Inspector Overview triage with on-demand sheet-tier Dependency Atlas | CL-05-02 |
| [D-52](#d-52) | V. Inspector | Inspector repair preview defaults to single cell with opt-in sibling-run batch toggle | CL-05-07 |
| [D-53](#d-53) | V. Inspector | Three-layer Calculation tab honesty: Dependency waves, Value diff, and optional DNA Calc's calculation order | CL-05-08 |
| [D-54](#d-54) | V. Inspector | Inspector print defaults to active instrument with one-click Full Workbook Audit Dossier | CL-05-12 |
| [D-55](#d-55) | VI. Notebook | Non-mutating Notebook First-Run Card with explicit scope selection | CL-06-02 |
| [D-56](#d-56) | VI. Notebook | Single-anchor dynamic-array cells as Notebook authoring default; atomic run cards for imported Excel runs | CL-06-04 |
| [D-57](#d-57) | VI. Notebook | Notebook Data Frame sort/filter defaults to View-Only with explicit reviewed Table-mutation action | CL-06-06 |
| [D-58](#d-58) | VI. Notebook | Standardise Notebook flow arrangement names on Vertical and Pipeline (Alt+P) | CL-06-10 |
| [D-59](#d-59) | VII. Model map | Map-created boxes default to workbook-scoped dotted visible defined names backed by sheet cells | CL-07-02 |
| [D-60](#d-60) | VII. Model map | Typeable, speakable self-reference qualifiers in Presented view (no Unicode superscripts in formula text) | CL-07-03 |
| [D-61](#d-61) | VII. Model map | Model map canvas wheel scrolls/pans by default; Ctrl+Wheel zooms | CL-07-04 |
| [D-62](#d-62) | VII. Model map | Three-stage Model map authoring rollout and zero-silent-zero wire deletion | CL-07-05 |
| [D-63](#d-63) | VII. Model map | Plain Excel-terms calculation chips (retire 'Director' jargon) and unified Inspector Live plotter | CL-07-09 |
| [D-64](#d-64) | VII. Model map | Model map L2 R1C1 run folding default with opt-in bounded geometry-preserving sheet excerpt | CL-07-10 |
| [D-65](#d-65) | VIII. App | App opening requires authored customXml; explicit switch offers badged certain-signal draft or clean decline | CL-08-02, CL-11-11 |
| [D-66](#d-66) | VIII. App | App inputs recalculate live by default; batched 'Apply inputs' is an explicit author option | CL-08-03 |
| [D-67](#d-67) | VIII. App | App output tiles stay read-only with opt-in inline Goal Seek over unlocked inputs | CL-08-05 |
| [D-68](#d-68) | VIII. App | Native Excel Scenario Manager for saved scenarios; transient Pin Baseline for ad-hoc deltas; no Goal Seek guessing | CL-08-06, CL-10-08 |
| [D-69](#d-69) | VIII. App | App windowed schedules with opt-in badged annual rollup and 1-page default Executive Summary print | CL-08-07, CL-08-08 |
| [D-70](#d-70) | IX. Formula Explorer | Standalone Skin-0 Level-0 refusal of caller/workbook functions plus badged Local Test Inputs shelf | CL-07-01, CL-09-03 |
| [D-71](#d-71) | IX. Formula Explorer | Docked Formula Explorer keeps trial evaluation strictly Level-0; host shell renders optional downstream impact | CL-09-05 |
| [D-72](#d-72) | IX. Formula Explorer | Honest native-code trust disclosure for unsigned local .xll add-ins (never call it a 'Sandbox') | CL-09-10 |
| [D-73](#d-73) | IX. Formula Explorer | Standalone Formula Explorer persistence: local restore, .xlsx graduation export, and .dnafe.json developer test suites | CL-09-12 |
| [D-74](#d-74) | X. Patterns | Qualitative pattern provenance and four lifecycle states (no fake numeric confidence percentages) | CL-10-01 |
| [D-75](#d-75) | X. Patterns | Pattern confirmation and correction write strictly to customXml; workbook edits stay below a divider | CL-10-02 |
| [D-76](#d-76) | X. Patterns | Canonical 20-pattern / 5-family spreadsheet and systems-model catalogue | CL-10-03, CL-10-06, CL-10-10 |
| [D-77](#d-77) | X. Patterns | Pattern recognition guardrails for databases, inputs, and time-series captions | CL-10-04, CL-10-05, CL-10-07 |
| [D-78](#d-78) | X. Patterns | Honesty rules for systems roles, live-feed pause labels, external links, and check/override exceptions | CL-10-11, CL-10-15, CL-10-18 |
| [D-79](#d-79) | XI. Big workbooks | Unified WebView2/Leptos performance budgets (Target + Hard Ceiling) and progress cadence | CL-11-05, CL-11-06 |
| [D-80](#d-80) | XI. Big workbooks | Unified per-view scale simplification and decline caps across Inspector, Notebook, Map, and Explorer | CL-11-08, CL-11-09, CL-11-10, CL-11-11 |
| [D-81](#d-81) | XII. Customisation and the skin API | Level-0 formula host in its own engine-free crate beside/below calc-host | CL-09-01, CL-12-01 |
| [D-82](#d-82) | XII. Customisation and the skin API | Contrast-checked non-semantic token overrides; violations block Apply (no silent rewriting) | CL-12-04, CL-12-09 |
| [D-83](#d-83) | XIII. Persistence | Canonical JSON (dna.view/1) for skin files/Studio and standard OOXML /customXml/itemN.xml (urn:dnacalc:views:v1) inside .xlsx | CL-12-05, CL-13-01 |
| [D-84](#d-84) | XII. Customisation and the skin API | Named built-in UI command sequences permitted in declarative skins on explicit user invocation only | CL-12-05, CL-12-06 |
| [D-85](#d-85) | XII. Customisation and the skin API | Canonical Customisation worked examples (strict presentation-only Classic restyle + canonical JSON Loan App) | CL-12-11, CL-12-12 |
| [D-86](#d-86) | XIII. Persistence | Per-sheet preferred view stays a tab badge and Bridge pill (never unexpected switch on tab click) | CL-03-09, CL-13-04 |
| [D-87](#d-87) | XIII. Persistence | Missing or stripped customXml falls back to Classic with on-demand auto-inference in other views | CL-13-06 |
| [D-88](#d-88) | XIV. Automation | Three-tier macro security (Off/Guarded/Unguarded), honest static preflight, and observed-only macro effects | CL-01-02, CL-10-16, CL-14-01 |
| [D-89](#d-89) | XIV. Automation | Minimal 3-verb late-bound Views API for VBA (COM + Application.Run over one dispatcher) | CL-14-03 |
| [D-90](#d-90) | XV. Testing the experience | Two-phase usability and automated scripted-key evaluation protocol | CL-15-03 |
| [D-91](#d-91) | Appendices | Canonical cast workbook fixture contracts (GPT/Pro 21 verified figures) across SB1-SB7 | CL-15-04, CL-15-05, CL-16-05 |
| [D-92](#d-92) | XV. Testing the experience | Two-tier golden visual regression matrix (25 workbookxview Tier 1 + lifecycle Tier 2) | CL-15-08 |
| [D-93](#d-93) | Appendices | Unified M0-M6 front-end implementation sequence | CL-16-04 |

### Section: I. Principles

### D-15: Per-view quality priority matrix under an inviolable Honesty constraint

- **Authority:** Decider ruling | **Primary topic:** I. Principles | **Clusters:** `CL-01-04`
- **Options & sources:**
  - **Option A: Global fixed ranking — Honesty > Familiarity > Clarity > Density** [CL-01-04] (P19-CARBON; `P19-CARBON-0006`): Always let familiarity of hands beat clarity of structure across every view.
  - **Option B: Global fixed ranking — Honesty > Clarity > Familiarity (Classic) / Density** [CL-01-04] (P19-NEON; `P19-NEON-0007`): Put clarity above familiarity globally while qualifying familiarity as Classic-specific.
  - **Option C: Honesty as hard constraint + explicit per-view priority matrix** [CL-01-04] (P19-FABLE; `P19-FABLE-0019`): Treat Honesty as an inviolable constraint (not a weighted axis) and rank the remaining qualities per view: Classic (familiarity > density > clarity); Inspector & Model map (clarity > density > familiarity); Notebook (clarity > familiarity > density); App (clarity > task familiarity > density).
- **Decision:** Adopt Fable's per-view quality priority matrix operating under an inviolable Honesty constraint: Classic ranks Familiarity > Density > Clarity; Inspector and Model map rank Clarity > Density > Familiarity; Notebook ranks Clarity > Comfort > Density; App ranks Clarity > Comfort > Familiarity; Formula Explorer ranks Clarity > Speed > Density.
- **Not taken (Appendix Z):** A single global quality ranking forced uniformly across all five views (P19-CARBON-0006, P19-NEON-0007) — parked in Appendix Z.
- **Reason:** Different personas open different views for opposite trade-offs (dense Excel muscle memory in Classic vs spacious calm controls in App). A single global ranking forces incoherent compromises.

### D-16: Strand v0.2 unified paper ground, petrol chrome, and contrast-hardened semantic inks

- **Authority:** Decider ruling | **Primary topic:** I. Principles | **Clusters:** `CL-01-05`, `CL-12-03`
- **Options & sources:**
  - **Option A: Use mist (#EDF1F2) as both light paper base and surround, and retain legacy Rich-Tree token labels** [CL-01-05] (FRAMING; `FRAMING-0092`, `FRAMING-0093`): Keep STRAND.md v0.1 literally: mist (#EDF1F2) as the light paper surface and keep 'Rich-Tree identity' notes on sage/forest.
  - **Option B: Bright working paper (#FFFFFF) inside mist surround (#EDF1F2), and retire Rich-Tree identity labels** [CL-01-05] (P21; `P21-0001`): Place active working surfaces (grid cells, Notebook paper, cards) on bright paper (#FFFFFF) framed by mist (#EDF1F2) surround, and drop obsolete Rich-Tree identity labels since DNA Calc is Excel-only.
  - **Strand v0.2 unified token set (petrol-chrome inks + darkened light-paper inks + WCAG AA `--amber-ink`)** [CL-12-03] (P19-CARBON, P19-FABLE, P19-NEON; `P19-FABLE-0011`, `P19-NEON-0001`, `P19-CARBON-0001`): Extend `dna-strand` with petrol-800 chrome ink tokens (`--c-ink`..`--c-const`), darkened light-paper inks (`#1B5F6B`, `#2E6E5B`, `#A35400`, `#C12923`), dedicated `--amber-ink` (`#8A5A00` light / `#FFD04D` dark) while keeping `#FFB301` strictly for non-text dual-stroke focus, plus 45° Preview Seam hatch and 6-tone Formula Reference Palette.
  - **Strand v0.1 baseline or raw `#FFB301` amber as text ink on light paper** [CL-12-03] (FRAMING, P21; `FRAMING-0102`, `P21-0002`): Keep v0.1 without petrol-chrome ink tokens, or use `#FFB301` directly for amber text on light paper.
- **Decision:** Standardise on Strand v0.2 in `dna-strand`: crisp `#FFFFFF` working paper framed by `#EDF1F2` mist surround (retiring legacy Rich-Tree naming), `#193F49` petrol chrome with Fable's petrol-800 chrome inks, charcoal `#353938` body ink, P21's darkened light-paper semantic inks for teal/green/signal/red, and Neon's WCAG AA `--dna-amber-ink` (`#8A5A00` light / `#FFD04D` dark) for amber text and badges (reserving `#FFB301` strictly for non-text dual-stroke focus).
- **Not taken (Appendix Z):** STRAND v0.1 `#EDF1F2` paper ground and legacy Rich-Tree identity (FRAMING-0092, FRAMING-0093); raw `#FFB301` amber text on light paper (FRAMING-0102, ~1.84:1 contrast failure) — retired to Appendix Z.
- **Reason:** Maximises tabular legibility on white working paper while mechanically passing WCAG 2.2 AA contrast on every light and dark ink token.

### D-17: Unified collision-free non-color signal vocabulary and dual-stroke focus ring

- **Authority:** Decider ruling | **Primary topic:** I. Principles | **Clusters:** `CL-01-06`, `CL-04-06`, `CL-05-06`
- **Options & sources:**
  - **Option A: Raw amber outline (#FFB301) alone** [CL-01-06] (FRAMING, P19-FABLE; `FRAMING-0094`, `P19-FABLE-0002`): Use a single `#FFB301` outline around focused/selected cells and items.
  - **Option B: 2px amber outer + 1px petrol inner keyline plus `--dna-amber-ink`** [CL-01-06] (P19-NEON; `P19-NEON-0015`): Place amber on the outside and petrol `#193F49` on the inside, and introduce `--dna-amber-ink` (#8A5A00 light / #FFD04D dark) for amber text/badges.
  - **Option C: Dark outer focus stroke + amber inner accent (Pro 20 adopted) plus `--dna-amber-ink` for text/badges** [CL-01-06] (FRAMING, P19-GPT; `FRAMING-0133`, `P19-GPT-0008`): Place a high-contrast dark stroke on the outer boundary against white paper with the amber accent inside so the outer edge passes WCAG 2.2 AA (>=3:1 against paper), combined with `--dna-amber-ink` for small amber text/badges.
  - **Option A: Carbon glyph table** [CL-01-06] (P19-CARBON; `P19-CARBON-0174`): Assign `[!]` to errors, `◈` to external/volatile, `[↻]` to recalc, double-frame to selection.
  - **Option B: Neon glyph table** [CL-01-06] (P19-NEON; `P19-NEON-0193`): Assign `⊗`/`⊘` to root/propagated errors, `▸` to external/volatile, `⇄` to agent proposals, `↻` to fresh/recalc.
  - **Option C: Fable paper-safe cell marks + unified non-colliding glyph table** [CL-01-06] (P19-FABLE; `P19-FABLE-0006`, `P19-FABLE-0007`, `P19-FABLE-0008`, `P19-FABLE-0009`, `P19-FABLE-0012`, `P19-FABLE-0036`): Mark cell errors with a red left-edge tick (leaving workbook formatting intact), fresh cells with a fading green underline, external/volatile with signal-orange `◊`, warnings with bold no-hue hollow triangle `△`/`[!]`, and saved/stale with diagonal hatch.
  - **Option A: Green means 'fresh/verified values'** [CL-01-06] (FRAMING; `FRAMING-0094`): Keep STRAND.md v0.1 wording describing `green-500` as 'fresh/verified values'.
  - **Option B: Green strictly means 'freshly calculated in the current revision'** [CL-01-06] (P19-FABLE, P19-GPT; `P19-GPT-0008`, `P19-FABLE-0008`): Define green exclusively as 'calculated for this revision' (transient highlight on values that updated in the latest recalc), never implying human audit or correctness.
  - **Red left-edge tick for errors + teal top-right fold for notes (Fable D-5)** [CL-04-06] (P19-FABLE; `P19-FABLE-0398`): Reserve red strictly for errors on the left edge and move Excel's red note corner to a teal fold so red has one unambiguous meaning.
  - **Error triangles and ⊗/⊘ root-vs-propagated pips on overlay (Neon)** [CL-04-06] (P19-NEON; `P19-NEON-0020`, `P19-NEON-0203`): Draw corner triangles for errors and distinguish root (⊗) vs propagated (⊘) errors when Provenance Overlay is toggled on.
  - **Option A: Hue-free achromatic typed warning badge `[!]`** [CL-05-06] (P19-FABLE; `P19-FABLE-0136`): Typed warnings carry no colour hue (neutral slate `[!]` badge), keeping red strictly for Excel `#` errors and amber strictly for active/recalculating/dirty state.
  - **Option B: Coloured non-red warning accents (amber/gold badges)** [CL-05-06] (P19-CARBON, P19-NEON; `P19-CARBON-0034`, `P19-NEON-0031`): Flag clipped ranges and structural anomalies with non-red amber/gold warning callouts.
- **Decision:** Adopt one non-colliding Strand v0.2 visual truth vocabulary across every view: (1) Focus/selection = dark-outer (`#193F49`) / amber-inner (`#FFB301`) dual stroke; (2) Excel `#` errors = red left-edge cell tick (base Classic chrome) + `⊗` (root) / `⊘` (propagated) in badges, Inspector, and Provenance Overlay; (3) Warnings and structural anomalies = bold ink hollow triangle `△` with NO color hue (keeping red strictly for `#` errors and amber strictly for active calc/dirty/focus); (4) External/volatile/RTD = signal-orange diamond `◊`; (5) Freshly recalculated = fading green underline (`#3F8B76`, meaning strictly 'calculated for this revision', never 'verified/audited'); (6) Saved/stale/pending = neutral diagonal hatch + distinct word/ARIA state; (7) Notes = teal top-right corner fold; (8) Truth-channel line styles = solid (workbook fact), dashed (inferred/suggested), dotted (uncommitted proposal/presentation).
- **Not taken (Appendix Z):** Raw single-stroke amber focus border (FRAMING-0094, P19-FABLE-0002); calling green 'fresh/verified' (FRAMING-0094); using `[!]` for errors or warnings (P19-CARBON-0174, P19-FABLE-0136, P19-CARBON-0034, P19-NEON-0031), `◈` or `▸` for external links (P19-CARBON-0174, P19-NEON-0193), or corner error triangles on cells (P19-NEON-0020, P19-NEON-0203) — retired to Appendix Z.
- **Reason:** Eliminates glyph and color collisions across the four Pro 19 sources, ensures full legibility in greyscale/colorblindness, and preserves Excel's cell fills cleanly.

### D-18: Quiet steady-state Strip surface with explicit orthogonal abnormal-state readouts

- **Authority:** Decider ruling | **Primary topic:** I. Principles | **Clusters:** `CL-01-08`, `CL-02-12`
- **Options & sources:**
  - **Option A: Always display persistent verbose status text and revision indicators** [CL-01-08] (FRAMING; `FRAMING-0088`): Show standing text readouts ('Clean', revision numbers, calc duration) continuously on working screens.
  - **Option B: Quiet steady mark when normal; explicit words and badges only when abnormal (Pro 20 Change 1)** [CL-01-08] (FRAMING, P21; `FRAMING-0136`, `P21-0005`): Keep the Strip present as a standing instrument showing only a small steady mark when the workbook is clean and current; surface words/badges immediately for pending, stale, previous, as-saved, error, missing source, or slow calc (>300 ms), and keep revision numbers in History and print footers.
  - **Option A: Show standing text like `✓ Clean`, last duration, and revision numbers in the Strip and working panels** [CL-02-12] (P19-FABLE, P19-GPT, P19-NEON; `P19-NEON-0046`, `P19-FABLE-0134`, `P19-GPT-0242`): Always show explicit status words (`✓ Clean`), calc duration, and calculation revision numbers on working screens.
  - **Option B: Quiet steady mark when clean/current; surface explicit orthogonal badges on abnormal states and keep revision numbers in History/print footers/live-feed inspector** [CL-02-12] (P19-CARBON, P19-GPT; `P19-GPT-0039`, `P19-CARBON-0052`): Align with Pro 20 Change 1 (FRAMING-0136, P21-0005): show only a small steady mark when clean and current; expand Strip badges immediately for Manual/Dirty `[F9]`, Calculating `% (Esc)`, Iterative unconverged, Errors, As-saved, or Missing source, and keep revision IDs in History, print footers, and expanded live-stream details (while still enforcing zero mixed-revision reports).
  - **Option A: Collapse pending, stale, and as-saved visually into one undifferentiated hatch** [CL-02-12] (P19-FABLE; `P19-FABLE-0134`): Share one diagonal hatch across pending, stale, and as-saved cells.
  - **Option B: Share subtle neutral hatch on grid paper + distinct words, Strip banner, hover/Bridge readout, and ARIA state for `as saved`, `stale`, `previous`, and `pending`** [CL-02-12] (P19-FABLE, P19-GPT; `P19-FABLE-0134`, `P19-GPT-0039`, `P19-GPT-0238`): Avoid inventing three new background hues on the grid by keeping the neutral diagonal hatch for non-current cell values, while distinguishing `as saved (<date>)`, `stale (manual)`, `previous (mid-recalc/cancelled)`, and `pending` explicitly via distinct Strip badges, Bridge/hover words, and ARIA accessible descriptions.
- **Decision:** Adopt Pro 20 Change 1: when a workbook is clean and current, the Instrument Strip shows only a quiet steady mark (no constant '✓ Clean' banner or revision-counter noise). The instant any abnormal condition holds (as saved, stale, previous, pending, calculating >300 ms, errors, quarantined macros, missing links), surface explicit orthogonal state words, badges, and distinct ARIA labels. Keep revision numbers in History, diagnostic drawers, and print footers while guaranteeing internally that no mixed-revision result is ever labelled 'current'.
- **Not taken (Appendix Z):** Always-on verbose revision/cleanliness banners in the Strip (FRAMING-0088, P19-NEON-0046, P19-FABLE-0134, P19-GPT-0242) and conflating `saved`/`stale`/`previous`/`pending` under one single label without distinct accessible states (P19-FABLE-0134) — retired to Appendix Z.
- **Reason:** Quiet surfaces let abnormal states stand out immediately when it matters, while distinct state labels preserve strict truthfulness about calculation freshness.

### D-19: View density and typography roles across Working, Notebook prose, and App surfaces

- **Authority:** Decider ruling | **Primary topic:** I. Principles | **Clusters:** `CL-01-09`
- **Options & sources:**
  - **Option A: Reading density + Source Serif 4 for Notebook prose only; App uses Working density** [CL-01-09] (FRAMING; `FRAMING-0086`, `FRAMING-0099`): Keep Source Serif 4 strictly for Notebook explanatory prose and restrict Reading density to Notebook/published surfaces.
  - **Option B: Reading density for both Notebook and App (while keeping Source Serif 4 strictly for narrative prose, not form controls)** [CL-01-09] (P19-CARBON, P19-NEON; `P19-CARBON-0010`, `P19-NEON-0011`): Use Working density (22 px rows, 4–6 px gaps) for Classic, Inspector, Model map, and Formula Explorer, and Reading density (generous spacing, >=36 px control hit targets, ~68 ch measure) for Notebook and App—using Source Serif 4 for Notebook/App narrative prose and Recursive sans/tabular numerals for App KPI tiles and controls.
- **Decision:** Working density governs Classic, Inspector, Model map, and Formula Explorer (Recursive sans-linear with `tnum` tabular figures, mono formulas). App uses Reading-density layout (`>=36 px` interactive targets) with Recursive sans-linear `tnum` KPI figures and Source Serif 4 only for long explanatory prose. Inside Notebook (single Jupyter chain per D-06), formula->result cells use crisp Working-style monospace/tabular presentation while Markdown narrative cells between links use comfortable Source Serif 4 prose typography (68 ch measure). Replace hardcoded 'Segoe UI Variable' fallback names with generic `system-ui`.
- **Not taken (Appendix Z):** Restricting Reading-density spacing exclusively to Notebook/published mode (P19-CARBON-0010, P19-NEON-0011) or applying a separate Reading-density formula-hiding mode inside Notebook (superseded by D-06); hardcoded OS font names (`Segoe UI Variable` in FRAMING-0099) — parked in Appendix Z.
- **Reason:** Aligns typography cleanly with Owner Ruling #6 (Jupyter Notebook vs App forms/reader) and cross-platform offline font hygiene.

### D-20: Motion budget: 400 ms dependency-propagation ripple and static dashed copy outline default

- **Authority:** Decider ruling | **Primary topic:** I. Principles | **Clusters:** `CL-01-03`, `CL-01-10`, `CL-04-20`
- **Options & sources:**
  - **Option A: Animate recalc ripple strictly along engine `evaluation_order` without total cap** [CL-01-10] (FRAMING; `FRAMING-0101`): Follow STRAND.md v0.1 wording literally, rippling cell-by-cell along `evaluation_order`.
  - **Option B: Animate topological dependency waves capped at 400 ms (block-stepped >200 items), never claiming Excel execution order, and replace animated marching ants with a static high-contrast dashed copy border** [CL-01-10] (P19-CARBON, P19-FABLE; `P19-CARBON-0175`, `P19-FABLE-0038`): Treat the ripple as a topological dependency propagation visual capped at 400 ms total (stepping by block beyond 200 items), never presenting it as Excel's execution chain, and do not animate a marching-ants copy outline.
  - **Static crisp dashed copy outline + Strip clipboard readout (Fable D-2)** [CL-04-20] (P19-FABLE; `P19-FABLE-0395`): Keep the copy outline completely still (Strand limits ambient motion to purposeful transitions) and name the copied range in the Status Strip.
  - **Animated marching dashed border respecting prefers-reduced-motion (Neon A.7, GPT A-57)** [CL-04-20] (P19-GPT, P19-NEON; `P19-GPT-0164`, `P19-NEON-0175`): Animate the dashed source border like Excel's marching ants, falling back to a static double-dashed border under prefers-reduced-motion.
  - **Additional governing / aligned cards across topics** (P21): `P21-0058`
- **Decision:** Recalculation ripples animate topological dependency propagation only (never claiming Excel's execution order, per D-51), cap total duration at 400 ms (stepping by block above 200 affected items), and collapse to 0 ms under `prefers-reduced-motion`. Classic copy/cut selection uses a static high-contrast dashed Strand outline plus explicit Strip clipboard readout ('Copied B4:M12 · Esc cancels') by default.
- **Not taken (Appendix Z):** STRAND v0.1's wording claiming ripples follow real evaluation order (FRAMING-0101) and default animated marching-ants copy outline (P19-GPT-0164, P19-NEON-0175, kept only as an optional off-by-default Classic setting, always disabled under reduced motion) — parked in Appendix Z.
- **Reason:** Honours owner-accepted Push-back #2 (dependency != execution order), respects vestibular accessibility, and avoids idle GPU/CPU wakeups.

### D-21: WCAG 2.2 AA accessibility baseline and phased touch/pen ergonomics

- **Authority:** Decider ruling | **Primary topic:** I. Principles | **Clusters:** `CL-01-11`, `CL-01-12`
- **Options & sources:**
  - **Option A: Target WCAG 2.1 AA** [CL-01-11] (P19-CARBON; `P19-CARBON-0173`): Use WCAG 2.1 AA contrast thresholds as the formal compliance version.
  - **Option B: Target WCAG 2.2 AA (strict superset)** [CL-01-11] (FRAMING, P19-NEON; `P19-NEON-0192`, `FRAMING-0097`): Adopt WCAG 2.2 AA (matching P19-GPT-0008), keeping identical 4.5:1 / 3:1 contrast thresholds while adding focus appearance, dragging-movement click alternatives, and minimum target size requirements.
  - **Option A: Defer all touch and pen concerns as out of scope** [CL-01-12] (P19-FABLE; `P19-FABLE-0157`): Keep touch and pen interactions completely out of scope for now; add later starting with App, then Model map, then Notebook.
  - **Option B: Keyboard/mouse first, with >=36 px Reading-density hit targets and zero hover-only primary actions from day one** [CL-01-12] (P19-NEON; `P19-NEON-0238`): Prioritise keyboard and pointer workflows while ensuring App and Notebook controls already meet >=36 px touch-friendly hit targets and no primary action depends solely on hover.
- **Decision:** Enforce WCAG 2.2 AA as the normative accessibility standard across all three built-in themes and every custom skin via automated build checks. Meet baseline touch ergonomics from day one on Reading-density controls (`>=36 px` hit targets in App/Notebook, zero hover-only primary actions, keyboard/click alternatives for every drag), while deferring dedicated touch/pen authoring gestures (pen-dragging map wires, ink markup) to a later milestone in the order App -> Model map -> Notebook.
- **Not taken (Appendix Z):** Weakening the accessibility baseline to WCAG 2.1 AA (P19-CARBON-0173) or deferring even basic `>=36 px` non-hover ergonomics (P19-FABLE-0157's full deferral) — parked in Appendix Z.
- **Reason:** WCAG 2.2 AA adds target-size and focus-appearance protections directly aligned with Strand v0.2's dual-stroke focus ring without adding premature pen-authoring scope.

### Section: II. Conceptual model

### D-22: Unified canonical address grammar for Excel targets, inferred blocks, objects, and view items

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-03`
- **Options & sources:**
  - **Option A: Carbon sigil syntax (`#Monthly.OpEx`, `@notebook/sec-1/prose-1`)** [CL-02-03] (P19-CARBON; `P19-CARBON-0048`): Use `#` prefix for inferred blocks, bare names for objects, and `@` prefix for presentation view items.
  - **Option B: Neon scheme-prefixed syntax (`block:Sheet/BlockId`, `obj:Sheet/ObjectName`, `view:ViewId/ItemId`)** [CL-02-03] (P19-NEON; `P19-NEON-0042`): Use explicit URI-like prefixes (`block:`, `obj:`, `view:`) for all non-grid entities.
  - **Option C: Fable range-backed blocks + quoted objects + `view:` prefix (with GPT scope display)** [CL-02-03] (P19-FABLE, P19-GPT; `P19-FABLE-0120`, `P19-FABLE-0121`, `P19-GPT-0033`): Address inferred blocks by their real Excel range (`Monthly!B5:O9`) displayed as `‹label›`, objects via `Sheet!"ObjectName"`, and view items via `view:notebook/Summary/text-1`, showing name scope plainly in the UI (e.g. `Workbook::Name` badge) without polluting formula syntax.
- **Decision:** Adopt one collision-free canonical address grammar: (1) Native Excel targets use exact Excel syntax (`Sheet!A1`, `Sheet!A1:D12`, `Sheet!C21#`, `Name`, `Table[Col]`) with workbook/sheet scope shown as a UI badge; (2) Inferred Blocks are canonically addressed by their backing Excel range (`Monthly!B5:O9`) and displayed with inferred guillemets `‹label›` until pinned as a real Name; (3) Sheet Objects use `obj:Sheet/ObjectName` (displayed `Sheet · ObjectName`); (4) Presentation View Items use `view:<view>/<item-path>` with a dotted focus frame, muted ink, and `[View]` badge.
- **Not taken (Appendix Z):** Synthetic `block:` canonical IDs that break after external Excel edits and `#`/`@` prefix sigils that collide with Excel spill/implicit-intersection syntax (P19-CARBON-0048, P19-NEON-0042) — parked in Appendix Z.
- **Reason:** Keeps every model address 100% copy-pasteable into Excel while unambiguously distinguishing workbook objects and presentation-only view items.

### D-23: Separate Read-Only Formula Lens from Scoped-Name Interpreter; no automatic LET refactoring

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-04`, `CL-05-06`, `CL-06-09`, `CL-09-11`
- **Options & sources:**
  - **Option A: Strictly read-only Formula Lens everywhere (no input rewriting)** [CL-02-04] (FRAMING; `FRAMING-0063`): Allow the formula lens only to format/display formulas read-only; editing always happens on raw Excel formula syntax.
  - **Option B: Separate Read-Only Formula Lens from explicit Scoped-Name Input Interpreter** [CL-02-04] (FRAMING; `FRAMING-0012`, `FRAMING-0063`, `FRAMING-0107`): Keep Formula Lens (names/LET/lambda pretty-printing) 100% read-only—never rewriting on display or copy—while allowing Model map and Notebook's Scoped-Name Interpreter to expand box-local shorthand (`Input1 * 2` -> `MyCell.Input1 * 2`) strictly via AST + W074 rules and visibly displaying the exact expanded Excel formula in the formula bar and preview seam before commit.
  - **Default to Names + Layout; make 'Show as steps' (virtual LET) an explicit opt-in mode** [CL-09-11] (P19-FABLE; `P19-FABLE-0158`, `P19-FABLE-0222`): Avoid synthesizing fake `LET` variable names by default so users never mistake synthetic variables for stored workbook content; offer 'Show as steps' as an explicit labelled toggle.
  - **Include read-only virtual `LET` decomposition directly inside the default Formula Lens toggle** [CL-09-11] (P19-CARBON, P19-NEON; `P19-CARBON-0068`, `P19-NEON-0058`, `P19-NEON-0228`): Automatically format deeply nested or repeated expressions into indented read-only `LET` bindings with inline chips when `Alt+L` is toggled.
  - **Accept defined names and group-shortened names during edit while previewing exact stored Excel text** [CL-09-11] (P19-CARBON, P19-FABLE; `P19-CARBON-0068`, `P19-CARBON-0113`, `P19-FABLE-0158`): Allow typing readable names/labels (resolved via AST) while continuously showing the exact Excel formula to be stored in the seam.
  - **Strictly read-only comprehension; pressing F2 always edits raw Excel formula with inline alias badges** [CL-09-11] (P19-NEON; `P19-NEON-0105`): Never rewrite typed text from Lens syntax; F2 opens the real Excel formula with read-only alias chips.
  - **Additional governing / aligned cards across topics** (P19-GPT): `P19-GPT-0090`
  - **Additional conflicting / superseded cards across topics** (P19-NEON): `P19-NEON-0031`
- **Decision:** Keep two cleanly separated formula-display mechanisms: (1) **Read-Only Formula Lens** (`Alt+Shift+L`): defaults to Names + Structural Layout only (defined names plain, inferred headers in `‹…›`, clean indentation); 'Show as steps' (read-only repeated sub-expression factoring) is an explicit opt-in sub-toggle badged `'Shown as steps · stored as one formula'`; 'Refactor cell to real LET' is strictly an explicit, previewed model edit (never claiming single-input parity proves universal equivalence). In Classic, `F2` always edits the real Excel formula with inline read-only alias chips. (2) **Scoped-Name Interpreter**: inside named Model map boxes and Notebook frames, users may type defined or prefix-shortened names that expand deterministically via AST under W074 rules, previewing the exact stored Excel formula before commit.
- **Not taken (Appendix Z):** Automatic synthetic LET decomposition on Formula Lens toggle (P19-CARBON-0068, P19-NEON-0031, P19-NEON-0058, P19-NEON-0228) and editing directly inside a synthesized virtual-LET view (rejected per Pro 19 Push-back #3, P19-GPT-0090, P19-CARBON-0113, P19-FABLE-0222, P19-NEON-0105) — parked in Appendix Z.
- **Reason:** Honours owner-accepted Push-back #3 and four-answer consensus (FRAMING-0063): synthetic LET rewriting can alter volatile multiplicity (`RAND()+RAND()`), `@` implicit intersection, and pre-365 compatibility.

### D-24: Cross-view selection continuity without silent substitution and minimal-scroll camera stability

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-05`
- **Options & sources:**
  - **Option A: Silently substitute a nearby anchor or last workbook anchor on the sheet** [CL-02-05] (P19-NEON; `P19-NEON-0043`): Fall back to a nearby cell/chip or last workbook anchor whenever the selected target has no projection in the destination view.
  - **Option B: Preserve exact selection without substituting; show 'Selected source is not in this layout' with Reveal/Add actions** [CL-02-05] (P19-GPT; `P19-GPT-0035`, `P19-GPT-0233`): Never manufacture a fake substitute selection when a model source is absent from a curated view (Notebook/App/Map); show 'Selected source is not in this layout' with 'Reveal in Classic' and 'Add to this view', while separately remembering the last model cell when switching away from source-less prose.
  - **Option A: Always center target in viewport within <=100 ms** [CL-02-05] (P19-CARBON, P19-NEON; `P19-NEON-0023`, `P19-NEON-0043`, `P19-CARBON-0179`): Recenter the viewport around the translated selection on every view switch.
  - **Option B: Preserve independent scroll/zoom per view & sheet; scroll only if target is off-screen (<=150 ms)** [CL-02-05] (P19-FABLE; `P19-FABLE-0124`, `P19-FABLE-0429`): Remember each view's scroll and zoom independently and scroll only as far as needed to bring an off-screen selection into view.
- **Decision:** When switching views: (1) if the destination view has a containing item for the selected model source, select it and state the coarser mapping in the Strip; (2) if no item represents the selected model source, preserve the exact model selection intact (never silently substituting a neighbour cell!) and show a Strip notice with 'Reveal in Classic' and 'Add to this view'; (3) switching away from source-less prose activates the remembered last model cell with a Strip notice. Preserve each view's camera: never yank scroll if the selected target is already visible; center only when scrolling an off-screen target into view (100 ms target / 150 ms hard ceiling).
- **Not taken (Appendix Z):** Silently substituting the nearest mapped chip/cell on view switch (P19-NEON-0043, P19-CARBON-0049) and unconditionally re-centering the viewport on every view switch (P19-NEON-0023, P19-NEON-0043, P19-CARBON-0179) — parked in Appendix Z.
- **Reason:** Preventing silent selection drift and unnecessary viewport jumps keeps mental context intact across views.

### D-25: Preview Seam trigger scope: zero-ceremony Classic scalar entry vs mandatory seam for structural and non-grid edits

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-07`, `CL-04-08`
- **Options & sources:**
  - **Option A: Show a Preview Seam before Enter on literally every edit** [CL-02-07] (P19-FABLE; `P19-FABLE-0016`): Require every edit to show a preview seam before committing.
  - **Option B: Direct zero-ceremony commit for single-cell Classic typing; Preview Seam for non-grid model mutations, bulk/structural edits, and agent proposals** [CL-02-07] (P19-CARBON, P19-NEON; `P19-CARBON-0023`, `P19-NEON-0005`): Keep plain single-cell entry in Classic instant on Enter (zero modal friction), while routing every non-grid model-creating/structural edit, bulk fill, wire rewire, Formula Explorer apply, and agent proposal through the Preview Seam.
  - **Non-blocking Preview Seam inline warning before commit (Fable D-7, R-25)** [CL-04-08] (P19-FABLE; `P19-FABLE-0069`, `P19-FABLE-0400`, `P19-FABLE-0437`): Display a live inline warning in the seam while editing/before commit when an entry would overwrite a formula inside a uniform run, paste into hidden rows, or write across grouped sheets.
  - **Pure immediate Excel commit without extra pre-commit friction (GPT, Neon)** [CL-04-08] (P19-GPT, P19-NEON; `P19-GPT-0140`, `P19-NEON-0159`): Commit immediately on Enter/Tab matching Excel's zero-friction data-entry cadence.
- **Decision:** Keep single-cell Enter/Tab entry in Classic (and direct scalar bound controls whose target address is already shown inline) completely unblocked with zero extra confirmation dialog, displaying non-blocking inline Preview Seam warnings during edit/before drop when an action breaks a formula run, pastes into hidden rows, or writes to grouped sheets ('Replaces a formula in a run of 16 alike · Esc cancels', 'Grouped: 3 sheets'). Require the full Preview Seam (showing exact target cells, Model vs Presentation badge, and before/after formulas) before committing any non-grid model-creating edit, multi-cell run edit, Model map wire rewrite, Formula Explorer Apply, or agent proposal.
- **Not taken (Appendix Z):** Putting a blocking Preview Seam confirmation step in front of ordinary single-cell Classic edits (P19-FABLE-0016 strict reading) or suppressing live inline warnings when overwriting formula runs/grouped sheets (P19-GPT-0140, P19-NEON-0159) — parked in Appendix Z.
- **Reason:** Preserves analyst keystroke speed in Classic while keeping every non-grid or high-blast-radius edit transparent before commitment.

### D-26: Safe Delete on Model map and exception-preserving formula-run edits

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-09`
- **Options & sources:**
  - **Option A: `Delete` on a Map box clears its backing cell and deletes its name (via seam listing `#NAME?` dependents)** [CL-02-09] (P19-FABLE; `P19-FABLE-0129`): Treat deleting a box on Model map directly as deleting the underlying Excel cell and defined name, previewed in the seam.
  - **Option B: `Delete` removes view card in Notebook/App and prompts Map-only `[V]` vs Delete-backing-cell `[M]` (`Shift+Delete`) on Model map** [CL-02-09] (P19-CARBON; `P19-CARBON-0050`): Keep `Delete` non-destructive across non-grid views (removing only the presentation card in Notebook/App, and prompting Map-only vs backing-cell deletion with `#NAME?` blast radius on Model map, with `Shift+Delete` targeting backing cells directly).
  - **Option A: Write relatively identical formulas across the entire run as one step** [CL-02-09] (P19-FABLE; `P19-FABLE-0127`, `P19-FABLE-0450`): Always fill an edited run item across all cells in the run as one undo step.
  - **Option B: Fill uniform runs (`Shift+Enter` for single cell) and refuse silent clobbering of overrides/exceptions** [CL-02-09] (P19-CARBON; `P19-CARBON-0050`): Preview all R1C1-identical cells on run edit (`Shift+Enter` edits single cell) and never silently overwrite an existing exception cell.
- **Decision:** Pressing `Delete` clears cells in Classic, removes presentation cards `[V]` in Notebook and App, and on Model map opens a two-choice inline seam defaulting to 'Remove from Map (Presentation)' alongside 'Delete backing cell & name (Model — lists every #NAME? dependent)' (with `Shift+Delete` targeting backing model deletion directly through the same blast-radius seam). When editing a formula run containing exceptions ('11 alike, 1 differs'), the Preview Seam lists each exception and defaults to updating only the uniform cells unless the user explicitly checks 'Also overwrite N exception(s)' (`Shift+Enter` edits only the active cell).
- **Not taken (Appendix Z):** Clearing backing cells and names immediately on plain `Delete` in Model map (P19-FABLE-0129) and overwriting formula-run exceptions by default (P19-FABLE-0127, P19-FABLE-0450) — parked in Appendix Z.
- **Reason:** Prevents accidental `#NAME?` cascades and silent destruction of intentional overrides while keeping deliberate bulk edits one checkbox away.

### D-27: Hybrid Undo: one shared Model timeline + scoped per-view Presentation timelines unified in History

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-10`
- **Options & sources:**
  - **Option A: One single flat chronological `[Model]+[View]` stack across all views** [CL-02-10] (P19-CARBON, P19-GPT, P19-NEON; `P19-CARBON-0051`, `P19-CARBON-0182`, `P19-GPT-0038`, `P19-GPT-0251`, `P19-NEON-0045`, `P19-NEON-0197`): Every model edit and every view-layout edit pushes onto one chronological stack; pressing `Ctrl+Z` in Classic undoes a Map box drag made just prior and shows a Strip toast.
  - **Option B: One shared `[Model]` history + per-view `[View]` presentation history (unified in History drawer)** [CL-02-10] (P19-FABLE; `P19-FABLE-0082`, `P19-FABLE-0132`, `P19-FABLE-0501`): Keep one chronological `[Model]` stack shared across all views plus a small `[View]` presentation history per view; `Ctrl+Z` undoes whichever is more recent between the last `[Model]` step and the *focused view's* last `[View]` step, while `Ctrl+Shift+Z` shows the combined chronological timeline with `[Model]`/`[View]` filters.
- **Decision:** Maintain one shared workbook `[Model]` undo timeline plus scoped per-view `[View]` presentation timelines, surfaced together in the `Ctrl+Shift+Z` History drawer. Pressing `Ctrl+Z` undoes whichever step is more recent between the latest `[Model]` edit and the active view's latest `[View]` edit, announcing what was undone in the Strip (with a `[Jump]` link if off-screen).
- **Not taken (Appendix Z):** A single flat global `[Model]+[View]` stack where pressing `Ctrl+Z` in Classic silently undoes an old layout drag made earlier in Model map (P19-CARBON-0051, P19-CARBON-0182, P19-GPT-0038, P19-GPT-0251, P19-NEON-0045, P19-NEON-0197) — parked in Appendix Z.
- **Reason:** Preserves the 'one workbook, one undo history' guarantee for model truth while preventing disorienting invisible layout undos across views.

### D-28: Cross-view semantic clipboard matrix preventing silent model duplication or external links

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-11`, `CL-06-12`
- **Options & sources:**
  - **Option A: Pasting a Map box into Map creates new backing boxes/cells with name clashes listed** [CL-02-11] (P19-FABLE; `P19-FABLE-0505`): Default to creating new backing cells and names when pasting a Map box into Model map, resolving name clashes in the Preview Seam.
  - **Option B: Pasting a view frame duplicates presentation by default; source duplication is a distinct explicit choice** [CL-02-11] (P19-GPT; `P19-GPT-0250`): Default view-frame copy/paste within a workbook to duplicating the presentation view item only (`[V]`), while offering explicit, separately named `'Duplicate backing cells & names'` (`[M]`) via the Preview Seam.
  - **Option A: Same-workbook paste defaults to Linked Live Data Frame with post-paste chip; cross-workbook prompts explicitly** [CL-06-12] (P19-NEON; `P19-NEON-0214`): Pasting a range from Classic within the same workbook immediately inserts a Linked Live Data Frame with a non-blocking 5 s Paste Options chip (`Linked | Static snapshot`), while cross-workbook pastes always prompt explicitly so no silent external link is created.
  - **Option B: Always prompt or separate frame copy from value copy** [CL-06-12] (P19-GPT; `P19-GPT-0019`): Distinguish copying a card frame (pastes second live view) from copying cell values, and prompt before creating cross-workbook bindings.
- **Decision:** Standardise cross-view copy/paste semantics: (1) pasting a Notebook/App frame within the same workbook duplicates presentation `[V]` by default; (2) pasting a Classic range into Notebook within the same workbook defaults to a Linked Live Data Frame with an unobtrusive post-paste chip (`Linked | Static snapshot`); (3) pasting a Model map box opens the inline Preview Seam offering 'Duplicate backing cells & names (Model — resolves name clashes)' vs 'Place second view reference (Presentation)'; (4) pasting across workbooks or from external tables always prompts explicitly (never silently creating external links or mutating unpreviewed cells).
- **Not taken (Appendix Z):** Silently minting duplicate backing cells/names on Model map paste (P19-GPT-0250) or defaulting same-workbook Classic->Notebook paste to a blocking modal (P19-GPT-0019 same-workbook case) — parked in Appendix Z.
- **Reason:** Makes the common same-workbook linking case effortless while preventing accidental model duplication or hidden external links.

### D-29: Uncommitted agent proposal visual grammar (dotted teal-600 + 8% hatch, never amber or dashed)

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-14`
- **Options & sources:**
  - **Option A: Carbon sage-400 diagonal hatch + `[Δ Agent]` twin-chip** [CL-02-14] (P19-CARBON; `P19-CARBON-0018`): Use `sage-400` diagonal hatching, casual-axis ghost text, and `[Δ Agent]` chip.
  - **Option B: Neon dashed `teal-600` border + 8% petrol-teal hatch + `[⇄ Agent]` badge** [CL-02-14] (P19-NEON; `P19-NEON-0016`): Use a dashed `teal-600` border with 8% petrol-teal 45-degree hairline hatch and `[⇄ Agent]` badge.
  - **Option C: Fable dotted teal border + light hairline hatch + `[Agent]` badge (never amber, never dashed)** [CL-02-14] (P19-FABLE; `P19-FABLE-0039`, `P19-FABLE-0498`): Use dotted `teal-600` border (matching Strand v0.2's dotted = ghost truth channel) over a subtle 8% petrol-teal hairline hatch, casual-axis ghost values, and a plain `[Agent]` badge, settling with a 150 ms green-500 fresh pulse on accept.
- **Decision:** Render uncommitted agent proposals with a **dotted `teal-600` border**, subtle 8% petrol-teal 45-degree hairline hatch, casual-axis ghost value tokens, and an `[Agent]` badge—never amber, never a new hue, and never dashed (since dashed belongs to inferred structure per D-17). On acceptance, settle with a 150 ms green fresh pulse.
- **Not taken (Appendix Z):** Dashed teal-600 border (P19-NEON-0016, collides with inferred/suggested structure) and sage-400 hatch (P19-CARBON-0018, clashes with green recalc tint) — parked in Appendix Z.
- **Reason:** Keeps Strand v0.2's line-style truth channel unambiguous (solid = fact, dashed = inferred, dotted = uncommitted proposal) without adding another color hue.

### D-30: Agent autonomy modes and Excel-safe invocation shortcuts

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-14`, `CL-02-15`
- **Options & sources:**
  - **Option A: Never allow interactive auto-apply of `[M]` edits; only unattended CLI writes to a branched copy** [CL-02-14] (P19-CARBON; `P19-CARBON-0122`): Force all interactive model edits through staging; route unattended CLI runs to `Name.agent.xlsx`.
  - **Option B: Three per-workbook modes (`Look`, `Propose` default, `Apply, then review` / explicitly delegated scopes) + branched CLI copy** [CL-02-14] (P19-FABLE, P19-GPT; `P19-FABLE-0139`, `P19-GPT-0099`): Default to `Propose`, support `Look` (read-only) and opt-in `Apply, then review` (where applied edits stay tagged `unreviewed` in the same Changes panel for one-key Keep/Revert), plus Carbon's branched-copy convention for unattended CLI runs.
  - **Option A: `Ctrl+I` (Neon) or `Ctrl+J` (Carbon)** [CL-02-15] (P19-CARBON, P19-NEON; `P19-NEON-0040`, `P19-CARBON-0044`): Bind a direct `Ctrl+letter` shortcut to open the Agent panel.
  - **Option B: `Tools > Agent` and `Alt+Q` command search fall-through (`Ask the agent: …`)** [CL-02-15] (P19-GPT; `P19-GPT-0025`): Invoke the agent from `Tools > Agent`, the Mast chip, or typing free-form requests into `Alt+Q` without stealing any Excel `Ctrl+letter` shortcut.
- **Decision:** Provide three per-workbook agent modes: `Look`, `Propose` (default), and opt-in `Apply, then review` (explicitly delegated scopes where every applied step stays tagged `unreviewed` in Changes until kept or reverted); unattended CLI agent runs write to a branched copy by default unless told otherwise, and trust-boundary actions (macros, external links, add-ins) always ask. Invoke the Agent via `Alt+Q` fall-through, `Tools > Agent`, or the Mast agent chip—never binding `Ctrl+I` or `Ctrl+J`.
- **Not taken (Appendix Z):** Forbidding opt-in delegated interactive apply entirely (P19-CARBON-0122); binding `Ctrl+I` (collides with Excel Italic, P19-NEON-0040) or `Ctrl+J` (collides with Excel Find newline, P19-CARBON-0044) to Agent invocation — parked in Appendix Z.
- **Reason:** Protects Classic keyboard parity while supporting both cautious proposal-first review and explicitly delegated multi-step workflows.

### D-31: Agent review, subset evaluation, Undo audit trail, and checked out-of-order reversal

- **Authority:** Decider ruling | **Primary topic:** II. Conceptual model | **Clusters:** `CL-02-15`, `CL-02-17`
- **Options & sources:**
  - **Option A: Show pre-computed per-hunk KPI deltas upfront and live-update ghost overlays on tick** [CL-02-15] (P19-CARBON, P19-FABLE; `P19-CARBON-0044`, `P19-FABLE-0109`): Compute each hunk's standalone impact immediately upon arrival.
  - **Option B: Keep checkbox ticking 100% inert and require `'Review selected'` (`P`) to evaluate the exact chosen subset** [CL-02-15] (P19-GPT, P21; `P19-GPT-0025`, `P19-GPT-0274`, `P21-0110`, `P21-0111`): Ticking checkboxes triggers zero side effects or partial recalc; `'Review selected'` computes exact combined arithmetic and scoped changed-cell counts specifically for the chosen subset (e.g., A+B = 276,000 vs A+C = 252,000).
  - **Option A: Return undone part to pending in Changes** [CL-02-17] (P19-FABLE; `P19-FABLE-0111`, `P19-FABLE-0495`): Move an undone agent part back to pending in the Changes panel so the user can re-tick and re-apply it easily.
  - **Option B: Mark unit `Undone` in History and let `Ctrl+Y` redo it** [CL-02-17] (P19-GPT, P21; `P19-GPT-0026`, `P21-0113`): Record `Undone` in the immutable audit History and allow `Ctrl+Y` redo until superseded by a new edit.
  - **Option A: Direct `Revert` action with conflict alert offering `'Show me'` and `'Revert anyway'`** [CL-02-17] (P19-FABLE; `P19-FABLE-0113`, `P19-FABLE-0496`): Write a new forward step directly if unconflicted; on overlapping later edits refuse and offer `'Show me'` / `'Revert anyway'`.
  - **Option B: `'Create reversal'` — generate a checked forward proposal through the Preview Seam** [CL-02-17] (P19-GPT; `P19-GPT-0026`, `P19-GPT-0278`): Never rewrite past history; compute a checked new forward reversal proposal comparing current values against the older state, highlighting any overlapping later edits before commit.
- **Decision:** Each agent proposal unit displays a read-only standalone summary on arrival; toggling checkboxes is completely inert (no writes, no flicker), and `'Review selected'` (`P`) evaluates and previews the exact combined subset arithmetic before `'Apply selected'` (`Ctrl+Enter`) commits. Undoing an accepted unit via `Ctrl+Z` records `Undone` in immutable History (preserving normal `Ctrl+Y` Redo) AND shows the unit as `Undone (re-selectable)` in Changes with stale-base revalidation. Reversing an earlier step out of order always uses `'Create reversal'`—a checked forward Preview Seam step that blocks automatic overwrite on overlapping later edits, names the conflicting cell/actor/time, offers `'Show me'`, and requires explicit override confirmation.
- **Not taken (Appendix Z):** Live mutating previews on checkbox click (P19-CARBON-0044, P19-FABLE-0109); erasing acceptance history on Ctrl+Z (P19-FABLE-0111, P19-FABLE-0495); blind `'Revert anyway'` without a checked forward preview (P19-FABLE-0113, P19-FABLE-0496) — parked in Appendix Z.
- **Reason:** Guarantees exact combined subset arithmetic, honest immutable audit logs, and zero silent clobbering of later human edits.

### Section: III. Shell

### D-32: Compact single-row Strand Command Deck with Excel tabs and exact Alt key tips

- **Authority:** Decider ruling | **Primary topic:** III. Shell | **Clusters:** `CL-03-02`, `CL-04-04`
- **Options & sources:**
  - **Compact single-row Command Deck with Excel tab names & exact key tips** [CL-03-02] (P19-FABLE, P21; `P19-FABLE-0021`, `P19-FABLE-0027`, `P19-FABLE-0422`, `P21-0009`): Include a slim single-row Strand-styled Command Deck exposing File, Home, Insert, Formulas, Data, Review, View with exact Excel Alt key-tip letters (and a 5-use onboarding summary of the Alt+Shift layer).
  - **No tab deck—contextual Bridge strip + Command Palette only** [CL-03-02] (P19-CARBON, P19-NEON; `P19-CARBON-0005`, `P19-CARBON-0012`, `P19-NEON-0012`, `P19-NEON-0014`): Omit any Excel-named command tabs entirely; rely on a 30–34 px Bridge contextual strip and Command Palette.
  - **Single fixed 30 px Command Strip (Carbon)** [CL-04-04] (P19-CARBON; `P19-CARBON-0056`): Keep command chrome strictly to a 30 px single-line strip at all times.
  - **Collapsible 32 px default / 68 px expanded tabbed Deck with overflow More (Neon, Fable, Pro21)** [CL-04-04] (P19-FABLE, P19-NEON, P21; `P19-NEON-0049`, `P19-FABLE-0383`, `P21-0044`): Default to a compact 32 px single-row Strand Action Band / Deck, expandable to 68 px for richer labeled controls and foldable with Ctrl+F1, with secondary tabs (Page Layout, Review, Developer) in More and always reachable directly by key-tip letters P, R, L.
- **Decision:** Ship a compact single-row Strand-styled Command Deck (32 px default, expandable to 68 px two-row band, foldable via `Ctrl+F1`) with Excel's tab names (`File`, `Home`, `Insert`, `Formulas`, `Data`, `View` visible directly; `Page Layout (P)`, `Review (R)`, `Developer (L)` under More visually while firing instantly by exact Excel Alt key-tip letters per D-11). In non-Classic views, the Deck collapses to view-contextual controls while keeping Alt key tips and `Alt+Q` reachable.
- **Not taken (Appendix Z):** Cloning Microsoft's 280 px multi-row Ribbon artwork (P19-CARBON-0005, P19-NEON-0012); removing the Command Deck completely in Classic in favour of a fixed 30 px ribbon-less bar (P19-CARBON-0012, P19-CARBON-0056, P19-NEON-0014) — parked in Appendix Z.
- **Reason:** Balances vertical workspace discipline with immediate Excel familiarity and exact key-tip conformance (D-11).

### D-33: Dedicated Stage-bottom sheet-tab bar visible across workbook views

- **Authority:** Decider ruling | **Primary topic:** III. Shell | **Clusters:** `CL-03-02`
- **Options & sources:**
  - **Dedicated sheet-tab strip along the bottom edge of the Stage** [CL-03-02] (P19-CARBON, P19-FABLE, P21; `P19-CARBON-0014`, `P19-FABLE-0027`, `P21-0009`): Place sheet tabs on their own thin bar directly below the Stage and above the 26 px Instrument Strip.
  - **Pack sheet tabs inside the 26 px Instrument Strip** [CL-03-02] (P19-NEON; `P19-NEON-0014`): Combine sheet tabs, calc state, error pill, aggregates, density, and zoom into a single 26 px bottom Strip.
  - **Keep workbook sheet tabs visible across views with preferred-view micro-glyphs** [CL-03-02] (P19-CARBON, P19-NEON; `P19-CARBON-0014`, `P19-NEON-0014`): Show workbook sheet tabs with per-sheet preferred-view micro-glyphs across views so users always see workbook structure.
  - **Replace sheet tabs with sections/pages/levels outside Classic** [CL-03-02] (P19-FABLE; `P19-FABLE-0027`): Show sheet tabs only in Classic; replace that bottom strip with sections, pages, or hierarchy levels in other views.
- **Decision:** Place workbook sheet tabs on a dedicated thin bar along the bottom edge of the Stage directly above the 26 px Instrument Strip (with an `[≡ N Sheets]` drawer for large workbooks). Keep sheet tabs (with per-sheet preferred-view micro-glyphs) visible across all workbook views, while view-local sub-navigation (Notebook outline, Model map levels, App pages) lives inside each view's own header or outline rail.
- **Not taken (Appendix Z):** Packing sheet tabs into the 26 px Instrument Strip (P19-NEON-0014) and hiding sheet tabs completely in non-Classic views (P19-FABLE-0027) — parked in Appendix Z.
- **Reason:** Keeps the Strip dedicated to status/provenance signals and lets users orient across multi-sheet workbooks in every view.

### D-34: Unified Excel-safe Command Palette (Alt+Q) and Alt+Shift+<key> DNA Calc tool shortcut layer

- **Authority:** Decider ruling | **Primary topic:** III. Shell | **Clusters:** `CL-03-02`, `CL-03-04`, `CL-04-05`, `CL-04-27`, `CL-09-04`, `CL-09-05`, `CL-09-11`
- **Options & sources:**
  - **Excel-safe Mast shortcuts (Alt+Q search, Ctrl+Alt+1..5 views, Alt+Shift tool toggles)** [CL-03-02] (P19-FABLE, P21; `P19-FABLE-0027`, `P21-0009`): Use Alt+Q for unified command search in Mast and keep all Excel Ctrl/Alt bindings untouched.
  - **Browser/IDE-style Mast shortcuts (Ctrl+K, Ctrl+E, Ctrl+I, Alt+L, Ctrl+1..5)** [CL-03-02] (P19-CARBON, P19-NEON; `P19-CARBON-0012`, `P19-NEON-0014`): Bind Mast triggers to Ctrl+K (Palette), Ctrl+E/Ctrl+Shift+E (Explorer), Ctrl+I (Agent), Alt+L (Lens), and Ctrl+1..5 (Carbon views).
  - **Alt+Q for Command Palette and Alt+Shift+<key> for DNA Calc tools (Alt+Shift+L Lens, E Explorer, T Trace, D Details, 0 companion grid)** [CL-03-04] (P19-FABLE; `P19-FABLE-0026`, `P19-FABLE-0420`, `P19-FABLE-0421`): Match Excel's Tell Me key (Alt+Q) for Command Search and reserve Alt+Shift+<key> for all DNA Calc tool toggles.
  - **Ctrl+K for Command Palette and Alt+L for Formula Lens** [CL-03-04] (P19-CARBON; `P19-CARBON-0054`): Use IDE-style Ctrl+K for Command Palette and Alt+L for Formula Lens.
  - **Ctrl+K / Ctrl+Shift+P for Command Palette (Carbon, Neon)** [CL-04-05] (P19-CARBON, P19-NEON; `P19-CARBON-0132`, `P19-NEON-0128`): Use web-app/IDE convention Ctrl+K (or Ctrl+Shift+P) to open Command Palette in Classic.
  - **Alt+Q for Command Search; keep Ctrl+K for Hyperlink (Fable, GPT, Pro21)** [CL-04-05] (P19-FABLE, P19-GPT, P21; `P19-FABLE-0388`, `P19-GPT-0227`, `P21-0049`): Bind Alt+Q to Command Search (exact Excel Tell Me shortcut) and reserve Ctrl+K strictly for Insert/Edit Hyperlink.
  - **`Ctrl+Shift+E` / `Alt+Shift+E` to open; `Enter` evaluates trial, `Ctrl+Enter` applies to cell** [CL-09-04] (P19-CARBON, P19-FABLE; `P19-CARBON-0035`, `P19-CARBON-0197`, `P19-FABLE-0149`, `P19-FABLE-0156`): Avoid Excel's `Ctrl+E` (Flash Fill) collision; make `Enter` evaluate trial and `Ctrl+Enter` commit Apply to cell.
  - **`Ctrl+Enter` evaluates trial; explicit Apply button commits to cell** [CL-09-04] (P21; `P21-0081`): Use `Ctrl+Enter` to run trial evaluation inside the multi-line formula editor so `Enter` inserts newlines cleanly, reserving Apply for an explicit button/chord.
  - **Additional governing / aligned cards across topics** (P19-GPT): `P19-GPT-0204`
  - **Additional conflicting / superseded cards across topics** (P19-CARBON, P19-NEON): `P19-CARBON-0030`, `P19-NEON-0039`
- **Decision:** Adopt one coherent, Excel-collision-free shortcut layer across the Shell: `Alt+Q` is the sole primary Command Search / Palette trigger everywhere (matching Excel Tell Me; `Ctrl+K` stays strictly Insert/Edit Hyperlink and `Ctrl+Shift+P` stays untouched for Excel Font Size). Place all DNA Calc tool toggles on the `Alt+Shift+<key>` layer: `Alt+Shift+L` = Formula Lens (rejecting `Alt+L`, which collides with Excel's Developer key tip), `Alt+Shift+E` = Docked Formula Explorer (plus Excel-native `Shift+F3` and `Alt,M,V`; rejecting `Ctrl+E` Flash Fill), `Alt+Shift+T` = Trace panel, `Alt+Shift+D` = Details panel, `Alt+Shift+P` = Classic Provenance Overlay, `Alt+Shift+0` / `Ctrl+Alt+0` = companion Classic grid. Inside Formula Explorer's editor, `Ctrl+Enter` runs trial evaluation and `Ctrl+Shift+Enter` or `[Apply to cell]` commits.
- **Not taken (Appendix Z):** Binding `Ctrl+K` or `Ctrl+Shift+P` to Command Palette (P19-CARBON-0054, P19-CARBON-0132, P19-NEON-0128); binding `Alt+L` to Formula Lens (P19-CARBON-0030, P19-CARBON-0054, collides with Alt+L Developer key tip); binding `Ctrl+E` (P19-NEON-0039, Excel Flash Fill) or an extra competing `Ctrl+Shift+E` (P19-CARBON-0035, P19-CARBON-0197) to Formula Explorer — parked in Appendix Z.
- **Reason:** Eliminates every Excel shortcut collision in Classic (D-11) and gives all DNA Calc-specific tools one predictable, teachable modifier layer.

### D-35: Save format downgrades: refuse silent loss on Save; require explicit confirmation on Export

- **Authority:** Decider ruling | **Primary topic:** III. Shell | **Clusters:** `CL-03-07`
- **Options & sources:**
  - **Hard refusal of legacy .xls and macro-losing .xlsx save choices in UI and handlers** [CL-03-07] (P21; `P21-0095`): Block saving to legacy .xls or macro-free .xlsx from a macro/view-bearing workbook in both UI and save handlers.
  - **Explicit informed content-loss confirmation identifying affected macros, formulas, and metadata** [CL-03-07] (P19-GPT; `P19-GPT-0297`): Permit format conversion only after an explicit content-loss choice enumerating every macro, formula, or metadata item that will be dropped.
- **Decision:** Standard Save never silently downgrades `.xlsm` to macro-free `.xlsx`, never drops CustomXML view parts silently, and refuses legacy `.xls` writes. Provide an explicit 'Save a Copy / Export as macro-free `.xlsx`' workflow that requires a dedicated content-loss confirmation listing every dropped macro module and presentation part.
- **Not taken (Appendix Z):** Unconditional hard refusal of macro-free `.xlsx` export with no user export path (P21-0095 strict reading) or silent format conversion without enumerating dropped content — parked in Appendix Z.
- **Reason:** Protects authors against accidental macro stripping on Ctrl+S while still supporting deliberate macro-free sharing.

### D-36: Single non-blocking startup Trust & Notice bar (never stealing Alt+Q or shrinking grid)

- **Authority:** Decider ruling | **Primary topic:** III. Shell | **Clusters:** `CL-03-08`
- **Options & sources:**
  - **Quarantine Bar on Alt+Q plus auto-Triage Strip** [CL-03-08] (P19-CARBON; `P19-CARBON-0027`, `P19-CARBON-0176`): Bind Alt+Q to a dedicated Quarantine Bar and pair it with an auto-Triage Strip linking to Inspector.
  - **Single non-modal Trust & Triage Banner (Neon) without stealing Alt+Q** [CL-03-08] (P19-NEON; `P19-NEON-0025`): Dock one compact non-modal Trust & Triage Banner summarizing sheets, formulas, error roots, missing links, blocked macros, and volatiles.
  - **Single notice line plus auto-opened Overview panel (Fable)** [CL-03-08] (P19-FABLE; `P19-FABLE-0042`): Show one non-blocking notice line and automatically open the docked Overview panel with live 'so far' counts.
- **Decision:** On opening workbooks with quarantined macros, missing external links, manual calculation, or error roots, dock one compact non-blocking Trust & Notice bar with a one-click `[Open Overview / Inspector]` action and an `[≡ N Sheets]` drawer showing hidden/VeryHidden counts. Reach the bar via `F6` region cycling, the Mast trust pill, or Command Search—never `Alt+Q`—and never auto-open a side panel that shrinks the grid on startup.
- **Not taken (Appendix Z):** Binding `Alt+Q` to the Quarantine Bar (P19-CARBON-0027, P19-CARBON-0176, steals Command Search) and auto-opening an Overview side panel on workbook open (P19-FABLE-0042) — parked in Appendix Z.
- **Reason:** Gives immediate safety triage without modal dialogs, shortcut collisions, or lost grid real estate.

### D-37: Frozen calculation snapshot for Print Preview with explicit Refresh and blocked-calc honesty

- **Authority:** Decider ruling | **Primary topic:** III. Shell | **Clusters:** `CL-03-10`
- **Options & sources:**
  - **Frozen calculation-revision snapshot with explicit Refresh and blocked/error honesty** [CL-03-10] (P19-GPT, P21; `P19-GPT-0077`, `P19-GPT-0328`, `P21-0097`): Capture a single completed-result snapshot labelled Current/Previous/Saved + timestamp/filters/missing sources (snapshot ID in print footer only) that never mutates until explicit Refresh; print real formula errors as errors and refuse to disguise blocked/stale results as fresh values.
  - **Live Print Preview overlay synchronized continuously** [CL-03-10] (P19-NEON; `P19-NEON-0085`): Render Print Preview as a live overlay that tracks ongoing model updates automatically.
- **Decision:** Print Preview always captures a single consistent, frozen calculation snapshot (labelled `Current`, `Previous`, or `Saved`, with timestamp, snapshot ID in footers, active filters, and missing sources) that never mutates mid-preview until the user clicks Refresh (showing a quiet 'Model changed — Refresh preview' notice when edits or RTD ticks advance). Real Excel errors print honestly as errors; blocked or incomplete calculations never print stale cached numbers disguised as current output.
- **Not taken (Appendix Z):** Live-mutating Print Preview that changes values mid-print while RTD ticks arrive (P19-NEON-0085) — parked in Appendix Z.
- **Reason:** Ensures printed PDFs and paper briefs are internally consistent snapshots whose provenance matches every number on the page.

### Section: IV. Classic

### D-38: Pinned Microsoft 365 Windows desktop Excel oracle with T1/T2 parity tiering

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-02`
- **Options & sources:**
  - **Flat 100% Appendix A check (Carbon, Neon)** [CL-04-02] (P19-CARBON, P19-NEON; `P19-CARBON-0183`, `P19-NEON-0201`): Require every item in Appendix A to pass before Classic exits beta without release tiers.
  - **Pinned M365 Windows US oracle + T1/T2 tiers + explicit deliberate-differences list (Fable, GPT)** [CL-04-02] (P19-FABLE, P19-GPT; `P19-FABLE-0246`, `P19-FABLE-0433`, `P19-GPT-0095`, `P19-GPT-0107`, `P19-GPT-0245`): Pin a specific Microsoft 365 Windows US desktop Excel build as oracle; ship 100% of T1 as the blocking v1 release bar, track T2 as follow-on parity, and enumerate deliberate Strand differences in Part 12.
- **Decision:** Pin a current Microsoft 365 Windows English/US desktop Excel build as the Classic interaction oracle. Organize Appendix A conformance into T1 (mandatory v1 release bar) and T2 (tracked follow-on parity), maintain a concise Part 12 deliberate-differences register (where every difference is justified by safety or Strand clarity), and include 'Report a difference from Excel' in Command Search.
- **Not taken (Appendix Z):** An undifferentiated flat 100% Appendix A release bar with no T1/T2 staging (P19-CARBON-0183, P19-NEON-0201) — parked in Appendix Z.
- **Reason:** Enables rigorous measurable parity in M1 without stalling release on obscure T2 edge interactions while tracking every remaining gap explicitly.

### D-39: Browser-tab keyboard fallbacks documented in live Shortcut Differences table

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-03`
- **Options & sources:**
  - **Ctrl+Alt+PgUp/PgDn for sheets + Palette for Alt/reserved keys (Fable)** [CL-04-03] (P19-FABLE; `P19-FABLE-0406`, `P19-FABLE-0408`): Use Ctrl+Alt+PageUp/PageDown for sheet tabs in browser tabs and raise key tips and reserved chords via the palette.
  - **Alt+Shift+1..5 + Ctrl+K palette + Keyboard Lock API (Neon)** [CL-04-03] (P19-NEON; `P19-NEON-0109`): Use Keyboard Lock API in installed PWA and dynamically surface Alt+Shift+1..5 and Ctrl+K palette fallbacks in browser tooltips.
- **Decision:** In browser builds where the host captures chords: `F10` activates Key Tips when the browser grabs lone `Alt`, `Ctrl+Alt+PageUp/PageDown` switches sheets when the browser owns `Ctrl+PageUp/PageDown`, `Alt+Q` reaches browser-reserved file/window commands (never hijacking `Ctrl+K`), opportunistic Keyboard Lock is used only in installed PWA mode, and every difference is listed live in the 'Browser shortcut differences' table.
- **Not taken (Appendix Z):** Rebinding `Ctrl+K` in browser tabs to replace `Alt+Q` (P19-NEON-0109) or leaving browser shortcut captures undocumented — parked in Appendix Z.
- **Reason:** Preserves muscle memory across desktop and browser builds with zero silent key-stealing.

### D-40: Classic Provenance Overlay strictly OFF by default (author owns the paper)

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-06`
- **Options & sources:**
  - **Provenance Overlay OFF by default in Classic; paper stays purely author-formatted (Fable)** [CL-04-06] (P19-FABLE; `P19-FABLE-0034`, `P19-FABLE-0434`): Default Classic to zero diagnostic overlays so spreadsheets look crisp and unpolluted; users opt into Provenance Overlay on demand.
  - **Toggleable Provenance Overlay bound to Ctrl+Shift+P (Neon, Carbon)** [CL-04-06] (P19-CARBON, P19-NEON; `P19-CARBON-0184`, `P19-NEON-0020`, `P19-NEON-0189`, `P19-NEON-0203`): Provide Provenance Overlay on Ctrl+Shift+P (with Carbon also showing subtle constant markers on classic screen M-12).
- **Decision:** Keep Provenance Overlay strictly OFF by default in Classic ('the paper belongs to the workbook author', drawing Excel cell formats faithfully). Provide the full non-destructive Provenance Overlay (constants, formula-run breaks, volatile/external/UDF, iterative cells, block frames, and `⊗`/`⊘` error pips) via the View band, `Alt+Q`, and `Alt+Shift+P` (never `Ctrl+Shift+P`, which focuses Excel Font Size).
- **Not taken (Appendix Z):** Showing provenance micro-ticks or badges on Classic cells by default (P19-CARBON-0184, P19-NEON-0020, P19-NEON-0189, P19-NEON-0203) or binding `Ctrl+Shift+P` to Provenance Overlay (collides with Excel Font Size) — parked in Appendix Z.
- **Reason:** Keeps Classic visually indistinguishable in cleanliness from a crisp spreadsheet until the analyst explicitly asks for diagnostic overlays.

### D-41: Desktop Excel parity on Ctrl+F2 (Print Preview) and double-click with in-cell editing off

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-07`, `CL-04-31`
- **Options & sources:**
  - **Double-click jumps to the cell's direct precedents (Fable A-3)** [CL-04-07] (P19-FABLE; `P19-FABLE-0249`): Match actual Windows desktop Excel: when in-cell editing is disabled, double-clicking a formula cell selects its direct precedents (a beloved financial-modelling auditing shortcut).
  - **Double-click focuses the Formula Bar editor (GPT A-30)** [CL-04-07] (P19-GPT; `P19-GPT-0137`): When in-cell editing is off, both F2 and double-click move the caret into the Formula Bar.
  - **Ctrl+F2 focuses/transfers draft to Formula Bar and must not print (GPT A-32)** [CL-04-07] (P19-GPT; `P19-GPT-0139`): Follow Microsoft's web/recent accessibility docs binding Ctrl+F2 to Formula Bar editing.
  - **Keep Ctrl+F2 as Desktop Excel Print Preview; reach Formula Bar via click, F2 (in-cell off), or F6 (Fable)** [CL-04-07] (P19-FABLE; `P19-FABLE-0250`): Preserve desktop Excel for Windows Ctrl+F2 = Print Preview (see P19-FABLE-0197, P19-FABLE-0388) and use click / F2 / F6 / Alt+Q for Formula Bar focus.
  - **Support both Ctrl+P and Ctrl+F2 for Print Preview (Fable)** [CL-04-31] (P19-FABLE; `P19-FABLE-0197`): Preserve Windows desktop Excel's Ctrl+F2 Print Preview shortcut alongside Ctrl+P.
  - **Ctrl+P only for Print Preview; repurpose Ctrl+F2 to Formula Bar (GPT)** [CL-04-31] (P19-GPT; `P19-GPT-0210`): Restrict Print Preview to Ctrl+P because P19-GPT-0107 / P19-GPT-0139 follow Microsoft web/accessibility docs mapping Ctrl+F2 to Formula Bar.
- **Decision:** Follow the pinned Windows desktop Excel oracle: `Ctrl+F2` opens Print Preview alongside `Ctrl+P` (reach the Formula Bar via click, `F2` when in-cell editing is off or cell is off-screen, `F6`, or `Alt+Q`). When 'Allow editing directly in cells' is disabled, `F2` focuses the Formula Bar while double-clicking a formula cell selects its direct precedents.
- **Not taken (Appendix Z):** Rebinding `Ctrl+F2` to Formula Bar focus from Excel-for-the-Web documentation (P19-GPT-0139, P19-GPT-0107 exception) and routing double-click to Formula Bar instead of direct precedents when in-cell editing is off (P19-GPT-0137) — parked in Appendix Z.
- **Reason:** Matches desktop Windows Excel muscle memory consistently (D-11, D-38).

### D-42: Editing-only 5-step cool Formula Reference Palette reserving amber strictly for active caret

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-09`
- **Options & sources:**
  - **4-step cool structural ramp + geometric corner notches; amber kept strictly off references (Carbon)** [CL-04-09] (P19-CARBON; `P19-CARBON-0016`, `P19-CARBON-0043`): Use teal-600, sage-500, petrol-500, slate-600 paired with 4 geometric corner notches (solid, double-tick, long-dash, chamfered pill), reserving both red and amber.
  - **Teal family + superscript numbers, turning the reference at the caret amber (Fable M8.1, D-3, R-24)** [CL-04-09] (P19-FABLE; `P19-FABLE-0144`, `P19-FABLE-0396`, `P19-FABLE-0436`): Draw passive references in calm teal tones with superscript numbers, and switch whichever reference touches the text caret to amber.
  - **6-tone editing-only palette including amber-ink as a regular slot (Neon)** [CL-04-09] (P19-NEON; `P19-NEON-0017`, `P19-NEON-0202`): Cycle 6 WCAG-checked accents (teal, blue/const, green, amber, slate, plum — excluding red) paired with circled numbers ①②③④.
- **Decision:** During formula editing (`F2`), colour referenced ranges with a 5-step cool non-red structural ramp (`teal-ink`, `petrol-500`, `green-ink`/`sage-500`, `slate-ink`, `plum-ink`) paired with circled numbers `①②③④⑤` and distinct border/corner-notch patterns (colorblind-proof), and reserve **amber** strictly for the SINGLE reference currently touched by the editing caret—never spending amber as a passive palette slot.
- **Not taken (Appendix Z):** Spending amber as a passive reference slot (P19-NEON-0017, P19-NEON-0202) or omitting the caret-active amber highlight (P19-CARBON-0016, P19-CARBON-0043) — parked in Appendix Z.
- **Reason:** Preserves Strand's rule that amber signals live focus/attention while giving multi-reference formulas unmistakable colorblind-safe distinction.

### D-43: F9 in-formula sub-expression evaluation with non-blocking warning and Ctrl+Z un-bake

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-12`
- **Options & sources:**
  - **Reversible preview pill with Ctrl+Z un-baking back to AST (Carbon)** [CL-04-12] (P19-CARBON; `P19-CARBON-0149`): Show evaluated literal inside a reversible preview pill where Ctrl+Z un-bakes just the F9 evaluation, Esc cancels the full edit, and Enter commits.
  - **In-place literal replacement with 'Draft contains evaluated values' notice (Fable, GPT, Neon)** [CL-04-12] (P19-FABLE, P19-GPT, P19-NEON; `P19-FABLE-0276`, `P19-GPT-0153`, `P19-NEON-0154`): Replace the highlighted expression inline with its literal result (Esc reverts, Enter commits); GPT adds an explicit 'Draft contains evaluated values' banner so users do not accidentally bake constants.
- **Decision:** Pressing `F9` over a highlighted formula sub-expression replaces it inline with its literal value (matching Excel; `Enter` commits, `Esc` cancels), shows a non-blocking `'Draft contains evaluated values · Esc to revert / Ctrl+Z to un-bake'` notice, and supports editor-level `Ctrl+Z` to un-bake just the `F9` evaluation back to the prior expression.
- **Not taken (Appendix Z):** Replacing `F9` literal evaluation with a purely read-only hover pill that breaks Excel's bake-value workflow (P19-CARBON-0149) — parked in Appendix Z.
- **Reason:** Delivers 100% Excel `F9` parity while eliminating the classic analyst mistake of accidentally pressing Enter and permanently hardcoding a formula.

### D-44: Classic live tick and Form Control ScrollBar bounded recalculation

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-12`
- **Options & sources:**
  - **120 fps Form Control ScrollBar syncing + 140 ms topological recalc ripple (Carbon, Neon)** [CL-04-12] (P19-CARBON, P19-NEON; `P19-CARBON-0039`, `P19-NEON-0035`): Sync ScrollBars continuously at 120 fps and animate a 140 ms amber topological ripple on tick updates.
  - **Honest bounded iteration status and explicit commit semantics (Pro21, paired with Fable D-10)** [CL-04-12] (P21; `P21-0051`): Display exact Excel iteration parameters, never invent iteration stops, and keep Classic recalculation disciplined and predictable.
- **Decision:** In Classic, show exact Excel iteration settings (max iterations, tolerance) with no fabricated stop counts; render Form Control ScrollBars with ghost-value feedback while dragging and commit-on-release by default (allowing continuous scrubbing only when workbook recalc fits within `<16 ms`); allow an optional subtle 140 ms flash on RTD/tick advances that snaps off under `prefers-reduced-motion`.
- **Not taken (Appendix Z):** Unbounded continuous 120 fps Form Control recalculation syncing (P19-CARBON-0039, P19-NEON-0035) — parked in Appendix Z.
- **Reason:** Prevents input lag and dropped frames on real-world workbooks while honoring Excel's commit-on-release ScrollBar semantics.

### D-45: Non-modal inline #SPILL! card with explicit undoable remediation actions

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-14`
- **Options & sources:**
  - **Ghost spill outline + inline card with Go to blocker / Clear blocker / Move formula (Fable M6.3, D-6)** [CL-04-14] (P19-FABLE; `P19-FABLE-0102`, `P19-FABLE-0399`): Draw the needed spill extent as a ghost and show an inline cell card naming the exact blocker cell with three explicit one-click actions (Go to blocker, Clear blocker, Move formula down).
  - **Keep #SPILL! at anchor and require explicit undoable worksheet edit (GPT A-53)** [CL-04-14] (P19-GPT; `P19-GPT-0160`): Reveal obstruction cells without ever auto-repairing; clearing a blocker must be an explicit user worksheet edit with Undo.
- **Decision:** Render the ghost spill extent and exact obstruction highlight on `#SPILL!`, anchor a non-modal inline `#SPILL!` card at the anchor cell naming the blocker cell and content, and offer `'Go to blocker'`, `'Clear blocker'`, and `'Move formula'` strictly as explicit user-triggered, previewed, single-step Undoable edits (never automatic repair).
- **Not taken (Appendix Z):** Omitting inline blocker remediation actions (P19-GPT-0160 strictly manual-only) or clearing blockers automatically without confirmation — parked in Appendix Z.
- **Reason:** Saves tedious manual chasing on wide sheets while strictly respecting explicit user-confirmed model edits.

### D-46: Non-modal Find & Replace docked in top Bridge with full Excel Options drawer

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-17`
- **Options & sources:**
  - **Non-modal docked Find/Replace bar in Bridge with expandable Options drawer (Neon)** [CL-04-17] (P19-NEON; `P19-NEON-0188`): Dock Find/Replace non-modally in the top Bridge so it never obscures target cells, expanding smoothly for full options and Find All.
  - **Full Excel Find/Replace option parity (Fable, GPT)** [CL-04-17] (P19-FABLE, P19-GPT; `P19-FABLE-0316`, `P19-GPT-0190`): Expose every Excel Find/Replace option (Selection/Sheet/Workbook, Rows/Cols, Formulas/Values/Notes/Comments, Formats, wildcards, Alt mnemonics).
- **Decision:** Dock Find & Replace as a compact non-modal bar in the top Bridge (never floating over the active cell) with an expandable Options drawer (`Alt+T`) and collapsible `Find All` results list exposing 100% of Excel's Find/Replace fields and Alt-mnemonics.
- **Not taken (Appendix Z):** Stripping Excel Find/Replace options into a minimal search-only bar (P19-NEON-0188 minimal variant) or floating a modal dialog that obscures the matched cell — parked in Appendix Z.
- **Reason:** Eliminates modal occlusion while retaining full Excel Find/Replace power and keyboard mnemonics.

### D-47: Excel-observed double-click Fill Handle rules and border-drag overwrite preference

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-18`, `CL-04-19`
- **Options & sources:**
  - **Left-neighbor-first, else right-neighbor, stop at first blank / total row (Carbon, Fable, Neon)** [CL-04-18] (P19-CARBON, P19-FABLE, P19-NEON; `P19-CARBON-0168`, `P19-FABLE-0344`, `P19-NEON-0167`): Fill downward to the bottom of the adjacent left column (falling back to right), stopping at the first blank gap or Table total row, and show a 1-line Strip toast ('Filled 360 rows · Ctrl+Z').
  - **Observed Excel baseline fixtures across all edge cases (GPT A-64)** [CL-04-18] (P19-GPT; `P19-GPT-0171`): Refuse to rely on a single hand-invented stopping rule; verify blank gaps, unequal neighbors, existing non-empty target cells, and filtered rows against observed Microsoft 365 Excel baseline fixtures.
  - **Always prompt/warn before overwriting non-empty target cells (Carbon, Fable A-105)** [CL-04-19] (P19-CARBON, P19-FABLE; `P19-CARBON-0169`, `P19-FABLE-0351`): Ask/warn before replacing occupied cells on every border drop.
  - **Honour Excel's overwrite-warning preference while always highlighting occupied cells in live drag preview (GPT A-69, Neon)** [CL-04-19] (P19-GPT, P19-NEON; `P19-GPT-0176`, `P19-NEON-0169`): Always show occupied destination cells visually in the Preview Seam during drag, and honour Excel's 'Alert before overwriting cells' user setting (ON by default) for the confirmation prompt.
- **Decision:** Verify double-click Fill Handle stopping rules against an observed Microsoft 365 Excel fixture matrix (continuous runs, blank gaps, unequal left/right neighbors, existing targets, Table totals, filtered lists) and show a non-blocking `'Filled N rows -> range · Ctrl+Z'` readout. On border drag-move, always highlight occupied destination cells in the live Preview Seam and honor Excel's `'Alert before overwriting cells'` preference (ON by default).
- **Not taken (Appendix Z):** Hardcoded left-then-right fill-stopping heuristic without Excel fixture verification (P19-CARBON-0168, P19-FABLE-0344, P19-NEON-0167) and unconditional blocking overwrite dialogs ignoring Excel's preference (P19-CARBON-0169, P19-FABLE-0351) — parked in Appendix Z.
- **Reason:** Matches observed Microsoft 365 Excel behavior while giving clear non-blocking visibility into multi-row fills and overwrites.

### D-48: Classic 10%-400% worksheet zoom range with Semantic Zoom block labels below 40%

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-23`, `CL-11-07`
- **Options & sources:**
  - **25%-250% zoom with structural minimap LOD below 70% (Carbon)** [CL-04-23] (P19-CARBON; `P19-CARBON-0171`): Cap worksheet zoom at 25%-250% and replace grid cells with a structural minimap below 70%.
  - **Excel 10%-400% zoom with block/range-name labels below 40% (Fable A-74, D-9; GPT A-119)** [CL-04-23] (P19-FABLE, P19-GPT; `P19-FABLE-0320`, `P19-FABLE-0402`, `P19-GPT-0226`): Match Excel's full 10%-400% zoom range (essential for wide financial models and low-vision users) and swap cell text for legible block/named-range labels below 40% (matching Excel's 39% named-range overlay).
  - **Option A: Switch to block outlines below 40% zoom** [CL-11-07] (P19-FABLE; `P19-FABLE-0051`, `P19-FABLE-0166`): Keep real cell text visible down to 40% zoom (matching typical Excel zoom-out workflows on wide sheets) and switch to legible block outlines/labels below 40%.
  - **Option B: Switch below 55% (Neon) or 70% (Carbon)** [CL-11-07] (P19-CARBON, P19-NEON; `P19-NEON-0064`, `P19-CARBON-0074`): Swap cell text for L1/L2 block headers below 55% zoom (Neon) or Semantic Minimap below 70% zoom (Carbon).
- **Decision:** Support Excel's full `10%–400%` worksheet zoom range (kept strictly independent of shell/UI zoom) and switch to legible Semantic Zoom block/range-name labels below `40%` zoom (or whenever rendered cell text drops below 6–7 px, matching Excel's 39% defined-name threshold).
- **Not taken (Appendix Z):** Restricting worksheet zoom to `25%–250%` or triggering Semantic Zoom at `55%–70%` (P19-CARBON-0074, P19-CARBON-0171, P19-NEON-0064) — parked in Appendix Z.
- **Reason:** Analysts routinely work at 60%–75% zoom on wide financial sheets and expect real readable numbers there; switching below 40% honors Strand Tenet 8 right where text turns illegible.

### D-49: Form Control non-operating selection supports Ctrl+click, right-click, and Selection panel

- **Authority:** Decider ruling | **Primary topic:** IV. Classic | **Clusters:** `CL-04-28`
- **Options & sources:**
  - **Ctrl+click selects a Form Control without operating it (Fable A-91)** [CL-04-28] (P19-FABLE; `P19-FABLE-0337`): Match Excel's Ctrl+click gesture to select a Form Control directly.
  - **Right-click or Selection panel selects a Form Control without firing (GPT A-115)** [CL-04-28] (P19-GPT; `P19-GPT-0222`): Use right-click or the Selection panel to select a control with clear handles distinct from a focused slider thumb.
- **Decision:** Support all three Excel-standard non-operating selection paths for Form Controls seamlessly: `Ctrl+click`, right-click (selecting + opening context menu), and the Selection panel all select a control with editing handles without invoking its action or macro.
- **Not taken (Appendix Z):** Restricting Form Control edit selection to only one of Ctrl+click or right-click — both partial specifications merged cleanly.
- **Reason:** Zero-cost full Excel parity across every habit users bring from desktop Excel.

### Section: V. Inspector

### D-50: Dockable Inspector instrument panels enlarging to a 5-tab full-stage Inspector

- **Authority:** Decider ruling | **Primary topic:** V. Inspector | **Clusters:** `CL-05-01`
- **Options & sources:**
  - **Option A: Dockable instrument panels enlarging to full-stage Inspector** [CL-05-01] (FRAMING, P19-FABLE; `FRAMING-0066`, `P19-FABLE-0068`, `P19-FABLE-0218`, `P19-FABLE-0477`): Every Inspector instrument docks as a resizable side or bottom panel inside Classic, Notebook, Map, and App, and enlarges into the full-stage Inspector view with its exact state preserved.
  - **Option B: Translucent Alt+I X-Ray overlay on top of the active view** [CL-05-01] (P19-CARBON; `P19-CARBON-0111`): Promote Root-Error Trace, Discrepancy Waterfall, and Volatility Halo into a toggleable Alt+I X-Ray overlay painted directly over Classic, Notebook, and Model Map.
  - **Option A: Focused 4-tab probe bench (Precedents, Dependents, Calculation, Live) plus Overview entry** [CL-05-01] (P19-GPT, P21; `P19-GPT-0041`, `P21-0063`): Organize Inspector into focused tabs (Precedents, Dependents, Calculation, Live) with pinned Watch signals, Copy evidence, and whole-workbook Overview counts as the workbook-level entry state.
  - **Option B: Single dense multi-pane dashboard** [CL-05-01] (P19-CARBON, P19-NEON; `P19-CARBON-0058`, `P19-NEON-0051`): Display Workbook Summary, Causal Graph, Calculation Cost Meter, Hygiene Scanner, Live/External Inventory, and an inline 5-row Cell Peek simultaneously on one full-screen canvas.
- **Decision:** Build every Inspector instrument first as a dockable panel usable inside every other view, enlarging cleanly into full-stage Inspector (`Ctrl+Alt+2`) with state preserved. Standardise full-stage Inspector on 5 focused tabs—`Overview`, `Precedents`, `Dependents`, `Calculation`, `Live`—plus a collapsible pinned Watch strip and on-demand inline cell peek.
- **Not taken (Appendix Z):** Carbon's separate floating `Alt+I` X-Ray Overlay and fixed 5-pane dashboard (P19-CARBON-0058, P19-CARBON-0111) and Neon's 7-instrument top-level split (P19-NEON-0051) — folded cleanly into the 5-tab dockable architecture and parked in Appendix Z.
- **Reason:** Honours the four-answer consensus (FRAMING-0066: Inspector tools are panels in every view) while avoiding duplicate floating overlay surfaces.

### D-51: Count-first Inspector Overview triage with on-demand sheet-tier Dependency Atlas

- **Authority:** Decider ruling | **Primary topic:** V. Inspector | **Clusters:** `CL-05-02`
- **Options & sources:**
  - **Option A: Count-first Overview triage list with optional Sheet-level Atlas view** [CL-05-02] (P19-CARBON, P19-GPT; `P19-GPT-0014`, `P19-CARBON-0028`): Open Overview on structured counts (error roots, warnings, hidden sheets, external links, volatile fan-out, block summaries) and offer a sheet-tier Atlas view on demand without rendering a graph by default.
  - **Option B: Always render a 4-tier graphical Workbook Atlas (L0 Atlas) on entry** [CL-05-02] (P19-NEON; `P19-NEON-0026`, `P19-NEON-0205`): Default Inspector Overview to a 4-tier topological Workbook Atlas diagram showing all 40 worksheets including hidden/veryHidden nodes.
- **Decision:** Open Inspector `Overview` on fast, keyboard-accessible count-first triage tables (error roots, structural anomalies, hidden/VeryHidden sheets, external links, VBA modules, volatile fan-out), with a one-click toggle to a sheet-tier Dependency Atlas capped at sheet/block granularity (refusing raw cell-level graph expansion on large workbooks).
- **Not taken (Appendix Z):** Launching Inspector directly into a graphical L0 Sheet Atlas canvas before count-first triage tables (P19-NEON-0026, P19-NEON-0205) — made secondary opt-in tab/toggle.
- **Reason:** Count-first triage scales cleanly to 300k-formula workbooks like Inherited, loads immediately, and is directly screen-reader friendly.

### D-52: Inspector repair preview defaults to single cell with opt-in sibling-run batch toggle

- **Authority:** Decider ruling | **Primary topic:** V. Inspector | **Clusters:** `CL-05-07`
- **Options & sources:**
  - **Option A: Exact one-formula repair by default, with explicit opt-in sibling-run batch toggle** [CL-05-07] (P21; `P21-0055`): Default the preview to the exact single inspected cell (`1 formula changed`); when identical relative-run siblings carry the same defect, offer an explicit opt-in checkbox to include those siblings in the same preview and atomic Undo step.
  - **Option B: Automatically bundle sibling cells into the primary Apply action** [CL-05-07] (P19-NEON; `P19-NEON-0032`): Detect matching sibling cells in the same run (e.g. `Report!D6:D9`) and propose fixing all of them together by default.
- **Decision:** Default every Inspector repair preview to the single inspected cell (`1 formula changed`). When identical relative-run siblings share the exact defect, display an unchecked opt-in control (`Also apply to N matching cells in run`) that live-updates the preview diff and commits atomically under one Undo step.
- **Not taken (Appendix Z):** Defaulting Inspector repairs to batch-mutating an entire sibling run before user opt-in (P19-NEON-0032) — parked in Appendix Z.
- **Reason:** Prevents unexpected multi-row mutations during focused single-cell debugging while keeping bulk run repair one explicit click away.

### D-53: Three-layer Calculation tab honesty: Dependency waves, Value diff, and optional DNA Calc's calculation order

- **Authority:** Decider ruling | **Primary topic:** V. Inspector | **Clusters:** `CL-05-08`
- **Options & sources:**
  - **Option A: Strict 3-layer separation with optional 'DNA Calc's calculation order'** [CL-05-08] (FRAMING, P19-GPT; `FRAMING-0135`, `P19-GPT-0089`, `P19-GPT-0097`, `P19-GPT-0240`, `P19-GPT-0319`): Separate (1) Dependency explanation (topological waves/SCCs), (2) Value delta (before→after), and (3) optional Observed execution—shown only when genuinely recorded, preserving parallel/concurrent bands, strictly labelled 'DNA Calc's calculation order' (never Excel's), with honest 'Not observed' state otherwise.
  - **Option B: Always show an exact serial evaluation-order list or animate topological waves as execution** [CL-05-08] (P19-CARBON, P19-FABLE, P19-NEON; `P19-FABLE-0145`, `P19-FABLE-0476`, `P19-CARBON-0210`, `P19-NEON-0208`, `P19-NEON-0250`): List the last recalculation in exact serial evaluation order by default or present topological dependency waves as the workbook's execution order.
- **Decision:** Inspector's `Calculation` tab strictly separates three evidence layers: (1) **Dependency waves** (labelled 'Dependency explanation', topological waves & SCCs), (2) **Value changes** (before->after diff), and (3) optional **DNA Calc's calculation order** (recorded only when enabled, preserving concurrent bands, never presented as Excel's execution order, and displaying `Not observed` when tracing is off or unavailable).
- **Not taken (Appendix Z):** Presenting serial evaluation logs or dependency animations as Excel's execution recording (P19-FABLE-0145, P19-FABLE-0476, P19-CARBON-0210, P19-NEON-0208, P19-NEON-0250 where not separated) — rejected per owner-accepted Push-back #2 (FRAMING-0068, FRAMING-0135, P19-GPT-0089).
- **Reason:** Enforces honesty rule #1 (provenance before value) and owner-accepted Pro 20 refinement FRAMING-0135.

### D-54: Inspector print defaults to active instrument with one-click Full Workbook Audit Dossier

- **Authority:** Decider ruling | **Primary topic:** V. Inspector | **Clusters:** `CL-05-12`
- **Options & sources:**
  - **Option A: Active instrument report by default, with Full Workbook Audit Dossier option** [CL-05-12] (P19-FABLE; `P19-FABLE-0198`): Ctrl+P prints the currently open instrument (e.g. active Trace outline or Errors by root) with honesty header, and exposes a one-click 'Full Workbook Audit Dossier' scope selector in the Print/Export dialog.
  - **Option B: Always print the complete multi-section Workbook Audit Dossier** [CL-05-12] (P19-CARBON, P19-NEON; `P19-CARBON-0095`, `P19-NEON-0087`, `P19-NEON-0209`): Always generate a full executive health summary, root-error table, anomalies, missing links, volatile inventory, and active trace.
- **Decision:** Default `Ctrl+P` in Inspector to printing the active diagnostic instrument (stamped with file hash, timestamp, and `Clean / Cached / Dirty` state), with `'Full Workbook Audit Dossier'` selectable in one click inside the same Print/Export sheet.
- **Not taken (Appendix Z):** Defaulting `Ctrl+P` to a multi-page Full Workbook Audit Dossier on every quick print (P19-CARBON-0095, P19-NEON-0087, P19-NEON-0209) — kept as a one-click option.
- **Reason:** Avoids printing dozens of pages when an analyst only wants a 1-page trace or root-error summary, while keeping full dossiers one click away.

### Section: VI. Notebook

### D-55: Non-mutating Notebook First-Run Card with explicit scope selection

- **Authority:** Decider ruling | **Primary topic:** VI. Notebook | **Clusters:** `CL-06-02`
- **Options & sources:**
  - **Option A: Explicit First-Run Card (Start with selection / Draft from workbook / Blank)** [CL-06-02] (P19-FABLE, P19-GPT, P19-NEON, P21; `P19-FABLE-0053`, `P19-FABLE-0455`, `P19-GPT-0015`, `P19-NEON-0028`, `P19-NEON-0210`, `P21-0036`): When no saved Notebook exists, show a non-mutating choice card offering (1) Start from current selection (e.g. 'Add selected result'), (2) Draft computation chain from workbook (<400 ms on small models, subset picker on huge models), or (3) Blank notebook.
  - **Option B: Auto-synthesise full outline immediately on first switch** [CL-06-02] (P19-CARBON; `P19-CARBON-0031`, `P19-CARBON-0059`): Immediately auto-arrange Inputs, Calculations, and Summaries on the first visit to Notebook.
- **Decision:** When opening Notebook on a workbook with no saved notebook layout, display a lightweight non-mutating First-Run Card offering three direct actions—**Start from current selection**, **Draft from workbook structure** (with 'Why these?' explanation and large-workbook subset guardrail), and **Blank notebook**—keeping the workbook 'saved' until the user explicitly confirms.
- **Not taken (Appendix Z):** Silently auto-synthesising a whole-workbook outline on first visit (P19-CARBON-0031, P19-CARBON-0059) — parked in Appendix Z.
- **Reason:** Prevents noisy auto-generated notebooks on large legacy files and keeps dirty state honest.

### D-56: Single-anchor dynamic-array cells as Notebook authoring default; atomic run cards for imported Excel runs

- **Authority:** Decider ruling | **Primary topic:** VI. Notebook | **Clusters:** `CL-06-04`
- **Options & sources:**
  - **Option A: Dynamic-array single-anchor cells for new authoring, plus linked formula-run cards for existing multi-cell rows** [CL-06-04] (FRAMING, P21; `FRAMING-0146`, `P21-0022`, `P21-0025`): Use single-cell dynamic arrays (`FILTER`, `GROUPBY`, `PIVOTBY`, `SORTBY`) as the native Notebook computation cell, while allowing existing Excel multi-cell formula runs (e.g. `Monthly!C8:N8`) to appear as a unified run card edited atomically through a dotted preview seam.
  - **Option B: Model every Notebook formula block primarily as a multi-cell relative formula run** [CL-06-04] (P19-FABLE; `P19-FABLE-0057`, `P19-FABLE-0062`): Treat row-wise copied formula runs (like 12 monthly columns) as the primary Notebook formula item.
- **Decision:** New computation cells authored in Notebook default to single-anchor dynamic-array formulas (clean Jupyter input->output chain with explicit spill headroom on the backing sheet). Multi-cell relative formula runs linked from existing Excel models render as unified formula-run cards whose edits pass through the Preview Seam as one atomic Undo step (preserving exceptions per D-26).
- **Not taken (Appendix Z):** Authoring new Notebook steps as multi-cell copied relative formulas by default (P19-FABLE-0057, P19-FABLE-0062) — reserved strictly for existing imported runs.
- **Reason:** Dynamic arrays map 1:1 to Jupyter's 'one formula -> one spilled result' mental model while remaining 100% native Excel 365.

### D-57: Notebook Data Frame sort/filter defaults to View-Only with explicit reviewed Table-mutation action

- **Authority:** Decider ruling | **Primary topic:** VI. Notebook | **Clusters:** `CL-06-06`
- **Options & sources:**
  - **Option A: Default View-only sort/filter, plus explicit reviewed 'Changes the Table' action** [CL-06-06] (FRAMING, P21; `FRAMING-0139`, `P21-0040`): Clicking a column header or filter defaults to explicitly labelled 'View only' (leaving Excel Table filter/row order untouched), while offering a clearly segregated 'Changes the Table' path checked by an explicit review/Apply preview and full Undo.
  - **Option B: View-only sort/filter exclusively inside Notebook** [CL-06-06] (P19-FABLE, P19-GPT; `P19-FABLE-0058`, `P19-GPT-0258`): Restrict Notebook Data Frame sorting and filtering strictly to display-only presentation; require switching to Classic to alter Excel Table filters or row order.
- **Decision:** Default all Notebook Data Frame sorts and filters to explicitly badged **View only** (never mutating worksheet row order or breaking positional references), and offer an opt-in **Changes the Table…** menu action that previews affected row addresses and commits with full Undo.
- **Not taken (Appendix Z):** Mutating the underlying Excel Table directly on every Data Frame sort/filter click (P19-FABLE-0058, P19-GPT-0258) — moved behind the explicit 'Changes the Table…' menu.
- **Reason:** Honours Pro 20 Change 4 (FRAMING-0139) and protects positional Excel formulas from silent row-reordering breakage during exploratory analysis.

### D-58: Standardise Notebook flow arrangement names on Vertical and Pipeline (Alt+P)

- **Authority:** Decider ruling | **Primary topic:** VI. Notebook | **Clusters:** `CL-06-10`
- **Options & sources:**
  - **Option A: 'Pipeline' (toggle Alt+P; Vertical ↔ Pipeline)** [CL-06-10] (P19-CARBON, P19-NEON, P21; `P19-CARBON-0060`, `P19-CARBON-0190`, `P19-NEON-0053`, `P19-NEON-0213`, `P21-0042`): Name the horizontal multi-stage layout 'Pipeline' with keyboard toggle Alt+P.
  - **Option B: 'Across' (Page ↔ Across)** [CL-06-10] (P19-FABLE; `P19-FABLE-0146`): Name the vertical/horizontal layout pair 'Page' and 'Across'.
- **Decision:** Name the two Notebook flow arrangements **Vertical** (default Jupyter chain) and **Pipeline** (horizontal cross-sheet stages, toggled via `Alt+P`), storing arrangement strictly as presentation metadata in CustomXML.
- **Not taken (Appendix Z):** Naming the horizontal arrangement 'Across' (P19-FABLE-0146) — retired to Appendix Z for terminology unity.
- **Reason:** Matches the owner's own wording ('horizontal and multi-sheet pipelines' in SKIN_DISCUSSION.md #1) and four of five sources.

### Section: VII. Model map

### D-59: Map-created boxes default to workbook-scoped dotted visible defined names backed by sheet cells

- **Authority:** Decider ruling | **Primary topic:** VII. Model map | **Clusters:** `CL-07-02`
- **Options & sources:**
  - **Workbook-scoped dotted names by default (sheet-scoped only for template sheets)** [CL-07-02] (P19-CARBON; `P19-CARBON-0036`, `P19-CARBON-0118`): Allocate workbook-scoped dotted names (`Store.Stock`) on a structured backing sheet so cross-sheet references work cleanly without sheet qualifiers; use sheet scope only when duplicating a template sheet.
  - **User-named backing sheet with explicit placement override in Preview Seam** [CL-07-02] (P19-FABLE; `P19-FABLE-0074`, `P19-FABLE-0234`): Place label and value cells in two columns on a user-named visible sheet, surfacing a 'place it elsewhere…' option in the Preview Seam without assuming a single scope.
  - **Additional governing / aligned cards across topics** (P19-CARBON): `P19-CARBON-0112`
- **Decision:** Boxes created on the Model map default to workbook-scoped dotted defined names backed by two-column (label + value) cells on the active visible sheet (never hidden names per D-13, never pure Name-Manager ghost workbooks), with an inline Preview Seam option to pick another target sheet/cell or sheet-scoped names for repeated template sheets. Still render existing pure-name workbooks read-only cleanly.
- **Not taken (Appendix Z):** Creating pure Name-Manager formula-only boxes with blank sheets by default (rejected per Carbon Push-back P19-CARBON-0112 and four-answer consensus FRAMING-0065) — parked in Appendix Z.
- **Reason:** Guarantees every map-authored model opens cleanly as a legible spreadsheet in desktop Excel.

### D-60: Typeable, speakable self-reference qualifiers in Presented view (no Unicode superscripts in formula text)

- **Authority:** Decider ruling | **Primary topic:** VII. Model map | **Clusters:** `CL-07-03`
- **Options & sources:**
  - **Superscript minus notation (`OnHand⁻`) in Presented mode** [CL-07-03] (P19-NEON; `P19-NEON-0215`): Render self-references using compact superscript `⁻` notation inside composite boxes, mapping back via AST.
  - **Real Excel formula via Source toggle with explicit plain/speakable labels** [CL-07-03] (P21; `P21-0065`): Display actual source-bound ports and real Excel formulas via Source toggle without inventing non-typeable superscript glyphs.
- **Decision:** Show exact Excel syntax on the Source toggle and typeable, screen-reader-speakable qualifiers (e.g., `Stock[prev]` or explicit `Prev.Stock` cell names) in Presented view.
- **Not taken (Appendix Z):** Non-typeable Unicode superscripts like `OnHand⁻` inside formula text (P19-NEON-0215) — parked in Appendix Z.
- **Reason:** Formula expressions must be typeable on an ordinary keyboard and unambiguous to screen readers.

### D-61: Model map canvas wheel scrolls/pans by default; Ctrl+Wheel zooms

- **Authority:** Decider ruling | **Primary topic:** VII. Model map | **Clusters:** `CL-07-04`
- **Options & sources:**
  - **Mouse wheel scrolls canvas; Ctrl+wheel / explicit controls zoom** [CL-07-04] (P19-GPT; `P19-GPT-0043`): Preserve standard document/spreadsheet scrolling on plain mouse wheel and reserve zoom for Ctrl+wheel, +/- keys, and explicit Zoom/Fit controls.
  - **Semantic zoom canvas with sheet-page tabs** [CL-07-04] (P19-NEON; `P19-NEON-0216`): Treat map navigation as nested composite drill-in plus optional sheet pages.
- **Decision:** On the Model map canvas, plain mouse wheel scrolls/pans (consistent with Classic and Notebook); `Ctrl+Wheel`, `+`/`-`, and toolbar Zoom/Fit execute semantic zoom, paired with `Enter`/`Esc` composite drill-down and optional sheet-page tabs.
- **Not taken (Appendix Z):** Plain mouse wheel zooming directly on the map canvas (P19-NEON-0216) — parked in Appendix Z.
- **Reason:** Keeps wheel behavior consistent across every view and prevents accidental zoom storms on trackpads.

### D-62: Three-stage Model map authoring rollout and zero-silent-zero wire deletion

- **Authority:** Decider ruling | **Primary topic:** VII. Model map | **Clusters:** `CL-07-05`
- **Options & sources:**
  - **Stage 1 reader -> Stage 2 named-port rewiring only (refuse raw positional A1 blocks)** [CL-07-05] (FRAMING, P19-FABLE, P19-NEON; `FRAMING-0065`, `P19-FABLE-0220`, `P19-NEON-0104`): Ship Model map as a reader first; restrict drag-to-wire formula edits strictly to explicit named-actor ports and existing unambiguous reference replacements so relative/mixed `$` anchors on positional A1 blocks are never corrupted.
  - **Operand-targeted rewiring on specific cell formulas with F2 candidate disambiguation** [CL-07-05] (P19-GPT, P21; `P19-GPT-0021`, `P19-GPT-0260`, `P21-0067`): Allow wiring a port onto a specific operand of a cell formula (e.g., `Stock!H15`) via an explicit before/after formula Preview Seam, refusing ambiguous aggregated/multi-operand edges by opening operand selection.
  - **Require explicit replacement expression/edit; never silently substitute zero** [CL-07-05] (P19-GPT; `P19-GPT-0021`, `P19-GPT-0261`): Deleting a wire opens an explicit formula edit requiring the user to choose a valid replacement (or edit the formula), refusing silent substitution of `0`.
  - **Rewiring replaces one reference with another directly** [CL-07-05] (P19-FABLE; `P19-FABLE-0078`): Drag-rewiring swaps an existing reference for a new reference with before/after preview.
- **Decision:** Ship Model map in three stages: (1) architectural reader (levels, zoom, chips, coverage accounting, Connections list, click-to-inspect), (2) unambiguous rewiring on named ports and single-cell explicit operand targets via Preview Seam (refusing multi-row positional A1 runs and opening F2 operand selection when multiple slots exist), and (3) blank-paper `name = formula` authoring. Unwiring a simple passthrough input port (`MyBox.In = Other.Out`) previews clearing to empty or restoring its prior literal; deleting a wire embedded inside a compound formula ALWAYS requires an explicit user-confirmed replacement edit and NEVER silently substitutes `0`.
- **Not taken (Appendix Z):** Immediate unconstrained visual wiring on positional A1 ranges (rejected per FRAMING-0065, P19-FABLE-0220, P19-NEON-0104) and silently substituting `0` on compound-formula wire deletion (P19-GPT-0021, P19-GPT-0261) — parked in Appendix Z.
- **Reason:** Prevents formula corruption and silent arithmetic distortion while still enabling productive named-port and single-operand rewiring.

### D-63: Plain Excel-terms calculation chips (retire 'Director' jargon) and unified Inspector Live plotter

- **Authority:** Decider ruling | **Primary topic:** VII. Model map | **Clusters:** `CL-07-09`
- **Options & sources:**
  - **Ptolemy-style 'Director' bar (`Recalc`, `Ticks`, `Iteration`, `Sweep`, `Events`)** [CL-07-09] (P19-CARBON, P19-NEON; `P19-CARBON-0038`, `P19-CARBON-0061`, `P19-CARBON-0062`, `P19-NEON-0054`, `P19-NEON-0055`, `P19-NEON-0217`): Expose a dedicated Director bar naming five simulation regimes mapped onto Excel mechanisms.
  - **Plain Excel-terms calculation chips (`Auto`, `Iterates · Max 100`, `RTD clock`, `Data table`)** [CL-07-09] (P19-FABLE; `P19-FABLE-0159`): Replace 'Director' jargon with plain Excel mechanism chips so users immediately understand what Excel setting or RTD/macro mechanism is running.
  - **Dockable transient History Strip / Plotter inside Model map** [CL-07-09] (P19-CARBON, P19-NEON; `P19-CARBON-0062`, `P19-NEON-0055`): Dock a collapsible plotter strip directly at the bottom of Model map.
  - **Unified Inspector › Live / Watch panel shared across all views** [CL-07-09] (P19-FABLE; `P19-FABLE-0080`, `P19-FABLE-0159`): Pin any box to the shared Watch / Inspector Live panel so plotting works identically in Classic, Model map, and App without duplicating chart chrome.
- **Decision:** Use plain Excel-terms calculation chips (`Auto`, `Manual`, `Iterates · Max N`, `RTD clock`, `Macro-ticked`, `Data table`) in product UI rather than Ptolemy `'Director'` jargon, while still showing playback controls (`Run / Pause / Step / Rate / Reset`) whenever a workbook has a confirmed RTD clock or macro-ticked runner (D-03). Route live tick-history plotting to the shared dockable Inspector › `Live` / `Watch` panel instead of building a second map-only plotter.
- **Not taken (Appendix Z):** Exposing Ptolemy `'Director'` terminology in product UI (P19-CARBON-0038, P19-CARBON-0061, P19-CARBON-0062, P19-NEON-0054, P19-NEON-0055, P19-NEON-0217) and a duplicate map-only History Strip plotter (P19-CARBON-0062, P19-NEON-0055) — parked in Appendix Z.
- **Reason:** Spreadsheet users understand 'Manual / Iterates / RTD clock' immediately without learning academic systems-modelling jargon, and sharing Inspector's Live panel enforces 'one of everything'.

### D-64: Model map L2 R1C1 run folding default with opt-in bounded geometry-preserving sheet excerpt

- **Authority:** Decider ruling | **Primary topic:** VII. Model map | **Clusters:** `CL-07-10`
- **Options & sources:**
  - **Decline raw cell explosions; stop at L2 R1C1 formula-pattern runs and jump to Classic for cells** [CL-07-10] (P19-FABLE, P19-NEON; `P19-FABLE-0047`, `P19-FABLE-0154`, `P19-NEON-0027`): Never render hundreds of thousands of raw cell boxes on the map; fold into exact R1C1 formula runs with 100% coverage accounting and jump to Classic to inspect individual cells.
  - **Allow an optional bounded geometry-preserving sheet excerpt inside the map** [CL-07-10] (P21; `P21-0075`): Provide a bounded, paged sheet excerpt (e.g., 7 of 1,000 rows × 12 columns) inline inside the map with exact addresses, formulas, full-precision values, boundary ports, and fingerprint checks.
- **Decision:** Open Model map at `<=60` boxes by default and fold large dense sheets into exact L2 R1C1 formula-pattern runs (100% coverage accounting, zero cell-node explosion), while supporting Pro 21's opt-in bounded geometry-preserving sheet excerpt (`M05`) when an analyst drills into a folded block and needs exact row/column coordinates without leaving the map—enforcing explicit fingerprint confirmation if the block binding shifts.
- **Not taken (Appendix Z):** Forbidding any bounded sheet-excerpt preview inside Model map drill-in (P19-FABLE-0047, P19-NEON-0027 strict reading) — harmonized with P21-0075.
- **Reason:** Preserves map legibility at scale while letting analysts inspect concrete row/column geometry inside a drilled block without losing context.

### Section: VIII. App

### D-65: App opening requires authored customXml; explicit switch offers badged certain-signal draft or clean decline

- **Authority:** Decider ruling | **Primary topic:** VIII. App | **Clusters:** `CL-08-02`, `CL-11-11`
- **Options & sources:**
  - **Auto-draft from certain Excel evidence (unlocked, validated, control-linked cells) with 'Drafted from file' badge** [CL-08-02] (P19-FABLE, P19-NEON; `P19-FABLE-0148`, `P19-FABLE-0467`, `P19-NEON-0056`): When a user switches to App on an unconfigured but clean workbook, synthesize a clearly badged draft page using only certain Excel signals (unlocked/validated/control-linked cells + scenario/chart structure), declining messy files like `Inherited`.
  - **Never invent an App; fall back to Classic or builder confirmation when no authored layout exists** [CL-08-02] (P19-GPT; `P19-GPT-0044`, `P19-GPT-0298`): Require an explicit authored App layout in `customXml` before opening a workbook in App; if missing or corrupted, fall back to Classic / Restore rather than auto-publishing.
  - **Option A: Heuristic auto-App on clean small workbooks (<=25 inputs+outputs)** [CL-11-11] (P19-CARBON, P19-NEON; `P19-CARBON-0078`, `P19-NEON-0068`): Auto-generate an App view whenever <=25 clear inferred inputs/outputs exist; decline messy workbooks like Inherited.
  - **Option B: Require authored contract or explicit/certain-evidence draft ('No App is defined' fallback)** [CL-11-11] (P19-FABLE, P19-GPT; `P19-FABLE-0170`, `P19-GPT-0061`): Render an authored App contract; if none exists, either show a clear 'No App is defined — Choose inputs and outputs / Draft from controls / Open Classic' state or draft solely from certain workbook controls/validated inputs without guessing across hundreds of constants.
- **Decision:** Separate *preferred opening* from *on-demand drafting*: a workbook NEVER auto-opens in App (`preferredView='app'`) without an author-confirmed layout in CustomXML (missing/broken layouts fall back cleanly to Classic, and broken bindings always show `'Source missing'`). When a user explicitly switches to App on an unconfigured workbook, show a clean `'No App is defined yet'` entry card offering `[Draft from file · Edit page…]` ONLY from certain Excel signals (unlocked-on-protected cells, Data Validation lists, Form Controls) on clean workbooks, `[Choose inputs & outputs]`, `[Ask agent to draft]`, or `[Open Classic]`—honestly declining unstructured massive workbooks like `Inherited`.
- **Not taken (Appendix Z):** Heuristic auto-generation of App forms from guessed constants on unconfigured or messy workbooks (P19-CARBON-0078, P19-NEON-0056, P19-NEON-0068) — parked in Appendix Z.
- **Reason:** Prevents broken or misleading auto-generated dashboards while making form creation one click away for well-structured models.

### D-66: App inputs recalculate live by default; batched 'Apply inputs' is an explicit author option

- **Authority:** Decider ruling | **Primary topic:** VIII. App | **Clusters:** `CL-08-03`
- **Options & sources:**
  - **Live recalculation by default; batched Apply as an explicit author option** [CL-08-03] (FRAMING, P19-FABLE, P21; `FRAMING-0137`, `P19-FABLE-0087`, `P21-0076`, `P21-0078`): Valid edits recalculate immediately (<100 ms) with one-gesture Undo; authors opt into batched 'Apply inputs' for slow or multi-input models with the reason stated on screen.
  - **Batched 'Apply inputs' by default for all App forms** [CL-08-03] (P19-GPT; `P19-GPT-0023`, `P19-GPT-0264`): Hold every form edit as a draft until the operator clicks Apply inputs.
- **Decision:** App inputs recalculate results live by default (per Pro 20 Change 2 / FRAMING-0137 and Pro 21 P21-0076, with immediate Undo). Batched `'Apply inputs'` (P21-0078) is a first-class author-chosen mode for heavy/multi-field models with its reason visibly stated beside the controls and explicit draft-discard confirmation when toggling back to Live.
- **Not taken (Appendix Z):** Requiring batched `'Apply inputs'` as the mandatory default for all App pages (P19-GPT-0023, P19-GPT-0264) — kept strictly as a named author option.
- **Reason:** Matches Pro 20 Change 2 and Excel users' expectation of instant reactive feedback while preserving batched submission for slow/transactional models.

### D-67: App output tiles stay read-only with opt-in inline Goal Seek over unlocked inputs

- **Authority:** Decider ruling | **Primary topic:** VIII. App | **Clusters:** `CL-08-05`
- **Options & sources:**
  - **Inline refusal offering single-input Goal Seek (`Solve Principal for $2,400/mo…`) or Switch to Classic** [CL-08-05] (P19-CARBON; `P19-CARBON-0040`, `P19-CARBON-0195`, `P19-CARBON-0228`): Turn accidental typing on a target KPI into a helpful Goal Seek callout (backed by `C-22`), plus a route to Classic.
  - **Read-only tile with provenance hover and 'See the cells' jump** [CL-08-05] (P19-FABLE; `P19-FABLE-0091`): Keep output tiles strictly non-editable with provenance peek and one-key jump to Classic.
- **Decision:** Keep App output tiles strictly non-editable (with provenance peek and `'See the cells'`), and offer Excel-native Goal Seek (`C-22`) as an explicit inline action on numeric formula outcomes (via tile menu or politely if a user attempts to type a target value) once `C-22` is available—solving only a user-selected unlocked input cell with full Undo and never altering the formula.
- **Not taken (Appendix Z):** Hard refusal with no Goal Seek affordance (P19-FABLE-0091) or direct editing of output formulas inside App — harmonized cleanly.
- **Reason:** Turns a natural user intent ('what rate gives $2,000/month?') into a safe, Excel-native What-If workflow without violating output immutability.

### D-68: Native Excel Scenario Manager for saved scenarios; transient Pin Baseline for ad-hoc deltas; no Goal Seek guessing

- **Authority:** Decider ruling | **Primary topic:** VIII. App | **Clusters:** `CL-08-06`, `CL-10-08`
- **Options & sources:**
  - **Bind reusable scenarios strictly to Excel's Scenario Manager** [CL-08-06] (P19-FABLE, P19-GPT; `P19-FABLE-0089`, `P19-FABLE-0469`, `P19-GPT-0265`): Read and write named scenarios directly as native Excel Scenario Manager entries (previewing changing cells), keeping ad-hoc comparisons transient.
  - **In-memory 'Pin Current as Baseline' and/or `customXml` scenario presets** [CL-08-06] (P19-CARBON, P19-NEON; `P19-CARBON-0041`, `P19-CARBON-0196`, `P19-NEON-0036`): Allow zero-friction baseline pinning in memory or `customXml` presets for side-by-side comparison tables.
  - **Option A: Allow customXml preset snapshots and heuristic Goal Seek guessing** [CL-10-08] (P19-FABLE, P19-NEON; `P19-NEON-0275`, `P19-FABLE-0598`): Store extra scenario preset snapshots in customXml (Neon) and guess prior Goal Seek runs from round target values (Fable D-23).
  - **Option B: Bind strictly to workbook scenarios/tables/Solver; creating new scenarios is an explicit workbook edit** [CL-10-08] (P19-FABLE, P19-GPT; `P19-FABLE-0580`, `P19-FABLE-0581`, `P19-GPT-0339`): Keep Scenario switching and Data Tables strictly backed by real Excel cells/Scenario Manager/{=TABLE}, treat creating new scenarios as an explicit workbook edit, and report Solver only when saved Solver parameters exist in the file.
- **Decision:** All reusable saved scenarios read and write Excel's native Scenario Manager (or explicit workbook scenario columns)—NEVER a parallel CustomXML scenario store—so desktop Excel users see every saved scenario identically. Keep `'Pin Current as Baseline'` / `'Compare with opening'` as a zero-friction transient in-session observation clearly labelled non-persisted unless `'Save as scenario…'` is invoked. Surface saved Solver settings (`From workbook`) in Inspector/Notebook, but never guess Goal Seek targets from round numbers.
- **Not taken (Appendix Z):** Storing saved scenarios in CustomXML presets invisible to Excel (P19-CARBON-0041, P19-CARBON-0196, P19-NEON-0036, P19-NEON-0275) and speculative Goal Seek pattern guessing from round numbers (P19-FABLE-0598) — parked in Appendix Z.
- **Reason:** Guarantees 100% Excel round-trip fidelity for scenarios and prevents hallucinated model semantics.

### D-69: App windowed schedules with opt-in badged annual rollup and 1-page default Executive Summary print

- **Authority:** Decider ruling | **Primary topic:** VIII. App | **Clusters:** `CL-08-07`, `CL-08-08`
- **Options & sources:**
  - **Auto-group >50-row schedules by natural axis (e.g., 30 expandable annual rows) with explicit total count** [CL-08-07] (P19-NEON; `P19-NEON-0224`): Group schedules exceeding 50 rows by natural period (Year) as a non-mutating display rollup with per-group expansion and visible total count.
  - **Windowed frame showing N rows with explicit count and 'all ►' expander (paired with 36–40 px controls)** [CL-08-07] (P19-FABLE; `P19-FABLE-0094`): Show a compact windowed excerpt with exact count disclosure without synthesizing derived rollup rows unless backed by or clearly marked as presentation rollup.
  - **Default 1-page Executive Summary with opt-in schedule choices (none / annual / full)** [CL-08-08] (P19-FABLE, P21; `P19-FABLE-0092`, `P19-FABLE-0201`, `P21-0079`): Default to a crisp 1-page answer (inputs, KPI/comparison summary, full-precision chart) with checkboxes to include annual summary (+1 p) or full schedule (+N pp).
  - **Default 2-page Executive Decision Brief (Page 1 summary + Page 2 annual rollup)** [CL-08-08] (P19-CARBON, P19-NEON; `P19-CARBON-0098`, `P19-NEON-0037`, `P19-NEON-0090`): Always include the 30-row annual schedule as Page 2 by default, with checkbox for the full monthly schedule.
- **Decision:** Default long App schedules (>50 rows) to a windowed table frame showing exact row counts (`360 payments · showing 1–12 · Expand all`) with an explicit opt-in `'Group by Year (display rollup)'` toggle badged as non-mutating display aggregation. Default App `Ctrl+P` to a frozen 1-page Executive Summary with live page-cost schedule checkboxes (`None [1 p]`, `Annual summary [+1 p]`, `Full schedule [+N pp]`).
- **Not taken (Appendix Z):** Auto-collapsing raw schedules into derived annual rollups by default (P19-NEON-0224) and defaulting print to a 2-page brief (P19-CARBON-0098, P19-NEON-0037, P19-NEON-0090) — kept strictly as opt-in checkboxes.
- **Reason:** Never confuses view-computed rollups with real sheet formulas and keeps default executive handouts crisp at 1 page.

### Section: IX. Formula Explorer

### D-70: Standalone Skin-0 Level-0 refusal of caller/workbook functions plus badged Local Test Inputs shelf

- **Authority:** Decider ruling | **Primary topic:** IX. Formula Explorer | **Clusters:** `CL-07-01`, `CL-09-03`
- **Options & sources:**
  - **Strictly refuse caller/workbook-dependent functions as an explicit Level-0 tool/host state** [CL-09-03] (P19-GPT, P21; `P19-GPT-0024`, `P19-GPT-0269`, `P21-0082`): Never fabricate a caller cell or empty sheet; refuse `DNA.CALLERADDRESS()`, `ROW()`, `CELL()` with a clear Level-0 host message preserving the draft and offering 'Open in DNA Calc'.
  - **Provide an explicit, editable assumed cell context ('as if in cell A1 of an empty sheet · change…')** [CL-09-03] (P19-FABLE; `P19-FABLE-0150`): Let standalone formulas calling `ROW()` or `CELL()` evaluate against a disclosed synthetic `A1` caller context.
  - **Explicit Local Test Inputs shelf (named scalars, arrays, errors) and reusable named lines** [CL-09-03] (P19-CARBON, P19-FABLE, P19-NEON; `P19-CARBON-0042`, `P19-CARBON-0067`, `P19-FABLE-0150`, `P19-NEON-0038`): Let users bind named local test inputs (scalars, small arrays, error values) and chain named lines cleanly inside the CalcFml evaluation environment.
  - **Literal arguments only in standalone expressions** [CL-09-03] (P19-GPT, P21; `P19-GPT-0024`, `P19-GPT-0269`, `P21-0082`): Accept literal arguments (and inline `LET`) only so standalone never resembles a hidden spreadsheet.
  - **Additional governing / aligned cards across topics** (FRAMING): `FRAMING-0108`
- **Decision:** In standalone `DNA Calc - Formula Explorer` (Skin 0, engine-free Level-0 host), strictly refuse caller- and workbook-dependent functions (`ROW()`, `CELL()`, `DNA.CALLERADDRESS()`) as an explicit Level-0 host state (never disguised as `#REF!`/`#VALUE!` and never fabricating a fake `A1` caller), preserving the draft with a one-click `'Open in DNA Calc'` handoff. Support literal formulas immediately on line 1 PLUS an optional, clearly badged **Local Test Inputs** shelf (scalars, arrays, errors bound to CalcFml's name map) and multi-line `LET`/`LAMBDA` editing; route multi-line chained scratchpad cells and linked-boxes playgrounds to DNA Calc's Notebook / Model map.
- **Not taken (Appendix Z):** Fabricating a fake `A1` caller or silent workbook in standalone Explorer (rejected per Pro 19 Push-back #5, P19-GPT-0092) and building a multi-cell chained scratchpad or mini linked-boxes canvas inside Skin 0 (P19-FABLE-0150, FRAMING-0108 question) — parked in Appendix Z.
- **Reason:** Keeps Skin 0 mechanically engine-free and honest about host capabilities while giving formula authors and add-in developers a practical local testbench.

### D-71: Docked Formula Explorer keeps trial evaluation strictly Level-0; host shell renders optional downstream impact

- **Authority:** Decider ruling | **Primary topic:** IX. Formula Explorer | **Clusters:** `CL-09-05`
- **Options & sources:**
  - **Render speculative cell and downstream dependent deltas across the Preview Seam before Apply** [CL-09-05] (P19-NEON; `P19-NEON-0039`, `P19-NEON-0226`): Show how the edited formula changes both the active cell and downstream workbook dependents before committing.
  - **Keep Explorer strictly a Level-0 single-expression evaluator; separate expression testing from temporary workbook recalc** [CL-09-05] (P19-GPT; `P19-GPT-0045`, `P19-GPT-0314`): Preserve honest Level-0 layering by evaluating only the target expression against captured precedent values, explicitly distinguishing expression trials from temporary whole-workbook replacement (`C-15`).
- **Decision:** Keep Formula Explorer's own trial evaluation strictly Level-0 (evaluating only the target formula against a Level-1 precedent snapshot—never importing `calc-engine` into Skin 0), while letting the Level-1 host shell optionally render downstream dependent deltas in the workbook Preview Seam when the user clicks `[Preview Impact]` or prepares to Apply (`C-15`).
- **Not taken (Appendix Z):** Coupling `dna-formula-ui` / Skin 0 directly to `calc-engine` for live downstream recalculation (P19-NEON-0039, P19-NEON-0226 architectural reading) — resolved cleanly at the Level-0 / Level-1 boundary.
- **Reason:** Preserves the mechanical dependency ceiling (`dna-onecalc` and `dna-formula-ui` never reach `calc-engine`) while still showing downstream blast radius inside the full DNA Calc app.

### D-72: Honest native-code trust disclosure for unsigned local .xll add-ins (never call it a 'Sandbox')

- **Authority:** Decider ruling | **Primary topic:** IX. Formula Explorer | **Clusters:** `CL-09-10`
- **Options & sources:**
  - **Non-modal signal-orange Dev Sandbox consent banner for unsigned `.xll`/JS** [CL-09-10] (P19-NEON; `P19-NEON-0227`): Use a lightweight non-blocking banner labelled 'Dev Sandbox' to allow quick local unsigned `.xll` loading.
  - **Explicit host/trust authorization clearly stating native XLLs are NOT VBA-sandboxed** [CL-09-10] (P19-GPT, P21; `P21-0087`, `P19-GPT-0270`): Require explicit trust consent naming host, version, and native-code risk, refusing the word 'Sandbox' for unsandboxed native binaries.
- **Decision:** Loading an unsigned local `.xll` add-in requires an explicit non-modal trust card (`Load once` / `Trust folder` / `Cancel`) disclosing file path, signature status, version, host, and clearly stating that native `.xll` binaries run native code outside VBA Guarded containment. NEVER label unsandboxed native `.xll` execution a `'Sandbox'`.
- **Not taken (Appendix Z):** Calling native `.xll` developer loading a `'Dev Sandbox'` (P19-NEON-0227) — retired to Appendix Z.
- **Reason:** Security honesty: mislabelling native DLL execution as sandboxed gives users dangerous false assurance.

### D-73: Standalone Formula Explorer persistence: local restore, .xlsx graduation export, and .dnafe.json developer test suites

- **Authority:** Decider ruling | **Primary topic:** IX. Formula Explorer | **Clusters:** `CL-09-12`
- **Options & sources:**
  - **Local scratchpad auto-persistence + explicit JSON test-suite files (`.dnafe.json`) with provider requirements + Export to `.xlsx`** [CL-09-12] (P19-CARBON, P19-GPT; `P19-CARBON-0121`, `P19-GPT-0046`): Save reusable formula/UDF test suites (expected outputs, error assertions, format checks, provider/version requirements) as plain JSON files rather than disguised `.xlsx` files, while also offering explicit Export to `.xlsx`.
  - **Export/save solely as plain `.xlsx` with zero new file format** [CL-09-12] (P19-FABLE; `P19-FABLE-0223`): Avoid inventing any new file format; save standalone lines and inputs directly as a plain Excel workbook.
- **Decision:** Support three distinct, non-overlapping persistence routes in standalone Formula Explorer: (1) local auto-restore of recent scratchpads in app storage, (2) **Export as `.xlsx`** when graduating a formula and its local test inputs into a real workbook (ordinary cells, zero proprietary workbook format), and (3) **Save Test Suite (`.dnafe.json`)** strictly for developer expected-value/format/provider test suites that have no native `.xlsx` representation.
- **Not taken (Appendix Z):** Forbidding developer test-suite files (P19-FABLE-0223 strict reading) or inventing a proprietary workbook file format — cleanly separated by purpose.
- **Reason:** Respects 'no proprietary workbook format' while still supporting Ade's add-in developer test-suite workflow.

### Section: X. Patterns

### D-74: Qualitative pattern provenance and four lifecycle states (no fake numeric confidence percentages)

- **Authority:** Decider ruling | **Primary topic:** X. Patterns | **Clusters:** `CL-10-01`
- **Options & sources:**
  - **Option A: Numeric confidence tiers** [CL-10-01] (P19-CARBON, P19-NEON; `P19-CARBON-0070`, `P19-NEON-0060`): Display calibrated numeric confidence thresholds (100% / >=85% / 50-84% / <50%) as percentage pills.
  - **Option B: Four provenance states + qualitative evidence grades** [CL-10-01] (FRAMING, P19-FABLE, P19-GPT, P21; `FRAMING-0154`, `P19-FABLE-0161`, `P19-FABLE-0162`, `P19-GPT-0047`, `P19-GPT-0334`, `P21-0088`): Use four lifecycle states ('From workbook', 'Suggested', 'Confirmed by <user>', 'Needs review after change') paired with qualitative evidence levels (Certain/Explicit workbook fact, Structural suggestion, Semantic guess) and Strand line styles (solid/dashed/dotted), forbidding invented percentages.
- **Decision:** Represent every inferred pattern using four explicit lifecycle states (`From workbook`, `Suggested`, `Confirmed by <user>`, `Needs review after change`) paired with qualitative evidence grades (`Certain/Workbook`, `Structural`, `Weak`) and Strand line styles (solid/dashed/dotted per D-17). Leave weak/ambiguous regions as raw ranges rather than fabricating abstractions.
- **Not taken (Appendix Z):** Displaying heuristic numeric confidence percentages like '92% confidence' (P19-CARBON-0070, P19-NEON-0060) — parked in Appendix Z.
- **Reason:** Pseudo-precise percentages imply statistical calibration that heuristics lack; qualitative evidence + 'Why suggested' lets analysts verify structural facts directly.

### D-75: Pattern confirmation and correction write strictly to customXml; workbook edits stay below a divider

- **Authority:** Decider ruling | **Primary topic:** X. Patterns | **Clusters:** `CL-10-02`
- **Options & sources:**
  - **Option A: Include labelled 'Pin with a name...' model edit inside the reading card** [CL-10-02] (P19-FABLE; `P19-FABLE-0163`, `P19-FABLE-0488`): Allow one model-changing action ('Pin with a name...', creating a visible Excel Defined Name) inside the reading card, explicitly labelled as changing the workbook.
  - **Option B: Strict presentation-metadata-only correction popover** [CL-10-02] (P19-CARBON, P19-GPT; `P19-CARBON-0071`, `P19-GPT-0054`): Keep pattern correction actions strictly presentation-only in customXml; never convert ranges to Tables or define names from pattern confirmation.
- **Decision:** All pattern confirmations, re-classifications, and rejections write exclusively to CustomXML (keyed to cells and existing user names, never hidden names per D-13). Explicit Excel workbook edits (`Define Name…`, `Convert to Table Ctrl+T`) appear only below a clear divider in a `'Workbook edits'` footer and route through the normal undoable model-edit path.
- **Not taken (Appendix Z):** Mixing model-mutating `'Pin with a name'` actions inline alongside metadata-only pattern corrections without a clear Model/Presentation boundary (P19-FABLE-0163, P19-FABLE-0488) — separated cleanly.
- **Reason:** Upholds the fundamental conceptual rule that every non-grid edit is labelled Model vs Presentation before commitment.

### D-76: Canonical 20-pattern / 5-family spreadsheet and systems-model catalogue

- **Authority:** Decider ruling | **Primary topic:** X. Patterns | **Clusters:** `CL-10-03`, `CL-10-06`, `CL-10-10`
- **Options & sources:**
  - **Option A: Compact 15-pattern baseline** [CL-10-03] (FRAMING; `FRAMING-0124`): Stick strictly to the 15 framing patterns, omitting explicit entries for checks, overrides, dynamic arrays/LAMBDAs, repeated-sheet families, and external-link hubs.
  - **Option B: Mid-size 18-19 pattern catalogue in 5 families** [CL-10-03] (P19-CARBON, P19-NEON; `P19-CARBON-0072`, `P19-NEON-0062`): Extend the 15 core patterns with high-impact audit/modern additions (Reconciliations/checks, Dynamic arrays/LAMBDAs, Overrides or External links or Ragged rollups) in 5 families.
  - **Option C: Expanded 23-27 pattern catalogue** [CL-10-03] (P19-FABLE; `P19-FABLE-0571`): Split Running Balances from Circular References, Scenarios from Sensitivity Tables, Stocks from Feedback/Delays, and add Repeated-sheet families, External consolidation, Irregular forms, Scratch areas, Switches, Solver/Goal Seek, and Two-way tables.
  - **Option A: Separate top-level patterns for Ragged Hierarchies and Two-Way Tables** [CL-10-06] (P19-CARBON, P19-FABLE; `P19-CARBON-0245`, `P19-FABLE-0602`): Elevate Ragged hierarchies/SUBTOTAL rollups (#17) and Two-way row/col-anchored tables (D-27) as independent top-level patterns.
  - **Option B: Sub-variants of Calculation Blocks** [CL-10-06] (P19-GPT, P19-NEON; `P19-GPT-0337`, `P19-NEON-0273`): Keep Calculation Blocks as one primary pattern and handle ragged rollup double-count audits and 2D mixed-anchor formula runs as specialized block variants.
  - **Option A: Single combined pattern (#8)** [CL-10-10] (P19-CARBON, P19-NEON; `P19-CARBON-0236`, `P19-NEON-0278`): Combine row-by-row running balances and iterative circular loops under one pattern entry.
  - **Option B: Two strictly separate patterns with distinct visual notations** [CL-10-10] (P19-FABLE, P19-GPT; `P19-FABLE-0584`, `P19-FABLE-0585`, `P19-GPT-0342`, `P19-GPT-0343`): Keep Running Balances (acyclic neighbour recurrence with opening seed, [t-1] notation) strictly separate from Deliberate Circular References (SCC under Iterate=ON, Iteration enclosure and convergence readout).
- **Decision:** Consolidate the four overlapping source lists (15/18/19/23/27 patterns) into one crisp 20-pattern / 5-family catalogue: (1) Data & Reference (Sheet-as-database, Reference/lookup tables, PivotTables, Dynamic array spills & LAMBDAs); (2) Inputs & Controls (Inputs & assumptions, Control panels [with Switches as sub-case], Scenarios & sensitivity tables [with saved Solver settings]); (3) Calculation & Flow (Calculation blocks [with Ragged rollups & Two-way grids as sub-variants], Time series, Pipelines across sheets, Repeated-sheet families); (4) State, Feedback & Execution (Running balances, Deliberate circular references, Stocks/flows/feedback/delays, Simulations, Live data, External-link consolidation, Macro-driven models); (5) Output & Audit (Reports/dashboards [with Irregular forms & Unread scratch fallback roles], Reconciliations/checks & Manual overrides). Keep acyclic Running Balances strictly separate from iterative Circular References.
- **Not taken (Appendix Z):** Bundling Running Balances with Deliberate Circular References (P19-CARBON-0236, P19-NEON-0278) and promoting Ragged Rollups, Two-Way Grids, or Switches to separate overlapping top-level patterns (P19-CARBON-0245, P19-FABLE-0597, P19-FABLE-0602) — folded into sub-variants and documented in Appendix Z.
- **Reason:** Avoids a sprawling mess of overlapping pattern names while keeping every genuine structural distinction sharp.

### D-77: Pattern recognition guardrails for databases, inputs, and time-series captions

- **Authority:** Decider ruling | **Primary topic:** X. Patterns | **Clusters:** `CL-10-04`, `CL-10-05`, `CL-10-07`
- **Options & sources:**
  - **Option A: High-confidence heuristic auto-DB + 15-row (Carbon) or 200-row (Neon) Notebook sample** [CL-10-04] (P19-CARBON, P19-NEON; `P19-CARBON-0229`, `P19-NEON-0271`): Auto-classify >50 or >100 uniform typed rows as High-confidence DB, window Notebook to 15 or 200 rows, and offer Ctrl+T Table conversion in the chip.
  - **Option B: Declared ListObject as 'From workbook', ordinary ranges as 'Suggested' with full-scope checks and unresolved INDIRECT separation** [CL-10-04] (P19-FABLE, P19-GPT, P21; `P19-FABLE-0576`, `P19-GPT-0335`, `P21-0091`): Recognise ListObject as 'From workbook' and ordinary rectangular ranges as 'Suggested' (never silently converting to a Table), run full-scope key-uniqueness checks, and list unresolved dynamic consumers (INDIRECT) separately.
  - **Option A: Auto-promote inferred constants to App controls** [CL-10-05] (P19-CARBON, P19-NEON; `P19-CARBON-0230`, `P19-NEON-0272`): Treat labelled constants with >=2 formula dependents as High-confidence Input Blocks and promote them directly to App sliders/inputs (with opt-out 'Ignore in App').
  - **Option B: Explicit-evidence inputs only in App draft; inferred constants require confirmation** [CL-10-05] (P19-FABLE, P19-GPT; `P19-FABLE-0577`, `P19-GPT-0336`): Only explicit workbook signals (Form Controls, Data Validation, unlocked cells on protected sheets, named inputs) seed an App draft automatically; ordinary constants stay 'Suggested' because they may be historical data or fixed constants and require confirmation before App promotion.
  - **Option A: Uniform High-confidence detection for date or period-label headers** [CL-10-07] (P19-CARBON, P19-NEON; `P19-CARBON-0232`, `P19-NEON-0274`): Treat both true dates and monotonic period text headers ('Jan-Dec', 'Q1-Q4') as High confidence (>=0.92).
  - **Option B: Distinguish true serial dates from text captions** [CL-10-07] (P19-FABLE, P19-GPT; `P19-FABLE-0579`, `P19-GPT-0338`): True equal-step Excel serial dates provide stronger evidence than text captions ('Jan', 'Jul actuals'), which do not prove year, cadence, or chronology and require explicit confirmation.
- **Decision:** Declared Excel ListObjects are `From workbook`; uniform rectangular ranges (`>=50` rows with headers and lookup/pivot consumers) are `Suggested` (with full-table uniqueness checks and separate listing of dynamic `INDIRECT`/`OFFSET` consumers). Form Controls, Data Validation lists, and unlocked-on-protected cells are `From workbook` App seeds, while ordinary labelled constants are `Suggested` and never auto-promoted to App without author confirmation. True equal-step Excel serial dates provide strong structural evidence for `Suggested` Time Series, whereas text/mixed captions ('Jan', 'Jul actuals') note ambiguity in `'Why suggested'` and require caption review on confirmation.
- **Not taken (Appendix Z):** Low-threshold heuristic database promotion or auto-promoting inferred constants to App inputs without confirmation (P19-CARBON-0229, P19-CARBON-0230, P19-NEON-0271, P19-NEON-0272) and treating text captions as silently equivalent to serial dates (P19-CARBON-0232, P19-NEON-0274) — parked in Appendix Z.
- **Reason:** Eliminates false positives on real corporate spreadsheets while surfacing genuine structure transparently.

### D-78: Honesty rules for systems roles, live-feed pause labels, external links, and check/override exceptions

- **Authority:** Decider ruling | **Primary topic:** X. Patterns | **Clusters:** `CL-10-11`, `CL-10-15`, `CL-10-18`
- **Options & sources:**
  - **Option A: High-confidence auto-classification with Ticks Director** [CL-10-11] (P19-CARBON, P19-NEON; `P19-CARBON-0243`, `P19-NEON-0285`): Auto-classify additive state formulas, shift arrays, and IFS modal guards as High-confidence Stocks/Flows/Delays under a Ticks Director.
  - **Option B: Structural detection + 'Suggested' semantic role requiring confirmation, strictly separating lag from iteration** [CL-10-11] (P19-FABLE, P19-GPT; `P19-FABLE-0592`, `P19-GPT-0350`, `P19-GPT-0351`): Detect additive accumulation, shift arrays, and feedback structurally, keep Stock/Flow/Delay labels as 'Suggested' until confirmed, and never conflate circular iteration with a physical time lag.
  - **Option A: Direct feed hold/throttle and heuristic staging-sheet detection** [CL-10-15] (P19-CARBON, P19-NEON; `P19-CARBON-0240`, `P19-NEON-0288`): Offer 'Hold/resume feed' throttle controls on live cells (Carbon) and infer paste-special staging tabs from high-constant sheets with notes (Neon).
  - **Option B: Separate UI display-pause from provider state; strictly distinguish missing/unrequested/cached external links** [CL-10-15] (P19-FABLE, P19-GPT, P19-NEON; `P19-GPT-0347`, `P19-NEON-0282`, `P19-FABLE-0596`, `P19-GPT-0355`): Keep 'Pause display observation' explicitly distinct from stopping the RTD provider or mutating formulas, keep missing file / update not requested / cached result strictly distinct, and ground external consolidation in explicit OOXML external-link records.
  - **Option A: Auto-monitor inferred checks in the Health Strip and flag overrides as hygiene defects** [CL-10-18] (P19-CARBON, P19-NEON; `P19-CARBON-0244`, `P19-CARBON-0247`, `P19-NEON-0286`): Auto-promote inferred check cells (ABS<eps, OK/ERR, 'Check'/'Diff' labels) to the persistent Strip/App badge and treat interior formula overrides as immediate audit warnings.
  - **Option B: Confirm check pass criteria without inventing tolerances; keep 'Exception' strictly distinct from 'Error'** [CL-10-18] (P19-FABLE, P19-GPT; `P19-FABLE-0594`, `P19-FABLE-0595`, `P19-GPT-0352`, `P19-GPT-0353`): Never invent materiality/tolerance thresholds — surface candidate checks as 'Suggested' until the user confirms check cells and pass values; treat a broken formula run as an objective structural 'Exception' rather than assuming it is a mistake until noted or repaired.
- **Decision:** Detect additive state updates, shift registers, and closed loops structurally, marking Stock/Flow/Delay business roles `Suggested` pending user confirmation. Label UI live-feed throttling/pausing strictly as pausing display updates (never claiming to stop the underlying RTD server). Preserve the three-way external-link distinction (missing file, update not requested, cached result) and mark paste-special staging tabs strictly `Suggested` with explicit evidence. Surface candidate check tolerances as `Suggested` without inventing thresholds; keep structural formula-run overrides labelled `'Exception'` (distinct from `#Error`), letting confirmed checks drive the Strip summary and letting authors mark `'Noted override'` in CustomXML (D-05).
- **Not taken (Appendix Z):** Auto-classifying Stock/Flow semantics at High confidence (P19-CARBON-0243, P19-NEON-0285); implying UI pause stops RTD servers (P19-CARBON-0240); heuristic paste-special staging without evidence (P19-NEON-0288); auto-enforcing guessed check tolerances or alarming on overrides as `#Error` (P19-CARBON-0244, P19-CARBON-0247, P19-NEON-0286) — parked in Appendix Z.
- **Reason:** Ensures pattern detection never overclaims business semantics, server state, or error severity.

### Section: XI. Big workbooks

### D-79: Unified WebView2/Leptos performance budgets (Target + Hard Ceiling) and progress cadence

- **Authority:** Decider ruling | **Primary topic:** XI. Big workbooks | **Clusters:** `CL-11-05`, `CL-11-06`
- **Options & sources:**
  - **Option A: Ultra-tight native-like targets (80 ms cold shell, 250 ms Inherited open)** [CL-11-05] (P19-CARBON, P19-NEON; `P19-CARBON-0079`, `P19-CARBON-0080`, `P19-CARBON-0199`, `P19-NEON-0071`, `P19-NEON-0229`): Require cold shell <=80 ms desktop (150 ms warm WASM) and Inherited 18 MB cached first paint <=250 ms desktop (<=450 ms browser).
  - **Option B: Realistic WebView2/Leptos + mid-range laptop budgets (<=1 s cold shell, <=1 s p95 Inherited first viewport)** [CL-11-05] (P19-FABLE, P19-GPT; `P19-FABLE-0173`, `P19-FABLE-0174`, `P19-FABLE-0175`, `P19-GPT-0063`, `P19-GPT-0064`): Require cold desktop shell <=1 s (Formula Explorer <=0.5 s; Open acknowledgement <=100 ms / warm shell paint <=300 ms p95) and Inherited first useful cached viewport <=1 s p95 desktop (<=2 s worst-case / browser 2x).
  - **Option A: Hard 1.5 s full index and 8-16 ms hard ceilings** [CL-11-05] (P19-CARBON, P19-NEON; `P19-CARBON-0081`, `P19-CARBON-0082`, `P19-CARBON-0083`, `P19-NEON-0072`, `P19-NEON-0073`): Demand full 304k-formula indexing <=1,500 ms, keystroke <=12-16 ms p95, scroll <=8-16 ms, and hover peek <=32 ms.
  - **Option B: Two-tier target vs hard ceiling (2 s inventory / 5 s overview; 16 ms target / 50 ms p95 worst-case input)** [CL-11-05] (P19-FABLE, P19-GPT; `P19-FABLE-0176`, `P19-FABLE-0177`, `P19-FABLE-0178`, `P19-FABLE-0181`, `P19-GPT-0065`, `P19-GPT-0066`, `P19-GPT-0069`): Separate fast sheet inventory (<=2 s p95) from full 300k-formula overview/indexing (<=5 s p95 desktop, <=8 s browser); target 60 Hz (<=16 ms) for typing/scrolling with a hard ceiling of <=50 ms p95 (<=100 ms p99) under heavy background load, Alt+Q <=50 ms, and never >100 ms blank during scroll.
  - **Option A: Tight 60-100 ms warm switch (Carbon using Ctrl+1..5, Neon using Ctrl+Alt+1..5)** [CL-11-06] (P19-CARBON, P19-NEON; `P19-CARBON-0084`, `P19-NEON-0074`): Require view switch <=60-100 ms warm (<=200 ms initial synthesis); Carbon cites Ctrl+1..5 while Neon cites Ctrl+Alt+1..5.
  - **Option B: Two-phase 150 ms anchor continuity / 400-500 ms bounded render with Ctrl+Alt+1..5** [CL-11-06] (P19-FABLE, P19-GPT; `P19-FABLE-0180`, `P19-GPT-0067`): Place selection / continuity frame within <=150 ms p95 and complete bounded view render within <=400-500 ms (or explicit loading state on Inherited), using Ctrl+Alt+1..5.
  - **Option A: 150 ms Strip progress pill; 16-30 ms small recalc** [CL-11-06] (P19-NEON; `P19-NEON-0075`, `P19-NEON-0076`, `P19-NEON-0230`): Small recalc <=16 ms desktop / <=30 ms browser; show Strip progress pill at 150 ms with Esc cancel.
  - **Option B: 100 ms small recalc, 300 ms progress pill, >1 s explicit cancel bar, cancel ack <=100 ms / return <=500 ms** [CL-11-06] (P19-FABLE, P19-GPT; `P19-FABLE-0179`, `P19-FABLE-0182`, `P19-GPT-0068`): Small commit-to-dependents <=100 ms; on larger models update edited cell immediately, hatch dependents, show Strip progress at 300 ms, full cancel control at >1 s, background option >10 s, Esc ack <=100 ms and return control <=500 ms.
- **Decision:** Grounded in D-07 (WebView2/Leptos baseline stays), enforce Target + Hard Ceiling budgets: cold desktop shell `<=1 s`, standalone Explorer `<=0.5 s`, warm Open ack `<=100 ms` / shell frame `<=300 ms` p95, `Inherited` first cached viewport `<=1 s` p95 desktop (`<=2 s` browser 2x), sheet inventory `<=2 s` p95, 300k-formula overview `<=5 s` desktop (`<=8 s` browser) with zero main-thread blocking, keystroke/scroll `<=16 ms` target / `<=50 ms` p95 ceiling (`p99 <=100 ms`), hover peek & Explorer click `<=50 ms`, `Alt+Q` palette `<=50 ms`, live-value coalescing `<=100 ms` p95, view-switch anchor `<=150 ms` (`<=100 ms` warm target) / bounded content `<=400–500 ms`, small-model (`<5k` cells) edit-to-dependents `<=100 ms` with zero spinner `<150 ms`, quiet Strip progress pill at `300 ms`, explicit Cancel bar at `>1 s` (backgroundable `>10 s`), and `Esc` ack `<=100 ms` returning control `<=500 ms`. (View-switch shortcuts inside raw conflict #127 are governed by D-12.)
- **Not taken (Appendix Z):** Native-toolkit cold-shell `80 ms` / 18 MB open `250 ms` / 300k index `1.5 s` / `150 ms` spinner targets (P19-CARBON-0079..0084, P19-CARBON-0199, P19-NEON-0071..0076, P19-NEON-0229, P19-NEON-0230) — parked in Appendix Z as aspirational notes.
- **Reason:** Sets ambitious yet honest, enforceable WebView2/Leptos acceptance ceilings aligned with Owner Ruling #7.

### D-80: Unified per-view scale simplification and decline caps across Inspector, Notebook, Map, and Explorer

- **Authority:** Decider ruling | **Primary topic:** XI. Big workbooks | **Clusters:** `CL-11-08`, `CL-11-09`, `CL-11-10`, `CL-11-11`
- **Options & sources:**
  - **Option A: Tight graph frontier (<=200 nodes / <=400 edges) + single range node above 500 precedents** [CL-11-08] (P19-GPT, P19-NEON; `P19-GPT-0058`, `P19-NEON-0065`): Start at 1 dependency frontier; fold >500 direct cell precedents into one bounded range node; switch visual graph to block/list form above 200 nodes or 400 edges; refuse 'draw every dependency'.
  - **Option B: 2,000-row direct list / 2,000-formula full block graph; decline >2,500-edge hairballs or million-cell flat lists** [CL-11-08] (P19-CARBON, P19-FABLE; `P19-CARBON-0075`, `P19-FABLE-0167`): Show flat lists up to 2,000 rows directly; above 2,000 fold by root/block/run; sample value distributions >50k rows with exact recompute option; refuse million-cell lists or >2,500-edge cell hairballs offering 2-hop expansion, Export, or Narrow to sheet.
  - **Option A: Auto-scaffold small sheets (<=15 blocks Carbon / sheet picker Neon) with 15-row (Carbon) or 200-row (Neon) windows** [CL-11-09] (P19-CARBON, P19-NEON; `P19-CARBON-0076`, `P19-NEON-0067`): Carbon caps inline rendering at 15 blocks / 200-row tables (windowing >200 to 15 rows + sticky totals and declining >25k-formula dense sheets); Neon renders <=200 rows directly and samples large tables at 200 rows + exact aggregates.
  - **Option B: Saved Jupyter notebook / explicit selection + 20-row compact frame window + <=150-block single-sheet cap** [CL-11-09] (P19-FABLE, P19-GPT; `P19-FABLE-0168`, `P19-GPT-0059`): Render the saved notebook or an explicit selected-content/sheet scope (refusing 'paragraph per cell' and whole 40-sheet auto-dumps), fold runs to single items up to ~150 blocks per sheet, and window large frames to 20 rows with virtual scrolling and exact totals.
  - **Option A: Lower direct-box caps (60 Fable / 80 Carbon / 120 Neon) with varying refusal thresholds** [CL-11-10] (P19-CARBON, P19-FABLE, P19-NEON; `P19-FABLE-0169`, `P19-CARBON-0077`, `P19-NEON-0066`): Auto-group above 60 (Fable), 80 (Carbon), or 120 (Neon) blocks; decline flat canvases above 120 bespoke cells (Neon), 500 blocks (Carbon), or 2,000 clustered items (Fable).
  - **Option B: Two-level legible default (~60-80 ungrouped target) + hard 200-node / 400-edge auto-layout budget (Pro 21 M03)** [CL-11-10] (P19-GPT, P21; `P19-GPT-0060`, `P21-0072`): Enforce a clean, hard auto-layout viewport cap of 200 expanded nodes / 400 edges (matching Inspector's graph cap), collapsing overflow into counted sheet groups, boundary ports, and searchable inventory.
  - **Option A: Virtualise arrays >50 elements showing rows 1..20** [CL-11-11] (P19-NEON; `P19-NEON-0069`): Summarize arrays larger than 50 elements as virtualised previews showing first 20 rows + [Inspect All].
  - **Option B: Show arguments up to 200 cells directly; above 200 summarize by shape/type/first 20 rows** [CL-11-11] (P19-FABLE; `P19-FABLE-0171`): Render small 2D arrays up to 200 cells directly and summarize larger arrays by shape, type, and first 20 rows while confirming evaluation uses all rows.
- **Decision:** Enforce consistent scale ceilings across every non-grid view: (1) Visual dependency graphs in BOTH Inspector and Model map cap hard at `200 nodes / 400 edges` (Inspector starts at 1 frontier and folds `>500`-cell range refs; Model map auto-groups into Districts/Sheet Groups above `60` blocks for calm initial views); (2) Tabular Inspector lists render up to `2,000` rows directly and fold above `2,000` by root cause / R1C1 run / block with exact counts; (3) Notebook renders saved/selected scope (no auto-scaffolding dense sheets per D-06), shows tables `<=20` rows inline, windows `>20`-row tables to a `20`-row viewport (expandable to `200`/page with exact full-table aggregates), and caps auto-outlined single-sheet blocks at `150` folded runs; (4) Formula Explorer previews `50` elements / `20` rows inline with shape/type metadata and 1-click expansion up to `200` cells (or full virtualised modal via `[Inspect All]`).
- **Not taken (Appendix Z):** Divergent graph caps (`80`, `120`, `500`, `2,500` edges in P19-CARBON-0075/0077, P19-FABLE-0167/0169, P19-NEON-0066), 15-block Notebook hard cap (P19-CARBON-0076, P19-NEON-0067), and always-200-cell inline Explorer arrays (P19-FABLE-0171) — unified cleanly and superseded caps listed in Appendix Z.
- **Reason:** One coherent visual-graph ceiling across views makes big-workbook behavior predictable and testable against the Enron corpus.

### Section: XII. Customisation and the skin API

### D-81: Level-0 formula host in its own engine-free crate beside/below calc-host

- **Authority:** Decider ruling | **Primary topic:** XII. Customisation and the skin API | **Clusters:** `CL-09-01`, `CL-12-01`
- **Options & sources:**
  - **Option A: Separate Level-0 formula-host crate beside/below calc-host (Adopted)** (FRAMING; `FRAMING-0041`, `FRAMING-0025`, `FRAMING-0043`): Place Level 0 (CalcFml, CalcFunc, formatting, CF, add-ins, zero calc-engine) in its own crate so Cargo dependency checks mechanically enforce that dna-onecalc and dna-formula-ui never reach calc-engine or calc-host.
  - **Option B: Embed Level 0 inside calc-host behind a feature flag** (FRAMING; `FRAMING-0041`): Keep Level 0 as a Cargo feature inside calc-host (rejected: weakens crate-level dependency ceiling checks and risks feature unification pulling calc-engine into dna-onecalc).
- **Decision:** Resolve FRAMING-0041 (Work area 9b) by placing the Level-0 formula host in its own engine-free crate beside/below `calc-host`, with `calc-host` (Level 1) building on top of it, so Cargo's crate graph and `cargo xtask sweep` mechanically guarantee `dna-onecalc` and `dna-formula-ui` never reach `calc-engine` or `calc-host`.
- **Not taken (Appendix Z):** Embedding Level 0 inside `calc-host` via Cargo feature flags (FRAMING-0041 Option A) — rejected due to workspace Cargo feature-unification risks; details align with the merged SKIN_ARCHITECTURE.md (D-09).
- **Reason:** Matches AGENTS.md's mechanical dependency ceiling (`dna-onecalc-desktop` reaches no `calc-host` and no `calc-engine`) without fragile feature-flag hygiene.

### D-82: Contrast-checked non-semantic token overrides; violations block Apply (no silent rewriting)

- **Authority:** Decider ruling | **Primary topic:** XII. Customisation and the skin API | **Clusters:** `CL-12-04`, `CL-12-09`
- **Options & sources:**
  - **Base Strand theme + contrast-checked non-semantic token overrides** [CL-12-04] (P19-FABLE, P19-NEON, P21; `P19-FABLE-0037`, `P19-NEON-0018`, `P21-0102`): Allow declarative skins to override non-semantic surface, accent, border, density, and type-step tokens, checked mechanically by unrounded sRGB relative-luminance contrast checks across all 9 declared semantic pairs (≥ 4.5:1 text, ≥ 3:1 boundaries and dual-stroke focus).
  - **Lock palettes strictly to Strand's three built-in unit-tested themes** [CL-12-04] (P19-CARBON; `P19-CARBON-0019`): Restrict skins to choosing among Strand's three built-in themes and customising only layout, panels, density, and keymaps.
  - **Block Apply with exact sRGB ratios, line-numbered diagnostics, and one-click fix suggestions** [CL-12-09] (P19-CARBON, P19-FABLE, P21; `P19-CARBON-0089`, `P19-FABLE-0193`, `P19-FABLE-0509`, `P21-0100`, `P21-0101`): Reject invalid contrast, schema, or binding rules with plain-language, line-numbered diagnostics and explicit fix/reset buttons so the author approves every value.
  - **Auto-adjust failing contrast tokens on preview and report the adjustment** [CL-12-09] (P19-NEON; `P19-NEON-0081`): Automatically shift failing contrast tokens into compliance during live preview and notify the user of the change.
- **Decision:** Permit declarative non-semantic token overrides layered over any Strand v0.2 base theme, checked against P21-0102's exact 9-pair unrounded sRGB contrast check (`>=4.5:1` text, `>=3:1` borders/focus) while keeping semantic signal roles immutable. Any contrast, schema, or binding violation BLOCKS Review/Apply with exact sRGB ratios, line-numbered reasons, and an optional explicit `'Apply suggested accessible value'` button—never silently mutating author or agent tokens.
- **Not taken (Appendix Z):** Locking themes strictly to the 3 built-in Strand palettes with zero non-semantic overrides (P19-CARBON-0019) and silently auto-adjusting failing contrast tokens on Apply (P19-NEON-0081) — parked in Appendix Z.
- **Reason:** Enables real branding and user comfort customisation while preserving WCAG 2.2 AA guarantees and author agency.

### Section: XIII. Persistence

### D-83: Canonical JSON (dna.view/1) for skin files/Studio and standard OOXML /customXml/itemN.xml (urn:dnacalc:views:v1) inside .xlsx

- **Authority:** Decider ruling | **Primary topic:** XIII. Persistence | **Clusters:** `CL-12-05`, `CL-13-01`
- **Options & sources:**
  - **Canonical JSON (`dna.view/1`, `.dnaskin.json`) + OOXML `customXml` wrapper** [CL-12-05] (P19-CARBON; `P19-CARBON-0086`): Use one strict schema-validated JSON declaration format for workspace/exported files and wrap it in `xl/customXml/dnaCalcViews.xml` inside workbooks.
  - **Dual JSON/YAML (`.dnaskin`) syntax** [CL-12-05] (P19-NEON; `P19-NEON-0078`): Support both JSON and YAML as authoring and interchange formats.
  - **Standard OOXML dynamic /customXml/itemN.xml + itemPropsN.xml with XML namespace urn:dnacalc:views:v1** [CL-13-01] (P21; `P21-0093`, `P21-0094`): Allocate a free standard OOXML customXml part (/customXml/itemN.xml with companion itemPropsN.xml) storing versioned XML under urn:dnacalc:views:v1 (version='1'), keyed by stable sheet IDs, layout IDs, and source bindings, with opaque retention of newer versions.
  - **Legacy urn:dnacalc:view-manifest or arbitrary XML/JSON blob in a custom part** [CL-13-01] (FRAMING, P19-CARBON, P19-NEON; `FRAMING-0013`, `P19-CARBON-0220`, `P19-NEON-0241`): Store arbitrary UTF-8 XML/JSON blobs or use the earlier unversioned urn:dnacalc:view-manifest namespace.
- **Decision:** Standardise standalone skin files, Skin Studio editing, and agent output on **canonical JSON (`dna.view/1`, `.dnaskin.json`)**. Inside `.xlsx`/`.xlsm` packages, serialize the view manifest, declarative skins, notes/findings (D-05), and toggleable agent change trail (D-14) strictly into standard OOXML dynamically allocated `/customXml/itemN.xml` + `itemPropsN.xml` with workbook relationships under versioned XML namespace `urn:dnacalc:views:v1` (`version="1"`), mapping losslessly 1:1 with `dna.view/1` JSON and retaining unknown newer schema versions opaquely.
- **Not taken (Appendix Z):** YAML `.dnaskin` format (P19-NEON-0078); non-standard hardcoded paths like `xl/customXml/dnaCalcViews.xml` or hardcoded `item1.xml` (P19-CARBON-0220, P19-NEON-0241, FRAMING-0013 old namespace); raw JSON blobs stuffed outside valid OOXML CustomXML XML structure — parked in Appendix Z.
- **Reason:** JSON gives zero-ambiguity schema validation for agents/Studio, while standard OOXML `/customXml/itemN.xml` guarantees Excel round-trip retention without colliding with existing CustomXML parts.

### Section: XII. Customisation and the skin API

### D-84: Named built-in UI command sequences permitted in declarative skins on explicit user invocation only

- **Authority:** Decider ruling | **Primary topic:** XII. Customisation and the skin API | **Clusters:** `CL-12-05`, `CL-12-06`
- **Options & sources:**
  - **Allow explicit user-invoked sequences of built-in UI commands** [CL-12-05] (P19-FABLE, P19-NEON; `P19-FABLE-0188`, `P19-NEON-0079`): Allow skins to declare named sequences of existing built-in UI/navigation/formatting commands that never auto-run on open and never bypass standard Undo.
  - **Passive layout, bindings, and single-command keymap placements only** [CL-12-05] (P19-CARBON, P19-GPT; `P19-CARBON-0087`, `P19-GPT-0070`): Restrict skins to theme, layout, bindings, print presets, and binding keys to individual built-in commands.
  - **Additional governing / aligned cards across topics** (FRAMING, P19-CARBON, P19-NEON): `FRAMING-0067`, `P19-CARBON-0114`, `P19-NEON-0107`
- **Decision:** Keep workbook-embedded skins 100% declarative (zero executable code in `.xlsx`; coded widgets remain strictly host-installed signed extensions with declarative fallbacks per FRAMING-0067). Permit declarative named sequences of existing built-in UI commands inside skins strictly on explicit user invocation—never on workbook open, disclosed on import, never hiding model mutations, and routed through normal Undo.
- **Not taken (Appendix Z):** Forbidding even explicit user-invoked declarative command macros of built-in UI verbs (P19-CARBON-0087, P19-GPT-0070 strict reading) — permitted under strict non-auto-run guardrails.
- **Reason:** Allows useful workflow shortcuts ('Jump to Summary and open Trace') without opening a macro-security hole inside macro-free `.xlsx` files.

### D-85: Canonical Customisation worked examples (strict presentation-only Classic restyle + canonical JSON Loan App)

- **Authority:** Decider ruling | **Primary topic:** XII. Customisation and the skin API | **Clusters:** `CL-12-11`, `CL-12-12`
- **Options & sources:**
  - **Presentation-only dark Classic restyle (view density, unchanged Classic keymap, zero workbook row-height mutations)** [CL-12-11] (P19-FABLE, P19-GPT, P21; `P19-FABLE-0195`, `P19-GPT-0073`, `P19-GPT-0290`, `P21-0106`): Demonstrate a dark Classic variant preserving Working density, 15.5 px formula text, docked 320 px Inspector, 9/9 sRGB contrast checks, unchanged Classic keymap, and zero workbook formatting/row-height changes.
  - **Power-audit restyle adding custom override/border shortcuts or fixed 19 px row heights** [CL-12-11] (P19-CARBON, P19-NEON; `P19-NEON-0083`, `P19-CARBON-0091`): Include extra key chords (`Alt+R` / `Alt+Shift+B`), provenance tints, and compact 19 px rows in Example 1.
  - **Canonical `dna.view/1` JSON with Read/Write disclosure, isolated trial inputs, strict address blocking, and separated model proposals** [CL-12-12] (P19-FABLE, P19-GPT, P21; `P19-GPT-0074`, `P19-FABLE-0196`, `P21-0107`, `P21-0108`, `P21-0109`): Use canonical JSON targeting the pinned Loan fixture (`B2:B4`, `B6:B7`, `A12:F371`), requiring explicit binding disclosure, isolated trial testing, and strict invalid-address rejection.
  - **YAML shorthand targeting `Loan!C3:C7` with embedded scenario presets** [CL-12-12] (P19-NEON; `P19-NEON-0084`): Author the Loan App skin in YAML with inline scenario presets.
- **Decision:** Use two canonical worked examples in the Design Book: (1) strict presentation-only dark Classic restyle (`P21-0106` + `P19-FABLE-0195` + `P19-GPT-0073` + optional provenance/trace-rail toggles), explicitly refusing workbook row-height mutations and leaving Classic's keymap untouched by default; (2) canonical `dna.view/1` JSON Loan App (`P21-0107..0109` + `P19-GPT-0074` + `P19-FABLE-0196`, expressing baseline-delta KPIs and print checkboxes cleanly in JSON) with explicit Read/Write binding disclosure and isolated trial preview.
- **Not taken (Appendix Z):** Mutating worksheet pixel row heights or injecting default Classic keybindings inside a presentation restyle (P19-NEON-0083, P19-CARBON-0091) and YAML shorthand for the Loan App example (P19-NEON-0084) — parked in Appendix Z.
- **Reason:** Keeps the two flagship examples 100% consistent with D-25 (Model vs Presentation separation) and D-83 (canonical JSON).

### Section: XIII. Persistence

### D-86: Per-sheet preferred view stays a tab badge and Bridge pill (never unexpected switch on tab click)

- **Authority:** Decider ruling | **Primary topic:** XIII. Persistence | **Clusters:** `CL-03-09`, `CL-13-04`
- **Options & sources:**
  - **Keep active view pinned on tab click during authoring; show tab badge + one-click Bridge suggestion pill (auto-switch only on initial open or explicit Guided/App links)** [CL-13-04] (P19-FABLE, P19-NEON; `P19-FABLE-0221`, `P19-NEON-0103`, `P19-NEON-0114`): Prevent spatial whiplash while navigating tabs during authoring: honour workbook preferred view once on open, display each sheet's preferred view as a tab micro-glyph and clickable Bridge chip, and auto-switch only in Guided/Published App flows.
  - **Automatically switch to each sheet's preferred or last-used view on tab click** [CL-13-04] (P19-GPT, P21; `P19-GPT-0034`, `P19-GPT-0244`, `P21-0092`): Clicking a sheet tab immediately switches the Stage to that sheet's preferred/last-used view unless the user has set a local session override.
  - **Additional governing / aligned cards across topics** (FRAMING): `FRAMING-0064`
- **Decision:** Keep the active view pinned when clicking sheet tabs during authoring (matching four-answer consensus FRAMING-0064). Apply the workbook's preferred view once on open; display each sheet's preferred view as a tab micro-glyph plus a one-click Bridge suggestion pill ('Open in Notebook'), and permit automatic view transitions only when following an explicit Guided/App navigation link targeting a sheet+view.
- **Not taken (Appendix Z):** Automatic view switching on every sheet-tab click during authoring (P19-GPT-0034, P19-GPT-0244, P21-0092) — parked in Appendix Z.
- **Reason:** Prevents spatial whiplash when an analyst clicks through tabs in Classic or Inspector while still honouring per-sheet view preferences.

### D-87: Missing or stripped customXml falls back to Classic with on-demand auto-inference in other views

- **Authority:** Decider ruling | **Primary topic:** XIII. Persistence | **Clusters:** `CL-13-06`
- **Options & sources:**
  - **Open in Classic by default when view part is absent/stripped, with explicit notice and on-demand auto-inference for other views** [CL-13-06] (P19-GPT; `P19-GPT-0292`): Default to Classic whenever no presentation part is present, keep broken bindings visibly unresolved, and warn cleanly if stripping is detected.
  - **Auto-infer Notebook/Inspector/Model Map directly without customXml** [CL-13-06] (P19-FABLE, P19-NEON; `P19-FABLE-0523`, `P19-NEON-0111`): Regenerate sensible non-Classic views automatically from model structure whenever customXml is absent.
- **Decision:** Open any workbook lacking a valid CustomXML view part in Classic by default (displaying a quiet notice and JSON view-import recovery option if a previously tracked file lost its part), while ensuring Inspector, Notebook, and Model map remain 100% functional via deterministic auto-inference the moment the user switches to them.
- **Not taken (Appendix Z):** Auto-landing in a synthesised non-Classic view when a workbook's CustomXML part is absent or stripped (P19-FABLE-0523, P19-NEON-0111) — parked in Appendix Z.
- **Reason:** Gives predictable, familiar landing footing on plain Excel files and graceful recovery if Document Inspector or a third-party editor strips CustomXML (Push-back #4).

### Section: XIV. Automation

### D-88: Three-tier macro security (Off/Guarded/Unguarded), honest static preflight, and observed-only macro effects

- **Authority:** Decider ruling | **Primary topic:** XIV. Automation | **Clusters:** `CL-01-02`, `CL-10-16`, `CL-14-01`
- **Options & sources:**
  - **Option A: Promise reversible [M] Macro Undo checkpoint and 'Likely' static VBA range dependencies** [CL-10-16] (P19-CARBON, P19-FABLE; `P19-CARBON-0241`, `P19-FABLE-0590`): Promise a reversible [M] Macro Undo checkpoint on every macro run (Carbon) and treat range strings parsed from VBA code text as 'Likely' write dependencies (Fable).
  - **Option B: Design for macro undo without promising it in v1; record only observed post-run changes** [CL-10-16] (P19-GPT, P19-NEON; `P19-GPT-0348`, `P19-NEON-0283`): Show macros as action/quarantined nodes without claiming unexecuted static code text as formula dependency truth; record actual changed regions after a run; design the undo seam without promising reversible macro undo in release 1.
  - **Three explicit host-controlled levels: Off, Guarded, Unguarded** [CL-14-01] (FRAMING, P21; `FRAMING-0159`, `P21-0115`, `P21-0116`): Use Off (all macros refused), Guarded (workbook-scoped macros with trust review and blocked external I/O), and Unguarded (full execution for explicitly trusted files/signatures), avoiding the misleading label 'Safe'.
  - **Binary Blocked / Allow-this-session toggle** [CL-14-01] (P19-CARBON, P19-FABLE, P19-NEON; `P19-CARBON-0099`, `P19-FABLE-0048`, `P19-FABLE-0514`, `P19-NEON-0262`): Quarantine all macros on open until the user clicks a single 'Allow macros this session' or signature trust action.
  - **Honest preflight boundary: reject statically known unavailable host capabilities; treat general VBA inspection as advisory** [CL-14-01] (P21; `P21-0119`): Acknowledge that declared action plans are a review UI pattern, not proof that a production host can statically preflight, sandbox, or atomically roll back arbitrary dynamic VBA.
  - **All-or-nothing preflight batch rejection before the first write** [CL-14-01] (P21; `P21-0118`): Reject an entire macro action batch before the first cell mutation whenever any step needs an unavailable external capability.
  - **Additional governing / aligned cards across topics** (FRAMING): `FRAMING-0057`
- **Decision:** Adopt the standing three-tier host-controlled macro security model (`Off` / `Guarded` / `Unguarded`, SKIN_DISCUSSION.md, FRAMING-0159) paired with Fable's inline blocked-control inspection card. Untrusted files open quarantined; `Off` refuses all macros with inline code inspection; `Guarded` requires trust review for workbook-scoped runs while refusing external I/O; `Unguarded` allows full desktop execution for trusted/signed workbooks. Apply upfront rejection whenever a macro requires a statically absent host capability (e.g. browser build) or runs a declared action plan; for arbitrary dynamic VBA, present static call/range scans strictly as weak advisory hints (never formula dependency edges) and enforce runtime host guards without falsely promising atomic preflight rollback. (Macro-run Undo in raw conflict #122 is governed by D-04.)
- **Not taken (Appendix Z):** Collapsing macro security to a binary Blocked/Session-Trusted toggle (P19-CARBON-0099, P19-FABLE-0048, P19-FABLE-0514, P19-NEON-0262); falsely promising atomic preflight rollback for arbitrary dynamic VBA (P21-0118 overstated reading); drawing static VBA source-text range literals as formula dependency edges (P19-FABLE-0590) — parked in Appendix Z.
- **Reason:** Aligns with standing security policy and Pro 21's explicit honesty caveat (P21-0119).

### D-89: Minimal 3-verb late-bound Views API for VBA (COM + Application.Run over one dispatcher)

- **Authority:** Decider ruling | **Primary topic:** XIV. Automation | **Clusters:** `CL-14-03`
- **Options & sources:**
  - **Late-bound Application.Run strings ('DnaCalc.View.Switch', 'DnaCalc.View.Select', 'DnaCalc.View.ShowPanel')** [CL-14-03] (P19-CARBON, P19-FABLE; `P19-CARBON-0101`, `P19-FABLE-0205`, `P19-FABLE-0516`): Invoke DNA Calc view verbs through Application.Run so VBA modules require zero external typelib references and degrade harmlessly under On Error Resume Next in Microsoft Excel.
  - **Small DNA Calc Views COM library (CreateObject / Views.TryShow, Views.SelectBlock)** [CL-14-03] (FRAMING, P21; `FRAMING-0017`, `P21-0117`): Expose a small COM object for non-Classic views (e.g., Views.TryShow, Views.SelectBlock, Views.ShowPanel) once the views settle.
  - **Minimal 3-verb presentation surface: SwitchView, Select, ShowPanel** [CL-14-03] (FRAMING, P19-CARBON, P19-FABLE, P19-GPT, P21; `FRAMING-0017`, `FRAMING-0128`, `P19-CARBON-0101`, `P19-FABLE-0205`, `P19-FABLE-0516`, `P19-GPT-0078`, `P19-GPT-0295`, `P21-0117`): Limit the v1 DNA Calc views API strictly to switching views, selecting a cell/block address, and showing/hiding a panel.
  - **Expanded 6-verb View API adding SetFormulaLens, SetScenario, and ShowNoticeBanner** [CL-14-03] (P19-NEON; `P19-NEON-0092`, `P19-NEON-0236`): Add Formula Lens toggle, Scenario switching, and custom notice banners to the View API.
- **Decision:** Expose one small late-bound COM object (`CreateObject("DnaCalc.Views")` with `TryShow`, `Select`, `ShowPanel`) AND matching `Application.Run` shims (`DnaCalc.View.Switch`, `DnaCalc.View.Select`, `DnaCalc.View.ShowPanel`) backed by one host command dispatcher—strictly those 3 verbs in v1 under full safety guardrails (macros can never bypass trust notices or Preview Seams). Recommend late-bound invocation guarded by `On Error Resume Next` so workbooks never break with missing typelib references in Excel.
- **Not taken (Appendix Z):** Early-bound COM typelib dependency that fails in desktop Excel; `SetScenario` verb (duplicates native Excel `Worksheet.Scenarios(name).Show`); `SetFormulaLens` and `ShowNoticeBanner` verbs (P19-NEON-0092, P19-NEON-0236) — rejected/parked in Appendix Z.
- **Reason:** Preserves seamless Excel portability while keeping the v1 automation surface small, auditable, and free of overlap with Excel's own object model.

### Section: XV. Testing the experience

### D-90: Two-phase usability and automated scripted-key evaluation protocol

- **Authority:** Decider ruling | **Primary topic:** XV. Testing the experience | **Clusters:** `CL-15-03`
- **Options & sources:**
  - **Role-matched cohorts (8–10 per cycle) + task-scaled time budgets + mandatory 90% model comprehension quiz + scripted-key conformance pass** [CL-15-03] (P19-FABLE, P19-GPT; `P19-FABLE-0216`, `P19-GPT-0080`): Pair goal-based human sessions (including real screen-reader users and a 5-statement true/false model-truth quiz at 90% pass) with deterministic scripted-key runs and interaction/file logs.
  - **Fixed 8-participant panel / uniform 15-minute task blocks without comprehension quiz** [CL-15-03] (P19-CARBON, P19-NEON; `P19-CARBON-0102`, `P19-NEON-0093`): Run a fixed 8-person panel (4 analysts, 2 auditors/engineers, 2 managers) or uniform 15-minute blocks scored on task completion time alone.
- **Decision:** Adopt a two-phase evaluation protocol: (1) goal-based zero-hint moderated sessions with 8–10 role-matched participants per release cycle (including real keyboard-only and screen-reader users), task-scaled time budgets (5–20 min), and Fable's mandatory 5-statement model-truth comprehension quiz (`>=90%` pass bar); (2) GPT's automated scripted-key conformance pass recording interaction logs, screen-reader transcripts, and Excel round-trip file diffs.
- **Not taken (Appendix Z):** Uniform 15-minute mixed-participant-only testing without a model-truth comprehension quiz (P19-CARBON-0102, P19-NEON-0093) — subsumed into the stronger two-phase protocol.
- **Reason:** Combines real human comprehension checks with deterministic regression automation.

### Section: Appendices

### D-91: Canonical cast workbook fixture contracts (GPT/Pro 21 verified figures) across SB1-SB7

- **Authority:** Decider ruling | **Primary topic:** Appendices | **Clusters:** `CL-15-04`, `CL-15-05`, `CL-16-05`
- **Options & sources:**
  - **Pin SB1–SB3 to GPT/Pro 21's arithmetically verified cast fixtures while merging the strongest pass criteria from all four** [CL-15-04] (P19-GPT; `P19-GPT-0081`, `P19-GPT-0082`, `P19-GPT-0083`): Use one canonical fixture per workbook: Inherited `F18` broken `Revenue.Window` `#REF!` + missing `Rates2024.xlsx` + cached-value awareness; Budget live note + price edit + blocked-spill recovery + Excel-visibility comprehension; Sales `Orders` 25k-row table with 1 Pending order (`SO10482`, `4,200`) missing from `Report!B5` criteria + repair + Classic undo.
  - **Use Carbon's, Fable's, or Neon's alternate planted faults (`#DIV/0!` / trailing `'West '` space / timestamp mismatch / `$20000` ceiling)** [CL-15-04] (P19-CARBON, P19-FABLE, P19-NEON; `P19-CARBON-0103`, `P19-CARBON-0104`, `P19-CARBON-0105`, `P19-FABLE-0209`, `P19-FABLE-0210`, `P19-FABLE-0211`, `P19-NEON-0094`, `P19-NEON-0095`, `P19-NEON-0096`): Maintain separate or alternate planted bugs across scripts.
  - **Pin SB5–SB7 to GPT/Pro 21's exact fixtures (`Loan` 1M/10.5%, `DiscountFunctions.xll`, 3-hunk + conflict `Budget` proposal) enriched with hot-reload and audit-trail checks** [CL-15-05] (P19-GPT; `P19-GPT-0085`, `P19-GPT-0086`, `P19-GPT-0087`): Use one canonical fixture per storyboard while adding Fable/Carbon's add-in hot-reload check to SB6 and Neon's Audit Log / change-trail check (persisted per Ruling #14) to SB7.
  - **Use alternate add-ins (`FinKit.xll`, `TaxPack.xll`) and alternate Loan/Budget numbers** [CL-15-05] (P19-CARBON, P19-FABLE, P19-NEON; `P19-CARBON-0107`, `P19-CARBON-0108`, `P19-CARBON-0109`, `P19-FABLE-0213`, `P19-FABLE-0214`, `P19-FABLE-0215`, `P19-NEON-0098`, `P19-NEON-0099`, `P19-NEON-0100`): Keep each source's separate add-in name and numbers.
  - **GPT/Pro 21 verified fixture contracts (Budget 216k/yr, Sales 25k Orders w/ SO10482 Pending 4,200, Loan 1M@10.5%/360m, Stock 30-day schedule separating `INDEX` delay from `B8` iteration, `Inherited.xlsm` w/ `Revenue.Window` `#REF!`)** [CL-16-05] (P19-GPT, P21; `P19-GPT-0358`, `P19-GPT-0359`, `P19-GPT-0360`, `P19-GPT-0361`, `P19-GPT-0362`, `P19-GPT-0363`, `P21-0122`): Use the complete formula-level specification already cross-checked across all 73 Pro 21 interactive scenes and independent aggregation loops.
  - **Fable's pinned cast numbers (Budget 2027 3.94M rev, Sales 1,204,318.50 vs 1,251,907.25, Loan 250k@6.5%/30y, `Inherited.xls` 4,226 errors from 3 roots)** [CL-16-05] (P19-FABLE; `P19-FABLE-0010`): Adopt Fable's smaller-scale USD/unit figures and `.xls` Inherited baseline.
- **Decision:** Pin Appendix D and all seven storyboard scripts (SB1–SB7) to **GPT/Pro 21's arithmetically verified fixture contracts (`P19-GPT-0358..0364`, `P21-0122`)** so every screenshot, script, and assertion shares one arithmetic truth (and already aligns with D-03 on Stock). Enrich SB1–SB7 with Fable's comprehension checks (as-saved awareness on Inherited, Excel-visibility on Budget) and add-in hot-reload without restart (SB6), Carbon/Neon's non-blocking cold-start and zero-macro-execution bars (`100%` avoid macro run), persisted change-trail verification (D-14) in SB7, and Neon's UT-8 keyboard+SR cross-cast audit.
- **Not taken (Appendix Z):** Fable's alternate cast numeric set (P19-FABLE-0010, P19-FABLE-0209..0215) and Carbon/Neon's divergent planted fault numbers (P19-CARBON-0103..0109, P19-NEON-0094..0100 where numeric fixtures diverge) — parked in Appendix Z.
- **Reason:** Eliminates conflicting numbers across Design Book chapters and automated test fixtures.

### Section: XV. Testing the experience

### D-92: Two-tier golden visual regression matrix (25 workbookxview Tier 1 + lifecycle Tier 2)

- **Authority:** Decider ruling | **Primary topic:** XV. Testing the experience | **Clusters:** `CL-15-08`
- **Options & sources:**
  - **Two-tier matrix: 25 workbook×view + Explorer + paired normal/exception/active lifecycle states across 3 themes, 200% zoom, and reduced motion** [CL-15-08] (FRAMING, P19-FABLE, P19-GPT, P21; `P19-GPT-0299`, `P19-FABLE-0217`, `FRAMING-0144`, `P21-0013`): Cover every workbook×view intersection (5×5 = 25), both Explorer modes, the Strand component sheet, and paired normal/exception/active storyboard states at 1440×900, 1280×720@200% zoom, high-contrast, dark, and reduced-motion.
  - **Compact 24–27 curated screenshot set** [CL-15-08] (P19-CARBON, P19-NEON; `P19-CARBON-0110`, `P19-NEON-0102`): Capture a fixed list of 24–27 representative scenes across the three themes.
- **Decision:** Adopt a two-tier golden visual matrix: **Tier 1** (fast per-milestone check) captures the 25 workbook×view states + docked/standalone Formula Explorer + Strand component sheet across `cockpit-light`, `cockpit-dark`, `high-contrast`, and 200% zoom; **Tier 2** (full sweep) captures every storyboard's paired normal/exception/active lifecycle states (at rest, mid-edit+seam, mid-calc/as-saved, error+root, agent proposal, print preview, focus per control).
- **Not taken (Appendix Z):** Running an unbounded 5×5×8 full Cartesian cube on every milestone (P19-CARBON-0110, P19-NEON-0102) or limiting coverage solely to 24 static rest states — balanced cleanly across Tier 1 and Tier 2.
- **Reason:** Keeps routine checks fast while still covering every critical abnormal and interactive visual state on sweeps.

### Section: Appendices

### D-93: Unified M0-M6 front-end implementation sequence

- **Authority:** Decider ruling | **Primary topic:** Appendices | **Clusters:** `CL-16-04`
- **Options & sources:**
  - **M3 CustomXML + Declarative Skin Engine + Notebook -> M4 Model Map -> M5 App View + Print -> M6 Agent Proposals + Hardening** [CL-16-04] (P19-FABLE, P19-NEON; `P19-FABLE-0240`, `P19-FABLE-0241`, `P19-FABLE-0242`, `P19-FABLE-0243`, `P19-NEON-0121`, `P19-NEON-0123`): Ship CustomXML persistence and the declarative skin engine in M3 alongside Notebook (the flagship Jupyter-style compute view), build Model Map in M4, build declarative App View + Print on top of the skin engine in M5, and complete Agent Proposals + Macro/Coded-Extension Hardening in M6.
  - **Ship App View + Print + Skins in M3 ahead of Notebook (M4) and Model Map (M5)** [CL-16-04] (P19-CARBON; `P19-CARBON-0125`, `P19-CARBON-0126`): Deliver `Loan` App view before `Budget` Notebook.
- **Decision:** Adopt one canonical milestone sequence honoring D-02, D-04, D-06, D-09, D-10, D-11, and D-14: **M0** Merged Skin Architecture, Contract v2, Strand v0.2, Scale & Undo Foundation; **M1** Fresh One Shell, Classic Parity (exact Excel key tips) & Standalone Formula Explorer Teaser; **M2** Inspector + Docked Formula Explorer + Formula Lens; **M3** CustomXML View Persistence + Declarative Skin Engine + Jupyter-style Notebook; **M4** Model Map (reader -> named-port/operand wiring -> RTD/macro-ticked playback controls); **M5** App View (easy forms maker & reader presentation) + Print Engine; **M6** Agent Staged Proposals + File-Persisted Change Trail + Macro Undo architecture + Signed Coded Extensions + VBA Views API.
- **Not taken (Appendix Z):** Carbon's App-before-Notebook ordering (P19-CARBON-0125 before P19-CARBON-0126) and Neon's Standalone-Explorer-last M6 ordering (P19-NEON-0124) — unified cleanly and superseded sequences noted in Appendix Z.
- **Reason:** Delivers high-impact milestones early (Classic + Standalone Explorer teaser in M1, Inspector in M2, Persistence + Notebook in M3) while building each layer cleanly on the previous one.
