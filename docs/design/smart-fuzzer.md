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
