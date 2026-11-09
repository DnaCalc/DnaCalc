# calc-engine contract (draft)

Read at origin/master `cb0b76a5` of DnaCalc. Short paths (`recalc.rs:113`) are under `crates/calc-engine/src/`.
RC = `docs/architecture/RECALC_CONTRACT.md`; probes = `docs/architecture/EXCEL_CALC_PROBES.md`; review and plan =
`docs/architecture/review-2026-11/` `reviews/fable-evaluator-review.md` and `REWORK_PLAN.md`.

## 1. Purpose and position

- The one dependency graph of a workbook and the derived calculation state: order, evaluation through CalcFml's
  one evaluator, staged results, publication (`lib.rs:1-3`). It holds no authored input, source bytes or
  formatting; literals are read from the `ModelRoot` snapshot each call borrows (`state.rs:300-301`).
- Depends on `calc-catalogue`, `calc-value`, `calc-doc-model`, `calc-fml`; dev-only `calc-verdict`
  (`crates/calc-engine/Cargo.toml`). Declared `portability = "portable"`. calc-host drives it
  (`crates/calc-host/src/state.rs:462-469`); no application calls it directly (inferred).
- `tools/xtask/architecture.json`: its closure must not contain `calc-doc`, `calc-host`, `dna-onecalc`,
  `dna-calc`. Must not reach it: `calc-func`, `calc-fml`, `calc-doc-model`, `calc-verdict`, `dna-onecalc`,
  `dna-formula-ui`, `dna-calc`, `xtask`.

## 2. Public API by concept

- **State and bindings.** `EngineState` (`state.rs:316`), caller-owned. `new(&ModelRoot)` (`state.rs:726`) adopts
  formula cells; `with_file_caches` (831) attaches file values, never read or counted as calculated. `FormulaNode`
  (`state.rs:21`), ids keyed by anchor `CellKey` (`node_at`, 916). A node is *pending* when a change reached it
  directly; pending plus readers is the dirty closure, listed by `stale` (1004). Plans: one per normal form
  (`state.rs:347`), templates per sheet (385); binding is crate-private (`Binding` `state.rs:100`, `bind`
  `recalc.rs:2923`). Host inputs, all `state.rs`: `set_environment` 1272, `set_clock` 1298, `set_workbook_facts` 1267,
  `set_callable_bindings` 616, `set_random_seed` 1156, `set_tables_deferred` 811, `invalidate_rtd_topic` 672.
- **Graph and reverse index.** `Prerequisite::{Name, Cell, Area, AxisValue, SheetSpan}` (`graph.rs:106`), one edge
  each, never one per cell. `ReverseIndex` (`graph.rs:349`, crate-private) finds the readers of a cell.
  `set_prerequisites` (`state.rs:1359`) is public for tests; the product caller is `begin_recalc`. Queries:
  `dependents_at` (1463), `name_dependents` (1414). `ReadersCache` (`recalc.rs:3266`) keeps readers and levels.
- **Edits.** `prepare_edit(OpBatch)` -> `PreparedEngineChange` (stages only, `edit.rs:211`); `commit_edit` ->
  `CommitReceipt` (`edit.rs:787`): added or replaced nodes and readers of changed inputs become pending.
- **Recalculation.** `begin_recalc(state, inputs, mode)` -> `RecalcWork` or `StartError` (`recalc.rs:1055`, 122).
  Scoped: `begin_recalc_scoped` (1073), `..._with_poststate` (1092), taking `CalcScope::{Workbook, Sheet, Range}` and
  `EvalSelection::{DirtyAndVolatile, ForceAll, FormulaEntry}` (`contract.rs:88,101`). `step(state, inputs, work,
  WorkBudget, &mut dyn ReadObserver)` -> `RecalcProgress::{Pending, Waiting, Complete, Failed}` (`recalc.rs:1841`,
  278). `supersede_recalc[_scoped]` (1641, 1659) replaces work after an edit and reuses validated pure results
  (`CalcEvent::Reused`, `recalc.rs:1805`). Receipts: `RecalcWork::admission()` -> `AdmissionReceipt` (`recalc.rs:865`,
  `contract.rs:372`); `ValueDelta::completion()` -> `CompletionReceipt` with `ScopeCoverage` (`result.rs:313`,
  `contract.rs:392-416`); `take_events()` -> `CalcEvent` (`contract.rs:418`). Async: `pending_async`, `accept_async`
  (`recalc.rs:6332,6349`), `AsyncReturns` (`external.rs:23`).
