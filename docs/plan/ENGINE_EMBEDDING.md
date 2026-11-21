# Engine embedding preparation (dc-qm0h)

Recommend three entry points for the first owner-approved crate release.

| Use | Crate and entry point |
|---|---|
| Open, edit, recalculate, read, save | `calc-host`. Open bytes with `HostSession::open`, submit edits as `SkinIntent`, drive `advance_calculation` and read `project` with its value provenance. Save to a caller-owned byte sink. See the [round-trip example](../../crates/calc-host/src/lib.rs). |
| Parse a formula for an editor | `calc-fml::parse_formula` returns a lossless syntax tree and diagnostics. See the [parsing example](../../crates/calc-fml/src/lib.rs). |
| Invoke a worksheet function from a host | `calc-func` resolves the function in the shared catalogue and calls `invoke` with prepared arguments, evaluation services and scratch. `calc-value` carries Excel values and errors. See the [function contract](../../crates/calc-func/CONTRACT.md). |

The existing host is the proposed workbook facade. It already exposes the types needed for the
round trip and preserves its single owner for edits, calculation and save. Document those entry
points as the initial supported surface. Treat the underlying document and engine contracts as
implementation dependencies when embedding a workbook. Pass the typed waiting, failure and
refusal results through to callers. Opening a workbook can expose file-cached results, whose
provenance remains distinct from the result of recalculation.

The existing native runner covers file-based use. `dnacalc recalc book.xlsx --out checked.xlsx`
calculates and writes a workbook. `dnacalc diff book.xlsx` compares calculated formula cells with
the values cached in the input file. See the [runner reference](../../deploy/linux/README.txt).

## Names and licensing

The public crates.io sparse index returned HTTP 404 on 2026-11-21 for all ten current package
names. This is a point-in-time listing check. Recheck the names at the first release.

| Dependency order | Package names |
|---|---|
| 1 | `calc-value`, `calc-syntax` |
| 2 | `calc-catalogue`, `calc-doc-model` |
| 3 | `calc-func` |
| 4 | `calc-fml`, `calc-object-model` |
| 5 | `calc-doc`, `calc-engine` |
| 6 | `calc-host` |

Use the workspace version for the initial set. Before publication, the owner reviews the
[MIT licence](../../LICENSE), the [licence ledger](../../migration/licence-ledger.json), and the
[third-party review](THIRD_PARTY_REVIEW.md) for source distribution. Include the applicable
[ported notices](../../deploy/notices/ported.txt) and licence texts in the relevant package
archives. Version the local dependencies and test the packaged examples in a clean consumer
workspace after the owner chooses the package names and signs off on the distribution scope.
