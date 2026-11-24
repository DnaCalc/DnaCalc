# Smart Fuzzer (`cargo xtask fuzz`)

Deterministic, code-steered, Excel-compared fuzzing for CalcFunc functions (`dc-ztvv.21`).
Extends the existing witness tooling (`calc_func::invoke` through `witness::calculate_with_context`,
`calc_func::trace`, `calc_verdict::Severity` and `WitnessSet`, and `touchstone-client`) with no
parallel evaluator, verdict path, or witness format.

## Command

```sh
cargo xtask fuzz \
  --func RATE \
  --seed 20261123 \
  --budget 110000 \
  --min-budget 100000 \
  --round-size 20000 \
  --batch 5000 \
  --in-flight 4 \
  --frontiers 60 \
  --window 12 \
  --dry-rounds 3 \
  --lane sf1 \
  --out ~/codex-runs/sf1/rate-sweep \
  --bank-rows-out ~/codex-runs/sf1/bank-rows/RATE.json \
  --pin-out ~/codex-runs/sf1/rate-sweep/pinned-representatives.json
```

Pass `--dry-run` to run locally without Excel (generates the exact strategic suite, runs code
steering and local frontier bisection, freezes predictions, and writes `spec.json`, `summary.json`,
`frontiers.json`, and `report.md`). Pass `--endpoint <path>` to target a specific Touchstone Unix
socket or TCP `endpoint.json`.

## Architecture

| Module | Responsibility |
|---|---|
| `crates/calc-func/src/trace.rs` | Portable, zero-cost-when-off thread-local route/branch/iteration/boundary-margin tracer (`with_trace`, `hit`, `observe_boundary`, `record_iterations`). |
| `tools/xtask-harness/src/fuzz/rng.rs` | Seeded Xoshiro256\*\* PRNG (`(seed, tag, generator_version)`), monotone IEEE-754 ordered-bit bijection (`f64_to_ordered`/`ordered_to_f64`), Value2-safe ULP stepping (skips subnormals and `-0.0`), canonical case keys, and streaming SHA-256 (`StreamCaseHasher`). |
| `tools/xtask-harness/src/fuzz/domain.rs` | Automatic `calc-catalogue` introspection (arity, parameter classes, name-inferred kinds, 6 universal regions) plus optional per-function plug-in regions (`RATE`, `IRR`, `YIELD`). |
| `tools/xtask-harness/src/fuzz/steer.rs` | `CoverageTracker` and `generate_steered_round`: oversamples each region, immediately accepts brand-new kernel route bits, mutates rare-route / small-margin exemplars, and densifies neighborhoods around earlier Excel discrepancies. |
| `tools/xtask-harness/src/fuzz/boundary.rs` | Local 64-step IEEE-754 bisection stratified across all scalar axes, array elements (e.g. `IRR` `values[0..3]`), and regions, prioritizing `number <-> error` and `error <-> error` flips before `route_flip`, plus dense consecutive-double windows (`[lo - W ..= hi + W]`). |
| `tools/xtask-harness/src/fuzz/oracle.rs` | Touchstone execution: freezes and SHA-256-hashes `round-NN-predictions.jsonl` BEFORE submitting; verifies echoed inputs bit-for-bit (`echo_inputs = true`); pipelines `Bulk` rounds (`w1`/`w2`) concurrently with `Interactive` boundary probes (`w3`); adapts region weights (1.5x boost on discrepant regions, 0.8x decay on clean regions, stopping after `K` dry rounds once `--min-budget` is reached); bisects shifted Excel frontiers in batched 64-point ordered-bit steps. |
| `tools/xtask-harness/src/fuzz/cluster.rs` | Classifies mismatches via `calc_verdict::Severity` + ULP bucket + result transition + semantic `feature_signature`, selects minimal-complexity and worst-ULP representatives, and writes **discrepancies-only** `dnacalc-witness-set/1` JSON validated with `WitnessSet::decode`. |

## Outputs and Honesty Rules

