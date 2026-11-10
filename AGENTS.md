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
| `crates/` | `calc-value`, `calc-catalogue` and `calc-func` (CalcFunc); `calc-fml` (CalcFml); `calc-doc-model` and `calc-doc` (CalcDoc); `calc-engine` (CalcEngine); `calc-host` (CalcHost); `calc-addin-host` (the loaded add-in set both desktop applications share) |
| `apps/` | `dna-onecalc`, `dna-calc`; `dna-onecalc-desktop` and `dna-calc-desktop` (the two Windows executables); `dna-formula-ui` (the formula UI both show) and `dna-strand` (design tokens) |
| `platform/` | `calc-platform-desktop` (window chain, WebView2 host, WinHTTP, file dialogs; no product dependency), `calc-oracle-excel` (native-only adapters) |
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
   - passes `cargo xtask check` on the machine you work on (Windows or Linux);
   - is then pushed.

   The message starts with the bead ID and names the legacy repository, commit and paths it
   ported from. Several commits per package is normal.

   `cargo xtask check` is the light per-commit check:
   `cargo check --locked --workspace --all-targets`, and tests of touched packages only.
   Windows also checks touched packages and dependents for Linux when that target is installed
   (otherwise it prints a note); touched portable packages get a wasm32 check.
   `--dependents` builds the workspace and tests touched packages and their dependents; use it
   for core contract changes. `--all` builds and tests every package. `--base <rev>` selects
   the change baseline used by keepers. Run logs include ordered output and command timings.

   Launchers export `DNACALC_AGENT_LOG` as `<host>:<path>`. Commit with
   `git commit --trailer "Agent-Log: $DNACALC_AGENT_LOG"`; the light check warns about
   unpushed non-merge commits without this trailer. Keep work logs for at least 60 days.
   Find a commit's log with `git log --format='%h %(trailers:key=Agent-Log,valueonly)'`.

   A portable package's test runs natively and in the browsers: give it
   `#[cfg_attr(not(target_arch = "wasm32"), test)]` with
   `#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]`, and embed its data
   with `include_bytes!` in test code. A test that cannot (it drives Excel or the file system)
   keeps `#[test]` and a `/// Native-only: <reason>` doc line. A new file under
   `crates/calc-func` or `crates/calc-value` gets an entry in `migration/licence-ledger.json`
   (`path`, `class`, `reason`). What you may read or port when working out how Excel computes a
   function is in `docs/plan/RESEARCH_SOURCES.md`: four source tiers, and an analyst/implementer
   split for sources whose code we may not use.

   `cargo xtask sweep` runs the slow and structural validation: the four architecture checks
   (dependencies, os-imports, wasm-refusal, tree-reference-tokens), the licence ledger's completeness, the dependency ceiling (dna-onecalc and
   dna-formula-ui never reach calc-engine or calc-host, and may use calc-doc; dna-onecalc-desktop
   reaches no calc-host and no calc-engine; dna-calc does not depend on dna-onecalc, nor dna-calc-desktop), the portable packages' tests in headless Chrome and Firefox (a test that is
   neither run there nor marked `Native-only:` fails it), and xtask's acceptance suite. It takes
   hours and is never required for a commit: run it at a milestone or on a schedule, on a clean
   tree. Its release step runs workspace release tests once, then compares debug/release oracle corpus and full function-bank counts. `--skip <step>` leaves a step out.
4. **Measure.** Run the corpus comparison (`cargo xtask corpus`, or `cargo xtask compare` per
   workbook until it lands) before and after the package, and record the summary.
   Corpus comparisons use debug by default; `--release` selects the speed build in `target/release`.
   Dev builds optimise external dependencies and calc-verdict (corpus hashing); the calculation crates stay unoptimised.
