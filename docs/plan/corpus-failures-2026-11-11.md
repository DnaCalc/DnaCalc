# Corpus failures in 17 handbook entries (2026-11-11)

The handbook refresh of 2026-11-11 (ExcelFunctionsHandbook 7bbed60, pinned to DnaCalc b8738ae6)
moved 17 entries from "matches" to "differs": SUM, SUBTOTAL, AGGREGATE, ROW, COLUMN, XMATCH,
DSUM, DCOUNT, DAVERAGE, DMIN, DMAX, DPRODUCT, REGEXTEST, REGEXEXTRACT, REGEXREPLACE, IMCOT and
IMTAN. This file lists every failing corpus cell behind those entries, groups them by cause, and
says what was fixed and what stays open. Bead: dc-vdma.

## How the cells were found

`cargo xtask corpus --oracle` on dna-koderbot (Linux, x86_64) at 4a3f5896 (b8738ae6 plus one
tracker commit): 62 workbooks, 78,264 formula cells, pass 76,769, fail 1,495 (mismatched 627,
refused 672, failed or missing 1), the same totals as the handbook pin. Each workbook's
`triage.json`, `evaluations.json` and its capture were read with the handbook's own labelling
(`ExcelFunctionsHandbook/tools/efh-calcfunc/tally.py`, `formula_label`): a failing cell counts
for a function when the function is the formula's label or is called anywhere in the formula.

All 35 failing cells are in one workbook, `crates/calc-func/tests/fixtures/jnt/oracle.xlsx`, the
dc-jnt migration of legacy unit-test and w111 readings (added 2026-11-09). No other workbook has
a failing cell that calls one of the 17 functions. In every cell the failing function is the
formula's only call, so no failure comes from another function or from a referenced cell.

| Cause | Cells | Functions | State |
|---|---:|---|---|
| A. The jnt capture holds an answer to a different formula | 21 | SUM, SUBTOTAL, AGGREGATE, the six D-functions, ROW, COLUMN, XMATCH | open: needs a live read-back of the sheet, listed below |
| B. REGEX pattern engine: `^`, `$` and an unclosed class | 4 | REGEXTEST, REGEXEXTRACT, REGEXREPLACE | fixed |
| C. IMTAN/IMCOT kernels off the real axis | 10 | IMTAN, IMCOT | open: research, Excel questions listed below |

**Platform.** None of the 35 cells depends on the platform. Causes A and B are formula-entry and
text rules. The complex kernels of cause C use the software CRT functions (`crt_libm.rs`) and
the x87 primitives, which run as the same instructions on x86_64 Linux and Windows; their
failures are a different operation graph from Excel's, not a Linux leaf.

## A. The jnt capture holds an answer to a different formula (21 cells, open)

The jnt rows written by hand from OxFunc unit tests (`tools/excel-service/jnt_hand.py`) were
answered by the question service, not read back from `oracle.xlsx`.
`jnt_import.py hand_rows` writes the sheet row with literals (a number argument as `9.0`, an
array argument as `{1,2;3,4}`, an omitted argument as nothing), but the service builds its own
formula (`service.py witness_request`): every number is a cell (`client.number` has no literal
mode), an array is `CHOOSE({1,2;3,4},RC[1],...)` over cells, an empty argument is a blank cell,
and the formula sits in column A of the service's sheet with its inputs to its right. The
capture then records that answer as Excel's value of the sheet's cell. For these 21 cells
Excel's own measurements contradict the recorded answer:

