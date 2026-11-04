# DNA Calc: the bulk-port plan (2026-10-23)

This plan replaces the roll-out in `ROLLOUT_BRIEF.md` and the bead-by-bead build loop in
`ULTRACODE_WORK_PROMPT_OPUS_5_5.md`. The architecture in `DNA_CALC_REIMPLEMENTATION_PLAN.md`
still holds: the library split, the layering, Excel as the only model, and Rust everywhere,
running natively and in the browser. What changes is how the work is done.

## 0. Owner direction, 2026-10-27

The bulk port is functionally complete. The work now goes in this order of priority; each line
names its tracker epic.

1. **Retire the legacy source repositories** (dc-jnt): finish the items each assessment named
   (`parked/assess/*.md` outside the repository), review their tests under a test migration plan
   (dc-jnt.1), evaluate local-only content, then archive each GitHub repository read-only.
2. **Every in-scope function computes as Excel does** (dc-zmk): no refusals left among the
   functions DNA Calc means to cover (INFO, CELL, FILTERXML, the 16th-digit tie, the missing
   functions). Out of scope for now: functions whose Excel implementation relies on an external
   service, and pivot-table functions (moved to dc-8wp). Hard numerical bit-exactness cases such
   as the PMT family may stay outstanding (dc-j9k).
3. **Text, collation and character support** (dc-zsn), soon and without blockers.
4. **Circular references with full Excel parity, iteration optional** (dc-y2o).
5. **Publish DNA OneCalc and DNA Calc on dnacalc.com** with an owner-approved update pipeline
   (dc-ysl). The current state of the site does not matter.
6. **Add-ins** (dc-snz): user-defined functions hosted by DNA OneCalc first (.xll and VBA;
   JavaScript later), then VBA in workbook files in DNA Calc, then Windows parity with full COM
   client support. The VBA engine (OxVba, to be renamed) and OxForms continue as their own
   project, with a debugger and compiled targets beyond Excel's VBA; DNA Calc pins them.
7. **Pivot tables end to end** (dc-8wp), soon after.
8. **Functions handbook** (dc-dfl): its own project and the authoritative reference of the
   functions, referenced to calc-func's implementations and metadata.
9. **Performance** (dc-dxm, dc-u5c) waits until dnacalc.com is published and the core is
   settled.

The IDE repositories are reviewed separately and may be rebuilt.

## 1. Why the change

From 14 to 21 October the repository gained 232 commits and 131k lines of Rust. Yet only 51 of
Excel's 518 functions reached the catalogue, the engine is 2.3k lines, and there is no
application a user can open. The largest crate is the process tooling (`xtask`, 42k lines with
its tests).

Each bead was one small function family. It went through an implementer, two refuters, a judge
and one or two reviewers, receipts on three platforms, and an Excel capture for every row. Every
refusal also needed a bead of its own. On 21 October that meant 32 beads created and 7 closed.
Six logical functions cost about two hours and over a million tokens. Lanes that all edited the
same test file spent much of their time on merges.

The legacy code already does most of what is needed:

| Legacy | Size | What it holds |
|---|---|---|
| OxFunc | 140k lines of source | 263 function modules covering about 500 functions, coercion, Excel numerics, number formatting |
| OxFml | 34k lines | parser, binder, evaluator, language service |
| OxCalc | 198k lines | grid engine, recalculation, dependency graph, structural edits, tables (plus the tree-reference language and the consumer layer, which stay behind) |
| OxDoc | 57k lines | xlsx reader and writer, fidelity, authoring |
| DnaTreeCalc | 183k lines | host core, shell, skin framework, Leptos skins, sheet, notebook and atlas stages |
| DnaOneCalc | 71k lines | the OneCalc formula app |

**Owner direction (2026-10-23):** port the existing code selectively but in bulk into a coherent
new version. That means all the functions, the full CalcFml scope, a correct engine that can be
optimised later, and front-end apps that can be skinned. Then get to the real new work:
add-in integration, testing against a large corpus, and performance. The third-party numeric
code the legacy functions use is not a licensing problem; the numeric-provenance quarantine is
withdrawn.

## 2. What stays

- **The libraries and their layering:** calc-value and calc-func (CalcFunc), calc-fml (CalcFml),
  calc-engine (CalcEngine), calc-doc-model and calc-doc (CalcDoc), calc-host (CalcHost), and the
  apps dna-onecalc and dna-calc. The layering check in `cargo xtask accept` stays.
- **Portable code:** portable packages build for wasm32 and touch no OS APIs.
- **One of everything:** one evaluator, one dependency graph, one capture lineage, one verdict
  path.
- **What the bulk port leaves behind:** the tree-reference language, the dual-profile model, and
  OxCalc's consumer layer.