- `spec.json`: compact reproducibility manifest (`generator_version`, `function`, `seed`, budgets, regions, `case_list_sha256`, `predictions_sha256`). Re-running with the same seed reproduces the identical initial case stream.
- `round-NN-predictions.jsonl`: DNA Calc answers written and SHA-256-hashed before Excel is called.
- `summary.json` and `report.md`: per-region coverage table (`matched / asked` over ALL calls — mismatched, unread, and crashed all count as failed), route coverage (`hit/total`), throughput, worker counts, clusters, and frontier table.
- `frontiers.json`: DNA Calc adjacent-double flip words alongside Excel's observed flip and ULP shift.
- `--bank-rows-out` (`dnacalc-witness-set/1`): **discrepancies only** (owner ruling 2026-11-21); rows that matched Excel are refused at encode time.

## How to Plug In Another Function

Every function in `calc-catalogue` works immediately with zero code changes via automatic metadata
introspection and the 6 universal regions (`special_values`, `sign_patterns`, `log_uniform`,
`near_integers`, `arity_and_optional`, `type_coercion_edges`). To add deep mathematical steering
for a new function `FOO`:

1. **Strategic regions (optional, ~50–150 lines in `tools/xtask-harness/src/fuzz/domain.rs`)**:
   Add `RegionKind::Foo*` variants, return their `RegionSpec`s in `custom_regions("FOO")`, and add
   the generator arms in `DomainSpec::generate`. Keep numeric inputs Value2-exact (`hex_f64`,
   `is_value2_exact`, `step_ulps(..., true)`) so Excel readback stays clean.
2. **Code-guided route hooks (optional, ~15 lines in `crates/calc-func/src/trace.rs` + kernel)**:
   Add `pub const FOO_ROUTES: &[&str]` in `trace.rs`, register it in `route_names("FOO")`, and
   guard branch markers inside the kernel with `if crate::trace::is_active() { crate::trace::hit(bit); }`
   (and `observe_boundary` / `record_iterations` for iterative solvers). When tracing is off, the
   single thread-local boolean check adds negligible cost and changes zero numerical operations.
3. **Semantic clustering features (optional, ~20 lines in `tools/xtask-harness/src/fuzz/cluster.rs`)**:
   Add a `"FOO"` arm in `feature_signature` to separate distinct root causes within the same
   severity/ULP bucket (e.g., iteration cap vs domain boundary vs scale regime).
4. **Verify**:
   Run `cargo xtask fuzz --func FOO --dry-run --budget 10000 --out target/fuzz/foo-dry` to verify
   route coverage and determinism from seed, then run live against Touchstone with `--bank-rows-out`.

## Live Excel Sweep Results (`dc-ztvv.21`, Lane `sf2`)

All sweeps ran against live Excel `16.0.20625.20108` (`AMD Ryzen 7 4800H`, `env_id b9d22089d2f5`)
through Touchstone with predictions frozen and SHA-256-hashed before Excel submission (`0` unread,
`0` crashed across all `161,262` live evaluations). Every differing Excel reading is banked as
`dnacalc-witness-set/1` JSON in `dna-firehorse:~/codex-runs/sf2/bank-rows/` (verified by
`CalcFunc-Evidence/harvest-bank-rows.py:lane_discrepancies`); minimal cluster representatives are
pinned in `crates/calc-func/tests/witnesses/fuzz_representatives/{RATE,YIELD,IRR}.json` and tested
by `witnesses::fuzz_cluster_representatives_calculate_as_pinned`.

### Summary Table (Baseline `406f6e3f7`, plus Post-Fix `3766bca80` Holdout)

