# DNA Calc re-implementation: roll-out brief

**Date:** 2026-10-12. **Bundle:** `DNA_Calc_Reimplementation_Plan` (plan authored 2026-10-05 by GPT-6 Pro from the owner brief v0.6).
**Purpose:** organize the 2.6 MB bundle into what is needed to start, record what this review verified, corrected and changed, and list the decisions only the owner can take. The plan itself stays the authority for design detail; this brief does not restate it.

## 1. The decisions

**Fixed by the owner (brief §1 and §2).** Seven units: CalcFunc (OxFunc), CalcFml (OxFml), CalcEngine (OxCalc), CalcDoc (OxDoc), CalcHost (extracted from DnaTreeCalc), DNA OneCalc (formula app, no engine), DNA Calc (Excel, notebook, tree and canvas skins). Excel only: the tree reference language and the dual profile are gone. CalcHost owns the editable workbook; CalcEngine is kept in step through one operation stream. Phase-1 calculation scope includes iterative calculation, data tables, CF/DV formulas, chart series and pivots. Windows, Linux and wasm from the first version. Exact and Fast numeric modes. Correctness first, no performance targets, no timeline. The Excel skin is the full Excel UI. Clean room. Lean process: tests, CI, the Excel oracle, beads. The notebook, tree and canvas skins are never deferrable.

**Decided by the plan (plan §1.2), with the review's verdict.**

| Question | Plan decision | Review verdict |
|---|---|---|
| Language | Rust throughout; an isolated .NET COM worker only as Windows oracle tooling. | Agree. |
| Repository | One Cargo workspace, one lockfile, layered crates, a target directory per agent clone or CI job. | Agree. Owner rule of 2026-10-12: master only, pull then work on one bead then commit then push, no branches, no worktrees; the agent contract written by `infra-m0.1` says so. |
| UI stack | Keep Leptos, Tauri and a Canvas2D grid; replace the host and projection architecture. | Agree; reassess only on a concrete failing interaction test. |
| Migration | Carry Func kernels after provenance review; restructure Fml and Doc; rewrite Engine and Host; rebuild both apps, reusing grid geometry and editor pieces. | Agree. CalcDoc firmed from "provisional" to "restructure" (its source is local, see §4). |
| Stateless | Caller-owned state handles, immutable shared pages and plans, prepare/commit under one host mutation owner, recalc as a separate cancellable publication. | Agree. |
| Skin persistence | Cell-backed hybrid: ordinary visible cells carry computation and prose, hidden locator names give stable identity, a versioned CustomXML manifest carries order, tree, canvas positions and view state, named functions are real defined-name LAMBDAs. No sidecar. | Agree. The earlier hidden `_names` sheet design is deliberately not adopted. |
| Oracle | One capture lineage (the smart-fuzzer COM acquisition), one verdict library for cached-`<v>` and live capture, old bundles as read-only importers. | Agree. |
| Corpus | Generated and owner-cleared workbooks first; Enron, EUSES and FUSE only after a recorded licence and privacy review. | Agree. |
| Numeric modes | Fast is the default everywhere with pinned portable math; Exact only on validated native x86-64 targets at first; an unsupported Exact request fails explicitly; Fast is scored against Exact with per-function tolerances. | Agree. One bead contradicted the default; fixed (§5). |
| Separable areas | Charts and drawings, pivots, printing and page layout, external data and analytics, the extended desktop command surface. Each gets its attachment points now and one planning bead after M3. | Agree. Their implementations remain first-version release requirements through the fail-closed checker. |

## 2. Architecture in brief

Dependency direction, enforced on resolved Cargo metadata: DNA Calc skins → skin contract and shared controllers → CalcHost → CalcDoc, the CalcDoc model, CalcEngine and CalcFml. CalcEngine → the model and CalcFml. CalcFml → CalcFunc → `calc-value`. DNA OneCalc → CalcFml and CalcFunc only, including transitively.

Crates: `calc-value`, `calc-func`, `calc-fml`, `calc-doc-model`, `calc-doc`, `calc-engine`, `calc-host`; apps `dna-onecalc`, `dna-calc`; `ui/`, `platform/`, `tools/` (xtask and the headless `calcctl`), `corpus/`.

The contracts that everything else hangs on:

