# Foundation Excel evidence

These are historical observations. The original finding pages identify setup,
capture paths, runner version and Excel executable hash. The twelve curated
findings used Excel `16.0.19725.20126` and the Foundation `excel-probe` runner.
Capture dates were 28 February 2026 for EMP-0001 through EMP-0010 and 2 March
for EMP-0011 and EMP-0012. Status is the original registry's judgment on
that case and build. [Original index][index] · [Capture tool][probe]

## Curated observations

| Finding and scenario | Recorded observation | Original status |
| --- | --- | --- |
| [EMP-0001][emp01], SCN-EB030-AMBIG-DOUBLE-COMMA | Excel stored and evaluated `=SUM(A1,,B1)` with the missing argument preserved. | Provisional |
| [EMP-0002][emp02], SCN-EB030-DOTFIELD-PROBE | Excel admitted `=A1.Price` and reported a field-related error at evaluation. | Provisional |
| [EMP-0003][emp03], SCN-EB025-AGG-COERCION | SUM, AVERAGE and COUNT over a mixed text/numeric range had rows that disagreed with the seed expectations. Use the linked rows CW1-022, CW1-024 and CW1-025 to reconstruct the inputs. | Provisional |
| [EMP-0004][emp04], SCN-EB033-CF-TABLE-SPILL | The captured spill targets were white (16777215) where the seed expected yellow (65535). | Provisional |
| [EMP-0005][emp05], SCN-EB034-STRUCTREF-SPILL | The captured table-growth case using `=SEQUENCE(ROWS([Val]),1,1,1)` retained 1 in TBW1-002 where the seed expected 3. | Provisional |
| [EMP-0006][emp06], SCN-EB015 series | A local RTD server produced topic updates. A missing ProgID yielded `#N/A`. The five cases included reopen and calculation-mode transitions. | Confirmed |
| [EMP-0007][emp07], SCN-EB016 series | Cross-workbook formula copies remained formulas. Copying a serial as a value retained the numeric serial while the date display changed with the destination date system. | Confirmed |
| [EMP-0008][emp08], SCN-EB012 series | RAND changed on recalculation. SUM stayed stable after an unrelated edit and recalc. `INFO("recalc")` followed the calculation-mode changes. | Confirmed |
| [EMP-0009][emp09], SCN-EB040-SUMIF-UNRELATED-EDIT | The control sequence produced mixed results against the proposed single reason code. | Provisional |
| [EMP-0010][emp10], ECS-EB-018/019 | Mixed-type dynamic-array and LAMBDA-helper differences recurred on targeted replay. See T45W1-002, 003, 012 and 013 for the individual cases. | Provisional |
| [EMP-0011][emp11], NFCP1-LINK series | With the supporting workbook open, the fixture returned 77 at both link-update settings tested. The closed and missing cases returned `#REF!`. The finding leaves build and channel variation open. | Provisional |
| [EMP-0012][emp12], NFCP1-MERGE-UNMERGE-DIRECT | An explicit merge set the captured merge state and reported area `$A$2:$B$2`. Unmerge cleared that state. | Confirmed |

For coercion, formatting, or spill counterexamples, read the original case row
with its scenario and raw capture before deriving a rule. In particular a
difference from a seeded label establishes which case needs checking. Formula
input, range input, output shape, visible format, and the precise action order can
all change what a probe observes.

## Tiny numbers

The April [Foundation cutoff note][cutoff] records separate observations for
entry and evaluation. Excel rewrote literals through `2.2250738585072099E-308`
to zero in the tested entry route, and first preserved
`2.2250738585072100E-308` as `2.22507385850721E-308`. Dividing that admitted
literal by two gave zero, including when followed by multiplication by
`1E307`. The note also records `POWER(2,-1023)` as zero and
`POWER(2,-1022)` as the minimum normal binary64 number. Keep the formulas and
scope of these observations when referring to the note. Some raw paths cited
there are local scratch paths and the note alone supplies no universal rule.

The November [DnaCalc change][computed] records a computed exception on Excel
build 20527. Its evaluator test checks
`=POWER(2,-1022)*(1+POWER(2,-52))-POWER(2,-1022)` against binary64 bits
`0x0000000000000001`. The dc-1943 tracker records the underlying capture at
`r2:dnacalc-evidence/research/round02/dc-1943-20261108T104726Z`. The November
change retained earlier fractional-power observations. On 2026-11-22, dc-zuwqx.4
repeated the April divide and `POWER` cases and the subtraction in one owned
Excel 16.0.20527.20196 session on an AMD Ryzen 7 4800H. Excel flushed the
subnormal quotients and `POWER(2,-1023)` and preserved the least subnormal
from subtraction. [Ten input formulas, formula readbacks and Value2 words](../../research/numeric-cutoff-and-exactness-2026-04-27.md#2-post-evaluation-zero-flush-behavior-b-and-november-reconciliation)

The April note's cancellation examples likewise depend on where the expression
sits. The report records zero for ungrouped `=0.1+0.2-0.3` and a nonzero
binary64 residue for `=(0.1+0.2-0.3)`. It also supplies controls with a
condition, a comparison, a scale factor and meaningful small results. Its
examples preserve the distinction between cancellation behavior and magnitude
underflow. [April observations][cutoff]

## Preserved ERFC regime clue

Foundation's [exactness bit-reproduction policy][policy] (superseded by
[BRIEFING.md](../../architecture/BRIEFING.md) §4.4) preserved one positive-tail clue
for `ERFC` / `ERFC.PRECISE`: routing through `libm::erfc` matched Excel at `x = 1`
(`FTC-0573`) and `x = 1.25`, while diverging at `x = 1.5, 1.75, 2, 2.25, 2.5, 3` for both
`ERFC` and `ERFC.PRECISE`.

[index]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings_index.md
[probe]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/tools/excel-probe/README.md
[emp01]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0001-formula-double-comma-accepted.md
[emp02]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0002-dotfield-accepted-field-error.md
[emp03]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0003-aggregate-range-text-coercion-counter-signal.md
[emp04]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0004-cf-spill-target-color-counter-signal.md
[emp05]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0005-table-structref-spill-growth-counter-signal.md
[emp06]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0006-rtd-lifecycle-baseline.md
[emp07]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0007-date-system-cross-workbook-baseline.md
[emp08]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0008-volatility-context-controls.md
[emp09]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0009-sumif-mixed-reason-code-signal.md
[emp10]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0010-dynamic-array-mixed-type-counter-signals.md
[emp11]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0011-external-reference-open-state-policy.md
[emp12]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0012-direct-merge-unmerge-capture.md
[cutoff]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/test-corpus/workspace/monitoring/EXCEL_NUMERIC_CUTOFF_AND_EXACTNESS_THEORY_NOTE.md
[policy]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/test-corpus/workspace/monitoring/EXACTNESS_EXCEL_BIT_REPRODUCTION_POLICY.md
[computed]: https://github.com/DnaCalc/DnaCalc/commit/e042619e35426ffac3d8cfaaa2dd2961abe3d02c