| Function | Sweep / Seeds | Asked | Matched | Failed (`mism / unread / crash`) | Pass Rate | Routes | Frontiers (`exact / shifted / unconf`) | Excel Calls/s | Workers |
|---|---|---:|---:|---|---:|---:|---|---:|---|
| `RATE` (baseline) | 3 sweeps (`20261123..20261125`) | 106,412 | 95,968 | 10,444 (`10444 / 0 / 0`) | 90.19% | 30/30 | 120 (`113 / 2 / 5`) | 57.6* | `w1:3779 w2:102633` |
| `RATE` (re-judged post-`3766bca80`) | unique `s1+s2+s3` cases | 106,174 | 104,476 | 1,698 (`1698 / 0 / 0`) | 98.40% | 30/30 | — | — | — |
| `RATE` (live post-`3766bca80` holdout) | `seed 20261126` | 2,146 | 2,119 | 27 (`27 / 0 / 0`) | 98.74% | 30/30 | 12 (`12 / 0 / 0`) | 70.7* | `w2:2146` |
| `YIELD` (generalization) | `seed 20261123` | 27,953 | 23,842 | 4,111 (`4111 / 0 / 0`) | 85.29% | 14/15 | 30 (`22 / 8 / 0`) | 555.1 | `w1:26172 w2:96 w3:1685` |
| `IRR` (generalization) | `seed 20261123` | 22,749 | 15,620 | 7,129 (`7129 / 0 / 0`) | 68.66% | 20/20 | 20 (`13 / 5 / 2`) | 382.6 | `w1:21363 w3:1386` |

\* `RATE` was pre-persisted in GOVERT-OMEN's `isolated.json` before lane `sf2` started, forcing 1 call/batch on `w2` (~55–71 calls/s vs 555 calls/s on `YIELD` and 383 calls/s on `IRR`). Removing `"RATE"` from `isolated.json` on Windows restores full 3-worker batch throughput.

### `RATE` Number-Versus-Error Frontier Map (`120` Frontiers Across All 6 Axes)

- Axes covered: `nper` (21), `pmt` (24), `pv` (24), `fv` (25), `type` (8), `guess` (18).
- **113 / 120** matched Excel at **0-ULP shift** on baseline (`12 / 12` exact on post-fix holdout `seed 20261126`).
- **2 / 120** shifted on baseline — both fixed in `3766bca80`:
  - `s1 frontier-RATE-014` (`nper`, `error:Div0 <-> error:Num`, shifted `-24,789,332,301,330` ULP): `balance(guess)` overflowed to `+inf` and continued to `balance(guess - 0.001)` (`#DIV/0!`) instead of returning `#NUM!` immediately as Excel does.
  - `s2 frontier-RATE-017` (`fv`, `number <-> error:Num`, shifted `-499` ULP): `balance(0.0)` added `(fv + pv) + pmt * nper` instead of `(pv + pmt * nper) + fv`.
- **5 / 120** unconfirmed: 4 `type` frontiers (`-MIN_POSITIVE <-> 0.0` where `step_ulps` stepped directly from `-MIN_POSITIVE` to `+MIN_POSITIVE`, skipping `0.0`) and 1 `route_flip` where both sides are `#NUM!`.

### Discrepancy Clusters and Owning Beads

| Function | Cluster | Rows | Root Cause | Disposition / Bead |
|---|---|---:|---|---|
| `RATE` | `R1` | 8,710 | `rate_secant::balance(0.0)` addition order `(fv+pv)+pmt*nper` vs FV's `(pv+pmt*nper)+fv` | **Fixed** (`3766bca80`) |
| `RATE` | `R3` | 172 | `rate_secant::balance` returned `Ok(+-inf)` instead of `#NUM!` on overflow | **Fixed** (`3766bca80`) |
| `RATE` | `R2` | 1,031 | Tiny-cashflow (`1e-308..1e-260`) subnormal slope / `flat_change` / `financial_daz` `#DIV/0!` | `dc-j9k.97` |
| `RATE` | `R4` | 667* | Fractional `nper` negative reduced-fraction `F2XM1` amplified near `r = 0` | `dc-j9k.79` |
| `YIELD` | `Y1` | 139 | Multi-period huge-maturity (`2958435..2958465`) returns finite instead of `#NUM!` | `dc-j9k.98` |
| `YIELD` | `Y2` | 890 | Multi-period extreme price/coupon convergence `#NUM!` frontier (+8 shifted frontiers) | `dc-j9k.99` |
| `YIELD` | `Y3` | 33 | Deep-premium zero/tiny-coupon negative-base `#NUM!` vs Excel huge negative yield | `dc-j9k.100` |
| `YIELD` | `Y4` | 3,049 | Multi-period Newton objective, seed, and step arithmetic (`yield_single_period` 2,261/2,261 exact) | `dc-j9k.101` (relates `dc-3gx6`) |
| `IRR` | `I1` | 1,009 | Leading-zero cashflow discounting shifts time 0 and yields bogus huge roots or `#NUM!` | `dc-j9k.102` |
| `IRR` | `I2` | 1,962 | `#DIV/0!` erased by `Option<f64>` secant return and masked by retry ladder (+2 shifted frontiers) | `dc-j9k.103` |
| `IRR` | `I3` | 4,144 | Huge-root and near-`-1` acceptance/stopping frontier (+3 shifted frontiers) | `dc-j9k.104` |
| `IRR` | `I4` | 14 | `+-DBL_MAX` overflow and near-`-1` long-chain retry refusal (`irr_multi_flow_ordinary` 1,791/1,791 exact) | `dc-j9k.105` (relates `dc-j9k.43`) |

