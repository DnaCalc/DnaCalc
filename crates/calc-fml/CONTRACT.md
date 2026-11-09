# calc-fml contract (draft)

Read at `origin/master` = `cb0b76a5` (DnaCalc-win), 2026-11-09. Paths are relative to `crates/calc-fml/src/`
unless they start with another directory. `path:line` was read at that commit; "(inferred)" was not.

## 1. Purpose and position
The Excel formula language: lossless parsing, reference binding, compilation to a shared immutable plan, and the
one evaluator (`lib.rs:1-33`). It holds no workbook, no graph and no schedule. Portable: builds for wasm32
(`crates/calc-fml/Cargo.toml:10`). Depends on `calc-catalogue`, `calc-syntax`, `calc-func` (with the calc-value
types) and `serde` (`Cargo.toml:12-17`). `tools/xtask/architecture.json:29-38`: its closure must not contain
`calc-engine`, `calc-host`, `calc-doc` (the codec) or `calc-vba`. The same file forbids calc-fml in the closures
of `calc-syntax`, `calc-func`, `calc-doc-model`, `calc-verdict`, `xtask` and `calc-platform-desktop`. Dependents
today (their `Cargo.toml`): calc-engine, calc-host, calc-doc, calc-vba, the add-in hosts, the apps, xtask-harness.

## 2. Public API by concept
**Syntax.** `parse_formula(&str) -> SyntaxTree` (`syntax/parser.rs:34`); the tree reprints the text exactly, with
UTF-16 spans (`lib.rs:5-6`). `check_entry(tree, loaded)` applies Excel's entry rules (`eval/entry.rs:112`).

**Reference model and binding** (`reference/`).
- Anchor-free operands: `AxisRef::{Absolute, Relative}`, `CellRef`, `ReferenceShape`, `SheetTarget`,
  `ReferenceOperand` (`reference/mod.rs:90-162`); `FormulaAnchor` (`reference/mod.rs:60`).
- Placement: `ReferenceOperand::instantiate`, and `instantiate_wrapped` for a defined name's references, give
  `Result<GridArea, PlacementError>` (`reference/mod.rs:204,244`).
- `bind_references` (`reference/bind.rs:360`) binds against the caller's `ReferenceCatalog`
  (`reference/bind.rs:46-103`: sheets, defined names, tables, sheet spans, external sheets, `workbook_name`).
- INDIRECT text: `bind_text(text, notation, anchor, catalog) -> TextReference` (`reference/text.rs:84`), the same
  binder, called by the evaluator as the call runs; without a catalogue the call is refused (`eval/mod.rs:31-34`).

**The plan** (`eval/mod.rs`).
- `CompiledPlan` (`:81-93`): blocks, reference operands, `wrapped` flags, externals, `FormulaForm`, name and
  callable binding proofs. Block 0 is the formula; other blocks are lazy arguments, LAMBDA bodies and name bodies.
  Each block leaves exactly one value (`:73-75`).
- `Instruction` (private, `:546-643`), 23 kinds: `UserCall`, `Literal`, `Omitted`, `Reference`, `Reference3D`,
  `Call`, `LazyCall`, `Range`, `Intersect`, `Union`, `Implicit`, `ImplicitReference`, `Spill`, `Local`, `Store`,
  `Lambda`, `Invoke`, `Function`, `External`, `Name`, `Copy`, `NearZeroGuard`, `NearZeroCheck`. A cell
  dereference is not an instruction: it happens in a call's argument admission (`eval/run.rs:1378`).
- Compile: `compile`, `compile_loaded`, `compile_with*`, `compile_loaded_with*` (`eval/compile.rs:156-284`);
  failures are `CompileError`/`Gap`, and `Gap::owner()` names the owning work package (`eval/compile.rs:54-101`).
- `PlanCache` (`:753`) interns by hash with `Weak` entries and an equality check; it never keeps a plan alive.
  `instantiate(plan, anchor) -> FormulaInstance` (`:835,885`) places the plan at one anchor.