- **The honesty core:**
  1. Only a value the engine calculated counts as calculated. A file-cached, stale or refused
     value never scores.
  2. What is not implemented returns a typed not-modelled refusal, and refusals are counted in
     every comparison. A refusal names the work package that owns it; it does not need a bead
     of its own.
  3. No faked behaviour, no hard-coded answers, no weakened assertions.
  4. Test the route the application runs.
  5. Claims never exceed the evidence.
- **Git:** master only, `git pull --no-rebase`, no branches, worktrees, rebase or force.
- **Excel on this machine** for captures, comparisons and probes. Any agent may use it.

## 3. What goes

- Refuters, judges and second reviewers per bead. One read-only review closes a work package.
- An Excel capture for every row before code lands. Correctness is measured afterwards, in bulk,
  by the corpus comparison (section 5).
- Receipts on three platforms per bead. Each commit is tested on Windows. Linux and wasm run as
  a whole-suite sweep at milestones (section 5).
- One bead for every refusal or gap. Gaps go into the owning package's notes. Corpus
  mismatches are filed in batches, one bead per root cause.
- Per-bead migration records under `migration/ports/`. The commit message names the legacy
  repository, commit and paths a port came from.
- The numeric-provenance quarantine, the `seams.json` traffic check, and the AGENTS.md and
  plan-bundle text checks in infra-m0.1's agent-contract case.
- The 204-task plan graph and its follow-ups. The tracker now holds the work packages below.

## 4. Work packages

Each package is one bead and one large piece of capability. One agent builds it in one long
session in its lane clone, porting legacy code wholesale, with types renamed and adapted to the
new APIs. Legacy tests come along wherever they still apply. The agent commits to master in
working chunks, each of which builds and passes `cargo test` for the touched crates on Windows.
Packages in the same lane run in order; lanes run in parallel.

### Lane F: functions (calc-func)

- **F1: math, statistics and engineering.** The remaining math and trigonometry, rounding,
  aggregate (SUBTOTAL, AGGREGATE), statistics (descriptive statistics and the distributions),
  engineering (bases, bits, complex numbers, Bessel, ERF) and random numbers. From OxFunc.
- **F2: text, dates, information and reference.** Text functions including TEXT and VALUE with
  number formats; dates and times; the rest of the information, logical and lookup functions
  (OFFSET, ADDRESS, IFS, SWITCH and the like); database functions. Web and cube functions get a
  typed refusal. From OxFunc, including `locale_format.rs`.
- **F3: financial, plus dynamic-array and LAMBDA functions.** Financial functions; then
  SEQUENCE, FILTER, SORT, SORTBY, UNIQUE, the stacking and shaping functions, MAP, REDUCE, SCAN,
  BYROW, BYCOL and MAKEARRAY. The array half needs L1.

### Lane L: formula language and engine (calc-fml, calc-engine)

- **L1: arrays, references and names.** Array constants, array evaluation and lifting, spill
  results, the reference operators (range, union, intersection, `#`, `@`), implicit
  intersection, defined names (workbook and sheet scope), LET and LAMBDA. From OxFml.
- **L2: engine semantics.** Spills (ownership and #SPILL!), dynamic dependencies (INDIRECT and
  OFFSET through the engine, previously dc-dumd), volatile functions, calculation modes,
  multi-sheet recalculation. From OxCalc's grid engine.
- **L3: the rest of the language, and edits.** Structured references and tables, 3-D references,
  external references (a typed refusal), the rest of R1C1, number-format and locale parsing.
  Cycles and iteration, and structural edits (inserting and deleting rows, columns and sheets,
  with references rewritten).

### Lane D: document, host and applications (calc-doc, calc-host, apps)

- **D1: document fidelity.** Open and save workbooks with dynamic-array and array formulas
  (previously dc-1u06 and cdoc-m3.3), defined names, tables, styles and themes; write the
  `_xlfn.` prefixes and the newer errors on save (previously dc-552j); calcChain and value
  caches. From OxDoc.
- **D2: the host round trip.** Save and delta projections, undo and redo, skin intents and
  subscriptions. The M1 proof belongs here: a real workbook is opened, edited, saved and
  reopened in Excel without repair. From DnaTreeCalc's host core.
- **D3: DNA OneCalc.** The browser app (formula entry, results, function help), its browser
  entry and its bundle. From DnaOneCalc and DnaTreeCalc's formula skin.
- **D4: DNA Calc.** The shell and the skin framework: the Excel grid skin first, then the
  notebook, tree and canvas skins. From DnaTreeCalc's shell, skin IR, Leptos skins and stages.

### Verification and process

- **V0: slim the runner.** Remove the withdrawn checks (section 3). Add `cargo xtask check`, which
  builds and tests every package on the host and, with `--wasm`, runs the portable packages'
  tests in Chrome and Firefox. This comes first.
- **V1: the corpus harness.** `cargo xtask corpus <dir>` captures each workbook in Excel once
  (and caches the capture), compares through the product route, and writes one summary: totals
  per workbook, mismatches and refusals grouped by root cause. Mismatches become beads in
  batches. The Ofgem workbooks are the first corpus; the oracle workbooks already in the tree
  join it. This also fixes compare's label for kernel refusals (previously dc-9vb7).
