# P04 result

Excel 16.0.20527.20088 x64, CalculationVersion 191029, Windows/AMD Ryzen 7 4800H, safe mode.
49 conditions x ten fresh workbooks plus six changed-seed conditions x ten: 550 saved workbooks,
zero probe faults, owned-process-zero cleanup. Full arrays, binary64 bits and saved XML are retained.

Across the 50 draw trials, RAND()-RAND(), each DrawName-DrawName, and two calls to a zero-argument
LAMBDA containing RAND were nonzero. LET(draw,RAND(),draw-draw) and LAMBDA(x,x-x)(RAND()) were zero.
MAP drew per element; capturing draw in LET repeated the same bits across the four MAP elements.
Decision: distinguish occurrence/name/LAMBDA-call/MAP-element paths; reuse an already bound LET value
and a LAMBDA argument. These are observed draws, not an assertion that unequal keys can never coincide.

The initial SizeUdf witnesses seeded a two-cell spill. A first returned size of 2 then required
only one call (zero for a clean nonvolatile F9), so the longer scripts never progressed. The separate
changed-seed lane starts from one cell and arms the exact 2/3 scripts after setup:

| Script, volatile | Command | Calls after arming | Result in ten trials |
|---|---|---|---|
| alternating 2,3 | range, Full, Rebuild (separate clones) | 8 | #SPILL! in all 30 |
| 2,3,3,3 | Full | 3 | three-cell spill |
| 2,2,3 | Full | 2 | two-cell spill; third value never requested |
| alternating, Error2015 on call 3 only | Full | 8 | #SPILL!; the transient error did not end sizing |

A separate 3,2,3 script from seed 2 took four calls and ended at size 3. Stabilization is sensitive
to the held footprint and repeated size, not a fixed number of calls for every formula. Choose new
semantic evaluations/draws during sizing and an eight-evaluation bound for this measured volatile
SEQUENCE sizing path. This does not establish a global eight-wave limit for a dependency graph.

The uninstrumented SEQUENCE(RANDBETWEEN(1,1000)) succeeded at entry in all ten clones; clean F9,
Full and Rebuild each produced #SPILL! in ten of ten, range in nine of ten. The one range success
is retained with its full array. Do not claim that every volatile-size calculation fails.

`raw-main.tsv` is the 490-workbook run; `raw-results.tsv` is the 60-workbook changed-seed run.
`supervisor-raw.zip` includes every run and exact injected sources. Initial seed 1 is an explicit
additional factor, needed to expose resizing; it does not replace or conceal the seed-2 trials.
