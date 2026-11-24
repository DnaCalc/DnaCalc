# Fuzz playbook: how we find and fix Excel differences at scale

A working guide for agents running, triaging, fixing and reviewing the CalcFunc smart-fuzzer
programme (`dc-ztvv.26`). Written in the form of [last-bit-guide.md](last-bit-guide.md): what
found bugs fastest, what wasted effort, which recipes and refinement scales paid off, and how the
process improves after each cycle. Started 2026-11-24 from the `RATE` pilot (`cb22`, `as2`, `sf1`,
`sf2`, `cb25`, `nl26`), the `IRR` solve (`sf2`, `cb25`, `nl26`), the independent reviews (`rw4`/`rk4`,
`rw5`/`rk5`), the domain/fractal/patient-zero tooling (`ds1`, `ds2`, `rk5`), and Pass 1 of the
standing sweep on `dna-firehorse` (`sweep`). Add to it after each sweep-and-fix cycle with measured
numbers and the lane they came from; review it at the weekly check.

## Premise

A `100%` passing witness bank does not mean a function's edges match Excel. `IRR` stood at
`3,211/3,211` (`100%`) in the committed full bank when `sf2`'s first live sweep found `7,129`
failures in `22,749` cases (`68.66%` pass rate) and `5` shifted frontiers. `ROUND` stood at
`36,953/36,953`, `AVERAGE` at `1,222/1,222`, `XIRR` at `2,861/2,861`, and `TEXT` at `4,708/4,708`
when Pass 1 of the standing sweep (`394–512` cases each) immediately surfaced real discrepancies on
all four. Gather data continuously at low intensity (`64–512` cases per function); cluster and rank
differences; solve root causes in fast Carbon rounds with pre-frozen discriminators; land with the
no-losses rule; review every landing with Neon; close only on a clean live re-sweep.

## What found bugs fastest

1. **Adaptive densification around first hits (`densify:*`).** Once Round 0 finds one mismatch in a
   region, mutating its neighbourhood and boosting that region's weight (`1.5x` vs `0.8x` decay)
   dominates discrepancy yield (`sf2`):
   - `RATE` (`s1`, `35,371` cases): `densify:rate_fv_neg_pv_and_zero_terms` failed `1,954/2,843`
     (`68.7%`), `densify:rate_ordinary_loans` `425/714` (`59.5%`), `densify:code_steered` `109/189`
     (`57.7%`).
   - `IRR` (`22,749` cases): `densify:irr_flat_and_edges` failed `1,141/1,203` (`94.8%`),
     `densify:sign_patterns` `98/99` (`99.0%`).
   - `YIELD` (`27,953` cases): `densify:yield_extreme_price_and_coupon` failed `1,287/2,216`
     (`58.1%`).
   - `ROUND` (`sweep` Pass 1, `394` cases): `densify:near_integers` found `45/45` gross misses at
     `num_digits = -0.9999999999999998` (`0xbfeffffffffffffe`, DNA `283` vs Excel `280`).
2. **Boundary bisection and shifted-frontier mapping (`boundary_bisect:*`).** Bisecting DNA Calc's
   own `number <-> error` and `error <-> error` flips to adjacent Value2-safe doubles locally, then
   probing dense consecutive-double windows in Excel, pinpointed root causes directly (`sf2`):
   - `RATE` (`120` frontiers across `6` axes): `2` shifted frontiers (`frontier-RATE-014` `-24.8T`
     ULP on `nper`, `frontier-RATE-017` `-499` ULP on `fv`) directly exposed `R3` (nonfinite
     overflow) and `R1` (`balance(0.0)` addition order). Fixing those two bugs in `3766bca80` lifted
     `106,174` unique baseline cases `90.16% -> 98.40%` (`+8,746` net bit-exact) and brought the
     unseen holdout (`seed 20261126`) to `12/12` frontiers at `0` ULP shift.
   - `IRR`: `boundary_bisect:values[0]` failed `1,236/1,619` (`76.3%`) and `values[1]` `741/1,220`
     (`60.7%`); `5` shifted frontiers mapped the exact `v >= 1e-7` / `>= 1e-4` pole cutoffs and
     `#DIV/0!` propagation (`dc-j9k.103`, `dc-j9k.104`).
   - `YIELD`: `boundary_bisect:pr` failed `1,261/1,550` (`81.4%`); `8` shifted frontiers isolated
     multi-period Newton convergence rejection (`dc-j9k.99`).
