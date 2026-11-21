# What is DNA Calc?
Tags: DNA Calc, Windows, VBA
Status: draft

DNA Calc is an open-source spreadsheet for Excel workbooks. It recalculates formulas in the
browser and has a Windows desktop version. DNA Calc - Formula Explorer uses the same formula engine and gives
you space to inspect one formula and its inputs.

## Open a workbook that uses VBA

In the Windows demo, open a copy of the sample `dna-simple.xlsm` workbook. Inspect the formula in
A1 and recalculate it with the sample's VBA project enabled. The cell calls `SimpleNeg(2.5)` and
the sample returns `-2.5`. Change the input and see the formula recalculate. Keep macros disabled
when opening files whose source you don't trust.

Native XLL add-ins use the Windows desktop add-in host. The browser demo uses workbook formulas,
and the next post follows an Excel-DNA add-in through Formula Explorer on Windows.

[Open DNA Calc - Formula Explorer in the browser](https://onecalc.dnacalc.com/)
