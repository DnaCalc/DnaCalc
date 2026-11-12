# Fun

- `circular-reference.html`: Circular Reference, snake on a spreadsheet (2026-11-12). The snake is a growing `=SUM`; food cells hold numbers (0.1 and 0.2 show binary64 rounding), gold cells `=PI()`; error cells end the game with that error, hitting yourself is a circular reference, leaving the sheet is `#REF!`. Single page, open it in a browser. A formulas-only workbook version that DNA Calc calculates with iterative calculation is being built (lane game1).
- `what-does-excel-say.html`: a ten-formula quiz on Excel's surprising answers (-2^2, DATE(1900,2,29), 0.5-0.4-0.1, SUM over cell booleans, MOD(-1,3), LEN(1/3), ROUND(-2.5,0), WEEKDAY(1), FACT(171)). The function answers were checked live with Excel 16.0 build 20527 through the answer service on 2026-11-12: MOD 2, LEN 17, ROUND -3, WEEKDAY 1, FACT(171) #NUM!, DATE 60, SUM of cell booleans 0 and of literal TRUE,TRUE 2; the three operator formulas are Excel's documented behaviour.
- `dna-calc-brochure.html`: a one-page brochure laid out as a sheet, with the 2026-11-12 measurements and a binary64 bit explorer.

Both pages load fonts from Google Fonts and need no other files.