3. **Code-steered route tracing (`calc_func::trace`).** Guarding branch markers behind a thread-local
   boolean costs near zero when off and drove `30/30` `RATE` routes, `20/20` `IRR` routes, and
   `14/15` `YIELD` routes (`sf1`/`sf2`). `code_steered` alone failed `226/3,195` (`7.1%`) on `RATE s1`,
   `438/1,428` (`30.7%`) on `IRR`, and `203/2,009` (`10.1%`) on `YIELD`, surfacing rare `13`-row
   and `14`-row edge clusters (`R2` `Div0<->Num`, `I4` `±DBL_MAX` / long-chain retry).
4. **Domain-shaped strategic regions plus generic edge regions.**
   - Hand-targeted mathematical regions found the largest solver clusters immediately (`irr_flat_and_edges`
     `1,318/2,484` = `53.1%` fail; `yield_extreme_price_and_coupon` `296/3,897`; `rate_fv_neg_pv_and_zero_terms`
     `189/3,199` in `s1`).
   - Zero-custom-code catalogue regions + per-family hints (`ds1`, `dh2`–`dh4`) caught bugs on
     ordinary functions in `<512` calls during `sweep` Pass 1: `AVERAGE` omitted `None` arguments
     (`140/401` gross in `spec_generic_domain`), `XIRR` mismatched array shapes (`148/406` `#NUM!`
     vs `#REF!` + `3` `#NUM!` vs number in `hint_ranges_and_relations`), `TEXT` format-code
     `NotImplemented` (`152`) and long-output `#VALUE!` (`11`).
5. **Pre-frozen rival discriminator suites in Carbon rounds.** Instead of running another `20,000`-case
   sweep to separate competing hypotheses, build `12–20` pre-frozen probes per question in C/Python
   against recorded bank/sweep rows first, hash the probe file, then ask Excel once:
   - `cb22` analyzed `12,932` recorded `RATE` rows offline during a Touchstone outage and solved both
     the 3-part flat-secant rule (`+10/-0` bank, `37/37` `gx1`) and the 8087 reciprocal `pow2_positive`
     rule (`+12/-0` `RATE`, `+11/-0` `EXP`, `+1/-0` `EXPON.DIST`, `0` losses across `174,347` rows).
   - `as2` settled `cb22`'s two open questions on live Excel in `353` calls (`29/29` immediate `#NUM!`
     on double-flat, `18/18` upward tie nudge; `48/48` on a fresh frozen holdout).
   - `cb25` solved all `4` `IRR` clusters (`7,129` `sf2` misses -> `22,749/22,749`) and `RATE` `R2`
     (`1,031/1,032`) in `160` live probes (`2` seconds of Excel time): `96/96` on `IRR` (refuting
     every pre-registered rival on its own block), `20/20` for pre-registered `rival_posinf` over
     primary (`17/20`) on `R-B`, and `12/12` settling `cb22` carry-overs on `R-D`.
   - `nl26` then landed `IRR` (`dc-j9k.102`–`.105`) to **`39,707/39,707` (`100.0000%`, `0` failed,
     `20/20` frontiers at `0` ULP)** across `seed 20261123` (`19,862`) and unseen `seed 20261127`
     (`19,845`), proving the sweep -> cluster -> Carbon -> Neon land -> re-sweep loop end to end.
6. **Independent Neon reviews (`rw4`, `rw5`) right after major fuzzer/harness landings.**
   - `rw4` caught `4` real fuzzer bugs (`dc-ztvv.22`–`.25`, fixed by `rk4`) that passed `sf1`/`sf2`'s
     own unit tests and live runs: duplicate banked rows and leftmost-flip selection in shifted-frontier
     bisection, non-adjacent window holes reported as `0`-ULP exact flips, cross-platform `f64::powf`
     and `budget/10` vs `budget/12` determinism breaks, hardcoded `gx1-RATE-2092` hex constants in a
     generator arm, and untested `"excel_build": "unknown"` fake-broker provenance.
   - `rw5` caught `2` witness/metadata integrity gaps (`dc-ztvv.27`, fixed by `rk5`): runtime
     `Target::legacy_place` diverging from the fixed `calc-catalogue` lifting table, and
     `witness refresh --context` bypassing `refuse_stale_evidence` on a write path.

