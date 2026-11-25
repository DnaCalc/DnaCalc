# calc-doc contract

CalcDoc reads and writes Excel packages through caller-supplied byte sources and sinks. It exposes the
authored workbook as CalcDoc model and keeps package metadata for round trips.

- `open` checks the package under an `OpenPolicy`. `load` produces a `LoadedWorkbook`, with
  file caches held separately from authored inputs (`src/lib.rs:166`, `src/load/mod.rs:2041`,
  `src/lib.rs:22`). Binary inputs use the codec's conversion routes.
- `prepare_edit` validates a batch and returns a prepared authored revision. `prepare_save`
  makes a plan from a document snapshot and `write` consumes that plan with a `PackageWrite`
  (`src/edit.rs:169`, `src/save/mod.rs:939`, `src/save/mod.rs:1134`).
- The save plan copies untouched parts and rewrites affected ones. Binary preservation save can
  copy the original .xlsb under its format policy. Authored .xlsb output is tracked in dc-snz.14
  (`src/lib.rs:17`, `src/lib.rs:50`). The loaded per-formula always-calculate behavior is under
  verification in dc-kapl.2.1.
- The acceptance, binary conversion and authoring tests exercise the codec (`tests/acceptance.rs`,
  `tests/dc_4j0e_xls.rs`, `tests/dc_5sr_xlsb.rs`, `tests/d1_authoring.rs`).
- `write_vba_project` authors a deterministic source-only VBA project from Windows-1252 module
  bodies. `source_vba_package` embeds it in a fresh .xlsm and binds its document modules by code
  name. The writer encodes source and metadata without executing code (`src/vba_write.rs`).

Surface: e6c5c73f8c5ed012 at 4fb74eaf
