# calc-fml contract

CalcFml owns Excel formula syntax, binding, evaluation and formatting. Hosts supply workbook services.

- `parse_formula` produces lossless syntax with editor spans (`src/syntax/parser.rs:34`, `src/lib.rs:5`).
  Reference binding takes a catalog, and evaluation reads cells through the caller's reference reader
  (`src/lib.rs:8`).
- `compile` and `compile_loaded` use the caller's `PlanCache`; `instantiate` places a shared plan at an
  anchor. Run the instance through `evaluate` or through `EvalContinuation` when the owner can resume
  suspended work. The caller owns scratch and services (`src/eval/compile.rs:156`, `src/eval/run.rs:325`).
- `CallableTable` composes provider registrations and binds callable identities (`src/callable.rs:285`,
  `src/callable.rs:325`). `RtdLedger` and `admit` track externally supplied RTD updates
  (`src/rtd.rs:48`, `src/rtd.rs:87`).
- `tests/acceptance.rs` exercises the formula route. The engine and formula explorer both use this crate.

Surface: e2727ccee25d7a80 at 4fb74eaf