- **Publication and storage.** `finish(state, inputs, work)` -> `ValueDelta` or `RecalcFailure` (`recalc.rs:2700`)
  publishes one generation. `publish_ready` (2499) may install certified scalar units earlier for cells named by
  `set_interest` (737). `FormulaResult::{Calculated, FileCached, NotModeled}` (`result.rs:172`), read with `result`,
  `results`, `permitted_cache`, `validated_result` (`state.rs:962,976,1081,1117`). Columnar store: `results.rs`.
- **Read view for the evaluator.** `WorkbookReader` (`read.rs:913`) implements CalcFml's `ReferenceReader`; `Observed`
  (`recalc.rs:4860`) wraps it per evaluation, shows each run to the caller's `ReadObserver` (`read.rs:141`) and
  collects the `Footprint` (`recalc.rs:6124`). `pending_producer(at)` (`read.rs:1624`) is true only for a bound,
  scheduled, not yet evaluated producer of the cell last reported uncomputed. A formula cell reads as what this
  recalculation staged, else the published calculated result; a file cache or refusal reads as a cell the reader cannot
  supply (`recalc.rs:1833-1836`). Cached (macro observation) reads are evidence only: `CachedObservation` with a pinned
  `CachedReadLease` (`contract.rs:161`, `read.rs:2069`); no edge, no demand.
- **Dynamic prerequisites overlay.** Areas read beyond placed references (INDIRECT, OFFSET, `#`, spill output cells)
  are `FormulaNode::dynamic_prerequisites` (`state.rs:53`), derived from the footprint (`recalc.rs:6148`).
  `set_dynamic` (`state.rs:1329`) replaces the vector and refiles the reverse index; it is called only at publication
  (`recalc.rs:2634,2840`). Old targets stay edges until the node publishes again.
- **Volatility.** `state.volatile` (`state.rs:369`), `is_volatile` (1060). Written at bind when `plan.volatile()`
  (`recalc.rs:3008`) and at publication from the certificate field `ReadDetails::udf_volatility:
  Option<calc_fml::UdfVolatility>` (`contract.rs:284`; no field named `udf_volatile` exists): `publish_ready`
  (`recalc.rs:2613-2623`) and `finish` (2800-2810) insert the node when the last executed user request or the plan says
  volatile, else remove it. A moved `volatile_input_version` makes volatile nodes pending (`recalc.rs:2832`).
- **Certificates and reuse.** `ResultStamp` (`contract.rs:362`): target, observation frontier, `ComputationOrigin`,
  `ReadCertificate` (255), `FreshnessClaim`. `certificate_valid_at` (`contract.rs:1018`); `RequirementState` (187).
- **Named only.** Iteration and cycles: `order_with_cycles`, `tarjan`, `next_pass`, `keep` (`recalc.rs:3519,3611,
  4241,4407`), notes `recalc.rs:28-71`; owners dc-m3u, dc-10y, dc-86f, dc-y2o. Spills: `spill.rs`, `next_wave`
  (`recalc.rs:4320`); dc-0k9. Data tables: `data_table.rs`; dc-m3u. Detached formulas and name calculation:
  `detached*.rs` (`detached_work.rs:72`); dc-6yw, dc-hsa5.6, dc-971p.34. Also `pinned.rs`, `linked.rs`, `pivot/`.
- **Parallel wave.** `mod parallel` (`recalc.rs:2212`), native only, entered from `step` (`recalc.rs:1868-1870`).
  `set_threads`, `set_parallel_minimum` (`state.rs:891,903`); defaults: 1 thread, wave 1,024, level 64.