| Cell | Formula | Capture (service answer) | DNA Calc | Kind | What the service asked; Excel's measured rule |
|---|---|---|---|---|---|
| jnt_aggregate_arrays!D8 | `=SUM()` | 0 | rejected at entry (too few arguments) | entry rejected | `SUM(RC[1])` of a blank cell; dc-mx3x measured `=SUM()` refused at entry |
| jnt_aggregate_arrays!D61 | `=SUBTOTAL(9.0,1.0,2.0,3.0,4.0)` | 10 | rejected at entry (argument 2 requires a reference) | entry rejected | five cell references; dc-mx3x: `=SUBTOTAL(9,2)` refused |
| jnt_aggregate_arrays!D62 | `=SUBTOTAL(1.0,10.0,20.0,30.0)` | 20 | rejected at entry (argument 2) | entry rejected | as D61 |
| jnt_aggregate_arrays!D63 | `=SUBTOTAL(2.0,1.0,2.0,3.0)` | 3 | rejected at entry (argument 2) | entry rejected | as D61 |
| jnt_aggregate_arrays!D64 | `=SUBTOTAL(4.0,10.0,50.0,20.0)` | 50 | rejected at entry (argument 2) | entry rejected | as D61 |
| jnt_aggregate_arrays!D65 | `=SUBTOTAL(5.0,10.0,50.0,20.0)` | 10 | rejected at entry (argument 2) | entry rejected | as D61 |
| jnt_aggregate_arrays!D66 | `=SUBTOTAL(6.0,2.0,3.0,4.0)` | 24 | rejected at entry (argument 2) | entry rejected | as D61 |
| jnt_aggregate_arrays!D67 | `=AGGREGATE(9.0,4.0,1.0,2.0,3.0,4.0)` | 10 | rejected at entry (argument 5 requires a reference) | entry rejected | cell references; dc-mx3x: `=AGGREGATE(9,0,B1,B1,2)` refused |
| jnt_aggregate_arrays!D68 | `=AGGREGATE(1.0,4.0,10.0,20.0,30.0)` | 20 | rejected at entry (argument 5) | entry rejected | as D67 |
| jnt_aggregate_arrays!D69 | `=AGGREGATE(4.0,4.0,10.0,50.0,20.0)` | 50 | rejected at entry (argument 5) | entry rejected | as D67 |
| jnt_aggregate_arrays!D70 | `=AGGREGATE(5.0,4.0,10.0,50.0,20.0)` | 10 | rejected at entry (argument 5) | entry rejected | as D67 |
| jnt_aggregate_arrays!D71 | `=AGGREGATE(6.0,4.0,2.0,3.0,4.0)` | 24 | rejected at entry (argument 5) | entry rejected | as D67 |
| jnt_aggregate_arrays!D111 | `=DSUM({1,2;3,4},1.0,{1})` | #SPILL! | rejected at entry (argument 1 requires a reference) | entry rejected | `CHOOSE({1,2;3,4},cells)`, an array of references, in a sheet without room to spill; dc-mx3x: `=DSUM({1,2},1,B1:B3)` refused |
| jnt_aggregate_arrays!D112 | `=DCOUNT({1,2;3,4},1.0,{1})` | #SPILL! | rejected at entry (argument 1) | entry rejected | as D111; dc-mx3x: `=DCOUNT({1,2},1,B1:B3)` refused |
| jnt_aggregate_arrays!D113 | `=DAVERAGE({1,2;3,4},1.0,{1})` | #SPILL! | rejected at entry (argument 1) | entry rejected | as D111 |
| jnt_aggregate_arrays!D114 | `=DMIN({1,2;3,4},1.0,{1})` | #SPILL! | rejected at entry (argument 1) | entry rejected | as D111 |
| jnt_aggregate_arrays!D115 | `=DMAX({1,2;3,4},1.0,{1})` | #SPILL! | rejected at entry (argument 1) | entry rejected | as D111 |
| jnt_aggregate_arrays!D116 | `=DPRODUCT({1,2;3,4},1.0,{1})` | #SPILL! | rejected at entry (argument 1) | entry rejected | as D111 |
| jnt_evaluator_callables!D57 | `=ROW(A57)` | 1 | 57 | mismatched | `ROW(RC[1])` in the service's first row: the service's position, not the sheet's |
| jnt_evaluator_callables!D58 | `=COLUMN(A58)` | 2 | 1 | mismatched | `COLUMN(RC[1])`: the input cell is in the service's column B |
| jnt_lookup!D4 | `=XMATCH(0.3,{0.30000000000000004},0.0)` | #N/A | 1 | mismatched | the lookup value was a cell holding 0x3FF3333333333334 exactly; in the sheet the literal keeps 15 significant digits and reads as 0.3 (dc-uar, 384 of 384 literals) |

The entry measurements are `crates/calc-fml/tests/fixtures/entry/excel-entry.tsv` (dc-mx3x,
Excel 16.0.20527, 2026-11-07, 3,822 formulas through `Range.Formula2` and `Range.Formula`),
replayed by CalcFml's `entry_matches_excel`. DNA Calc follows them here; changing the engine to
fit the jnt answers would contradict those measurements, so no engine change was made.