- One operation vocabulary (`WorkbookOp` inside `SessionOp` batches) that UI, headless tooling and later macros all use.
- Host edits are prepare, reserve, commit under a single mutation owner; document and engine patches share the base revision.
- Recalculation is a separate generation that publishes deltas with provenance (`Calculated`, `FileCached`, `Stale`, `NotModeled`) and counted escalation facts. A cached file value can never be scored as a calculated pass.
- CalcFml exposes public compile, instantiate and evaluate with `Arc` plans keyed by R1C1 template identity, explicit resumable frames, no `Rc`/`RefCell` across the boundary; it also owns Excel reference binding and number formatting, with a `NumberTextService` injected into CalcFunc.
- CalcFunc is one generated catalogue (metadata, dispatch, help, oracle routing and numeric-mode sensitivity declared once).
- The skin contract is capability-organized commands and bounded projections, with four state classes kept apart: authored data, calculated facts, durable per-skin layout, ephemeral editor state.

## 3. The graph

| Milestone | Tasks | Exit |
|---|---|---|
| M0 bootstrap checkpoint | 8 | Workspace, three-platform CI, acceptance runner and architecture checks, source pin, numeric quarantine, evidence backup, value types, neutral operation types. |
| M1 first functional milestone | 16 | `infra-m1.3`: real two-sheet Excel workbook opened, calculated through the product host, compared provenance-first with a deliberately wrong cache caught, edited, saved, reopened in Excel without repair. |
| M2 sharing and four skins | 20 | 100k fill-down formulas share one plan; OneCalc has no engine; notebook, tree, canvas and grid edit one workbook; early real-Excel persistence cases (`infra-m2.1`). |
| M3 workbook-wide semantics and attachments | 57 | Names, tables, 3D, dynamic references, spills, external snapshots in one graph (`infra-m3.2`); advanced persistence matrix (`infra-m3.3`); chart, pivot, print and external attachment points; the five area planning beads. |
| M4 structural editing, transactions, settings | 27 | Atomic structural edits, undo, calculation modes, numeric preferences, counted escalations, phase-2 seams with real callers (`infra-m4.1`). |
| M5 hard calculation | 19 | SCC iteration, one- and two-variable TABLE, CF, DV from independent Excel captures (`infra-m5.3`). |
| M6 core Excel UI and complete authoring | 39 | Every core command family has UI and headless tests; full core authoring and reopen matrix (`infra-m6.1`). |
| M7 measured conformance | 13 | Corpus floors, numeric evidence, Fast determinism, named benchmarks, fuzzing, one report (`infra-m7.10`). |
| M8 core integration and release scope | 5 | Shipped packages use the tested route (`infra-m8.1`); fail-closed release checker (`infra-m8.2`). |

Shape facts, measured on the imported graph:

- Single root `infra-m0.1`. Five lanes open once M0 closes: `infra-m1.1` (verdict library), `cfunc-m1.1`, `cfml-m1.1`, `cdoc-m1.1`, `ceng-m1.1`.
- Prerequisites with the widest fan-out: `infra-m0.4` source pin (28 direct dependents), `cfunc-m1.2` base coercions (22), `infra-m1.2` Excel capture adapter (10), `cfunc-m3.23` Fast backend (9).
- Betweenness bottlenecks per bv: `chost-m4.1`, `chost-m3.1`, `ceng-m4.1`, `infra-m6.1`, `cfml-m1.3`, `ceng-m3.1`, `infra-m5.3`, `chost-m4.8`, `ceng-m3.4`, `cfunc-m3.25`, `cfml-m3.4`. These are the beads to staff with the strongest agents and refute hardest.
- M1 critical path: `infra-m0.1` → `infra-m0.3`, `infra-m0.4` → `cfunc-m0.1` → `cdoc-m0.1` → the four lanes (`cfunc-m1.1`→`m1.2`; `cfml-m1.1`→`m1.2`→`m1.3`; `cdoc-m1.1`→`m1.2`→`m1.3`; `ceng-m1.1`→`m1.2`) → `chost-m1.1` → `chost-m1.2` → `infra-m1.3`, which also needs `infra-m1.2` (Excel on Windows).
- Beads needing live Excel (platform `oracle`, 16 tasks): `infra-m1.2`, `infra-m1.3`, `infra-m2.1`, `infra-m3.1`, `infra-m3.2`, `infra-m3.3`, `infra-m4.1`, `infra-m5.1`, `infra-m5.2`, `infra-m5.3`, `infra-m6.1`, `infra-m7.2`, `infra-m7.4`, `infra-m7.5`, `infra-m7.6`, `infra-m7.7`.

## 4. What this review verified

