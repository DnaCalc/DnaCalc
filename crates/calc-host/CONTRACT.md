# calc-host contract

CalcHost owns the authored workbook revision and its engine state. It drives CalcDoc and CalcEngine for
workbook commands, recalculation, projection and save.

- `HostState::open` loads the document and initializes calculation state (`src/state.rs:298`).
  Commands prepare document and graph edits against the same revision before committing.
  Projections carry value provenance (`src/command.rs:343`, `src/lib.rs:11`).
- `HostSession` is the application entry point. It submits `SkinIntent`s, owns undo, redo and the
  calculation lifecycle, and publishes changes to subscribers (`src/session.rs:385`,
  `src/session.rs:918`, `src/lib.rs:33`).
- Session save accepts an Excel save context; document writing happens through the prepared save
  (`src/session.rs:759`, `src/save.rs:157`). Workbook event sources and failure records go through
  the session and its event contract (`src/events.rs`).
- `tests/round_trip.rs` and `tests/events.rs` exercise the host route. Calculation and save ordering
  are specified in `docs/architecture/RECALC_CONTRACT.md`.

Surface: 23070e53e492871b at 4fb74eaf
