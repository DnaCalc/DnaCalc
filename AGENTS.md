# AGENTS.md: operating contract for this repository

This is the DNA Calc re-implementation: one Cargo workspace for CalcFunc, CalcFml, CalcEngine, CalcDoc, CalcHost and the DNA OneCalc and DNA Calc applications. It is Excel only. The legacy repositories (OxFunc, OxFml, OxCalc, OxDoc, DnaTreeCalc and the others beside this checkout under `C:\Work\DnaCalc`) are read-only inputs: port from them, never edit them from here.

## Read first

- `docs/plan/ROLLOUT_BRIEF.md`: what the plan decides, how the graph is shaped, the roll-out sequence.
- `docs/plan/DNA_CALC_REIMPLEMENTATION_PLAN.md`: the architecture and the rules every bead cites by section number.
- `docs/plan/BEAD_DETAILS.md`: every bead in full (identical to the tracker).

## Beads are the work queue

- Pick work with `br ready` or `bv --robot-next` (never bare `bv`; `bv --robot-plan` lists epics as tracks, ignore epic rows). Read the bead with `br show <id>`.
- Claim it: `br update <id> --status in_progress`. One bead per agent session, one library per bead.
- Do exactly the bead's **Do** and prove its **Acceptance**. Every named acceptance case must exist, run and pass on the declared platforms; keep the receipt. A zero-test run is a failure.
- Close with evidence: `br close <id> --reason "<what was proven, commands run, commit>"`, then `br sync --flush-only` and commit `.beads/`.
- A missing prerequisite or a discovered gap gets a new bead (`br create ... --deps blocks:<id>`), never a local substitute or a widened scope. New bead IDs use the `dc` prefix; imported IDs keep their library prefixes.
- Epics hold no work. A parent-child link never blocks a task.

## Git: master only, no branches, no worktrees, ever

- The loop is: `git pull`, work on one bead, `git commit` (bead ID first in the message), `git push`. Nothing else.
- Agents working at the same time use separate full clones of this repository, each on master with its own `target/`. If a push is rejected, pull master, re-run the bead's acceptance, push again.
- Never run two cargo invocations in one target directory at the same time.
- Commit messages end with the attribution line the harness asks for.

## Honesty rules (from the owner brief, section 6; they are requirements)

- Provenance before value: only a value the engine actually calculated may be scored as calculated. A file cache, a stale value or a typed not-modeled result never counts as a pass.
- Count every fallback. A conservative broadening is legal only when it is counted, reported and owned by a bead naming its replacement.
- No faked behaviour. Typed not-implemented is always better than a hard-coded answer.
- No seams without traffic: every production enum variant and interface has a non-test caller, checked mechanically.
- One of everything: one evaluator, one graph, one capture lineage, one verdict path.
- Measure, do not assert. Claims never exceed the evidence; a status flag assigned by the code under test proves nothing.

## Platforms

Windows, Linux and wasm from the first version; CI runs all three. Beads whose platform is `oracle` need live Excel and run on the owner's Windows machine; their retained captures replay everywhere else. Missing Excel is reported as not run, never as a pass.

## Bootstrap note

`infra-m0.1` creates the workspace and extends this file with the acceptance-runner commands (`cargo xtask accept ...`) once `infra-m0.3` provides them. Until then the M0 beads use the plain cargo commands written in their Acceptance sections.
