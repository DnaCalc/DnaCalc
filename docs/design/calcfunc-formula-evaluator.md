# Formula evaluation with a small workbook

Python, .NET and JavaScript should expose a formula API alongside their worksheet-function
bindings. A caller supplies formula text, a cell address, an immutable workbook snapshot and
evaluation options. One call evaluates one expression. This gives callers Excel formula syntax,
cell references and LAMBDA while preserving the bindings' value representations and binary64
transport. [CalcFunc bindings design](calcfunc-bindings.md)

## CalcFml path

Parse with `parse_formula` and return its UTF-16 diagnostics with the input. Compile with
`compile_with` using the supplied anchor, notation, formula form and reference catalogue. Use
`compile_loaded_with` when the caller explicitly supplies stored Excel formula text. Default to
A1 notation and dynamic-array form. A session may reuse its `PlanCache` and `EvalScratch`. Bind
against the current snapshot's namespace on each compilation, including when the interner
returns an existing plan. Place the plan with `instantiate` and run it with `evaluate`, an
`EvalFrame` and a `FormulaEvaluationContext`. CalcFml supplies entry rules, reference operators,
intersection, argument demand, lifting, and LET and LAMBDA. CalcFunc still owns kernels and
coercion. [CalcFml contract](../../crates/calc-fml/CONTRACT.md)

Return the evaluator's scalar or tagged rectangular array, including worksheet errors. Keep
parse diagnostics, entry rejections, unavailable inputs, typed capability refusals and adapter
errors distinguishable. The initial operation evaluates an expression at an address and returns
its value. Workbook recalculation and worksheet spill publication would use the existing
CalcEngine graph and placement rules in a later workbook session API. [Calculation contract](../architecture/RECALC_CONTRACT.md)

## Workbook snapshot

- One workbook with ordered sheets, stable sheet identities and case-insensitive Excel name
  lookup. The anchor is a valid one-based cell on a known sheet. Use CalcValue's `CellKey`,
  `GridArea` and `ReferenceLike` for reference identity and checked dimensions.
  [CalcFml references](../../crates/calc-fml/src/reference/mod.rs)
- Sparse cells hold typed constants or authored formula text with an optional supplied result.
  A cell absent from the sparse map is blank. A formula with no supplied result has an
  unavailable value and still has formula text and kind. A caller-supplied result is snapshot
  input and carries its provenance. It earns no newly calculated result for the source cell.
- Workbook and sheet-scoped names hold Excel definitions. Use the caller's scope for
  unqualified names and the qualified sheet for a local name. Preserve authored formula
  text, which can define constants, references, and named LAMBDAs. LAMBDA invocation, captures,
  recursion and callable-taking functions use CalcFml's existing paths.
  [CalcFml name binding](../../crates/calc-fml/src/reference/bind.rs)

## Context services

| Service | Snapshot answers |
|---|---|
| `ReferenceCatalog` | Sheet lookup and order, defined names and scope, and the workbook name when supplied. Use the same catalogue at runtime for INDIRECT. Table or external book resolution requires corresponding metadata and an explicit provider. |
| `ReferenceReader` | Values as row-major cell runs with complete area coverage, including blanks. Preserve an unavailable value as such. Supply spill extents and SUBTOTAL/AGGREGATE exclusions when the snapshot records them. |
| `HostFacts` | Caller, sheet names, order and count, and the formula kind and authored text of a cell or area. A known non-formula cell has no formula text. Missing facts remain unavailable. File identity, formatting, view and application facts require a provider for the relevant CELL and INFO requests. |
| Evaluation environment | Numeric mode and target, date system, compatibility version, and locale or formatting profile. Freeze the clock for a call and inject its random stream. Use an explicitly scoped state service where a function needs pass state. |

The adapter supplies these services over the same snapshot throughout evaluation. For an
unavailable service, propagate the typed refusal from the catalogue or evaluator. The first release covers Excel LAMBDA in
formula text and names. Foreign callbacks, RTD, file and network access, and pivot and data-model
queries each require their own supported provider and scope. [Reference reader contract](../../crates/calc-fml/src/reference/read.rs)
[Host facts contract](../../crates/calc-func/src/call.rs)
[Evaluation context](../../crates/calc-fml/src/eval/context.rs)

## Verification during implementation

Drive the binding through CalcFml with an in-memory two-sheet snapshot. Check relative and
qualified references, scoped names, ROW and COLUMN at different anchors, INDIRECT, ISFORMULA
and FORMULATEXT with a supplied and an unavailable cell result, and a named LAMBDA used by
MAP. Include a blank, a worksheet error, a shaped array and an unavailable provider. Assert
the Excel result or typed refusal and check bit-preserving marshalling across each language
boundary. Extend CalcFml's existing reference-backed acceptance cases for shared semantics.
[CalcFml acceptance cases](../../crates/calc-fml/tests/acceptance.rs)