**Every legacy path a bead cites exists locally.** Selected sizes: OxFunc `surface_dispatch.rs` 364 KB, `registry.rs` 112 KB, `function_call.rs` 52 KB, `locale_format.rs` 64 KB, parity ledger 528 lines; OxFml `eval/mod.rs` 288 KB, `syntax/` 93 KB, `format/` 89 KB, `language_service/` 44 KB, `binding/profile.rs` 20 KB; OxCalc `reference_engine.rs` 208 KB, `invalidation.rs` 144 KB, `r1c1_plan.rs` 76 KB, `spill_ledger.rs` 12 KB, `differential.rs` 32 KB, `workbook_nodes.rs` 24 KB, `optimized_storage.rs` 20 KB, `grid/scale/` 604 KB, `consumer.rs` 1.46 MB; DnaTreeCalc `dnacalc-host-core/workbook.rs` 156 KB, `dnacalc-skin-ir/` 373 KB, `dnacalc-stage-sheet` (geometry, render plan, selection) 28 to 36 KB each, `dnacalc-stage-notebook/` 200 KB, `dnacalc-conform/` 328 KB, `dnacalc-arch-gates/` 44 KB; OxVba `oxvba-host/engine.rs` 52 KB; ExcelFunctionsHandbook `data/functions/` 6.5 MB.

**The authoring run's "unavailable" sources are all present.** `OxDoc/crates/oxdoc-model/src/lib.rs` is 5,939 lines in one file with 191 `OxCalc*` symbols; `OxDoc/crates/oxdoc-xlsx/src/lib.rs` is 37,648 lines in one file with the `RoundTripRefusal` taxonomy, `WriteMode::FreshExport` and the W010 tranche-2 authoring (cell add, formula add/remove/change, sheet lifecycle, committed 2026-09-15). `Foundation/notes/PROGRAM_INVESTIGATION_2026-09-10.md` (57 KB) is the investigation the brief's lessons come from; its dual-profile direction is superseded. The brief's line "OxDoc cannot do these yet" predates that commit.

**The import works.** br 0.1.34 created 252 issues, `br dep cycles` is clean, one task is ready, and a flush-only re-export is identical to the import (no field dropped, dependencies intact). bv triage runs and picks `infra-m0.1`.

**Repository state at review time.** OxFunc `6fca3518` clean; OxFml `e2b6149` with `eval/mod.rs` modified; OxCalc `5ea74767` with six modified files including `consumer.rs`; OxDoc `baf05f9` with `oxdoc-xlsx/src/lib.rs` modified and an untracked cell-add conformance fixture; DnaTreeCalc `a1e486d` with only `.beads` dirty; Foundation `dd36664` clean on `master`. Every repository except OxFunc also carried 13 to 19 unpushed commits.

**The uncommitted legacy work builds, with one exception.** OxDoc: `cargo check -p oxdoc-conformance` clean and `cargo test -p oxdoc-xlsx --lib` 247 passed. OxCalc: `cargo check -p oxcalc-core` clean and the 11 `w063_3` transactional-edit tests pass. OxFml: the change compiles, but it replaces a test assertion with a print, and the run shows 310 SWITCH and 4 IFS disagreements between the surface-direct and wrapper routes for 1x1-array arguments.

**What was done about it on 2026-10-12.** OxCalc `1bd514ef` (calc-1q03.3 in progress, spec §10.1), OxDoc `c103183` (oxdoc-1vg.16 in progress, conformance probe wired in) and DnaTreeCalc `bf6dfd7` (bead flush) were committed and pushed together with the earlier unpushed commits. The OxFml exploration was not committed: it is `stash@{0}` on OxFml main, and bead `fml-gpz` records the mismatches, the stash and the fix. The source pin in `infra-m0.4` can now use committed, pushed blobs everywhere.

**The clean-room concern is real.** `x87.rs` (890 lines, 21 `asm!` blocks) says in its header that its EXP, LN, LOG and POWER sequences follow `fpw32/tran/i386/87tran.asm` and the `87disp.asm` control word from the NT CRT source tree; its ATAN path cites W111 black-box witnesses instead. The plan's quarantine (`infra-m0.5`, `cfunc-m3.24`) is the right response; the bead now says what the agent will find.

**The oracle evidence is present and unprotected.** `OxFunc/smart-fuzzer/cache` holds 1,618 files and `smart-fuzzer/runs` 26,774 files, untracked and not git-ignored. Byte size was not re-measured (the owner reported 3.4 GB).

## 5. What this review changed