\* Post-`3766bca80` count across the 106,174 unique `s1+s2+s3` cases (plus 26 in `seed 20261126` holdout; `R1+R3` dropped from 8,882 to 0).

## Continuous sweep scheduler

```sh
cargo xtask fuzz sweep --plan
cargo xtask fuzz sweep --out ~/codex-runs/sweep/state \
  --bank-rows-out ~/codex-runs/sweep/bank-rows --tracker /path/to/dedicated/master-clone
```

The queue uses `docs/fuzz/order.tsv`. Every function gets a first sweep, ordered by the
usage × complexity score, then by discrepancy class at equal scores. Subsequent sweeps
start with the least recently completed function. The score scales the budget from 64 to
512 cases. A default sweep uses the live fuzzer's adaptive rounds and leaves targeted
frontier mapping to follow-up runs. `--runs <N>` limits completions for a smoke run.
`--endpoint <path>` selects a specific broker or relay endpoint.

On `dna-firehorse`, the standing sweep runs in `tmux` session `sweep` via
`~/codex-runs/sweep/run.sh`: each pass pulls `master` in its dedicated `DnaCalc-sweep` clone,
executes `cargo xtask fuzz sweep --out ~/codex-runs/sweep/state --bank-rows-out ~/codex-runs/sweep/bank-rows --tracker ~/projects/DnaCalc/DnaCalc-sweep --runs 20`,
logs to `~/codex-runs/sweep/sweep.log`, and sleeps 60 seconds before the next pass (pause with
`touch ~/codex-runs/sweep/state/PAUSE`, stop with `touch ~/codex-runs/sweep/STOP`).

Create `PAUSE` in the state directory to stop submissions. Submitted chunks may finish
so their results can be recorded. Remove the file to resume. The scheduler retries an
interrupted function with its original recipe and seed in a new attempt directory. A
broker or relay restart can therefore repeat some calls.

Touchstone caps sweep work at one worker by default. Start the updated broker with
`--sweep-workers <N>` to use spare workers while the foreground is idle. Sweep requests
yield to ordinary work between short batches and leave the configured interactive reserve
available. Queued sweep requests expire after 12 seconds and the client waits up to 15
seconds per submitted request. The broker can keep its Excel workers running. Direct
connections need the updated broker. Relayed connections also need an updated relay
bundle so the sweep capability is advertised to the client.

Keep the state and bank outside the repository for a standing run. Each attempt records
the recipe, generator, CalcFunc commit and harness hash, and the observed Excel
environments include the build and CPU. Completed runs keep coverage, cluster
representatives, and prediction hashes. The bank receives verified discrepancies as
replies arrive, including observations from an interrupted attempt. With `--tracker`,
the scheduler files clusters in a dedicated master clone with `br`, using the function
and cluster key as a stable external reference across sweeps (`dnacalc-fuzz:<sha256>`).
It checks and pushes tracker updates using the clone's configured Git access and
`DNACALC_AGENT_LOG`. Without `--tracker`, the completed reports remain available for
filing from a later invocation. The dedicated clone should have no independent
unpublished work.

## Fractal refinement, patient-zero re-sweep, storage, and textures

