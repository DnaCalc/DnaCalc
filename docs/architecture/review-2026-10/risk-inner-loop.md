# Risk lens: inner loop, test strategy and process load

Read-only audit of `C:\Work\DnaCalc\DnaCalc-review` at 5a9348b (2026-10-31), plus worker logs in
`C:\Work\DnaCalc\codex-runs`. Nothing was built or run. Where a cost is not recorded anywhere it is
marked as an estimate.

## What the loops actually run

**`cargo xtask check`** (tools/xtask/src/check.rs:1-24, 135-300): new-text word scan, provenance
marker scan, `cargo build --workspace`, then one `cargo test --no-fail-fast` over touched packages
plus every workspace dependent (check.rs:110-123). Observed in worker logs (17 results): 0 min 15 s
(nothing touched) to 7 min 7 s; a calc-func change tests 10 packages, a calc-value or Cargo.lock
change tests all 16 (1,314 tests, 68 harnesses). The slow harnesses are debug-build numerics:
calc-func lib 429 tests 99.6 s, calc-func acceptance 28 tests 109.5 s, calc-host 81 s and 49 s
(target/check/*.log, 2026-10-30). The 10-30 rewrite (a029649) cut this from ~29 min (per-package
logs 11:15 to 11:44 on 10-30). **Keep it.** It is the right shape.

**`cargo xtask sweep`** (sweep.rs:1-23): architecture (21 s), provenance (1 s), ceiling (1 s), wasm
in Chrome and Firefox (14 min 50 s), acceptance (hours). Logged: `SWEEP RESULT passed: 4 steps, 1
skipped (15 min 15 s)` (codex-runs/calcvba3-dna-sweep-final.log). The calcvba-3 worker ran it at
least twice in one 4-hour run, so "milestone only" is already slipping toward "per run".

**App browser smokes** (`cargo xtask app smoke onecalc|calc`, tools/xtask/src/app.rs) are in
**neither** check nor sweep (sweep.rs:37-43 lists five steps, none is the app smoke; no
acceptance.json registers it). The UI lane runs them by convention on most commits (13 of 21
dc-971p commit messages since 10-30 cite them).

## Ranked findings (by drag on future work)

### 1. The app smoke is the fastest-growing, least-structured check (HIGH)
- 482 cases in one 8,754-line file in the tooling crate: ONECALC_CASES 51 (app.rs:989-1532),
  CALC_CASES 419 (app.rs:1546-7253, 5,700 lines of data), latency 10, worker fault 2.
- Growth: CALC_CASES 71 at b89e4b2 (10-30 09:41), 198 at badbf8f (10-31 01:55), 357 at 61ed09c
  (09:16), 404 at b77ae22 (11:39). +333 cases in ~10 hours over dc-971p.2 to .15, each commit adding
  185 to 966 lines to app.rs.
- Cost: every run first builds the bundle in the `web` profile (Cargo.toml:35-39: opt-level 3,
  `lto = true`, `codegen-units = 1`; app.rs:7 calls it "optimised for size", which it is not), then
  runs all cases sequentially in one stateful page per browser (app.rs:987-988, 1543-1545: "in the
  order each browser runs them in one page"). Fixed sleeps alone are 10.9 s per browser
  (20 `PauseMillis`), plus 198 `Wait` checks polling up to `SETTLE` = 8 s (app.rs:148). No elapsed
  time is printed and no run log records one, so the wall time is unknown; estimate 10-25 min per
  full run at 406 cases x 2 browsers plus an LTO build.
- No case filter: `--browser` is the only subset (app.rs:213-228). Because cases share page state,
  one late failure means re-running everything to reproduce it.
- Pins incidental detail: 390 exact `Check::Text`, 82 `Check::Style` (e.g. `"width:145px"`,
  `"width:64px"`), 54 `Check::Count` (page sizes 30, 50), 168 `Check::Attr`. Some pin product
  *gaps*: app.rs:6289 asserts `"93 formula cells: 1 calculated, 92 refused, ..."` and app.rs:1013
  asserts the provenance prose `"calculated in Sheet1!H1 by calc-fml's evaluator with calc-func's
  functions; no cells read"` (28 smoke strings name crates). Implementing a refusal or renaming a
  crate breaks the UI smoke.
- The app's behavioural spec lives in `tools/xtask`, so every UI change edits and recompiles the
  tooling crate (38 commits to app.rs) and conflicts there between UI lanes.

**Recommend:** move the case tables next to the app (apps/dna-calc/tests or a data file); split into
independent groups that each start from a fresh page; add `--case <substring>`; print elapsed
time per browser. Per commit (UI lane only, when apps/ change): a core of ~30 cases. Everything
else goes to sweep as a new step. Rule for new cases: only behaviour the 126 dna-calc browser
unit tests cannot reach (real key/pointer/clipboard/file events). Replace refusal totals and crate
names with role/state checks; drop pixel widths unless width is the feature under test.

### 2. Every product edit compiles the product twice; xtask depends on the whole product (HIGH)
- `.cargo/config.toml`: `xtask = "run --package xtask --locked --"`. xtask depends on calc-engine,
  calc-doc, calc-fml, calc-func, calc-host, calc-verdict, dna-strand, calc-oracle-excel,
  calc-platform-desktop (tools/xtask/Cargo.toml). So `cargo xtask check` after a calc-func edit
  first rebuilds calc-func..calc-host and 37k lines of xtask in `target/debug`, then check rebuilds
  the workspace in `target/xtask-accept` (check.rs:22, 55-62; cargo.rs:227). The reported CHECK
  time starts after the first build (check.rs:144), so it understates the real wait.
- xtask is a dependent of every product crate, so its unit tests run in every check.
- No `[profile.dev]`/`[profile.test]` tuning: ~3.5 min of the check is debug-mode numerics.

**Recommend:** split a thin `xtask` (check, sweep, word scan, arch, licence; no product deps)
from a harness binary (corpus, capture, witness, compare, bench, fidelity, app/site smoke). Add
`[profile.dev.package.calc-func] opt-level = 2` (and calc-value) or a test profile; measure.

### 3. Write-only evidence is the largest thing in the repository (HIGH, cheap to fix)
- Tracked bytes: accept-receipts 10.2 MB (19.3%), more than all product Rust (7.9 MB, 14.9%). 131
  files over 48 beads; largest accept-receipts/cfunc-m3.6/wasm.json 916 KB. Each receipt re-embeds
  the full architecture-check output.
- Nothing reads them except the receipt mechanism and its own tests (accept.rs:79, ci.rs:1253,
  infra_m0_2.rs:1414-1448, infra_m0_3.rs:590). AGENTS.md says `accept` "is not required for every
  commit", yet ed1df55 (10-31) is "Metadata/receipts only" (+714 lines of receipt).
- Tests narrate for receipts: 676 `ACCEPT-NOTE`/`note!` emissions (calc-func acceptance 277,
  calc-doc 130, calc-fml 83), often long prose (calc-engine/tests/acceptance.rs:315-332).
- 102 of 466 commits since 10-24 touch only tracker/docs/receipts.

**Recommend:** `git rm -r accept-receipts`, stop committing receipts, retire
`acceptance.json` manifests (cargo test is the registry) and `accept --platform` receipts; keep
any case that tests product behaviour as an ordinary test. Strip ACCEPT-NOTE prose when a file is
next touched, not as a project.

### 4. Tooling maintained for retired purposes (MEDIUM-HIGH)
xtask is 37,001 src + 10,419 test lines, more than calc-engine + calc-fml + calc-host src
combined (30,696). Of that:
- GitHub CI is disabled (AGENTS.md "Platforms"), yet ci.rs 2,490 lines (jobs windows, linux, wasm,
  licenses, desktop; ci.rs:1-40), ci_audit.rs 756, infra_m0_2 acceptance 1,716, plus
  ci-browser-fixture with a deliberate-failure injector. Only `ci wasm` is used (by sweep).
- OxFunc evidence backup (infra-m0.6): evidence.rs + evidence_labels.rs (1,023) + infra_m0_6
  acceptance 1,950 + migration/evidence/oxfunc-smart-fuzzer.json 368 KB. Owner memory says this
  evidence is recreatable and backup work stays minimal.
- migration/source-manifest.json, 3.3 MB, "written_by": "cargo xtask pin-sources", a command that
  no longer exists (main.rs:148-170); read only by acceptance tests dc_cyd7 / infra_m0_6.
- infra_m0_1__independent_lanes builds lane clones to prove lanes are independent.

**Recommend:** delete ci-audit, the ci windows/linux/desktop jobs, the evidence commands and
the infra-m0.x acceptance cases; fold `ci wasm` and `ci licenses` into sweep steps; delete
source-manifest.json. Sweep's `acceptance` step then shrinks from hours to whatever product
cases remain (or disappears).

### 5. Overlapping layering checks in four places (MEDIUM)
- tools/xtask/architecture.json `dependency_rules` (dna-onecalc forbids calc-engine, calc-host).
- sweep.rs:46-47 hard-codes a second "ceiling" (dna-onecalc, dna-formula-ui forbid calc-engine,
  calc-doc, calc-host; dna-calc not on dna-onecalc): overlapping rule, different forbidden set.
- calc-func's acceptance test cfunc_m0_1 parses calc-value's *source text*: a hard-coded list of
  its 7 files via `include_str!` (crates/calc-func/tests/acceptance.rs:382-396), allowed manifest
  tables (401-407), allowed `cfg` forms (412-416), and forbidden identifiers `Rc`, `Cell`,
  `RefCell`... "in comments and strings too" (421-430). Adding a module to calc-value, or the word
  "Cell" in a comment in a spreadsheet value crate, fails calc-func's test.
- `accept` re-runs all arch checks for every bead (accept.rs:1263), as does `ci` (ci.rs:1334).

**Recommend:** one rules file; express the ceiling as architecture.json entries; delete the
source-text checks in cfunc_m0_1 (keep the compile-time `Send + Sync` assertion, which is cheap
and real).

### 6. Tests and fixtures keyed by tracker IDs make crate moves expensive (MEDIUM)
- 183 test functions named `<bead>__<case>`; 49 of 64 fixture/capture dirs named by bead
  (`tests/captures/cfunc_m1_2`, `dc_10y`, `dc_e00t`...); files `dc_51ka.rs`, `dc_xrxb_support/`.
  Moving a behaviour means knowing tracker history to find its tests.
- Per-package acceptance.json files hold `cargo test -p calc-func --test acceptance cfunc_m0_1`
  command lines (crates/calc-func/acceptance.json): package names and test targets in data.
- calc-func/tests/acceptance.rs is one 22,639-line, 816 KB file (24 commits since 10-24): slow to
  compile, conflict-prone, and the 110 s harness above.
- Licence ledger: 563 path entries, scoped by directory to calc-func and calc-value only; a file
  moved to a new crate leaves the ledger's scope without any check noticing.
- Captures are keyed by content hash (good, survives moves) but consts embed fixture paths
  (acceptance.rs:1547-1557).

**Recommend:** no rename project. When a file is touched, name tests by behaviour; split
calc-func acceptance.rs by function family; make the ledger scope "any file with a ported-source
marker", not two directories.

### 7. Docs and tracker sprawl (MEDIUM, cheap to fix)
- docs/plan is 3.1 MB; ~2.6 MB are 10-14 planning artifacts untouched since import (one commit
  each): BEAD_DETAILS.md 698 KB/9,369 lines, beads.br.jsonl 915 KB, beads.plan.jsonl 723 KB,
  build_plan.py 171 KB, br-import-log 89 KB, validate_plan.py, coverage.json,
  validation-report.json, source-audit.json. All superseded by the tracker and BULK_PORT_PLAN.md.
- Tracker: 202 beads in 965 KB. dc-j9k alone is 99.6 KB (notes 69 KB, 155 lines, 16 comments),
  a ledger of tried-and-rejected numeric variants scored by match deltas ("gains 31385 but loses
  2068, so was discarded"; "Training-only ... model ... 141877 training rows/104007 controls").
  That is a record of fitting, not of identifying Excel's algorithm, and it is the shape the
  per-commit headline invites: f937b23 "retain full matches 3604716 to 3613585 (+8869/-0)";
  f95f13f "record software replay; native matches 3598530 to 3598530" (a commit for no change).
- Commit bodies since 10-24 total 371 KB (~800 bytes each), mostly Validation: paragraphs.

**Recommend:** `git rm` the frozen planning files (history keeps them). Bead notes hold the
current state and next step, overwritten not appended. A match-count delta is not a commit.

### 8. Silent non-runs read as passes in check (LOW, keep with a small fix)
53 `ACCEPT-NOT-RUN` sites; under plain `cargo test` such a case passes (ci.rs:6-9). E.g. a calc-func
oracle sheet edited but not recaptured prints NOT-RUN and passes (acceptance.rs:2845-2876). The
design is honest, but `CHECK passed tests` hides it. **Recommend:** check counts and prints
NOT-RUN lines in its summary.

## Keep as is
- `cargo xtask check` scope and its touched-plus-dependents selection (check.rs:85-123).
- Word scan over new text and provenance markers per commit: sub-second, catches real slips.
- Content-hash-keyed captures and the pending-capture flow: move-tolerant, no Excel at commit time.
- `cargo xtask corpus` (20,030 cells, seconds): the right headline measure, run per package.
- sweep's wasm step (14 min 50 s) on a schedule.

## Summary table
| Item | Action |
|---|---|
| App smoke (482 cases, one file, no filter, no home) | Move to app, split, filter, core per-commit for UI lane, rest to sweep |
| xtask depends on product; double build | Split thin xtask from harness binary; opt-level for numeric crates |
| accept-receipts (10.2 MB), ACCEPT-NOTE prose, acceptance.json | Drop |
| ci/ci-audit/evidence/pin-sources/infra-m0.x cases | Drop; fold ci wasm + licenses into sweep |
| Four layering mechanisms | One architecture.json; drop source-text tests |
| Bead-named tests, 22.6k-line acceptance.rs | Rename/split on touch |
| 2.6 MB frozen planning docs; 99 KB bead notes | Drop docs; cap notes |
| NOT-RUN shown as pass | Print counts in check |
