# calc-doc-model contract

CalcDoc model owns the authored workbook representation independently of the file codec and evaluator.

- `ModelRoot` is one authored revision. Sheet entries carry a session ID and a document-stable key.
  Cells are held in shared pages. `ModelRoot::apply` checks the batch and constructs the next root,
  sharing unchanged pages (`src/root.rs:251`, `src/root.rs:716`, `src/lib.rs:5`).
- `CellInput` represents authored input. File-cached values carry separate source provenance
  (`src/input.rs:233`, `src/lib.rs:13`). Definitions and layout live beside the cell pages.
- `OpBatch` names its base authored revision. `encode_batch` and `decode_batch` use the workbook
  root to translate sheet identities through a versioned operation format
  (`src/wire/mod.rs:360`, `src/wire/mod.rs:469`).
- The model's unit tests cover edits and wire operations. CalcDoc and CalcEngine both consume this model.

Surface: 47ffa8893da9fa79 at 4fb74eaf
