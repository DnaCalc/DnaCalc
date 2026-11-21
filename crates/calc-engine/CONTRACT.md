# calc-engine contract

CalcEngine owns the workbook dependency graph and derived calculation state. The host owns authored
changes and supplies the workbook model and external inputs.

- `EngineState` keeps formula nodes, prerequisites and the published values of a workbook
  (`src/state.rs:339`). Formula evaluation and reference binding run through CalcFml.
- Stage graph changes with `prepare_edit` against the authored batch and its base. `commit_edit` applies
  the prepared changes and marks their dependency effects (`src/edit.rs:221`, `src/edit.rs:871`).
- `begin_recalc` prepares work. `step` advances it and `finish` publishes the completed results.
  Publication distinguishes calculated values, file caches and typed refusals
  (`src/recalc.rs:1125`, `src/recalc.rs:1939`, `src/recalc.rs:2863`, `src/lib.rs:37`).
- `tests/acceptance.rs` exercises dependencies and recalculation. See
  `docs/architecture/RECALC_CONTRACT.md` for publication, scheduling and measurement details.

Surface: f017decdf4d347e7 at 4fb74eaf