- **Fractal boundary refinement (`dc-ztvv.26.5`, `tools/xtask-harness/src/fuzz/fractal.rs`):**
  `--fractal-depth <D>` (default `2` for standalone runs, `0` for standing discovery sweeps) refines
  DNA Calc's local `predicate_flip`, `route_switch`, `iteration_regime`, and `outcome_regime`
  transitions through `Decade -> Binade -> ULP` stencils, recursing onto secondary axes while
  holding the primary edge pinned ("edges of edges"), and escalating coarse Excel discrepancies to
  `Decade`/`Binade` + 64-point parallel bisection.
- **Patient-zero infestation protocol (`dc-ztvv.26.6`, `tools/xtask-harness/src/fuzz/patient_zero.rs`):**
  `cargo xtask fuzz --patient-zero <json> [--pz-siblings F1,F2] [--max-siblings 6]` expands a
  failing case across its fractal neighbourhood, the same kernel route under varied arguments, and
  sibling functions sharing `kernel_file` in `docs/fuzz/order.tsv`, writing a reproducible
  `resweep-recipe.json`. After fixing, `cargo xtask fuzz --resweep <resweep-recipe.json>` returns
  `can_close == true` only when live Excel confirms `0` failures (`0` mismatched, `0` unread, `0`
  crashed) on patient zero, the primary region, and every sibling.
- **Storage policy (`dc-ztvv.26.7`, `tools/xtask-harness/src/fuzz/storage.rs`):** every sweep writes
  `recipe.json` (`dnacalc-fuzz-recipe/1`) with seed, generator version, `calc_func_commit`,
  `hint_files_sha256`, `reproduce_command`, observed Excel builds/CPUs, counts, and a
  discrepancies-only attestation (matching readings are never stored). `--verify-storage <dir>`
  enforces this policy; `--audit-bank <witness.json>` produces a read-only retention plan for
  `dc-jkf7s`.
- **Interesting texture catalogue (`dc-ztvv.26.8`, `tools/xtask-harness/src/fuzz/texture.rs`):**
  every sweep emits `textures.json` (`dnacalc-texture-catalogue/1`, `--texture-out <json>`,
  `--validate-textures <json>`) keeping up to `--max-textures` passing or failing boundary,
  `error_vs_number_neighbour`, `route_switch`, `near_tie`, `regime_change`, and `extreme_magnitude`
  cases with hex-bit args and recipe provenance.

## Cluster triage cadence

Standing workflow (`dc-ztvv.26.4`; practical lessons and cycle log in
[docs/research/fuzz-playbook.md](../research/fuzz-playbook.md)):

1. **Automatic filing and pre-triage filter:** `cargo xtask fuzz sweep --tracker` files each new
   `(function, cluster_key)` once under `dc-ztvv.26` with label `fuzz-sweep-cluster` and external
   ref `dnacalc-fuzz:<sha256>` (`P1` for `gross`/`structural`, `P2` for `numeric`/`last_bit`).
   Before assigning a bead to a Carbon round, separate harness/transport faults
   (`fault:ReferenceRequired`, `error:SPILL!_vs_fault:ResultShape`, `fault:NotImplemented`) from
   genuine Excel-vs-CalcFunc discrepancies; route harness faults to fuzzer/hint maintenance
   (`dc-ztvv.26.11` / `dc-ztvv.26.10`) instead of spending Carbon cycles on them.
2. **Ranking order:** rank genuine open cluster beads by:
   1. **Kind / severity:** `number <-> error` first, then `error_A <-> error_B`, then `gross`
      (`> 1,024` ULP), then `numeric` (`5..1,024` ULP), then `last_bit` (`1..4` ULP).
   2. **Size:** discrepancy row count in the sweep, plus number of shifted frontiers when mapped.
   3. **Corpus reach and kernel grouping:** `priority_score` rank in `docs/fuzz/order.tsv` and
      shared `kernel_file` so related clusters in one solver or family are triaged in one batch.
