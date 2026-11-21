# calc-func contract

CalcFunc owns worksheet-function semantics. It shares CalcCatalogue declarations and CalcValue types.

- Resolve a call by ID, spelling and notation or add-in name. The result is a `FunctionCallTarget` with
  metadata for argument demand, entry rules and result form (`src/catalogue.rs:446`, `src/catalogue.rs:576`).
- Execute through `invoke`. The caller supplies prepared arguments, evaluation services and scratch.
  Reference reads, workbook facts and other host services come through that context. Worksheet errors are
  values and unavailable evaluation behavior is a typed fault (`src/call.rs:615`).
- The crate has no formula parser or dependency graph. Resumable function entries attach a start kernel
  to the same invocation route (`src/call.rs`, dc-kapl.2.2).
- `tests/witnesses.rs` exercises bit comparisons on pinned Excel results. The live corpus and witness judge
  use the same function entry. Remaining numerical differences are tracked in dc-j9k.

Surface: 237d8f2155b32621 at 4fb74eaf