## What wasted effort (traps we fell into)

- **Stale Touchstone `isolated.json` throttling throughput by `8–10x` (`sf2`).** A leftover `"RATE"`
  entry in Windows `isolated.json` forced `1` call/batch on `w2`: `RATE` ran at `57.6 calls/s`
  (`637.7s` for `35,371` cases), whereas un-isolated `YIELD` ran at `555.1 calls/s` (`w1/w2/w3`) and
  `IRR` at `382.6 calls/s` (`w1/w3`). Check worker counts and calls/s after Round 0; remove stale
  isolation entries before large sweeps.
- **Passing array literals to `Reference`-class parameters in the standing sweep (`sweep` Pass 1).**
  In Pass 1 (`18` functions, `7,593` live calls), `COUNTIF` (`0/397`), `SUMIF` (`0/388`), `OFFSET`
  (`0/380`), `SUBTOTAL` (`0/376`), and `CELL` (`265/436`) sent inline array literals to
  `Reference`-required parameters, producing `fault:ReferenceRequired` vs Excel `#SPILL!`/`#VALUE!`.
  That burned `1,541` live calls across four `0%`-match functions and filed `47+` spurious
  `fuzz-sweep-cluster` beads (`dc-ztvv.26.69` etc.). Similarly `INDEX` (`76+` `ResultShape`) and
  `VLOOKUP` (`127` `NotImplemented`) triggered `error:SPILL!` in single-cell bulk mode. Reference-taking
  and multi-cell-returning functions need range-backed Touchstone evaluation or explicit skipping of
  invalid inline-array shapes before automatic cluster filing.
- **Over-sampling already-clean regions (`sf2`).** `rate_convergence_budget` (`2,489` + `2,574`
  cases), `rate_near_flat_and_regimes` (`3,042` + `3,146` cases post-`dc-oqc1`), `irr_multi_flow_ordinary`
  (`1,791` cases), and `yield_single_period` (`2,261` cases) were `100.000%` exact. Keep clean-region
  allocations small so adaptive decay frees budget for failing regions and frontiers.
- **Hardcoding a known failing fixture into a generator (`sf1` -> caught by `rw4` / `dc-ztvv.25`).**
  Putting the `5` raw hex words of `gx1-RATE-2092` into `gen_rate_near_flat_and_regimes` wasted
  `1/7th` of that region on a fixture already pinned in unit tests and hid whether general sampling
  could rediscover the regime. Sample the regime, never the fixture.
- **Applying a bisection fix in only one module (`sf2` -> caught by `rw4` / `dc-ztvv.22`, `.23`).**
  `406f6e3f7` fixed closest-flip selection in `boundary.rs` but missed `oracle.rs`'s shifted-frontier
  bisector, which also discarded `seen_keys` and double-banked discrepancies. And `boundary.rs`
  dropped `seen_keys` window hits without asserting Value2-compact adjacency (`c1 == c0 + 1`),
  fabricating `0`-ULP exact flips across holes or `Unread` rows. Always test adjacency, caching, and
  dedup on the live oracle path, not only the local helper.
- **Fake-broker schema drift hiding `"unknown"` provenance (`sf1` -> caught by `rw4` / `dc-ztvv.25`).**
  Fake brokers emitted `"excel_build"` while `encode_witness_set_json` read `"file_version"`/`"build"`,
  so tests passed while writing `"excel_build": "unknown"`. Mirror `PROTOCOL.md` keys exactly and
  assert non-empty provenance fields in tests.
- **Platform `libm` calls and mismatched reserves breaking reproducibility (`sf1` -> `rw4` / `dc-ztvv.24`).**
  `f64::powf`/`ln` differ by `1` ULP between glibc and ucrt, and `budget / 10` (dry-run) vs
  `budget / 12` (live) diverged on Round 0. Keep generators pure IEEE-754 software arithmetic (`rng.rs`)
  and share one `main_round_budget(cfg)` helper.