**Bundle scripts.** `validate_plan.py` and `build_plan.py` read and write UTF-8 explicitly; before, both used the Windows console code page and the validator crashed on the first section sign, which also stopped `stage_br_import.py` before it reached `br`. `stage_br_import.py` now runs `br init --prefix dc`, verifies cycles, stats and the ready list after import, runs `bv --robot-insights` when available, decodes subprocess output as UTF-8 and survives a cp1252 console. `build_plan.py` now also generates `BEAD_DETAILS.md`, which was previously a one-off artifact that regeneration would have left stale.

**Plan text** (`DNA_CALC_REIMPLEMENTATION_PLAN.md`): revision note under the status header; executive decision on CalcDoc; §1.1 (two stale rows replaced, verification sentence, evidence counts); §7.1; §10.2; §11.1 CalcDoc row; §12 first risk; §13 import statement and prefix; evidence reference E6 and new E7.

**Beads**, all made in `build_plan.py` and regenerated:

| Bead | Change |
|---|---|
| `infra-m0.1` | Also commits the plan bundle under `docs/plan/` and the `.beads/` tracker, and writes `AGENTS.md` as the agent operating contract (claim, acceptance receipt, one commit per bead on master with the bead ID, close with evidence, follow-up beads instead of scope creep, pull/work/commit/push on master with no branches or worktrees, a full clone per concurrently working agent, never two cargo runs in one target directory, the honesty rules). New acceptance case `agent_contract`. |
| `infra-m0.4` | Names the eleven local repositories, the four dirty trees to commit or stash first, and the archived DnaOneCalc; acceptance now requires committed blobs and no longer presumes OxDoc is missing. |
| `infra-m0.5` | States what `excel_numeric/` contains (the NT CRT citations, the W111 black-box ATAN path, the tables and ground-truth files) so the quarantine has a per-file target list. |
| `infra-m0.6` | File counts for the cache and runs directories. |
| `cfml-m2.4` | Port-from now points at OxFml's existing lazy IFS/CHOOSE/SWITCH/IFNA scheduling as well as OxFunc's laziness metadata. |
| `chost-m4.8` | "Default native apps to Exact once certified" replaced by the plan's rule: Fast everywhere, Exact an explicit selection with its evidence badge. |
| all `cdoc-*` | Port-from text names the real OxDoc files, sizes, the refusal taxonomy, `FreshExport` and the existing authoring; keeps the rule that exact symbols are pinned before copying and that no `OxCalc*` type enters CalcDoc. |
| priorities | Explicit P0 set unchanged (50). Others banded by milestone and lifted one band when they transitively block 30 or more tasks: 31 P1 (the Fml, Engine and Doc spine of M3 and M4 plus `cfunc-m3.7`, `.16`, `.19`, `.22`), 48 P2, 70 P3, 5 P4 (M8). |
| `source-audit.json` | Method, local verification list and repository state added; the "unavailable" list now holds only the evidence directories. |

**Unchanged on purpose:** every bead ID, the graph topology (204 tasks, 48 epics, 527 blocking edges, no bead added or removed), all existing acceptance case names, the ten decisions, the milestone exits.

## 6. Deferrable details

These can change later without disturbing the roll-out, so they were left alone:

- Each task has exactly two or three named acceptance cases and shared boilerplate. That is enough to start M0 and M1. Later beads, especially the 22 M6 UI families, will need their `Do` text sharpened when their prerequisites exist and the command inventory (`infra-m3.1`) is captured. Polish them when claimed, not now.
- Labels are milestone, library and platform only. Add `lane:` or `area:` labels if orchestration wants them.
- Bead descriptions cite plan sections by number; keep the plan's numbering stable or regenerate the citations.
- Acceptance commands assume a `cargo xtask accept --id … --platform … --require-all` runner; its real shape is decided by `infra-m0.3`, and the M0 beads use bootstrap-native cargo commands until it exists.
- The priority bands and the 30-dependent threshold live at the top of the "Priority bands" block in `build_plan.py`.

## 7. Roll-out sequence

**Step 0, owner decisions.** See §8. The repository name is the only item that blocks step 2.

**Step 1, safety, before anything touches OxFunc.**