**Fix (open, needs Excel).** The capture of these rows has to come from the sheet itself. On the
Windows machine, a live read-back of the three sheets (`jnt_import.py capture ... --live <read-back>`,
as the operator rows were answered) records what Excel does with each formula as written: the
18 entry rows should come back as entry refusals (scored as agreed rejections), ROW and COLUMN as
57 and 1, and the XMATCH row as Excel's answer for the 15-digit literal. The formulas to read
back are exactly the 21 in the table, in `oracle.xlsx` as committed. In the importer, a hand row
whose asked formula differs from the written row (a number literal asked as a cell, an array
asked as `CHOOSE` over cells, an empty argument asked as a blank cell, or a function whose
result depends on the formula's position: ROW, COLUMN, ADDRESS, CELL, INDIRECT, OFFSET, ISREF,
AREAS) should take its answer from the live read-back, not the service. The same transport
difference sits under rows that pass today and so are not yet evidence of what they claim: 117
hand rows have an array argument, 49 call a position- or reference-sensitive function and 50
have a blank-cell argument.

## B. REGEX pattern engine (4 cells, fixed)

| Cell | Formula | Excel | DNA Calc before | Kind |
|---|---|---|---|---|
| jnt_text_formats!D5 | `=REGEXTEST("","^$")` | TRUE | refused (`^`, `$` not read) | refused: NotImplemented, the pattern argument |
| jnt_text_formats!D7 | `=REGEXTEST("hello","[")` | #VALUE! | refused | refused: NotImplemented, the pattern argument |
| jnt_text_formats!D11 | `=REGEXEXTRACT("abc","[")` | #VALUE! | refused | refused: NotImplemented, the pattern argument |
| jnt_text_formats!D15 | `=REGEXREPLACE("abc","[","x")` | #VALUE! | refused | refused: NotImplemented, the pattern argument |

These rows were asked with the same formula text the sheet holds (text arguments are literals in
both), so their answers are Excel's. The pattern engine (`crates/calc-func/src/ported/text_more.rs`)
refused `^` and `$` and every unclosed class. Now:

- `^` is the start of the text (PCRE2 without multiline);
- `$` is the end of the text; before a final newline PCRE2 also matches, and which characters end
  a line depends on how Excel's PCRE2 was built, so a pattern with `$` over a text holding a line
  break (LF, CR, VT, FF, NEL, LS, PS) stays refused;
- a bracketed class the pattern never closes (`[`, `a[bc`, `[^`) is a pattern PCRE2 does not
  compile, and the call is #VALUE!;
- POSIX forms inside a class (`[[:alpha:]]`) are now refused; before, they were read as literal
  characters, a wrong answer rather than a refusal.

Corpus after the fix: pass 76,773, fail 1,491 (mismatched 627, refused 668), +4 cells, no cell
lost. Full bank: REGEXTEST 18 of 18 before and after (the totals of both runs are in the commit).

## C. IMTAN and IMCOT off the real axis (10 cells, open)

| Cell | Formula | Excel (build 20430) | DNA Calc |
|---|---|---|---|
| jnt_parity_complex!D389 | `=IMTAN("3.141592653589793+1e-100i")` | -3.2311393144413E-15+1E-100i | -3.23108914886517E-15+1E-100i |
| jnt_parity_complex!D391 | `=IMTAN("134217727+1e-100i")` | 3.25845642561188+1.16175382776114E-99i | 3.25845642563678+1.16175382777736E-99i |
| jnt_parity_complex!D402 | `=IMCOT("0-0.1i")` | 10.0333111322541i | 10.033311132254i |
| jnt_parity_complex!D403 | `=IMCOT("0.1-0.1i")` | 4.96671119559759+5.03337769295277i | 4.96671119559754+5.03337769295272i |
| jnt_parity_complex!D404 | `=IMCOT("-0.1-0.1i")` | -4.96671119559759+5.03337769295277i | -4.96671119559754+5.03337769295272i |
| jnt_parity_complex!D407 | `=IMCOT("1.5707963267948966-0.1i")` | 6.65581526134877E-15+0.0996679946249558i | 6.65579042772583E-15+0.0996679946249558i |
| jnt_parity_complex!D408 | `=IMCOT("-1.5707963267948966-0.1i")` | -6.65581526134877E-15+0.0996679946249558i | -6.65579042772583E-15+0.0996679946249558i |
| jnt_parity_complex!D409 | `=IMCOT("3.141592653589793-0.1i")` | -3.22039035684448E-13+10.0333111322541i | -3.22034035815299E-13+10.033311132254i |
| jnt_parity_complex!D411 | `=IMCOT("134217727-0.1i")` | 0.303561173457951+0.108953168171161i | 0.303561173455636+0.108953168171019i |
| jnt_parity_complex!D415 | `=IMCOT("6.339-0.095i")` | 4.57890469107014+7.856857549072i | 4.57890469107008+7.85685754907189i |

