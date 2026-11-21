# calc-value contract

CalcValue supplies the shared worksheet values, identities and coordinates used by CalcFunc and CalcDoc model.

- `CalcValue` combines a `CoreValue` with an optional callable. Core values distinguish empty and missing,
  logicals, text, numbers, worksheet errors, arrays and references. Shared payloads are immutable
  (`src/value.rs:584`, `src/lib.rs:9`).
- `IdAllocator` issues generational identities. `CellKey` and `GridArea` use one-based Excel coordinates
  and check the worksheet bounds. A grid area stores its bounds and sheet (`src/id.rs`, `src/grid.rs:90`).
- Callers supply a `NumericMode`. The default is Fast (`src/numeric.rs:62`). Workbook and parameter names
  compare with the folding rules in `src/name_case.rs`.
- Consumers own evaluation, references to workbook contents, and value provenance. The crate's unit tests
  exercise values, coordinates, identities and name comparisons.

Surface: d5c55beafa7e5c79 at 4fb74eaf