- **V2: a large public corpus.** Find and fetch a large set of public workbooks and record each
  one's licence. Downloads need the owner's yes.
- **V3: the milestone sweep.** One run of every package's tests on Windows, Linux (the WSL
  clone) and wasm (Chrome and Firefox) at a single master commit, committed as one receipt.

### Later

- **Performance.** Time Excel's own recalculation of the corpus against DNA Calc's (dc-10d9),
  then file optimisation work.
- **Add-in integration.** Scope to be set by the owner.
- **dnacalc.com.** A holding page, then the OneCalc publish. Both need the owner's rulings in
  dc-oy6l, and nothing is published without the owner's yes.

### Kept as they are

- dc-oy6l: the owner's decisions about the website.
- dc-8abf: the scope goal, deferred.
- dc-6qki: the cells that differ from Excel by one unit in the last place.

## 5. How correctness is shown

1. **Per commit:** `cargo test` for the touched crates passes on Windows.
2. **Per package:**
   - the package's own tests, ported and new, pass;
   - the corpus comparison is run before and after the package lands, and its summary goes in
     the close note;
   - one read-only review agent checks the honesty core and the close note against tool
     output.
3. **Per milestone:** V3's whole-suite sweep on all three platforms.
4. **Continuously:** the corpus summary is the measure of progress over a growing set of real
   workbooks. Mismatches and refusals are fixed by root cause.

Every statistic counts a refused formula as a failure (owner ruling, 2026-10-23). The headline
is always pass = matched Excel and fail = mismatched + refused (+ failed or missing), over all
formula cells, with the fail count broken down into those parts. A rate over calculated cells
only is never the headline.

The dc-ztvv corpus command also counts external link caches, circular held values and cells
awaiting capture inside fail, with each qualification shown separately. A saved-cache reference
alone cannot score a live Excel match. Files that time out retain their known source formula
counts as failures; a file whose inventory cannot be read is listed with an unknown count.

For local files, use `cargo xtask corpus <dir> --recursive --oracle --no-capture --dna-only
--shard 0/4 --timeout 60 --resume --out <outside-repository>/shard-0` (one invocation per shard).
The shard index is zero based and follows the workbook's SHA-256; duplicate content is listed
in inventory.json and evaluated once. Folder inputs and result hashes use a 64 KiB read buffer
through the shared portable SHA-256 implementation. A source changed after that inventory is
refused before parsing or calculation; resume takes a new snapshot. `--dna-only` calculates files without observations through
the existing comparison route, retaining an explicit source inventory with no Excel answers;
none can score a match. Existing committed/cached live captures still supply the comparisons.
Every attempt keeps its own log and payload. Resume verifies workbook, capture, executable and
result hashes; a partial, timed-out or changed result is recalculated. `--timeout` covers the source census, optional capture and comparison of one file.
Package admission and census run in an owned child process too. Use `cargo xtask corpus --merge
<shard-0>/summary.json --merge <shard-1>/summary.json ... --out <corpus-report>` to combine
disjoint reports of the same corpus and executable. Missing shards remain explicit.

Per-file summary.json and the corpus summary use the same counts. source-caches.json compares
the input package's saved values against live Excel observations, independently of DNA's values;
drift does not remove cells from the comparison. Array cache shapes not reconstructed by this
classifier stay explicitly unclassified. Root groups include inferred functions, difference shape
and observed read types; problem-functions.json gives the inferred function list. `--minimize`
authors a bounded dependency slice for every mismatching cell, retaining differing producers and
freezing other observed inputs. Its manifest records the DNA reproduction check, omissions and
pending Excel confirmation. These cases are authored inputs, not new Excel captures or scored
passes. Corpus workbooks and generated reports remain outside repositories. The owner's download
hold and separate-VM requirement for untrusted corpus captures still apply.

## 6. Order

1. **Now:**
   - V0, done by the orchestrator;
   - land the in-flight lane work (cdoc-jpha, dc-nzpw, dc-51ka, dc-xrxb);
   - close the beads whose code already passes on master.
2. **Then, in parallel:** lane F starts F1, lane L starts L1, lane D starts D1, and the
   orchestrator builds V1.
3. **After each package:** a corpus run, then a batch of root-cause beads.
4. **When F1–F3, L1–L3 and D1–D2 are in:** V3, then V2, performance, and the apps (D3 and D4,
   which can start earlier when lane D frees up).
5. **Add-in integration** once the owner sets its scope.

The tracker (`br ready`) holds these packages with their dependencies. The old plan bundle
(`BEAD_DETAILS.md`, `BEAD_GRAPH.md`, `beads.*.jsonl`, `build_plan.py`) stays in `docs/plan/` as
history and no longer describes the tracker.