- **Landing a Carbon hypothesis without checking existing live-Excel tests (`nl26` on `dc-j9k.97`).**
  `cb25`'s `pc + mc` un-flushing scored `1,031/1,032` on `sf2` and `0` losses on the `4,867`-row bank,
  yet removing `excel_underflow_to_zero` broke existing live-Excel test
  `f3_financial::rate_stores_underflow_before_the_secant_division` (`RATE(1,-1e-304,1e-304,0,1,0)=#NUM!`).
  `nl26` caught this immediately, landed only the clean `pos_power == +inf` rescue (`68ba9bee5`,
  `0` `#DIV/0!` misses on re-sweep), and ran `20` live probes (`14` differing rows banked) that
  isolated a `BeginningOfPeriod` `MIN_POSITIVE/16` sub-regime (`P07=1.5e-296 #DIV/0!` vs
  `P08=1.0e-296 #NUM!`). Never override a live-Excel test to force a Carbon rule in; probe the
  contradiction instead.
- **Unquoted or concurrent `.beads/issues.jsonl` pushes against the standing sweep (`sweep`, `pb1`).**
  The standing sweep pushes cluster beads after each batch (`dc-ztvv.26: record new fuzz clusters`),
  so concurrent pushes hit non-fast-forward rejections or JSONL merge conflicts. Follow AGENTS.md:
  pull `--no-rebase` immediately before `br` writes; on a `issues.jsonl` conflict take theirs,
  re-import, redo your `br` write, flush, verify `git grep -c "^<<<<<<<" -- .beads` is empty, and
  push at once.
- **Leaving raw per-run outputs on disk (`firehorse` 2026-11-20 incident).** Keep only `recipe.json`,
  `summary.json`, `frontiers.json`, `textures.json`, `report.md`, and discrepancies-only `bank-rows/`.
  Never store matching readings (`dc-ztvv.26.7`); re-reading Excel is cheaper than storing passes.

## Recipes and refinement scales that paid off

| Stage / Purpose | Recipe & Parameters | Observed Cost & Yield (Lane) |
|---|---|---|
| **1. Catalogue discovery (standing sweep)** | `cargo xtask fuzz sweep --out ~/codex-runs/sweep/state --bank-rows-out ~/codex-runs/sweep/bank-rows --tracker <clone> --runs 20`; budget `64–512` scaled by `priority_score` (`docs/fuzz/order.tsv`), `round_size <= 128`, `batch 32`, `in_flight 3`, `max_frontiers 0`, `fractal_depth 0`, `1` worker (`w1`) | `sweep` Pass 1 (`11:01–11:07`, ~`2.8 min` live Excel after build): `18` functions, `7,593` live calls, `0` unread/crashed. Confirmed `5` clean (`SUM`, `MAX`, `MIN`, `MATCH`, `WEEKDAY`); found real bugs on `ROUND` (`45/394`), `AVERAGE` (`140/401`), `XIRR` (`153/406`), `TEXT` (`163/512`), `VALUE` (`183/476`), `DATEVALUE` (`137/418`) |
| **2. Deep pilot / function audit** | `cargo xtask fuzz --func <F> --seed <S> --budget 20000..36000 --min-budget <B> --round-size 20000 --batch 5000 --in-flight 4 --frontiers 20..60 --window 8..12 --dry-rounds 3` (`3` seeds for `100k+`) | `sf2`: `YIELD` `27,953` cases in `50.4s` Excel (`555.1/s`, `4,111` diffs, `30` frontiers); `IRR` `22,749` in `59.5s` (`382.6/s`, `7,129` diffs, `20` frontiers); `RATE` `106,412` (`3` seeds, `10,444` diffs, `120` frontiers, `30/30` routes) |
| **3. DNA-Calc fractal boundary refinement** | `--fractal-depth 2` (`Decade -> Binade -> ULP` stencils + recursive secondary-axis "edges of edges" around local `predicate_flip` / `route_switch` / `iteration_regime` / `outcome_regime` transitions; coarse Excel discrepancy escalates to `Decade`/`Binade` + `64`-point parallel bisection) | `ds2` (`dc-ztvv.26.5`, commit `45c8d1f9c`): deterministic local discovery spends `0` Excel calls on flat interiors and asks Excel only on predicted edges or coarse mismatches |
| **4. Patient-zero infestation & closure re-sweep** | `--patient-zero <json> [--pz-siblings F1,F2] [--max-siblings 6]` -> writes `resweep-recipe.json`; after fix run `--resweep <recipe.json>` (`can_close == true` only at `0` failures across patient zero, primary region, and all siblings) | `ds2` (`dc-ztvv.26.6`, commit `babca6d22`): transfers failing bit patterns into kernel siblings from `order.tsv` (`kernel_file` + `family`) and refuses closure if any sibling still fails |
| **5. Pre-frozen Carbon discriminator batch** | `12–20` hand-crafted probes per rival hypothesis (SHA-256 frozen before ask), plus `1` `SQRT(4)` health check | `cb25`: `160` live probes in `2s` settled `6` `IRR` blocks (`96/96`) and `4` `RATE` blocks (`R-B` `20/20`, `R-D` `12/12`); `nl26`: `20` probes exposed `BeginningOfPeriod` cancellation sub-regime |
| **6. Post-fix unseen holdout + full re-sweep** | Re-run original failing seed **plus** a fresh unseen seed (`--seed <new>`) at `2k–20k` cases with frontiers enabled | `sf2` `RATE` holdout (`2,146` cases, `30.4s`): `98.74%`, `12/12` frontiers exact; `nl26` `IRR` re-sweeps: `19,862/19,862` (`seed 20261123`) + `19,845/19,845` (`seed 20261127`) = `39,707/39,707` (`100%`) |

