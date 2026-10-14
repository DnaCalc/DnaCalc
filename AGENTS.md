# AGENTS.md: operating contract for this repository

This is the DNA Calc re-implementation: one Cargo workspace for CalcFunc, CalcFml, CalcEngine, CalcDoc, CalcHost and the DNA OneCalc and DNA Calc applications. It is Excel only. The legacy repositories (OxFunc, OxFml, OxCalc, OxDoc, DnaTreeCalc and the others beside this checkout under `C:\Work\DnaCalc`) are read-only inputs: port from them, never edit them from here.

## Read first

- `docs/plan/ROLLOUT_BRIEF.md`: what the plan decides, how the graph is shaped, the roll-out sequence.
- `docs/plan/DNA_CALC_REIMPLEMENTATION_PLAN.md`: the architecture and the rules every bead cites by section number.
- `docs/plan/BEAD_DETAILS.md`: every bead in full, as `docs/plan/build_plan.py` generated it. The tracker, read with `br show <id>`, is authoritative: it also carries the notes added when a bead is claimed or sharpened, so where the two differ the tracker wins.

`docs/plan/` holds the plan bundle that bead descriptions cite: the plan, `BEAD_GRAPH.md`, `BEAD_DETAILS.md`, the JSONL exports `beads.plan.jsonl` and `beads.br.jsonl`, `coverage.json` and `validate_plan.py`. Only `docs/plan/build_plan.py` writes its generated files and only `br` writes `.beads/`; neither is edited by hand, and nothing is re-imported over `.beads/issues.jsonl`.

## Workspace

One Cargo workspace with one committed `Cargo.lock`:

| Directory | Packages |
|---|---|
| `crates/` | `calc-value`, `calc-func` (CalcFunc); `calc-fml` (CalcFml); `calc-doc-model`, `calc-doc` (CalcDoc); `calc-engine` (CalcEngine); `calc-host` (CalcHost) |
| `apps/` | `dna-onecalc`, `dna-calc` |
| `platform/` | `calc-platform-desktop`, `calc-oracle-excel` (native-only adapters) |
| `tools/` | `xtask` (native-only repository tooling) |

Every package declares `[package.metadata.dnacalc] portability` as `"portable"` or `"native-only"`. Native-only packages refuse to compile for wasm32, and the wasm32 compile check (a compile check, not browser evidence) excludes exactly them:

```text
cargo check --workspace --target wasm32-unknown-unknown --exclude calc-platform-desktop --exclude calc-oracle-excel --exclude xtask
```

Each package owns its acceptance cases. They live in its `tests/acceptance.rs` (test target `acceptance`), are named `<bead>__<case>` with the bead ID in snake case (`infra_m0_1__workspace_builds`), and their exact names are registered in the package's own `acceptance.json`. An agent edits only the manifest of the package its bead owns; there is no shared registry. Until `infra-m0.3` extends the format, a manifest has this shape, where `commands` lists the bead's Acceptance commands, each as an argument vector, and a package with no cases yet has `"beads": []`:

```text
{"package": "<package name>", "beads": [{"id": "<bead id>", "cases": ["<case name>", ...], "commands": [["cargo", "<arg>", ...], ...]}]}
```

## Beads are the work queue

- Pick work with `br ready` or `bv --robot-next` (never bare `bv`; `bv --robot-plan` lists epics as tracks, ignore epic rows). Read the bead with `br show <id>`.
- Claim it: `br update <id> --status in_progress --assignee <clone-name> --actor <clone-name>`, where `<clone-name>` is the checkout's directory name (`DnaCalc`, `DnaCalc-<lane>`). One bead is being built per clone at a time; the only other in_progress beads a clone holds are held beads awaiting the owner's close and prepared oracle beads awaiting a capture, each carrying a handover note (`br update <id> --notes "..."`). One library per bead: the bead's Context names the library whose files it may change.
- Do exactly the bead's **Do** and prove its **Acceptance**. Every named acceptance case must exist, run and pass on the declared platforms; keep the receipt. A zero-test run, a misspelled case name, a skipped case or a compile check standing in for execution is a failure.
- Acceptance commands: run the commands in the bead's Acceptance section, and run each named case on its own with `cargo test -p <package> --test acceptance -- --exact <case> --nocapture`. Its transcript must report `1 passed`: a misspelled or partial case name runs 0 tests and cargo still exits 0. Keep every command's transcript and exit code under `target/accept/<id>/` as the receipt and quote the case names and exit codes in the close reason. `cargo xtask accept --id <id> --platform <platform> --require-all` replaces this form once `infra-m0.3` has built it.
- Commit: one build commit per bead on master, bead ID first:

  ```text
  br sync --flush-only
  git add <source paths> .beads/issues.jsonl
  git commit -m "<id>: <what was built, one line>"
  ```

  The message body says Product status / Evidence / Still open. The build commit and the later `<id>: close` commit are separate because the close comes only after evidence the build commit cannot carry, such as the Linux run on the pushed commit, the owner's answer on a held bead or an oracle capture, and the close commit carries only `.beads/issues.jsonl`.