1. Evidence backup, the raw half of `infra-m0.6`. Recommended store: a public sibling repository `DnaCalc/CalcFunc-Evidence` that holds `gzip -n` blobs of `smart-fuzzer/cache` and `smart-fuzzer/runs`, a manifest of raw sha256 hashes and sizes, and no third-party binaries. The content is JSON that compresses about 30:1 (the 233 MB `PMT.jsonl` deflates to 7 MB), so 3.4 GB raw packs to a few hundred megabytes and needs no Git LFS. Four raw files exceed GitHub's 100 MB limit, so the gzip step is not optional. The Intel MKL wheel and DLLs under `runs/w108a-reference-hunt/mkl_dl/` (32 files, 746 MB) are re-downloadable: record their version and hash, do not store them. This reuses the `archive-transcripts.sh` pattern of OxFunc-History without mixing transcripts and evidence in one repository; OxFunc-History's own README defines it as the transcript archive, and its 137 MB history should stay small. A second copy on a different medium (OneDrive or an external drive) completes the bead's two-backup rule.
2. Done on 2026-10-12: the uncommitted work in OxCalc, OxDoc and DnaTreeCalc was built, tested, committed and pushed; OxFml's exploration is stashed and filed as `fml-gpz` (see §4).

**Step 2, create the repository and import the plan.**

```text
python stage_br_import.py --destination C:\Work\DnaCalc\<NewRepo>
cd C:\Work\DnaCalc\<NewRepo>
git init -b master
mkdir docs\plan  (copy this bundle there)
git add .beads docs
git commit -m "Import the DNA Calc re-implementation plan and bead graph"
```

**Step 3, bootstrap serially.** One agent, one cargo at a time: `infra-m0.1` (skeleton, `docs/plan`, `AGENTS.md`), then `infra-m0.3` (acceptance runner and architecture checks), `infra-m0.4` (source pin), `infra-m0.2` (CI). `infra-m0.5` (numeric quarantine) and `infra-m0.6` (evidence backup) run alongside because they need no cargo. Then `cfunc-m0.1` and `cdoc-m0.1`. Review the skeleton and `AGENTS.md` yourself before fanning out: every later agent inherits them.

**Step 4, fan out.** Five lanes: `infra-m1.1`, `cfunc-m1.1`, `cfml-m1.1`, `cdoc-m1.1`, `ceng-m1.1`. No branches and no worktrees: each concurrently working agent has its own full clone on master and pulls before starting; a rejected push is resolved by pulling master and re-running the bead's acceptance before pushing again. The pattern that worked in the W011 and W012 campaigns still applies: per bead one implementer plus independent refuters (a tests lens that runs cargo and an ownership lens that reads), and the decisive bead reviewed by the owner before closing.

**Step 5, close M1.** `chost-m1.1`, `chost-m1.2`, then `infra-m1.2` and `infra-m1.3` on this machine with its Excel (owner-run; the captured evidence replays on the other platforms). This is the first demo: open, calculate, compare, edit, save, reopen in Excel.

**Step 6, M2.** The four skins and the Fml sharing work run in parallel; `infra-m2.1` needs Excel again.

**Checkpoints worth a human look:** after `infra-m1.3`, after `infra-m2.1`, and before fanning out the 21 M3 function-family ports (decide how many agents, and in what order, from the P1 list).

## 8. Decisions taken on 2026-10-12 and items still open

**Decided.**

- Excel work runs on this machine. The 16 `oracle` beads are owner-run steps here; their retained captures replay on Windows, Linux and wasm in CI.
- No worktrees, ever. Everything happens on master as pull, work on one bead, commit, push. Applied to `infra-m0.1`, the plan's CI section, the README and this brief. Agents that must work at the same time use separate full clones on master.
- The uncommitted legacy work was reviewed, built, committed and pushed (§4); the one dishonest change was stashed and filed instead.
- Until the new repository exists, this bundle lives in `Foundation/plans/calc-reimplementation-2026-10/`. `infra-m0.1` copies it into the new repository's `docs/plan/`, which becomes the authoritative copy from then on.

**Decided on 2026-10-14.**

- The repository is `DnaCalc/DnaCalc` (local checkout `C:\Work\DnaCalc\DnaCalc`, branch `master`, tracker prefix `dc`, private for now). The 252 bead records were imported with br 0.1.34 on creation; `docs/plan/` holds this bundle and `docs/plan/br-import-log-2026-10-14.json` the import receipt. From here on this copy and this tracker are authoritative; the copy in `Foundation/plans/calc-reimplementation-2026-10/` is the 2026-10-12 snapshot.
- The evidence lives in the public repository `DnaCalc/CalcFunc-Evidence` (first archive 2026-10-14, commit `4d5320df`): 28,360 files, 2.70 GB raw, 261 MB of deterministic gzip blobs with a SHA-256 manifest; the 32 MKL binaries (746 MB) are hashed in `excluded.tsv` but not stored; a 100-file sample verified. The second copy on another medium that `infra-m0.6` requires is still to be made.
- No license file has been chosen for the new repository yet; `infra-m0.1` or the owner adds one.