- Predicates: `volatile()` (`:364`), `has_user_calls()` (`:203`), `parallel_call_limit(bindings)` (`:213`),
  `only_cached_user_calls` (`:240`), `permits_early_execution(bindings)` (`:255`), `provisional_references`
  (`:272`), `references()` (`:324`), `external_references()` (`:341`), `calls()` (`:348`), `user_calls()` (`:370`).
  `name_memo_candidates()` (`:100`) gives `NameMemoCandidate`s with a `NameMemoEligibility` (`:712-739`):
  static preconditions only, no permission to reuse a value (`:98-99`).

**Evaluation** (`eval/run.rs`).
- `evaluate(EvalFrame, &mut FormulaEvaluationContext) -> Result<CalcValue, EvalFault>` (`:415`) runs to the end;
  a wait comes back as `MissingInput`, an external wait as `NotModelled` owned by dc-hsa5.4 (`:422-432`).
- `EvalContinuation::new(instance, scratch)`, `advance(services, quantum) -> EvalStep` (`:328,373`),
  `waiting_input()` (`:346`), `pending_external()`/`accept_external()` (`:339,350`), `into_scratch()` (`:391`).
- `EvalStep::{Suspended(ContinuationProgress), Complete(Result<CalcValue, EvalFault>)}` (`:167-170`). `EvalFault`
  (`:92-118`): `Function`, `MissingInput`, `ReadContract`, `AreaAsValue`, `NearZeroFinalSum`, `NotModelled`.

**Caller-supplied services.**
- `ReferenceReader` (`reference/read.rs:11-73`): `read_area`, `read_cell`, `spill_extent`, `subtotal_exclusions`,
  `read_fact`, `read_denial`, `pending_producer`, `provisional_*_pending`.
- `FormulaEvaluationContext::new(numeric, reader, environment, host, request)` with builders `with_catalog`,
  `with_functions`, `with_random`, `with_user_function_effects`, `observed_by`, `with_external_inputs` (the RTD
  cut), `with_rtd_connector` (`eval/context.rs:106-215`). `None` is an absent service, never a fallback (`:103-105`).
- `CallableTable`, `CallableBindings` (`callable.rs`). `UserFunction`, `UserFunctionContext`, `RandomSource`,
  `HostFacts`, `RtdCut` are calc-func re-exports (`lib.rs:63-79`). `EvaluationObserver` (`eval/mod.rs:915-933`).

**Effects and evidence.** `UserFunctionEffects` (`eval/mod.rs:427`): `calls()`, `invocations()`, `volatility()`,
`reevaluation()`, `external_inputs()`, `callable_reads()`, `name_invocations()` (at most 1024 recorded, `:439`).
The read footprint is not returned: the caller wraps its reader, which every read passes (inferred, review 1.9).

## 3. Guarantees and invariants
- **Plans are immutable and shared.** No evaluation writes into a plan (`eval/mod.rs:11-13`); equal normal forms
  share one `Arc` (`eval/mod.rs:69-71`); `evaluate` receives no text, tree or catalogue (`eval/mod.rs:22`).
- **Completed instructions never run again.** A finished block keeps `cursor.result` (`eval/run.rs:183,1238`).
- **Wait model** (`docs/architecture/EVALUATOR_SUSPENSION.md` section 3). Only `Call`, `LazyCall` and `Invoke`
  wait (`waits_for_inputs`, `eval/run.rs:214-219`), and only when the fault is `MissingInput`, the reader answers
  `pending_producer(cell)`, and no user function was entered since the instruction started (`:1205-1229`).
  `ResumePoint { depth, user_calls }` (`:200-206`) is the whole rewind state: a waiting instruction reads its
  operands in place and pops them on completion (peek, do not pop), then runs again from its start. Any other
  pending read ends the evaluation with the fault. Excel instead restarts the formula; pure values never differ.
- **A suspended `EvalContinuation` holds** the instance (and so the plan), the scratch (operand stack, locals)
  and the cursor tree: lazy arguments, child blocks, the pending user call (`eval/run.rs:173-185,320-325`). The
  effects sink and random source are the caller's and must be the same on every `advance` (`eval/run.rs:371-372`).