- Close with evidence, always passing the ID: `br close <id> --reason "Product status: ... Evidence: <cases, platforms, commands, commit>. Still open: ..."`, then `br sync --flush-only`, `git add .beads/issues.jsonl`, `git commit -m "<id>: close"` and `git push`. `--force` is never used on claim, close or sync.
- A missing prerequisite or a discovered gap gets a new bead, never a local substitute or a widened scope. Two directions, verified on br 0.1.34: a prerequisite that must land before the current bead is created with `br create ...` and attached with `br dep add <current> <new>` (the current bead then depends on it and cannot close first); a follow-up that the current bead must not wait for is created with `br create ... --deps blocks:<current>` (the new bead depends on the current one). New bead IDs use the `dc` prefix; imported IDs keep their library prefixes.
- Epics hold no work. A parent-child link never blocks a task.

## Git: master only, no branches, no worktrees, ever

- The loop is: `git pull --no-rebase`, work on one bead, `git commit` (bead ID first in the message), `git push`. Nothing else: no branches, no worktrees, no rebase, no force push, no reset.
- Agents working at the same time each use their own full clone of this repository on master (`git clone https://github.com/DnaCalc/DnaCalc.git C:\Work\DnaCalc\DnaCalc-<lane>`), which gives each its own `target/`. Pull before starting. If a push is rejected, run `git pull --no-rebase`, re-run the bead's acceptance, and push again.
- Never run two cargo invocations in one target directory at the same time. Cargo started from inside a test uses a target directory of its own; the outer `cargo test` owns `target/`.
- Never commit `target/`, Excel lock files, secrets or bulk evidence.
- Commit messages end with the attribution line the harness asks for.

## Honesty rules (from the owner brief, section 6; they are requirements)

1. Provenance before value: only a value the engine actually calculated may be scored as calculated. A file cache, a stale value or a typed not-modeled result never counts as a pass.
2. Count every fallback. A conservative broadening is legal only when it is counted, reported and owned by a bead naming its replacement.
3. No faked behaviour. Typed not-implemented is always better than a hard-coded answer.
4. No seams without traffic: every production enum variant and interface has a non-test caller, checked mechanically.
5. One of everything: one evaluator, one graph, one capture lineage, one verdict path.
6. The machine is not the product: test the route the application actually runs, not a parallel route built for the test.
7. Measure, do not assert. Claims never exceed the evidence; a status flag assigned by the code under test proves nothing.

## Writing

- Reports, commit bodies, handovers and close reasons use Product status / Evidence / Still open and claim only what a tool result shows.
- Never use the two banned words listed in `docs/plan/ULTRACODE_WORK_PROMPT_OPUS_5_5.md` §0; say what blocks what instead.

## Platforms

Windows, Linux and wasm from the first version; CI runs all three once `infra-m0.2` has installed it. Beads whose platform is `oracle` need live Excel and run on the owner's Windows machine; their retained captures replay everywhere else. Missing Excel is reported as not run, never as a pass.

## Bootstrap note

`infra-m0.1` created the workspace: the packages above exist, and none of them has a library API yet. Its case `infra_m0_1__workspace_builds` (in `tools/xtask/tests/acceptance.rs`) checks that the workspace has exactly these members, runs `cargo check` on every package for the host target and on every portable package for `wasm32-unknown-unknown`, and confirms from cargo's compiler-artifact messages that each of them was compiled in that run. Until `infra-m0.3` provides `cargo xtask accept` and the architecture checks, the M0 beads use the plain cargo commands in their Acceptance sections and the receipt form above; `infra-m0.3` then updates this file with the runner commands.
