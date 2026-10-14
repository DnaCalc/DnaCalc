# DNA Calc re-implementation planning bundle

## Start here

1. **`ROLLOUT_BRIEF.md`** (ten minutes): what the plan decides, how the graph is shaped, what the 2026-10-12 review verified and changed, the roll-out sequence and the open owner questions.
2. **`DNA_CALC_REIMPLEMENTATION_PLAN.md`**: the architecture, the answers to the brief's ten open questions, Rust API sketches, state and transaction model, four-skin persistence, Excel UI inventory, migration table, milestones, tests, risks and the complete readable graph (§13).
3. **`BEAD_GRAPH.md`** and **`BEAD_DETAILS.md`**: the graph as an outline and every bead in full.

The graph holds **204 executable tasks and 48 grouping epics (252 records)** with **411 named acceptance cases** and 527 blocking edges. Everything in it is a proposed implementation requirement for the new repository, not implemented behaviour. Epics hold no work. The five separable-area planning tasks (charts, pivots, printing, external data, extended UI) produce later implementation graphs; those implementations are still mandatory for the first-version release.

## Provenance

Authored on 2026-10-05 by GPT-6 Pro from the owner brief `calc-reimplementation-planning-gpt6pro.md` (draft v0.6, kept in `Foundation/prompts/`). Reviewed and corrected on 2026-10-12 against the local checkout under `C:\Work\DnaCalc`: see the revision note at the top of the plan and the change list in `ROLLOUT_BRIEF.md` §5.

## Files

| File | Use | Generated? |
|---|---|---|
| `ROLLOUT_BRIEF.md` | Organized summary, review findings, roll-out sequence, owner questions. | no |
| `DNA_CALC_REIMPLEMENTATION_PLAN.md` | The single main plan. Only its §13 outline is generated. | §13 only |
| `BEAD_GRAPH.md` | Milestone → epic → task outline with blocking prerequisites. | yes |
| `BEAD_DETAILS.md` | Every bead description in readable Markdown, identical to the JSONL. | yes |
| `beads.plan.jsonl` | The requested planning schema; library-prefixed provisional IDs. | yes |
| `beads.br.jsonl` | Native `br` Issue/Dependency export. IDs unchanged. Timestamps are regeneration times, not dates. | yes |
| `br-id-map.json` | Identity map from planning IDs to import IDs, kept for explicitness. | yes |
| `coverage.json` | Milestone, UI, numeric, persistence and phase-2 seam mappings to bead IDs. | yes |
| `source-audit.json` | Inspected sources, the 2026-10-12 local verification and repository state. | yes |
| `topological-order.md` | One valid task ordering; not a schedule. | yes |
| `validation-report.json` | Checks executed on the artifacts, with explicit not-run fields. | by `validate_plan.py` |
| `build_plan.py` | **The source of truth for the graph.** Edit this, then regenerate. | source |
| `validate_plan.py` | Standard-library validator with negative self-tests. | source |
| `stage_br_import.py` | Imports the native export into a new `br` workspace and verifies it. | source |
| `calc-reimplementation-planning-gpt6pro.md` | The owner brief the plan answers. | input |

Edit the generator, never the generated files: `python build_plan.py` rewrites every generated file, including `BEAD_DETAILS.md` and the plan's §13.

## What has been validated, and what has not

Executed on 2026-10-12 on Windows 11 with Python 3.12, br 0.1.34 and bv 0.15.2:

- `python validate_plan.py --self-test`: all records, required sections, unique IDs and test names, no dangling, duplicate or cyclic dependencies, coverage references, outline presence, native and planning record equivalence, every task an ancestor of the release checker, negative tests rejected.
- `python stage_br_import.py --destination <new dir>`: `br init --prefix dc`, `br sync --import-only` created all 252 issues, `br dep cycles` clean, `br stats` shows 1 ready and 203 blocked, `br ready` returns `infra-m0.1`. A `br sync --flush-only` re-export reproduced every title, description, priority, label and dependency with no field dropped.
- `bv --robot-insights`, `bv --robot-next` and `bv --robot-plan` run on the imported graph; `--robot-next` picks `infra-m0.1`.

**Not run:** Rust compilation, the legacy repositories' test suites, live Excel capture or reopen, any build. The plan's Rust signatures are design sketches; acceptance commands describe tests and tools the new repository must implement.

