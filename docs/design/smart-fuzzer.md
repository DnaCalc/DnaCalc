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
