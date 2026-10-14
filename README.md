# DNA Calc

The re-implementation of DNA Calc, an Excel-compatible calculation stack written in Rust: the libraries **CalcFunc** (worksheet functions), **CalcFml** (formula language and number formats), **CalcEngine** (dependency-based calculation), **CalcDoc** (`.xlsx`/`.xlsm` representation with round-trip fidelity) and **CalcHost** (the authoritative workbook model and skin contract), and the applications **DNA OneCalc** (formulas without an engine) and **DNA Calc** (Excel, notebook, tree and canvas skins). Excel only. Windows, Linux and the browser (wasm) from the first version.

## Where to start

- `docs/plan/ROLLOUT_BRIEF.md`: the plan in ten minutes, the graph, the roll-out sequence.
- `docs/plan/DNA_CALC_REIMPLEMENTATION_PLAN.md`: architecture, decisions, API sketches, milestones, risks.
- `AGENTS.md`: the operating contract for everyone working here, human or agent.
- The work queue is the `br` tracker in `.beads/`: 204 tasks and 48 grouping epics. `br ready` shows what can start; the first bead is `infra-m0.1`.

## Status

Bootstrap. `infra-m0.1` created the Cargo workspace; none of its packages has a library API yet. Its acceptance case `infra_m0_1__workspace_builds` runs `cargo check` on every package for the host target and on every portable package for `wasm32-unknown-unknown`. `infra-m0.3` added the acceptance runner `cargo xtask accept --id <bead> --platform <platform> --require-all` and the architecture checks it runs first; `AGENTS.md` describes both. The first functional milestone is `infra-m1.3`: a real Excel workbook opened, calculated, compared provenance-first, edited, saved and reopened in Excel without repair.

## Legacy sources

The previous implementation (OxFunc, OxFml, OxCalc, OxDoc, DnaTreeCalc and related repositories under the `DnaCalc` organization) is the port source for this one. Those repositories are read-only inputs; `infra-m0.4` pins the exact commits and files each bead ports from.