3. **Starting a Carbon round:** start a read-only Carbon batch when the top of the ranked queue
   contains `>= 1` non-harness `number <-> error` / `error <-> error` cluster (or a coherent batch
   of `1–3` related functions needing root-cause analysis). For clusters discovered by a low-intensity
   (`64–512`-case) standing sweep, first run `--patient-zero <rep.json>` (or a targeted `5k–25k`
   sweep with `--frontiers --fractal-depth 2`) so Carbon receives the full neighbourhood, shifted
   frontiers, and affected kernel siblings. Straightforward single-cause bugs already isolated by a
   shifted frontier may land directly without Carbon, still following steps 4–6. Each Carbon round
   produces `review.md` separating checked facts from speculation, validates rival hypotheses with
   pre-frozen live Excel discriminators (`12–20` probes per question, differing rows banked), and
   delivers a portable (`Hard == Soft`) candidate with full-bank before/after counts.
4. **Landing with the no-losses rule and pinned regressions:** a Neon or Astra lane claims the
   beads in ranked order and lands one root cause per commit, starting with a failing-first test:
   - Pin each cluster's minimal representative in
     `crates/calc-func/tests/witnesses/fuzz_representatives/<FUNC>.json` (verified natively and on
     `wasm32` by `witnesses::fuzz_cluster_representatives_calculate_as_pinned`). Open clusters carry
     `"calc_func"`; fixing a cluster removes `"calc_func"` so the exact Excel `expected_bits` joins
     the permanent portable regression set.
   - Verify `+0/-0` (zero regressions) via `cargo xtask witness judge --bank full` on the function
     and its kernel siblings, plus `cargo xtask check` (`--dependents` when shared helpers change).
     Never weaken or override an existing live-Excel test; if a Carbon proposal contradicts one,
     land only the clean part, probe the contradiction live, bank differing rows, and record a
     `visited:` note naming the owning bead.
5. **Independent Neon review before the next batch:** every landing (and every fuzzer or harness
   change) receives a read-only Neon review (`rw4`/`rw5` cadence) before the next Carbon or landing
   batch builds on that kernel or tooling. Reviewers check generality (no fixture-keyed rules),
   honesty (frozen predictions, no weakened tests, discrepancies-only banking), `Hard == Soft`
   portability, and runtime/table/provenance alignment; `P0`/`P1` findings are reworked before the
   next batch starts.
6. **Closure on a clean live re-sweep:** a cluster closes only after live Excel verifies `0`
   failures (`0` mismatched, `0` unread, `0` crashed):
   - via `cargo xtask fuzz --resweep <resweep-recipe.json>` yielding `can_close == true` across
     patient zero, the primary region, and every kernel sibling; or
   - via clean live re-sweeps on both the original failing seed and a fresh unseen holdout seed with
     frontiers mapped (all frontiers at `0` ULP for the closed cluster's root cause, with any
     separate remaining clusters explicitly owned by their own open beads).
   Record exact tool-output before -> after counts in `br close`, refresh
   `crates/calc-func/fuzz-status.tsv` via `cargo xtask fuzz coverage` (which resolves open vs closed
   beads via `dnacalc-fuzz:<sha256>`), and log the cycle in `docs/research/fuzz-playbook.md`.

## Per-function fuzz coverage (`crates/calc-func/fuzz-status.tsv`)

```sh
cargo xtask fuzz coverage --records ~/codex-runs/sweep/state --out crates/calc-func/fuzz-status.tsv
python3 docs/fun/tools/refresh_function_bank.py
```

`cargo xtask fuzz coverage` aggregates completed live Excel sweep records (both standalone
`cargo xtask fuzz` outputs and `cargo xtask fuzz sweep` run directories with `done.json`),
excludes dry runs and interrupted attempts, resolves open vs closed cluster status against
`.beads/issues.jsonl` using the shared `dnacalc-fuzz:<sha256>` external reference, and writes
`crates/calc-func/fuzz-status.tsv` (`function`, `sweeps`, `cases_swept`, `matched`, `failed`,
`regions_covered`, `regions_total`, `frontiers_mapped`, `open_clusters`, `last_sweep_date`,
`last_sweep_commit`). `dnacalc.com/status` (`tools/xtask-harness/src/site/marketing.rs`) reads
`crates/calc-func/fuzz-status.tsv` directly, and `docs/fun/tools/refresh_function_bank.py`
embeds it as `const FUZZ` in `docs/fun/function-bank-data.js` for the Function Bank detail card
and conformance table.