- **Effects at most once.** A wait after a user call inside the same instruction is not taken; the observer is
  shown a call when it completes, not on the attempt that waited (suspension doc section 6).
- **Typed refusals, never a guess.** `EvalFault::NotModelled { construct, owner }` names the owning work package
  (`eval/run.rs:112-117,633`); a fault is never a worksheet value (`eval/run.rs:89-90`).
- **Expected of the reader.** Row-major runs covering each cell of the area once (`reference/read.rs:39-43`);
  `Missing` is never blank (`:93-95`); `pending_producer` only for an admitted, unfinished producer (`:12-13`).
- **Depth.** Evaluation recurses for lazy arguments, LAMBDA calls and names: `MAX_CALL_DEPTH = 64` is a
  native-stack limit and deeper nesting is refused, not computed (`eval/run.rs:148,2080-2088`).

## 4. What it must never do
- Touch `std::fs`, `std::process`, `std::os` or `std::net`, or stop building for wasm32 (AGENTS.md, Portability).
- Depend on calc-engine, calc-host, the calc-doc codec or calc-vba; grow a second evaluator (honesty rule 4).
- Hard-code an answer, weaken an assertion, regenerate a capture to force a pass, or commit `todo!()` (rule 3).
- Guess a value for an unmeasured case: refuse with a typed, owned fault; a refusal counts as a failure (rule 2).
- Run a completed instruction or a user function twice in one evaluation, or read a cell other than through the
  caller's `ReferenceReader`.

## 5. Excel behaviours it owns
- **Legacy vs dynamic form.** A legacy formula intersects (`@`) or lifts by the place of the argument: four
  measured `Place` kinds, and `Unknown`, which is refused (`eval/legacy.rs:1-56,63-110`; `Admission`,
  `eval/mod.rs:529-542`; cell-result rules `eval/run.rs:540-560`). Measured: Excel 16.0 through COM, 2026-10-23,
  `Formula2` read-back; later rows from dc-m3u, dc-10y, dc-o9r, dc-b1l.
- **Root near-zero rule.** The formula's final `+`/`-` gives 0 when the left operand's biased exponent exceeds
  the result's by 50 or more; refused (`NearZeroFinalSum`) when the left operand is not a number
  (`eval/run.rs:120-130,2358-2373`). Root only (`eval/compile.rs:503-508`). Measured: Excel 16.0.20430, dc-b1l.
- **Lifting and broadcasting.** Result shape is max rows by max columns; a one-row or one-column array repeats;
  a short array leaves `#N/A` (`eval/lift.rs:1-27`; `eval/run.rs:1686,1766-1789`). Cited: OxFml fixture, W111.
- **Lazy functions.** Only IF and CHOOSE (`BranchOnCondition`) and IFERROR and IFNA (`FallbackOnError`) are lazy;
  IFS and SWITCH are eager with a whole-result rule (`crates/calc-catalogue/src/logical.rs:83`, `lookup.rs:149`,
  `error_fallback.rs:26,41`, `logical_more.rs:29-30,53-54`). An array condition runs every argument.
- **LET/LAMBDA.** A closure is callable only by the plan that made it (`eval/run.rs:2251-2253`). Budget 16,383
  units, 3 per call plus 1 per LAMBDA argument, over budget is `#NUM!` (`eval/run.rs:150-157,2259-2269`).
  Measured by OxFml: 4,094 levels pass, 4,095 is `#NUM!`; a named self-call reaches 5,460.
- **Defined names.** Body compiled once per plan as a block (`eval/compile.rs:17-19`), evaluated per use in its
  own frame, one record per completed use (`eval/run.rs:2134`; `docs/architecture/RECALC_CONTRACT.md:1201-1203`).
- **Plan volatility.** Any call with the catalogue's `volatile` trait in any block, untaken lazy branches
  included: RAND, RANDBETWEEN, RANDARRAY, NOW, TODAY, OFFSET, INDIRECT, CELL, INFO (`eval/mod.rs:360-366`;
  `crates/calc-catalogue/src/ported.rs:324-325,401-402,603-605`, `offset.rs:37`, `reference_text.rs:24`).