All ten are mismatches in the kernel. The jnt rows are 20 of each function's 200 legacy readings
(`OxFunc/docs/function-lane/evidence/w111-broad-20260929/complex-kernels/trig-excel.jsonl`,
Excel 16.0.20430). The real-axis rows (the full bank's 1,209 per function) are exact; off the
real axis the legacy work already left this open: its best candidates reached 173/200 (IMTAN)
and 154/200 (IMCOT) and were rejected as parity models (that directory's README). The production
kernels (`ParsedComplex::tan`, `ParsedComplex::cot` in `crates/calc-func/src/ported/complex_family.rs`)
compute `sin 2x / (cos 2x + cosh 2y)` from the CRT `sin`, `cos`, `sinh`, `cosh` and match 149/200
and 131/200 of those readings.

What the readings show about the shape (read off single rows, not fitted):

- **The circular factor is Excel's SIN chain, not the CRT's.** Near multiples of pi the real
  part is the reduction error of `FPREM1` with the 64-bit ROM pi: `IMTAN("3.141592653589793+1e-100i")`
  is -3.2311393144413E-15; a numerator built on `excel_sin` (`excel_sin(2x)`, or
  `2·excel_sin(x)·excel_cos(x)`) gives exactly that for the 15-digit coefficient 3.14159265358979,
  while the CRT `sin(2x)` gives -3.23108914886517E-15. The same holds at 134217727, where the CRT
  reduction is exact and Excel's is not.
- **The hyperbolic factor of tiny `y` is exact.** `IMTAN("0+1e-10i")` is `0.0000000001i`, so
  Excel does not form `sinh` as `(exp(y) - 1/exp(y))/2` here (that gives 1.00000008274037E-10),
  unlike IMSIN and IMCOS.
- **IMCOT's denominator cancels.** `IMCOT("0-0.1i")` is 10.0333111322541i where `coth 0.1` rounds
  to 10.0333111322540: the denominator is formed by a subtraction near 2, as in
  `(cosh²y + sinh²y) - 1`, not as `2 sinh²y`.
- **Large `y` gives exactly ±i.** `IMTAN("1e100+710i")` is `i` and `IMCOT("1e100+710i")` is `-i`,
  while `IMTAN("1e100+1i")` is #NUM!: a branch on `y` comes before the circular-argument check.
  Every `+710i` reading is ±i; DNA Calc gives #NUM! (cosh overflows). The readings have nothing
  between 100i and 710i, so the threshold is unknown.

A search over double-angle and product forms built from those parts (Excel SIN/COS of `x`, CRT
sinh/cosh of `y`) reaches 182/188 (IMTAN) and 177/188 (IMCOT) of the readings without the 710i
rows, with different `cos 2x` forms for the two functions and last-digit misses left. That is a
selection by match rate on the discovery set, not a measured operation graph, so it was not
landed. To settle the graph, Excel answers are needed for:

- the large-`y` branch: `IMTAN` and `IMCOT` of `"0+354i"`, `"0+355i"`, `"0+700i"`, `"0+709i"`,
  `"0+709.78i"`, `"0+709.79i"`, `"0+710i"`, `"1e100+355i"`, `"1e100+709.79i"`, and the same with
  `-` before the imaginary part;
- the imaginary axis, which isolates the hyperbolic part (`tan iy = i tanh y`): `IMTAN` and `IMCOT`
  of `"0+1e-8i"`, `"0+0.001i"`, `"0+0.01i"`, `"0+0.05i"`, `"0+0.1i"`, `"0+0.2i"`, `"0+0.5i"`,
  `"0+1i"`, `"0+2i"`, `"0+5i"`, `"0+10i"`, `"0+20i"`;
- a tiny imaginary part, which isolates the circular part (`cos 2x` against `2cos²x - 1`,
  `cos²x - sin²x` and `1 - 2sin²x`): `IMTAN` and `IMCOT` of `"<x>+1e-100i"` for `x` in 0.05, 0.1,
  0.2, 0.3, 0.5, 0.7, 1, 1.2, 2, 3, 5, 10;
- then a fresh held-out set of at least 400 random rows per function (the legacy generator
  `smart-fuzzer/tools/w111/gen_complex_trig_holdout.py` with a new seed) to test the graph the
  three probe sets point to.
