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

Each package owns its acceptance cases. They live in its `tests/acceptance.rs` (test target `acceptance`), are named `<bead>__<case>` with the bead ID in snake case (`infra_m0_1__workspace_builds`), and their exact names are registered in the package's own `acceptance.json`. A bead edits only its own package's `acceptance.json` and `seams.json`; there is no shared registry. A manifest has this shape; every key is required and no other key is accepted. `test` names the package's test target, `platforms` gives every registered case the platforms it runs on (`windows`, `linux`, `wasm`, `excel`), `commands` lists the bead's Acceptance commands, each as an argument vector, and a package with no cases yet has `"beads": []`:

```text
{"package": "<package name>", "beads": [{"id": "<bead id>", "test": "acceptance", "cases": ["<case name>", ...], "platforms": {"<case name>": ["windows", "linux"], ...}, "commands": [["cargo", "<arg>", ...], ...]}]}
```

Each portable package also keeps a reviewed `seams.json` beside its `Cargo.toml`: every pub enum variant and pub trait of its non-test sources, each with a non-test caller (file and function) and the registered acceptance case that executes that caller. The format is described at the top of `tools/xtask/src/seams.rs`; a package without such items has `"seams": []`. `infra-m0.3` created the `seams.json` of every portable package as part of the bootstrap, as `infra-m0.1` created the `acceptance.json` files; from then on each bead edits only its own package's two files.

## Beads are the work queue

