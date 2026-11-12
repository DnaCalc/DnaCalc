# SUBTOTAL table visibility (dc-bc1o), 2026-11-12

Codex on dna-koderbot. Existing application loader/engine and the original
live Excel capture, with its unchanged workbook; no Microsoft source.

Before implementation, predict that an empty supported table AutoFilter
marks no filtered rows. A physically hidden row is manual, so SUBTOTAL(9)
includes it and SUBTOTAL(109) excludes it. Unsupported filters remain opaque.

Captured workbook 8dfa997a642c00a1da8c0750c371f204553a98c537ca046028b052f9b424a074,
Regional sales3!B13=SUBTOTAL(9,Table610[Sales]): Excel 9,071,953, engine
8,171,954. The missing 899,999 is B7, the sole row hidden in sheet12.xml.
Table610 (xl/tables/table8.xml) has an empty <autoFilter ref="A3:B11"/>.
B12=SUBTOTAL(109,Table610[Sales]) correctly excludes B7. The capture is
retained evidence, not a new service query or a saved-cache pass.

The loader's visibility function already treats empty sheet filters as
non-filtering but explicitly exempts table filters. Apply the same rule to
both, remove that private distinction, and extend the existing generated
table-load acceptance case with a manual hidden row. Full application corpus
comparison must verify the predicted B13 gain and keep B12 unchanged.

The existing generated table fixture's row 3 now has manual hiding and
asserts RowHidden::Manual after loading. Both targeted save-refusal and
formula-authoring acceptance cases pass. Application replay compares all
352 captured cells of the source workbook: B13 gained, zero lost, B12
(SUBTOTAL 109) stays matched. The separate 517-cell statistical workbook
is unchanged at 504 matched.

Final full bank: unchanged 3,895,336 /3,910,795 matched, 15,459 failed
(4 unread, 2,064 excluded). Streaming comparison verifies all 3,910,791
decoded reading/input/selection/answer/source/environment identities and
zero changed actuals. No pin was refreshed; the status is from this full run.

The light check passes: 853 tests passed, none failed, 3 ignored and
1 not run across 26 harnesses; workspace and wasm checks passed.
Command timings: 80.783 seconds workspace, 41.784 tests, 21.082 wasm,
143.734 total. No assertion or retained Excel value was weakened.

Final oracle corpus: 208,197 ->208,198 /212,841 formula cells, +1 SUBTOTAL
and zero lost across all 212,217 recorded cells (same inventory, 364 cases).
The gained B13 actual equals its baseline live-Excel bits. Failures: 4,643
(3,132 mismatched, 771 refused, 545 failed/missing, 73 external, 14 circular,
108 awaiting capture); the tool's pass rate over all cells is 97.81%. The
known 516-cell timeout remains a failure, not an omitted denominator.

Evidence: dnacalc-evidence/research/round60/20261112-fn2-table-0920/,
32 files, 145,160,194 bytes; checksum copy and one-way check zero differences.
The original 17 statistical roots now have four fixed and 13 remaining;
dc-bc1o remains open. dc-3k5n remains blocked on US-MDY observations.