5. **Close.**
   - The close note's numbers come from tool output you ran, not from memory or a plan.
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
- new numeric research questions go in a separate new workbook, written by the same generator
  and read by the same capture route. Never add them to a captured workbook: CELL, SHEETS and
  INFO can depend on its sheet positions or full sheet list. Preserve existing capture keys;
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
- If a push is rejected, pull with `--no-rebase`, re-run the light check and push again.
- Never run two cargo invocations in one target directory at the same time.
- Everyday work builds debug; `cargo xtask bench` measures the `perf` profile (release optimisation with line tables).
- Before any performance change lands, including CPU features (`target-cpu`, FMA), SIMD or rewritten arithmetic, run `cargo xtask corpus --oracle --profile perf` and `cargo xtask witness judge --bank full --profile perf --report <json>`; their counts must match debug.
- Dev debug info is off; panic and assertion locations remain available. Set `CARGO_PROFILE_DEV_DEBUG=line-tables-only` locally for debugger or backtrace line numbers.
- Builds and tests share `target/debug`; only the lightweight xtask launcher uses `target/xtask`. The linked harness runs a temporary copy under `target/xtask/run` so Windows can rebuild its original.
- Never commit `target/`, Excel lock files, secrets or bulk corpus files.
- Commit messages end with the attribution line the harness asks for.
- Only `br` writes `.beads/`; never hand-edit `issues.jsonl`. Comment ids are numbered per clone (highest + 1), so two
  lanes that comment between pulls reuse an id and the next import renumbers one ("PRIMARY KEY constraint failed").
  Every tracker write: `git pull --no-rebase`, `br sync --import-only`, the `br` write, `br sync --flush-only`, then
  commit and push at once. On a conflicted merge of `issues.jsonl` take theirs, import, redo your `br` writes.
  On Windows set `RUST_MIN_STACK=67108864` for `br sync`; the default stack overflows.

## Writing

Write plainly about what the work does: say concretely what is done, what blocks what,
and what is left.

## Red master

- Keepers run about every ten minutes in their own clones and target directories, in tmux on Linux and a background process on Windows.
- They run dependent checks since last green, full checks every 20 commits or two hours, and a nightly Windows sweep; findings are keeper-owned `master-red` P0 beads.
- The Windows keeper runs `cargo xtask health` weekly during its nightly sweep. When any formatting, line-length (>160 chars) or clone-lint count has grown since the previous report, it files or updates one keeper-owned P2 bead 'Code health drift' with the table (never a P0, never master-red). It is never a per-commit check.
- Keep pushing after the light check; an exact test named for this platform in an open keeper-owned bead is printed as failed, known red. Full checks and keepers never excuse it.
- Fix forward within about an hour, with the causing lane or a repair lane citing the bead. The keeper closes the bead when green.
- Revert only for a major blunder: a platform cannot compile and has no fix within 30 minutes; secrets, unlicensed source or bulk corpus committed; corrupted evidence; or four hours red with no repair under way.

## Platforms

- **Per commit:** `cargo xtask check` on the machine you work on, Windows or Linux. Windows-only
  behaviour (Excel, COM, x87 on this CPU) is exercised by the Windows sessions and the sweep.
- Tests with platform-dependent behaviour assert both branches with `#[cfg(windows)]` and
  `#[cfg(not(windows))]`; an unavailable capability needs an asserted refusal.
- **Per milestone:** `cargo xtask sweep` on Windows (it includes wasm in headless Chrome and
  Firefox), and the tests on Linux (a WSL clone of the pushed commit; WSL is offline and builds
  from `~/cargo-vendor`).
- **GitHub CI:** the `ci` workflow is disabled (owner, 2026-10-20). Do not enable or dispatch it.
- **Excel on this machine** (owner ruling, 2026-10-21): any agent may use Excel here. That
  includes opening any workbook, macros, add-ins, COM automation, `cargo xtask capture`, and
  several Excel processes at once. Stop only the Excel processes your own scripts started; the
  owner's own Excel stays. Never change a Trust Center, macro-security or registry setting.
  Excel lock files are never committed, and a workbook from outside the repository keeps its
  provenance and licence record before its capture is committed.

## R2 storage (large evidence)

- Two private buckets on Cloudflare account `5514060cba509f0d9a72a45cdfb8ff1e` (endpoint
  `https://5514060cba509f0d9a72a45cdfb8ff1e.r2.cloudflarestorage.com`): `dnacalc-corpus`
  (`windows/corpus/`, `runs/<host>/<YYYYMMDD>-<label>/`) and `dnacalc-evidence`
  (`research/round<NN>/`, `excel/<capture-set>/`, `_setup/` for verification only).
- Use `rclone --config <file>` with remote `r2`: Windows `%APPDATA%\dnacalc\rclone.conf`, Linux
  `~/.config/dnacalc/rclone.conf`.
- Copy only, to a new dated prefix, with `--checksum`; afterwards `rclone check --one-way` and record
  the prefix with its file and byte counts. Never `sync`, `move`, `delete` or `purge` on R2.
- Never read, print, log or pass the keys (no `cat` of the config, no `rclone config show`, no keys
  on a command line or in the environment). If a key is shown to you, stop and tell the owner.
- Large raw run outputs go to R2 and are cited by prefix and counts in reports, not committed.