## 3. Guarantees and invariants

- **Order, read view, publication.** The dirty closure is ordered by a ready queue, ties by anchor (sheet id,
  row, column) (`recalc.rs:1046-1049`). `step` takes `&EngineState`: it stages and never publishes. Only
  `finish` and `publish_ready` write results, spills, volatility and dynamic edges.
- **A wait.** An evaluation that read a cell a scheduled producer has yet to calculate is suspended or
  discarded, never staged; the node waits (`Attempt::Waits`, `recalc.rs:5086`; raised at 5906, 5917, 6064;
  shown to `ReadObserver::deferred`). A node that would wait on itself is on a circular reference.
- **`UnscheduledRead`.** If the pending cell is named by the formula's own placed reference, holds a formula,
  and its producer is neither waiting nor cyclic, the order missed an edge: the recalculation fails
  (`recalc.rs:5890-5900`). The old value is not handed out.
- **Effects at most once per invocation** (RC decision 6). A wait keeps activation, footprint and random stream
  (`recalc.rs:5902-5906`). A pending read after a user call that cannot wait in place is refused: "an effectful
  callable requiring a dependency continuation", dc-hsa5.4 (`recalc.rs:5908-5915`).
- **Refused or failed.** A `StartError` evaluates and publishes nothing; pending stays pending
  (`recalc.rs:119-120`). A failed, incomplete or superseded work installs no further unit; units installed
  earlier stay (`recalc.rs:2699`). A formula's own refusal is published as `FormulaResult::NotModeled` naming
  construct and owner. Iteration off: cycle members keep held values (`held_by_circular_rule`, `result.rs:76`).
- **Thread count should not change outcomes.** The wave runs only when `eligible` holds (`recalc.rs:2221`); a
  worker attempt that did not finish is evaluated again on the owner thread (`recalc.rs:2415-2418`). Known
  exception (review D5): a wait by a node with a `parallel_call_limit` while `work.parallel_scheduling` is set is
  staged as refused, "a dynamic prerequisite inside a concurrent scalar callback", dc-snz.23
  (`recalc.rs:2105-2121`). The flag comes from callable limits and user calls (`recalc.rs:1469-1476,1505`);
  that it is reached only with concurrency is the review's statement (inferred, not traced to `state.threads`).

## 4. What it must never do

- Depend on `calc-doc`, `calc-host` or an application; touch `std::fs`, `std::process`, `std::os`, `std::net`
  (AGENTS.md, Portability). It builds for `wasm32-unknown-unknown`, where it runs on one thread.
- Keep a second graph or evaluator (honesty rule 4). Dynamic edges live in the same reverse index.
- Hand out a file cache, stale value or refusal as a calculated input (rule 1), or count a refusal as anything
  but a failure; every refusal names its owner (rule 2).
- Hard-code answers or weaken a check to pass (rule 3). A SliceV1 choice is not evidence of Excel behaviour.

## 5. Excel behaviours owned, with the measurement

| Behaviour | Measured by | In code |
|---|---|---|
| Iteration order, seeds, held values | `dc_86f`, `dc_10y*`, `dc_y2o`; P14 part | `recalc.rs:36-71` |
| Volatile spill size: eight evaluations, then `#SPILL!` | P04 | `recalc.rs:117,4768` |
| Manual range calculation leaves its targets required | P02 `StillRequired` | `state.rs:357`, `recalc.rs:2628` |
| Cached macro observation reads the old cache, no edge | P01 | `contract.rs:161` |
| Reader-only range gets cached absence; shrink clears children | P05 | `contract.rs:228` |
| Restart on an uncalculated read: documented difference | `excel-calc-probes/suspension/` | `random.rs:9-13` |
| Dependents of an unchanged precedent re-invoke user functions | probes section 1 | no early cutoff (RC decision 10) |
| Wave cap 32 for spill areas that keep changing: DNA bound | not measured (RC 13) | `recalc.rs:113` |