- **UDF volatility request.** Three states; the last executed request wins and an untaken branch asks nothing
  (`UdfVolatility`, `eval/mod.rs:403-422`; `set_volatile`, `eval/run.rs:4245-4247`). Cited: probe P17.

## 6. How it is tested
- Unit tests sit in the source files; replay tests in `eval/run/tests/retry.rs` (`:403`, `:525`).
- Acceptance: `crates/calc-fml/tests/acceptance.rs`; cases in `crates/calc-fml/acceptance.json` (cfml-m1.1 to .3,
  dc-2mlz, dc-besk, dc-ibp2, dc-l10u, dc-51ka), run with `cargo xtask accept --id <id> --platform <p>`.
- Oracle fixtures: `tests/fixtures/{dc_51ka,dc_ibp2,dc_l10u}/oracle.xlsx`, captures under `tests/captures/`.
- Whole route: `cargo xtask corpus --oracle` and `cargo xtask witness judge --bank full`; debug and perf counts
  must match (AGENTS.md). Last recorded (suspension doc section 5, not re-run here): 76,957 formula cells, pass
  75,209, mismatched 600, refused 952, failed or missing 1, awaiting capture 108; witness 3,887,850 of 3,910,795.
- Browser rule (AGENTS.md): a test runs natively and as `wasm_bindgen_test` (the two `cfg_attr` lines), with data
  through `include_bytes!`; one that cannot keeps `#[test]` and `/// Native-only: <reason>` (`callable.rs:600`).

## 7. Open items
- Rework packages R1-R8: `docs/architecture/review-2026-11/REWORK_PLAN.md`, which gives them no bead ids.
- dc-hsa5.18 owns the suspension model, dc-hsa5.19 the replay-safety proof (suspension doc header, section 6).
- dc-hsa5.4: an external wait without a continuation owner (`eval/run.rs:429-432`), callable reentry and nested
  callback depth (`:768-776`), a helper callback without a retained kernel continuation (`:2209-2212`).
- dc-10y: unmodelled names and the `MAX_CALL_DEPTH` refusal (`eval/run.rs:630,2082`); REWORK_PLAN R6 removes it.
- dc-m3u: `AreaAsValue` at an unmeasured legacy place (`eval/run.rs:100-103`). dc-ylr: external references
  (`:699-708`). dc-snz.23: RTD without a caller or connector (`:4082,4097`). dc-snz.2, dc-snz.5: host functions.
- Review defects. D1 fixed (`crates/calc-func/src/catalogue/reference_text.rs:9-14`). D2 fixed per the suspension
  doc (`crates/calc-func/src/catalogue/grouping.rs:342`). D3 fixed (`crates/calc-engine/src/random.rs:9-13`). D4:
  function gone, `eval/run.rs:129` still names it. D5 present (`crates/calc-engine/src/recalc.rs:2117`). D6 not checked.
- Stale doc: `eval/mod.rs:18-20` lists the lazy entries without CHOOSE. Open semantic item (REWORK_PLAN):
  effect-count parity with Excel's restart. Probes without bead ids: P12 (names), P17 (volatility), P19, P21.

Verified: every `path:line` above, read at `cb0b76a5` with `git show` / `git grep` (the Instruction count is 23).
Inferred: footprint collection on the engine side; D2's behaviour (doc claim plus the type's presence); D6.
Not read: `syntax/` internals, `assist.rs`, `reference/{structured,transform,tracked,template}.rs`, `rtd.rs` bodies.

## Findings for the tracker (as reported by the drafting agent at cb0b76a5; verify before filing)

- `eval/mod.rs:18-20` lists the lazy functions without CHOOSE; the catalogue declares it lazy (`calc-catalogue/src/lookup.rs:149`).
- The code has 23 `Instruction` kinds; the first review (1.1) says 24.
- D2: `GroupingTask` exists (`calc-func/src/catalogue/grouping.rs:342`) but the old refusal text is still at
  `eval/run.rs:2209-2212`; which path still reaches it was not traced.
- The doc comment on `CompiledPlan::volatile()` says an unasked lazy call counts "as it does in Excel"; no
  measurement in the repository backs it (probe P17 is not run).
