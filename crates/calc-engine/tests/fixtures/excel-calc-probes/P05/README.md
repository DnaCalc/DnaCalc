# P05 result

Excel 16.0.20527.20088 x64, CalculationVersion 191029, Windows/AMD Ryzen 7 4800H, safe mode.
22 conditions x ten fresh workbooks plus 22 controls recording HasSpill before SpillingToRange:
Those runs plus thirty existing-child/reader-first controls total 272 artifacts, no probe faults,
and owned-process-zero cleanup. `raw-main.tsv` is the repeated run;
`raw-results.tsv` is the flag control. Both injected sources and raw supervisor records are retained.

After Manual growth from size 1 to 10, C10-only returned 1 in ten of ten; anchor-only left C10 at 1.
Including the anchor and readers, sheet calculation and Full returned 11 in ten of ten each.
Calculating the currently blank A10 alone did not expand the anchor. Shrinking to size 1 while
calculating anchor/readers cleared former children and returned C10=1 in all 60 growth trials.
Decision: preserve qualified `CachedAbsent` for an excluded producer and publish footprint removals
with the new result.

`child-raw-results.tsv` adds ten trials each with an existing ten-cell footprint, pending growth
to 12 or shrink to 1, and A10-only calculation. It kept the old ten-cell footprint and C10=11;
F9 then grew to 12 or cleared the old children and set C10=1. Existing-child calculation therefore
did not pull the dirty anchor in this scope. Reader-first C10,E10,A1 calculation finished at 11
in all ten trials, but the E10 UDF received 1 and then 11 in separate invocations. This is observed
provisional evaluation, not proof that Excel waits before every in-scope reader callback.

Competing A3 and B1 spills had one winner under domain calculation; reader F1 was 16 or 36 according
to the winning footprint. A3-then-B1 entry with MTC off made B1 win in all ten; reversed entry made
A3 win in all ten. With MTC enabled the same nominal conditions sometimes had either winner
(for example A3-then-B1/cold: B1 won nine, A3 one). Rebuild did not impose one universal winner.
History/MTC belong in the observed profile; these results do not identify a unique internal
reservation algorithm or prove that history alone predicts every winner. SliceV1's semantic
reservation order remains an explicit DNA policy rather than a measured Excel ordering rule.

Literals, a formula returning empty text, a table and the grid edge obstructed the measured spill;
a formatted blank did not. Saved array metadata and every cell observation are in saved-caches.json.
The original helper logged error 91 when SpillingToRange was absent, losing its already-read false
flag; the separate control records HasSpill explicitly before attempting that property.

The merged-cell obstruction was omitted because the supplied AGENTS.md says "Never use merged
cells in any Excel workbook, under any circumstances." An explicit exception for this obstruction
fixture would make that factor runnable. No workbook here contains merged cells.