### Which refinement scale to use when

- **Decade (`10^k`)**: first pass on an unmapped function or when a coarse mismatch appears far from
  any DNA Calc transition (finds sign/magnitude regime changes and overflow ceilings).
- **Binade (`±2^e`, mid/top-of-binade)**: immediately inside every Decade bracket that flipped (finds
  power-of-two rounding shifts, `2^29` / `2^39` / `2^44` solver ULP thresholds like `cb22`'s `RATE`
  regimes, and `2^-1026` / `MIN_POSITIVE` underflow cliffs).
- **ULP (`64`-point Value2-compact binary search + `±W` consecutive-double window)**: only once a
  bracket is localized — resolves exact `number <-> error` and `error <-> error` cutoffs to `0` ULP
  (`120` `RATE` frontiers, `20` `IRR` frontiers, `30` `YIELD` frontiers).
- **Depth-2 ("edges of edges", `--fractal-depth 2`)**: hold axis `A` pinned on `(lo_bits, hi_bits)`
  and refine secondary axis `B != A` when two parameters interact (e.g., `RATE` `(nper, pv/pmt)` or
  `YIELD` `(pr, rate)`). Skip (`--fractal-depth 0`) during standing discovery sweeps (`64–512` cases).

## Toolbox

| Tool / Artefact | Where | Use |
|---|---|---|
| Smart fuzzer CLI | `tools/xtask-harness/src/fuzz/mod.rs` (`cargo xtask fuzz`) | Standalone, `--all-catalogue`, `--dry-run`, `--patient-zero`, `--resweep` |
| Standing sweep scheduler | `tools/xtask-harness/src/fuzz/sweep.rs`, `~/codex-runs/sweep/run.sh` (`tmux` `sweep`) | Continuous `64–512`-case sweeps in `order.tsv` order; banks diffs and files `fuzz-sweep-cluster` beads |
| Catalogue order & hints | `docs/fuzz/order.tsv` (`531` functions), `tools/xtask-harness/fuzz/hints/*.json` (`9` families) | `usage × complexity` queue order, kernel sibling discovery, per-family edges/regimes |
| Zero-cost kernel tracer | `crates/calc-func/src/trace.rs` | Thread-local route bitmask, iteration count, and boundary-margin telemetry |
| Storage & retention audit | `tools/xtask-harness/src/fuzz/storage.rs` (`--verify-storage`, `--audit-bank`) | `dnacalc-fuzz-recipe/1` (`recipe.json`), discrepancies-only audit, read-only retention planner (`dc-jkf7s`) |
| Interesting texture catalogue | `tools/xtask-harness/src/fuzz/texture.rs` (`textures.json`, `--validate-textures`) | `6` texture classes kept even when matching (regression, cross-build/CPU checks, handbook) |
| Coverage pipeline | `tools/xtask-harness/src/fuzz/coverage.rs` (`cargo xtask fuzz coverage`), `crates/calc-func/fuzz-status.tsv` | Feeds `dnacalc.com/status` and Function Bank (`docs/fun/tools/refresh_function_bank.py`) |
| Discrepancy harvester | `CalcFunc-Evidence/harvest-bank-rows.py:lane_discrepancies` | Validates `dnacalc-witness-set/1` `bank-rows/*.json` (rejects matching rows) |
| Pinned cluster reps | `crates/calc-func/tests/witnesses/fuzz_representatives/*.json` (`witnesses.rs`) | Portable native + wasm32 regression lock on fixed and open cluster representatives |

