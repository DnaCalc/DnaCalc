# AGENTS.md: how work is done in this repository

This is the DNA Calc re-implementation: one Cargo workspace for CalcFunc, CalcFml, CalcEngine,
CalcDoc and CalcHost, and the DNA OneCalc and DNA Calc applications. It is Excel only. The legacy
repositories beside this checkout under `C:\Work\DnaCalc` (OxFunc, OxFml, OxCalc, OxDoc,
DnaTreeCalc, DnaOneCalc and the others) are read-only inputs: port from them, never edit them.

## Read first

- `docs/plan/BULK_PORT_PLAN.md`: the current plan, the work packages and how correctness is
  shown.
- `docs/plan/DNA_CALC_REIMPLEMENTATION_PLAN.md`: the architecture (libraries, layering, contracts).
  Its roll-out sections are superseded by the bulk-port plan.
- The tracker: `br ready`, then `br show <id>`.

## Workspace

| Directory | Packages |
|---|---|
| `crates/` | `calc-value` and `calc-func` (CalcFunc); `calc-fml` (CalcFml); `calc-doc-model` and `calc-doc` (CalcDoc); `calc-engine` (CalcEngine); `calc-host` (CalcHost) |
| `apps/` | `dna-onecalc`, `dna-calc` |
| `platform/` | `calc-platform-desktop`, `calc-oracle-excel` (native-only adapters) |
| `tools/` | `calc-verdict` (capture records and the verdict path), `xtask` (repository tooling) |

- **Licence:** MIT; a new package uses `license.workspace = true`.
- **Portability:** each package declares `[package.metadata.dnacalc] portability` as `"portable"`
  or `"native-only"`. Portable packages build for `wasm32-unknown-unknown` and touch no
  `std::fs`, `std::process`, `std::os` or `std::net`. Native-only packages refuse to compile for
  wasm32.
- **Layering:** the layering rules in `tools/xtask/architecture.json` are checked mechanically.

## Doing a work package

1. **Claim it.** Pick a package with `br ready` and read it with `br show <id>`. Claim it with
   `br update <id> --status in_progress --assignee <clone-name>`. Each lane works in its own full
   clone (`C:\Work\DnaCalc\DnaCalc-<lane>`) on master.
2. **Port in bulk.** Copy legacy code wholesale, then rename types and adapt it to the new APIs.
   Keep the layering and the one-evaluator rule. Leave behind the tree-reference language, the
   dual-profile model and OxCalc's consumer layer. Bring the legacy tests over wherever they
   still apply. The third-party numeric code in the legacy functions may be ported as is (owner
   ruling, 2026-10-23).
3. **Commit in working chunks.** Each commit:
   - starts with `git pull --no-rebase`;
   - builds the workspace;
   - passes `cargo test` for the crates it touched, on Windows;
   - is then pushed.

   The message starts with the bead ID and names the legacy repository, commit and paths it
   ported from. Several commits per package is normal.

   `cargo xtask check` runs the five architecture checks (dependencies, os-imports,
   wasm-refusal, tree-reference-tokens, word-scan) and the provenance scan, builds the workspace
   and tests every package (one line each), and fails on any of them. The provenance scan needs
   an entry in `migration/licence-ledger.json` (`path`, `class`, `reason`) for every file under
   `crates/calc-func` and `crates/calc-value`, and fails when a marker of a removed third-party
   or Microsoft source reappears in code; `--wasm` adds the portable packages' tests in
   headless Chrome and Firefox. A portable package's test runs in both: give it
   `#[cfg_attr(not(target_arch = "wasm32"), test)]` with
   `#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]`, and embed its data
   with `include_bytes!` in test code. A test that cannot (it drives Excel or the file system)
   keeps `#[test]` and a `/// Native-only: <reason>` doc line; `--wasm` prints both counts per
   package and fails on a test that is neither.
4. **Measure.** Run the corpus comparison (`cargo xtask corpus`, or `cargo xtask compare` per
   workbook until it lands) before and after the package, and record the summary.
5. **Close.**
   - One read-only review agent checks the change against the honesty rules and checks the
     close note against tool output.
   - Close with `br close <id> --reason "<what works now; evidence: tests, corpus summary,
     commits; still open>"`.
   - Gaps go into the next package's notes or into one root-cause bead per batch of corpus
     mismatches; never one bead per refusal.

