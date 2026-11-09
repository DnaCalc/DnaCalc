Note: commit 146db3af (the suspension study, EVALUATOR_SUSPENSION.md) landed after the pin 95c1b3bf; the Pro and Fable reviews describe the code before it.

# Evaluator rework plan (November 2026)

Sources: the Fable review (EVALUATOR_REVIEW.md), the GPT-6 Pro answer (8-evaluator-design-answer.md) to the prompt
pinned at 95c1b3bf, and the suspension study that landed after that pin (146db3af, EVALUATOR_SUSPENSION.md:
peek-don't-pop, ResumePoint, measured Excel restart-on-uncalculated-read). Both reviews agree: keep the
architecture (immutable plans, separate reference placement, one evaluator, typed refusals, caller-owned services);
make call semantics, activation state and execution evidence explicit before any further optimisation.

Every package keeps current supported behaviour and refusals, proves equivalence on the existing oracle fixtures,
corpus and function bank (same counts, debug and perf), and compares event sequences (UDF calls, draws, name
records, reads) where it touches order or identity, not only values.

## R1. Equality, replay safety, small defects (first; low risk)
- B1 (Pro): `CalcValue::eq` omits cube metadata (distinct members with equal captions compare equal; cube vs
  text too). Define the four relations explicitly (worksheet comparison, runtime identity, representation
  equality, reuse equivalence per facet); fix the omission; audit equality consumers used for reuse, cutoff and
  publication (boxed arrays, callables, spill shape, binding absence).
- Replay safety of the retry path after 146db3af: prove, with tests, that no instruction in `waits_for_inputs`
  can advance random draws, name-use records, kernel progress or host requests before returning MissingInput;
  where one can, retain its progress instead of replaying (Pro A3, risk table row 1). The UDF counter alone is not
  the proof.
- Tri-state UDF volatility request (`NoRequest`, `Set(false)`, `Set(true)`) checked end to end (Pro A8).
- Fable D2: GROUPBY/PIVOTBY call through `CallableInvoker::call`, so a wait inside their LAMBDA refuses where the
  other helpers wait. D5: a dynamic prerequisite inside a concurrent scalar callback is refused only in the
  parallel wave (thread count changes the outcome). D6: unbounded, quadratically deduplicated per-cell footprints
  from cell-by-cell UDF reads. D1/D3/D4: stale INDIRECT volatility comment, stale random.rs retry doc, dead
  `carries_final_sum`.
- Arity-before-demand and zero-argument lazy calls; `Missing` result normalisation reachable or dead (Pro risk
  table).

## R2. Evaluation evidence and stopping reasons (B2 Pro, Fable 3)
`EvaluationEvidence` returned with completion (mandatory compact proof records; optional diagnostics); typed
outcomes for quantum yield, dependency wait, host await, and semantic re-evaluation (XLL provisional unwind)
instead of one `Suspended`. Correctness never depends on an observer being present.

## R3. Plan capability summary and named volatility policy (B4 Pro, Fable 2 and 6)
One conservative per-block/per-plan summary (structural and potential reads, facts, dynamic resolution, effects,
recalculation policy, suspension/replay, result capabilities, explicit unknowns), from which the nine existing
predicates (volatility, memo, worker, cutoff eligibility) are derived; shadow the old predicates first. A named
`Volatility` policy replacing `volatile: bool`, with argument-dependent cases (probe P17). Early cutoff never
removes callbacks Excel would make in dependents (Excel recalculates dependents of an unchanged precedent).

## R4. Plan verifier and stable site identities (B8 Pro)
Verify block and reference indexes, stack effects on every guard path, local/capture layouts, arity, one result
per block; typed block/site ids. Then, behind fixtures, a named `RootSumFinalization` for the near-zero rule.

## R5. Call policy, lifting and typed demand (B5 Pro)
A compiled `CallPolicy` view over the existing rules; typed `NeedArgument(i)` with a service-free selector instead
of the provisional `#CALC!` sentinel; argument origin (raw blank vs computed Empty, padding token, boxed vs held)
preserved; lifting as explicit, independently testable stages. IFS/SWITCH stay eager and whole-result.

## R6. Explicit frame machine (B6 Pro, Fable 5)
Flat frame arena and one dispatcher covering blocks, lazy arguments, names, LAMBDA and kernel-to-LAMBDA helper
callbacks; Excel's measured recursion budget is the only logical limit (removes MAX_CALL_DEPTH 64); nonrecursive
teardown. Largest package.

## R7. Worker capability boundary (B7 Pro) and R8. name code identity (B9 Pro, Fable 4)
Checked worker view of services and concurrent callables; shared immutable name bodies (`NameBodyId`) separate
from per-use evaluation identity; certified name-result reuse only later and only for eligible bodies.

## Open semantic item
Effect-count parity on the first calculation after entry: Excel restarts a formula on an uncalculated read
(static references too) and redraws RAND; DNA resumes in place. Values never differ. Decide after R2/R3 whether
to model Excel's restart (and its calc-chain order) as a named mode or keep it as a documented difference.

## Later, bold (not scheduled)
Certified trace segments; a portable semantic execution format; GPU execution of proven pure regions. Each needs
R3 (summaries), R4 (verified plans) and R6 (explicit frames) first.