## Cluster triage cadence (`dc-ztvv.26.4`)

The standing loop turns continuous low-intensity sweep output into closed, reviewed fixes (full
operational specification in [smart-fuzzer.md](../design/smart-fuzzer.md#cluster-triage-cadence)):

1. **Intake and pre-triage filter (`sweep` -> `fuzz-sweep-cluster`).** On `dna-firehorse`, `tmux`
   session `sweep` (`~/codex-runs/sweep/run.sh`) runs `cargo xtask fuzz sweep --runs 20` in a loop,
   banking discrepancies only and filing deduplicated `fuzz-sweep-cluster` beads keyed by
   `dnacalc-fuzz:<sha256(function + "\n" + cluster_key)>` (`P1` for `gross`/`structural`, `P2` for
   `numeric`/`last_bit`). Before queuing a bead for Carbon, separate harness/transport faults
   (`fault:ReferenceRequired`, `error:SPILL!_vs_fault:ResultShape`, `fault:NotImplemented`, as seen
   in Pass 1 on `COUNTIF`/`SUMIF`/`OFFSET`/`SUBTOTAL`/`CELL`/`INDEX`/`VLOOKUP`) from genuine
   Excel-vs-CalcFunc discrepancies; route harness faults to `dc-ztvv.26.11` / `dc-ztvv.26.10`.
2. **Rank open clusters by kind, size, and reach.** Sort genuine open cluster beads in strict order
   (the `cb25` ranking of `dc-j9k.97`–`.105`):
   - **Kind first:** (1) `number <-> error` (`structural`), (2) `error_A <-> error_B` (`structural`),
     (3) `gross` (`> 1,024` ULP), (4) `numeric` (`5..1,024` ULP), (5) `last_bit` (`1..4` ULP).
   - **Size second:** discrepancy row count plus shifted-frontier count.
   - **Reach third:** `priority_score` rank in `docs/fuzz/order.tsv` and shared `kernel_file`
     (batch clusters sharing a solver/kernel into one Carbon round, as `cb25` grouped `IRR` `I1`–`I4`
     plus `RATE` `R2`).
3. **When a Carbon round starts.** Launch a read-only Carbon round whenever a function or shared
   kernel has `>= 1` non-harness `number <-> error` or `error <-> error` cluster (or a ranked batch
   of `1–3` related functions with non-obvious rules). First expand low-intensity (`64–512`-case)
   hits with `--patient-zero <rep.json>` (or a `5k–25k` sweep with `--frontiers --fractal-depth 2`)
   so Carbon sees the neighbourhood, same route, and kernel siblings (`dc-ztvv.26.6`). Obvious
   single-cause fixes already pinned by a shifted frontier (like `sf2`'s `RATE` `R1`/`R3` in
   `3766bca80`) can land directly without waiting for Carbon. Carbon delivers `review.md` with
   checked-vs-speculation labels, pre-frozen live discriminators (`12–20` probes per rival, differing
   rows banked), portable drop-in Rust (`Hard == Soft`), and offline bank/sweep deltas.
4. **Land with the no-losses rule and grow pinned regressions.** Neon or Astra (`as2`, `nl26`) lands
   one root cause / bead per commit with failing-first tests:
   - Pin each cluster's minimal representative in
     `crates/calc-func/tests/witnesses/fuzz_representatives/<FUNC>.json` (tested natively and on
     `wasm32` by `witnesses::fuzz_cluster_representatives_calculate_as_pinned`). Open clusters pin
     `"calc_func"`; once fixed, remove `"calc_func"` so exact `expected_bits` stays locked forever.
   - Verify `0` full-bank regressions on the function and its kernel siblings (`cargo xtask witness
     judge --bank full`) and pass `cargo xtask check` (`--dependents` for shared crates). If a Carbon
     candidate contradicts an existing live-Excel test (`nl26` on `dc-j9k.97`), refuse that piece,
     probe the contradiction live, bank the differing rows, and leave a `visited:` note naming the
     owning bead.
5. **Independent Neon review before the next batch builds on it.** After each landing (and any
   fuzzer/harness change), run a bounded read-only Neon review (`rw4`/`rw5` pattern) checking
   generality (no fixture-keyed hacks), honesty (frozen predictions, no weakened tests), `Hard ==
   Soft` portability, and runtime/table/provenance consistency. Rework any `P0`/`P1` review findings
   (`rk4`/`rk5`) before the next batch builds on that kernel or tool.
6. **Close only on a clean live re-sweep.** A cluster bead closes only when live Excel confirms `0`
   failures (`0` mismatched, `0` unread, `0` crashed) via `--resweep <resweep-recipe.json>`
   (`can_close == true` across patient zero, primary region, and kernel siblings) or via re-sweeping
   both the original failing seed and an unseen holdout seed with frontiers mapped (`nl26` closed
   `dc-j9k.102`–`.105` on `19,862/19,862` + `19,845/19,845` = `39,707/39,707`, `20/20` frontiers at
   `0` ULP). Record exact before -> after counts in the close reason, refresh `fuzz-status.tsv`
   (`cargo xtask fuzz coverage`), and append a cycle-log entry below.

## Reflection cadence and weekly review

- **After every sweep-and-fix cycle:** the closing lane appends a short dated entry to the cycle log
  below (function/cluster, before -> after numbers, what found it fastest, any wasted effort or trap,
  and any recipe/hint/harness change needed — with an owning bead for every follow-up).
- **At the weekly check:** review the cycle log alongside `crates/calc-func/fuzz-status.tsv` and
  `cargo xtask health`. Retire rules that stopped paying off, promote recurring traps into automated
  checks or hint-file rules, and tune `sweep.rs` budgets and `order.tsv` weights from measured
  bug-per-call yield.

### Cycle log

- **2026-11-23/24 — Cycle 1 (`RATE` pilot, `YIELD`/`IRR` generalization, `IRR` full solve, `rw4`/`rw5` reviews, standing `sweep` Pass 1):**
  - `RATE`: `cb22` + `as2` closed `dc-oqc1` (`4,855 -> 4,865/4,867`); `sf2` swept `106,412` cases
    (`90.19%`) and fixed `R1`+`R3` (`90.16% -> 98.40%` on `106,174` unique cases, holdout `98.74%`,
    `12/12` frontiers exact); `cb25` + `nl26` landed `pos_power == +inf` rescue (`dc-j9k.97` partial,
    `0` `#DIV/0!` on `5,459`-case re-sweep; `BeginningOfPeriod` subnormal cancellation stays `dc-j9k.97`;
    `F2XM1` fractional-`nper` stays `dc-j9k.79`).
  - `IRR`: `sf2` (`15,620/22,749` = `68.66%`, `7,129` diffs, `5` shifted frontiers) -> `cb25`
    (`160` live probes, `96/96` `IRR`) -> `nl26` (`dc-j9k.102`–`.105` closed, `39,707/39,707` =
    `100.0000%` across original + unseen seed, `20/20` frontiers at `0` ULP).
  - `YIELD`: `sf2` (`23,842/27,953` = `85.29%`, `4,111` diffs, `8` shifted frontiers); `4` clusters
    ranked in `cb25` and open for next Carbon batch (`dc-j9k.98`–`.101`).
  - Fuzzer hardening: `rw4` found `4` fuzzer bugs (`dc-ztvv.22`–`.25`, closed by `rk4`); `rw5`
    found runtime/table and stale-evidence write-path gaps (`dc-ztvv.27`, closed by `rk5`); `ds1`,
    `ds2`, `dna-koderbot`, `rk5` shipped `531`-function domain specs, fractal refinement, patient-zero
    re-sweep, storage policy, texture catalogue, standing sweep scheduler, and `/status` coverage.
  - Standing `sweep` Pass 1 (`18` functions, `7,593` live calls): confirmed `5` clean, surfaced real
    bugs on `ROUND`/`AVERAGE`/`XIRR`/`TEXT`/`VALUE`/`DATEVALUE`, and exposed the `ReferenceRequired`
    / `SPILL!` inline-array harness gap on `COUNTIF`/`SUMIF`/`OFFSET`/`SUBTOTAL`/`CELL`/`INDEX`/`VLOOKUP`
    (tracked under `dc-ztvv.26.11` / `dc-ztvv.26`).