`cargo xtask accept --id <id> --platform <p>` still runs a bead's registered acceptance cases and
writes a receipt under `accept-receipts/`. Use it where a package registers cases. It is not
required for every commit.

## Extend, never duplicate

Before you create a fixture, oracle workbook, generator, test helper, adapter, registry, tool
command or document, look for the one that already exists and extend it. There is:
- one catalogue oracle for calc-func, `crates/calc-func/tests/fixtures/catalogue` and its generator, to which
  each package adds its own section (its own `sheet-*.txt` or `prefix-*.txt` file, appended to
  the lists in `make-catalogue-fixtures.ps1`). Adding a row: edit your section file, regenerate
  (`make-catalogue-fixtures.ps1 -OutDir crates/calc-func/tests/fixtures`), commit. Nothing else
  is required to build, test or commit. The catalogue is captured per sheet
  (`tests/captures/catalogue/sections/`), each capture keyed by its sheet's content, so your
  change leaves every other sheet's capture valid; your sheet is "awaiting capture" (counted apart
  in `cargo xtask corpus`, never a pass, failing no test) until someone runs `cargo xtask capture
  --pending`, which captures every pending sheet of every oracle, one Excel session per sheet,
  and writes the section captures (commit them). The same holds for every other oracle workbook
  and its `oracle.capture.json`. The Excel build, worker version and saved package a capture
  records are provenance, never a condition;
- one corpus harness (`cargo xtask corpus`);
- one adapter from OxFunc's types (`catalogue/kit.rs`);
- one verdict path.

A merge conflict in a shared file is cheaper than a second copy of it: pull often, keep your
additions in their own section, and resolve conflicts keeping both sides. If you believe a
second copy is really needed, say why in the commit message.

## Honesty rules

1. **Provenance before value.** Only a value the engine actually calculated counts as
   calculated. A file cache, a stale value or a typed not-modelled result never scores as a pass.
2. **Typed refusals, counted.** What is not implemented returns a typed not-modelled refusal
   naming the work package that owns it. A refused formula is a failure in every statistic
   (owner ruling, 2026-10-23): every report gives pass = matched Excel, and fail = mismatched +
   refused (+ failed or missing), as counts and a pass rate over all formula cells, never a
   rate over calculated cells only.
3. **No faked behaviour.** No hard-coded answers, no weakened assertions, no golden files
   regenerated to force green, no `todo!()` in commits.
4. **One of everything:** one evaluator, one graph, one capture lineage, one verdict path.
5. **Test the route the application runs**, not a parallel route built for the test.
6. **Measure, do not assert.** Claims never exceed the evidence. Reports say what the product can
   now do, then the evidence, then what is still open.

## Git: master only

- The loop is `git pull --no-rebase`, work, commit, `git push`.
- Never use branches, worktrees, rebase, force push or `reset --hard`. The one exception is the licence
  history cleanup (dc-96l.7): the orchestrator force-pushes once per repository, after a
  backup and with the lanes paused. No lane agent force-pushes.
- If a push is rejected, pull with `--no-rebase`, re-run the tests and push again.
- Never run two cargo invocations in one target directory at the same time.
- Never commit `target/`, Excel lock files, secrets or bulk corpus files.
- Commit messages end with the attribution line the harness asks for.
- Only `br` writes `.beads/`.

## Writing

Never use the two banned words listed in `docs/plan/ULTRACODE_WORK_PROMPT_OPUS_5_5.md` §0. The
word scan reads code, docs, tracker text and commit messages. Say what blocks what instead.

## Platforms

- **Per commit:** tests run on Windows.
- **Per milestone:** a whole-suite sweep runs on Windows, Linux (a WSL clone of the pushed
  commit; WSL is offline and builds from `~/cargo-vendor`) and wasm (headless Chrome and Firefox
  via wasm-bindgen-test-runner).
- **GitHub CI:** the `ci` workflow is disabled (owner, 2026-10-20). Do not enable or dispatch it.
- **Excel on this machine** (owner ruling, 2026-10-21): any agent may use Excel here. That
  includes opening any workbook, macros, add-ins, COM automation, `cargo xtask capture`, and
  several Excel processes at once. Stop only the Excel processes your own scripts started; the
  owner's own Excel stays. Never change a Trust Center, macro-security or registry setting.
  Excel lock files are never committed, and a workbook from outside the repository keeps its
  provenance and licence record before its capture is committed.