The original authoring run could not reach OxDoc, the 2026-09-10 investigation note or `consumer.rs`; all three exist locally and the plan and beads now say so. The authoring run also had no `br` executable; the import above is the first real one.

## Validate and regenerate locally

```text
python validate_plan.py --self-test
python build_plan.py
```

Both are UTF-8 safe on Windows since 2026-10-12 (the original bundle read files with the console code page and crashed on the plan's section signs). `build_plan.py` prints the task-priority histogram after exporting.

## Import into the new repository

The tracker lives in the new monorepo root, because the first bead (`infra-m0.1`) is claimed from it. Nothing in this bundle writes to an existing tracker.

**Before importing (owner actions, see `ROLLOUT_BRIEF.md` §7):** choose the repository name and location; back up `OxFunc/smart-fuzzer/cache` and `OxFunc/smart-fuzzer/runs` (about 3.4 GB, untracked); commit or stash the uncommitted changes in OxCalc, OxDoc, OxFml and DnaTreeCalc so that `infra-m0.4` can pin committed blobs.

**Automated:**

```text
python stage_br_import.py --destination C:\Work\DnaCalc\<NewRepo>
```

The helper validates the bundle, refuses an existing directory, runs `br init --prefix dc`, copies `beads.br.jsonl` to `.beads/issues.jsonl`, runs `br sync --import-only`, then `br dep cycles`, `br stats`, `br ready --json` and (if installed) `bv --robot-insights`, and writes `import-log.json`. It never uses force, rebuild, skip-invalid or rename-prefix flags. Pass `--allow-git-ancestor` only if the destination sits inside an existing git repository on purpose.

**Manual equivalent, only in a new empty directory:**

```text
br init --prefix dc
copy beads.br.jsonl .beads\issues.jsonl
br sync --import-only
br dep cycles
br ready
```

Then `git init -b master`, commit `.beads/`, and copy this bundle to `docs/plan/` so the bead descriptions' references to `DNA_CALC_REIMPLEMENTATION_PLAN.md` resolve inside the repository (`infra-m0.1` commits it).

**Prefix.** `dc` is the default for issues created later (follow-ups, bugs). Imported IDs keep their library prefixes (`infra-`, `cfunc-`, `cfml-`, `ceng-`, `cdoc-`, `chost-`, `onecalc-`, `dnacalc-`); br supports mixed prefixes and the import proved it. Do not use `calc`: OxCalc's existing tracker already issues `calc-*` IDs and handover notes would become ambiguous.

Do not overwrite an existing `.beads/issues.jsonl` or import with force into a live tracker.

## Execution and scope cautions

- Task dependencies are blocking prerequisites. Parent-child relations group tasks under epics and never make a task wait for its epic.
- Branching model (owner decision 2026-10-12): everything happens on master as pull, work on one bead, commit, push. No branches and no worktrees, ever. Agents that work at the same time use separate full clones on master; one cargo invocation per target directory at a time.
- `bv --robot-plan` lists the 48 epics as single-item tracks because epics have no blockers. Ignore epic rows; pick work with `br ready` or `bv --robot-next`, which chooses correctly.
- Priorities are banded: the explicit P0 set (M0, M1, all of M2, and the never-deferrable skin and numeric-backend beads) is unchanged; other tasks take a milestone band (M3 and M4 → P2, M5 to M7 → P3, M8 → P4) and move up one band when they transitively block at least 30 tasks. Histogram: 50 P0, 31 P1, 48 P2, 70 P3, 5 P4. Change `_BAND` or the threshold in `build_plan.py` if the ready list orders badly in practice.
- M0 is a checkpoint, not the first product milestone. The first functional milestone is `infra-m1.3`: a real Excel workbook opened, calculated, compared provenance-first, edited, saved and reopened in Excel without repair. That exit needs a Windows machine with Excel (`infra-m1.2`).
- Notebook, tree and canvas skins start in M2 beside the minimum grid and never wait for the Excel UI milestone.
- `infra-m8.2` implements a fail-closed release checker. Closing it proves the checker rejects a release without chart, pivot and print implementations; it does not mean the first version is ready.
- Exact mode is a per-workbook selection with an availability and evidence badge, never a blanket parity claim; Fast is the default everywhere; an unsupported Exact request never silently falls back.