Restart: Excel abandons the formula at the read and runs it again (UDF twice, RAND redrawn); DNA resumes in
place, so call counts differ, never values (`EVALUATOR_SUSPENSION.md` section 3). Fixtures: `tests/fixtures/`.

## 6. How it is tested

- `crates/calc-engine/tests/acceptance.rs`: cases named `<bead>__<case>`, registered in `acceptance.json`, driven
  through `prepare_edit`, `commit_edit`, `begin_recalc`, `step`, `finish`. AGENTS.md rule: a portable test has
  `#[cfg_attr(not(target_arch = "wasm32"), test)]` plus the `wasm_bindgen_test` attribute and embeds data with
  `include_bytes!`; one that cannot run in a browser keeps `#[test]` and a `/// Native-only: <reason>` line.
  Browser runs (headless Chrome and Firefox) are in `cargo xtask sweep`, never required for a commit.
- Oracle workbooks in `tests/fixtures/`, captures in `tests/captures/`. Probe fixtures: P01-P10, P12, P14, P22,
  suspension; P06 and P14 files are read by `crates/calc-host/tests/circular.rs`. Thread equivalence (1 vs 4, 8,
  16 threads): `crates/calc-host/tests/threads.rs`. `cargo xtask corpus --oracle`; `cargo xtask bench` with
  `--verify-threads` (`tools/xtask-harness/src/corpus.rs`, `bench.rs`). Per commit: `cargo xtask check`.

## 7. Open items

- Review D5 (above; owner string dc-snz.23), D6 (no bound on per-cell footprints; dedup now uses a `HashSet`,
  `recalc.rs:6124-6134`) and D3 (RC 3.3 draw-ledger text describes the old redraw model): plan R1, no bead named.
- Plan R3: a named `Volatility` policy replacing `volatile: bool` (P17). Plan open semantic item: model Excel's
  restart on an uncalculated read as a named mode, or keep the documented difference.
- dc-hsa5.4: effectful continuation; async wait inside a provisional evaluation (`recalc.rs:5869,5912,5923`).
- dc-hsa5.11: cyclic targets and iteration in Manual range calculation (`recalc.rs:1605-1629,2063`).
- dc-hsa5: certificates that did not stabilize; a spill occupying a blank it read (`recalc.rs:5142,5450`).
- dc-hsa5.6 (detached current-value evaluation), dc-971p.34 (deleted name), dc-8wp, dc-8wp.19 (pivots), dc-m3u
  (sheet-span binding has no product caller: `lib.rs:16-18`, `state.rs:1350`).
- Probes without a fixture folder (inferred: not run): P11, P13, P15-P21, P23-P29; P14 remainder (RC 13).

Verified: each `path:line` above was read or grepped at origin/master `cb0b76a5`, including D5 and volatility.
Inferred: who calls the crate; whether the D5 flag follows the thread count; that no footprint bound exists.
Not read: `recalc.rs` and `read.rs` in full, `pivot/`, `linked.rs`, `name_change.rs`, RC sections 3 and 5-10.

## Findings for the tracker (as reported by the drafting agent at cb0b76a5; verify before filing)

- D5's refusal (`recalc.rs:2105-2121`) fires under `work.parallel_scheduling` (`recalc.rs:1469-1476, 1505`), not in
  `mod parallel`; whether that flag follows the thread count is untraced, so the review's "thread count changes
  the outcome" is unconfirmed.
- D6 is partly stale: the quadratic `Vec::contains` dedup is now a `HashSet` (`recalc.rs:6124-6134`); a footprint
  bound was not found.
- The reviews and the plan name the certificate field `udf_volatile`; the code has `ReadDetails::udf_volatility`
  (`contract.rs:284`).
- "No early cutoff" is stated in RECALC_CONTRACT.md decision 10 only; no code line states or tests it.
- calc-doc reads `<f ca="1">` on load (`calc-doc/src/load/sheet.rs:493`, `calculate_always`); no engine consumer
  was found by grep (from the plan review, at 384a90f1).