- Pick work with `br ready` or `bv --robot-next` (never bare `bv`; `bv --robot-plan` lists epics as tracks, ignore epic rows). Read the bead with `br show <id>`.
- Claim it: `br update <id> --status in_progress --assignee <clone-name> --actor <clone-name>`, where `<clone-name>` is the checkout's directory name (`DnaCalc`, `DnaCalc-<lane>`). One bead is being built per clone at a time; the only other in_progress beads a clone holds are held beads awaiting the owner's close and prepared oracle beads awaiting a capture, each carrying a handover note (`br update <id> --notes "..."`). One library per bead: the bead's Context names the library whose files it may change.
- Do exactly the bead's **Do** and prove its **Acceptance**. Every named acceptance case must exist, run and pass on the declared platforms; keep the receipt. A zero-test run, a misspelled case name, a skipped case or a compile check standing in for execution is a failure.
- Acceptance commands: for each platform the bead's cases declare, run `cargo xtask accept --id <id> --platform <platform> --require-all`: `windows` on a Windows host, `linux` on a Linux host (a WSL clone of the pushed commit here), `wasm` in the browsers, and `excel` only on the owner's machine with Excel, never by an agent. The runner first runs the architecture checks below. It then runs each command in the bead's `commands` (the bead's Acceptance section) once, from the workspace root with `CARGO_TARGET_DIR=target/xtask-accept`, and records its argv, exit code and log; a non-zero exit fails the run, and a registered `cargo xtask accept ...` is skipped and listed, not run. It then lists the test target the manifest's `test` names and runs each registered case for that platform on its own, as `cargo test -p <package> --test acceptance -- --exact <case> --nocapture` (every package's `test` is `acceptance`; only a runner test fixture names another target) with `--locked` and its own target directory `target/xtask-accept`. A case passes only when the listing holds its exact name and its transcript must report `1 passed`: a misspelled or partial case name runs 0 tests and cargo still exits 0. A case that prints `ACCEPT-NOT-RUN: <case>: <reason>` on standard output or standard error (no Excel, no browser driver, no capture) is `not_run`, and so is a case the runner cannot start on its route: `excel` on a Windows host where Excel is not registered for automation, and `wasm` in a browser whose `wasm-bindgen-test-runner` or driver (`CHROMEDRIVER`, `GECKODRIVER`) is missing; on a host that cannot run the platform (`windows` off Windows, `linux` off Linux, `excel` off Windows) no command or case runs and every case is `not_run` with that reason, with or without `--require-all`. The receipt's `result` is `failed` when a manifest, an architecture check or a command failed, the bead ID is unknown, no case declares the platform, a case is absent from the listing (a misspelled or partial name), runs zero tests, is ignored or fails, or a test with the bead's case prefix is not registered; otherwise `not_run` when any case is `not_run`; otherwise `passed`. A run that executed nothing never records `passed`, and there is no `#[ignore]` route around a case. The run exits 0 for `passed`, exits 0 for `not_run` only without `--require-all`, and exits 1 otherwise: under `--require-all` only `passed` exits 0. The runner prints one line per command and per case with its outcome and exit code, keeps each transcript under `target/accept/<id>/<platform>/` and writes the machine-readable receipt `receipt.json` there; for a bead that some manifest registers it also copies the receipt to `accept-receipts/<id>/<platform>.json`, failed runs included (an unknown bead ID gets no committed copy). Receipt paths are relative to the workspace root, directories outside it appear as `<rustup>`, `<cargo-home>`, `<home>` or `<temp>`, the legacy root appears relative to the workspace root (normally `..`) or as `<legacy-root>`, and the receipt's own committed path is left out of its dirty paths. Keep the transcript and exit code of every other command under `target/accept/<id>/`, and quote the runner's case lines and exit codes in the close reason.
- Architecture checks: every `cargo xtask accept` run performs them first, whatever the bead, and records them in its receipt. `dependencies`: the plan section 1.3 layering rules in `tools/xtask/architecture.json` on resolved cargo metadata with all features for `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu` and `wasm32-unknown-unknown` (plus the host triple when it is none of them) on either host, and no portable package reaching a native-only one. `os-imports`: no `std::fs`, `std::process`, `std::os`, `std::net`, `extern "system"` or `#[link]` in the non-test code of the lib and bin targets of portable packages (build scripts run on the build host, so this scan reads them only for the include macros named below), and no OS-binding crate of the deny list as a direct normal or build dependency of a portable package on any of those targets or anywhere in its `wasm32-unknown-unknown` closure (a transitive one on a native target, such as libc or rustix, is allowed). `wasm-refusal`: each native-only package fails to compile for `wasm32-unknown-unknown` through its own `compile_error!`. `tree-reference-tokens`: no tree-reference or reference-profile token in the identifiers, strings and doc comments of the non-test code of portable packages, build scripts included; plain comments, `tests/`, `benches/`, `examples/` and `#[cfg(test)]` code are outside its scope by design. Neither source scan follows `include!`, `include_str!` or `include_bytes!`: a portable package that uses one fails both until a bead extends them to read the included file. `seams`: each portable package's `seams.json` against its parsed pub enum variants and pub traits. This is the manifest half of plan section 8.8; proving that each entry's case reaches its caller is owned by bead `dc-pz3u`, and every receipt counts the entries whose reach is still unproven. `word-scan`: the two banned words over `docs/`, `AGENTS.md`, `README.md`, tracker text and commit messages, against the counted baseline in `tools/xtask/word-scan.json` that bead `dc-0nvp` owns; fixed identifiers are excluded by name, every receipt reports how many occurrences each one masked, and any occurrence beyond the baseline fails. That file stores the words and identifiers as hex, so it does not use them itself.
- Commit: one build commit per bead on master, bead ID first. A defect found after the build commit (a mutation probe that stays green, a Linux failure) is fixed in a further commit on the same bead, also bead ID first, and the close reason cites every commit:

  ```text
  br sync --flush-only
  git add <source paths> accept-receipts/<id> .beads/issues.jsonl
  git commit -m "<id>: <what was built, one line>"
  ```

  The message body says Product status / Evidence / Still open. The build commit carries the receipts `accept-receipts/<id>/<platform>.json` of the platforms run before it. The build commit and the later `<id>: close` commit are separate because the close comes only after evidence the build commit cannot carry, such as the Linux run on the pushed commit, the owner's answer on a held bead or an oracle capture; the close commit carries `.beads/issues.jsonl` plus the receipts of the platforms run after the build commit.
- Close with evidence, always passing the ID: `br close <id> --reason "Product status: ... Evidence: <cases, platforms, commands, commit>. Still open: ..."`, then `br sync --flush-only`, `git add .beads/issues.jsonl accept-receipts/<id>`, `git commit -m "<id>: close"` and `git push`. `--force` is never used on claim, close or sync.
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

`infra-m0.1` created the workspace: the packages above exist, and none of them has a library API yet. Its case `infra_m0_1__workspace_builds` (in `tools/xtask/tests/acceptance.rs`) checks that the workspace has exactly these members, runs `cargo check` on every package for the host target and on every portable package for `wasm32-unknown-unknown`, and confirms from cargo's compiler-artifact messages that each of them was compiled in that run. `infra-m0.3` added `cargo xtask accept` and the architecture checks (`tools/xtask`); the `cargo xtask` alias lives in `.cargo/config.toml`, which sets no `build.target-dir`. Acceptance from then on goes through the runner. Its `wasm` route (wasm-bindgen-test-runner with chromedriver and geckodriver) has not yet run a real wasm case; until a bead declares one, that route is not exercised, not proven.
