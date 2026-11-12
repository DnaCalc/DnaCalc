# Function-bank coverage, 2026-11-12

Full bank totals: before 3,895,336 matched / 15,459 failed out of 3,910,795 (99.60% pass); after 4,293,791 matched / 75,323 failed out of 4,369,114 (98.28% pass).

Before: the previously committed full-bank status at the start of this lane; it is not a fresh baseline judgment. After: the completed `cargo xtask witness judge --bank full` at DnaCalc `92130e40` and CalcFunc-Evidence `d53a1faa`. Master advanced during the run to `2fd43a49`, including changes to FV, PV and RATE; the table remains the measured earlier snapshot. Rows are distinct selected bank identities, including unread failures; matched counts only Excel matches. A dash means there was no status row. Worst ULP covers finite numeric mismatches only; structural mismatches and refusals remain failures.

The seeded generator uses catalogue arities and parameter names, boundary and domain-biased values, text, booleans, blanks and errors; array spills are sampled at [1,1]. After the supplemental capture below, all five scalar kinds occur in captures for 177 functions, though not at every argument position. Numeric array input is available only for the service-supported functions. Input readbacks that differed, unread results and unmapped error codes were excluded from the bank. Full matrix-reference contexts are not reconstructed by the current witness route. For the original snapshot captures, the service client returned one representative environment per request and did not retain per-row worker environment IDs. The supplemental capture requires one consistent recorded environment for each request and across its spill batches.

The service refuses many reference, host-dependent, volatile and other functions. They remain below target rather than being credited with invented readings. Nullary functions have only one distinct argument identity in this bank format, so repeated calls cannot reach 1,000 distinct rows. RTD, GETPIVOTDATA, CUBEMEMBER, CUBEVALUE, CUBESET, CUBESETCOUNT, CUBERANKEDMEMBER, CUBEMEMBERPROPERTY and CUBEKPIMEMBER require workbook capture, outside this lane. Remote service functions, PY and HAS are deferred.

Functions below 1,000 after judgment (105): AGGREGATE, AREAS, ARRAYTOTEXT, AVERAGEIF, AVERAGEIFS, BYCOL, BYROW, CALL, CELL, CHOOSE, CHOOSECOLS, CHOOSEROWS, COLUMN, COLUMNS, COUNTIF, COUNTIFS, CUBEKPIMEMBER, CUBEMEMBER, CUBEMEMBERPROPERTY, CUBERANKEDMEMBER, CUBESET, CUBESETCOUNT, CUBEVALUE, DAVERAGE, DCOUNT, DCOUNTA, DGET, DMAX, DMIN, DPRODUCT, DROP, DSTDEV, DSTDEVP, DSUM, DVAR, DVARP, ENCODEURL, EXPAND, FALSE, FIELDVALUE, FILTER, FILTERXML, FORMULATEXT, GETPIVOTDATA, GROUPBY, HLOOKUP, HSTACK, HYPERLINK, IF, IFERROR, IFNA, IMPORTCSV, IMPORTTEXT, INDEX, INDIRECT, INFO, ISFORMULA, ISOMITTED, JIS, LOOKUP, MAKEARRAY, MAP, MATCH, MAXIFS, MINIFS, NA, NOW, OFFSET, PERCENTOF, PHONETIC, PI, PIVOTBY, RAND, RANDARRAY, RANDBETWEEN, REDUCE, REGEXEXTRACT, REGEXREPLACE, REGEXTEST, REGISTER.ID, ROW, ROWS, RTD, SCAN, SHEET, SHEETS, SORTBY, SUBTOTAL, SUMIF, SUMIFS, TAKE, TEXTAFTER, TEXTBEFORE, TOCOL, TODAY, TOROW, TRIMRANGE, TRUE, VLOOKUP, VSTACK, WRAPCOLS, WRAPROWS, XLOOKUP, XMATCH, _EUROTOOL.EUROCONVERT.

| Function | Rows before | Matched before | Worst ULP before | Rows after | Matched after | Failed after | Pass rate after | Worst ULP after |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| ABS | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ACCRINT | 146688 | 146688 | - | 146688 | 146688 | 0 | 100.00% | - |
| ACCRINTM | 3612 | 3612 | - | 3612 | 3612 | 0 | 100.00% | - |
| ACOS | 13962 | 13961 | 1 | 13962 | 13961 | 1 | 99.99% | 1 |
| ACOSH | 5730 | 5730 | - | 5730 | 5730 | 0 | 100.00% | - |
| ACOT | 16924 | 16924 | - | 16924 | 16924 | 0 | 100.00% | - |
| ACOTH | 269996 | 269994 | 1 | 269996 | 269994 | 2 | 100.00% | 1 |
| ADDRESS | 1284 | 1284 | - | 1284 | 1284 | 0 | 100.00% | - |
| AGGREGATE | — | — | — | — | — | — | — | — |
| AMORDEGRC | — | — | — | 2957 | 2308 | 649 | 78.05% | - |
| AMORLINC | — | — | — | 2958 | 2355 | 603 | 79.61% | 4660922149119197186 |
| AND | 1210 | 1210 | - | 1210 | 1210 | 0 | 100.00% | - |
| ARABIC | — | — | — | 2115 | 2115 | 0 | 100.00% | - |
| AREAS | — | — | — | — | — | — | — | — |
| ARRAYTOTEXT | — | — | — | — | — | — | — | — |
| ASC | — | — | — | 2090 | 0 | 2090 | 0.00% | - |
| ASIN | 21610 | 21609 | 1 | 21610 | 21609 | 1 | 100.00% | 1 |
| ASINH | 4534 | 4534 | - | 4534 | 4534 | 0 | 100.00% | - |
| ATAN | 181409 | 181408 | 1 | 181409 | 181408 | 1 | 100.00% | 1 |
| ATAN2 | — | — | — | 2746 | 2724 | 22 | 99.20% | 2 |
| ATANH | 22008 | 22008 | - | 22008 | 22008 | 0 | 100.00% | - |
| AVEDEV | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| AVERAGE | 1222 | 1222 | - | 1222 | 1222 | 0 | 100.00% | - |
| AVERAGEA | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| AVERAGEIF | — | — | — | — | — | — | — | — |
| AVERAGEIFS | — | — | — | — | — | — | — | — |
| BAHTTEXT | 2592 | 2592 | - | 2592 | 2592 | 0 | 100.00% | - |
| BASE | 3560 | 3560 | - | 3560 | 3560 | 0 | 100.00% | - |
| BESSELI | 1365 | 1365 | - | 1365 | 1365 | 0 | 100.00% | - |
| BESSELJ | 946 | 945 | 1 | 3803 | 3777 | 26 | 99.32% | 111 |
| BESSELK | 1236 | 1236 | - | 1236 | 1236 | 0 | 100.00% | - |
| BESSELY | 46 | 46 | - | 2901 | 2550 | 351 | 87.90% | 64 |
| BETA.DIST | 2822 | 1688 | 76 | 2822 | 1688 | 1134 | 59.82% | 76 |
| BETA.INV | — | — | — | 2952 | 2610 | 342 | 88.41% | 1188129402768354 |
| BETADIST | 294 | 208 | 57 | 3247 | 3103 | 144 | 95.57% | 468382554779545343 |
| BETAINV | 62 | 28 | 12 | 3016 | 2667 | 349 | 88.43% | 5480880746509893632 |
| BIN2DEC | 313 | 313 | - | 1347 | 1347 | 0 | 100.00% | - |
| BIN2HEX | 748 | 748 | - | 2748 | 2738 | 10 | 99.64% | - |
| BIN2OCT | 742 | 742 | - | 2757 | 2750 | 7 | 99.75% | - |
| BINOM.DIST | 70349 | 70326 | 651 | 70349 | 70326 | 23 | 99.97% | 651 |
| BINOM.DIST.RANGE | 1224 | 1224 | - | 1224 | 1224 | 0 | 100.00% | - |
| BINOM.INV | 2308 | 2308 | - | 2308 | 2308 | 0 | 100.00% | - |
| BINOMDIST | — | — | — | 2931 | 2834 | 97 | 96.69% | 2038 |
| BITAND | 5989 | 5989 | - | 5989 | 5989 | 0 | 100.00% | - |
| BITLSHIFT | 8131 | 8131 | - | 8131 | 8131 | 0 | 100.00% | - |
| BITOR | 5989 | 5989 | - | 5989 | 5989 | 0 | 100.00% | - |
| BITRSHIFT | 8128 | 8128 | - | 8128 | 8128 | 0 | 100.00% | - |
| BITXOR | 5989 | 5989 | - | 5989 | 5989 | 0 | 100.00% | - |
| BYCOL | — | — | — | — | — | — | — | — |
| BYROW | — | — | — | — | — | — | — | — |
| CALL | — | — | — | — | — | — | — | — |
| CEILING | 1335 | 1335 | - | 1335 | 1335 | 0 | 100.00% | - |
| CEILING.MATH | 1362 | 1362 | - | 1362 | 1362 | 0 | 100.00% | - |
| CEILING.PRECISE | 1335 | 1335 | - | 1335 | 1335 | 0 | 100.00% | - |
| CELL | — | — | — | — | — | — | — | — |
| CHAR | — | — | — | 1056 | 1035 | 21 | 98.01% | - |
| CHIDIST | 388 | 380 | 6 | 3241 | 3176 | 65 | 97.99% | 167 |
| CHIINV | 100 | 36 | 91 | 2964 | 2278 | 686 | 76.86% | 2466054876282016 |
| CHISQ.DIST | — | — | — | 2915 | 2832 | 83 | 97.15% | 2968641633616463328 |
| CHISQ.DIST.RT | 6 | 6 | - | 2859 | 2801 | 58 | 97.97% | 347 |
| CHISQ.INV | 40 | 14 | 20 | 2903 | 2363 | 540 | 81.40% | 560 |
| CHISQ.INV.RT | — | — | — | 2863 | 2237 | 626 | 78.13% | 2467188620951835 |
| CHISQ.TEST | 5 | 5 | - | 2860 | 2719 | 141 | 95.07% | - |
| CHITEST | — | — | — | 2853 | 2712 | 141 | 95.06% | - |
| CHOOSE | — | — | — | — | — | — | — | — |
| CHOOSECOLS | — | — | — | — | — | — | — | — |
| CHOOSEROWS | — | — | — | — | — | — | — | — |
| CLEAN | — | — | — | 2082 | 2082 | 0 | 100.00% | - |
| CODE | — | — | — | 2109 | 2109 | 0 | 100.00% | - |
| COLUMN | — | — | — | — | — | — | — | — |
| COLUMNS | — | — | — | — | — | — | — | — |
| COMBIN | 25199 | 25199 | - | 25199 | 25199 | 0 | 100.00% | - |
| COMBINA | 40991 | 40991 | - | 40991 | 40991 | 0 | 100.00% | - |
| COMPLEX | 10694 | 10654 | - | 10694 | 10654 | 40 | 99.63% | - |
| CONCAT | 20 | 15 | - | 2905 | 2900 | 5 | 99.83% | - |
| CONCATENATE | 10 | 5 | - | 2897 | 2890 | 7 | 99.76% | - |
| CONFIDENCE | — | — | — | 2910 | 2587 | 323 | 88.90% | 33 |
| CONFIDENCE.NORM | — | — | — | 2916 | 2573 | 343 | 88.24% | 46 |
| CONFIDENCE.T | — | — | — | 2912 | 2614 | 298 | 89.77% | 7697584227984561 |
| CONVERT | 33972 | 33972 | - | 33972 | 33972 | 0 | 100.00% | - |
| CORREL | — | — | — | 2853 | 2780 | 73 | 97.44% | - |
| COS | 3684 | 3651 | 1 | 3684 | 3651 | 33 | 99.10% | 1 |
| COSH | 33550 | 33550 | - | 33550 | 33550 | 0 | 100.00% | - |
| COT | 2130 | 2130 | - | 2130 | 2130 | 0 | 100.00% | - |
| COTH | 40241 | 40238 | 2 | 40241 | 40238 | 3 | 99.99% | 2 |
| COUNT | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| COUNTA | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| COUNTBLANK | — | — | — | 2092 | 2092 | 0 | 100.00% | - |
| COUNTIF | — | — | — | — | — | — | — | — |
| COUNTIFS | — | — | — | — | — | — | — | — |
| COUPDAYBS | 5584 | 5584 | - | 5584 | 5584 | 0 | 100.00% | - |
| COUPDAYS | 5584 | 5584 | - | 5584 | 5584 | 0 | 100.00% | - |
| COUPDAYSNC | 5584 | 5584 | - | 5584 | 5584 | 0 | 100.00% | - |
| COUPNCD | 5584 | 5584 | - | 5584 | 5584 | 0 | 100.00% | - |
| COUPNUM | 5584 | 5584 | - | 5584 | 5584 | 0 | 100.00% | - |
| COUPPCD | 5584 | 5584 | - | 5584 | 5584 | 0 | 100.00% | - |
| COVAR | — | — | — | 2855 | 2782 | 73 | 97.44% | - |
| COVARIANCE.P | — | — | — | 2849 | 2776 | 73 | 97.44% | - |
| COVARIANCE.S | — | — | — | 2850 | 2777 | 73 | 97.44% | - |
| CRITBINOM | 1420 | 1420 | - | 1420 | 1420 | 0 | 100.00% | - |
| CSC | 2136 | 2136 | - | 2136 | 2136 | 0 | 100.00% | - |
| CSCH | 33306 | 33305 | 1 | 33306 | 33305 | 1 | 100.00% | 1 |
| CUBEKPIMEMBER | — | — | — | — | — | — | — | — |
| CUBEMEMBER | — | — | — | — | — | — | — | — |
| CUBEMEMBERPROPERTY | — | — | — | — | — | — | — | — |
| CUBERANKEDMEMBER | — | — | — | — | — | — | — | — |
| CUBESET | — | — | — | — | — | — | — | — |
| CUBESETCOUNT | — | — | — | — | — | — | — | — |
| CUBEVALUE | — | — | — | — | — | — | — | — |
| CUMIPMT | 38 | 11 | 268435456 | 2995 | 2946 | 49 | 98.36% | 268435456 |
| CUMPRINC | 586 | 268 | 4 | 3541 | 3201 | 340 | 90.40% | 4 |
| DATE | 22250 | 22250 | - | 22250 | 22250 | 0 | 100.00% | - |
| DATEDIF | — | — | — | 2914 | 2907 | 7 | 99.76% | - |
| DATEVALUE | — | — | — | 2188 | 2061 | 127 | 94.20% | - |
| DAVERAGE | — | — | — | — | — | — | — | — |
| DAY | 10153 | 10153 | - | 10153 | 10153 | 0 | 100.00% | - |
| DAYS | 9565 | 9565 | - | 9565 | 9565 | 0 | 100.00% | - |
| DAYS360 | 4564 | 4564 | - | 4564 | 4564 | 0 | 100.00% | - |
| DB | 2034 | 2034 | - | 2034 | 2034 | 0 | 100.00% | - |
| DBCS | — | — | — | 2101 | 0 | 2101 | 0.00% | - |
| DCOUNT | — | — | — | — | — | — | — | — |
| DCOUNTA | — | — | — | — | — | — | — | — |
| DDB | 1761 | 1761 | - | 1761 | 1761 | 0 | 100.00% | - |
| DEC2BIN | 4807 | 4807 | - | 4807 | 4807 | 0 | 100.00% | - |
| DEC2HEX | 4807 | 4807 | - | 4807 | 4807 | 0 | 100.00% | - |
| DEC2OCT | 4807 | 4807 | - | 4807 | 4807 | 0 | 100.00% | - |
| DECIMAL | 1230 | 1230 | - | 1230 | 1230 | 0 | 100.00% | - |
| DEGREES | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| DELTA | 1092 | 1092 | - | 1092 | 1092 | 0 | 100.00% | - |
| DEVSQ | 23106 | 23106 | - | 23106 | 23106 | 0 | 100.00% | - |
| DGET | — | — | — | — | — | — | — | — |
| DISC | 2400 | 2400 | - | 2400 | 2400 | 0 | 100.00% | - |
| DMAX | — | — | — | — | — | — | — | — |
| DMIN | — | — | — | — | — | — | — | — |
| DOLLAR | — | — | — | 2022 | 0 | 2022 | 0.00% | - |
| DOLLARDE | 4847 | 4847 | - | 4847 | 4847 | 0 | 100.00% | - |
| DOLLARFR | 3641 | 3641 | - | 3641 | 3641 | 0 | 100.00% | - |
| DPRODUCT | — | — | — | — | — | — | — | — |
| DROP | — | — | — | — | — | — | — | — |
| DSTDEV | — | — | — | — | — | — | — | — |
| DSTDEVP | — | — | — | — | — | — | — | — |
| DSUM | — | — | — | — | — | — | — | — |
| DURATION | 7884 | 7859 | 2 | 7884 | 7859 | 25 | 99.68% | 2 |
| DVAR | — | — | — | — | — | — | — | — |
| DVARP | — | — | — | — | — | — | — | — |
| ECMA.CEILING | 6 | 6 | - | 2773 | 2754 | 19 | 99.31% | - |
| EDATE | 9838 | 9838 | - | 9838 | 9838 | 0 | 100.00% | - |
| EFFECT | 2584 | 2584 | - | 2584 | 2584 | 0 | 100.00% | - |
| ENCODEURL | — | — | — | — | — | — | — | — |
| EOMONTH | 9838 | 9838 | - | 9838 | 9838 | 0 | 100.00% | - |
| ERF | — | — | — | 2479 | 2460 | 19 | 99.23% | - |
| ERF.PRECISE | 618437 | 618437 | - | 618437 | 618437 | 0 | 100.00% | - |
| ERFC | 544 | 544 | - | 2636 | 2624 | 12 | 99.54% | 1 |
| ERFC.PRECISE | 1863 | 1863 | - | 1863 | 1863 | 0 | 100.00% | - |
| ERROR.TYPE | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| EVEN | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| EXACT | — | — | — | 2856 | 2856 | 0 | 100.00% | - |
| EXP | 2456 | 2455 | 1 | 2456 | 2455 | 1 | 99.96% | 1 |
| EXPAND | — | — | — | — | — | — | — | — |
| EXPON.DIST | 49346 | 49345 | 1 | 49346 | 49345 | 1 | 100.00% | 1 |
| EXPONDIST | 9740 | 9740 | - | 9740 | 9740 | 0 | 100.00% | - |
| F.DIST | 369 | 343 | 4 | 3314 | 3203 | 111 | 96.65% | 5036 |
| F.DIST.RT | 6 | 6 | - | 2914 | 2817 | 97 | 96.67% | 745 |
| F.INV | — | — | — | 2917 | 2714 | 203 | 93.04% | 20007 |
| F.INV.RT | — | — | — | 2919 | 2709 | 210 | 92.81% | 2088116889639963193 |
| F.TEST | 78 | 62 | 6 | 2927 | 2838 | 89 | 96.96% | 6 |
| FACT | 2709 | 2709 | - | 2709 | 2709 | 0 | 100.00% | - |
| FACTDOUBLE | 2710 | 2710 | - | 2710 | 2710 | 0 | 100.00% | - |
| FALSE | 1 | 1 | - | 1 | 1 | 0 | 100.00% | - |
| FDIST | 4399 | 3861 | 3 | 4399 | 3861 | 538 | 87.77% | 3 |
| FIELDVALUE | — | — | — | — | — | — | — | — |
| FILTER | — | — | — | — | — | — | — | — |
| FILTERXML | — | — | — | — | — | — | — | — |
| FIND | — | — | — | 2863 | 2844 | 19 | 99.34% | - |
| FINDB | — | — | — | 2861 | 2848 | 13 | 99.55% | - |
| FINV | 32 | 4 | 41 | 2938 | 2673 | 265 | 90.98% | 1235948291200398959 |
| FISHER | 34166 | 34166 | - | 34166 | 34166 | 0 | 100.00% | - |
| FISHERINV | — | — | — | 2087 | 2086 | 1 | 99.95% | - |
| FIXED | 989 | 989 | - | 3415 | 2384 | 1031 | 69.81% | - |
| FLOOR | 1335 | 1335 | - | 1335 | 1335 | 0 | 100.00% | - |
| FLOOR.MATH | 1362 | 1362 | - | 1362 | 1362 | 0 | 100.00% | - |
| FLOOR.PRECISE | 1335 | 1335 | - | 1335 | 1335 | 0 | 100.00% | - |
| FORECAST | 70 | 70 | - | 2984 | 2922 | 62 | 97.92% | - |
| FORECAST.ETS | — | — | — | 2959 | 2217 | 742 | 74.92% | - |
| FORECAST.ETS.CONFINT | — | — | — | 2957 | 2429 | 528 | 82.14% | 5494198 |
| FORECAST.ETS.SEASONALITY | — | — | — | 2958 | 847 | 2111 | 28.63% | - |
| FORECAST.ETS.STAT | — | — | — | 2955 | 0 | 2955 | 0.00% | - |
| FORECAST.LINEAR | — | — | — | 2910 | 2847 | 63 | 97.84% | - |
| FORMULATEXT | — | — | — | — | — | — | — | — |
| FREQUENCY | — | — | — | 1200 | 1144 | 56 | 95.33% | - |
| FTEST | — | — | — | 2849 | 2776 | 73 | 97.44% | - |
| FV | 2561 | 2558 | 100663296 | 5515 | 5384 | 131 | 97.62% | 14931799827530013823 |
| FVSCHEDULE | — | — | — | 2857 | 2636 | 221 | 92.26% | - |
| GAMMA | 12603 | 8193 | 5 | 12603 | 8193 | 4410 | 65.01% | 5 |
| GAMMA.DIST | 77975 | 77897 | 8 | 77975 | 77897 | 78 | 99.90% | 8 |
| GAMMA.INV | 204 | 71 | 55 | 3113 | 2399 | 714 | 77.06% | 1291568952534135155 |
| GAMMADIST | 6 | 6 | - | 2954 | 2816 | 138 | 95.33% | 94 |
| GAMMAINV | — | — | — | 2918 | 2304 | 614 | 78.96% | 1291568952534135155 |
| GAMMALN | 4538 | 2646 | 7 | 4538 | 2646 | 1892 | 58.31% | 7 |
| GAMMALN.PRECISE | 14278 | 11643 | 7 | 14278 | 11643 | 2635 | 81.55% | 7 |
| GAUSS | 9728 | 9728 | - | 9728 | 9728 | 0 | 100.00% | - |
| GCD | 7966 | 7966 | - | 7966 | 7966 | 0 | 100.00% | - |
| GEOMEAN | 1225 | 1225 | - | 1225 | 1225 | 0 | 100.00% | - |
| GESTEP | 1096 | 1096 | - | 1096 | 1096 | 0 | 100.00% | - |
| GETPIVOTDATA | — | — | — | — | — | — | — | — |
| GROUPBY | — | — | — | — | — | — | — | — |
| GROWTH | 1260 | 1254 | 1 | 1260 | 1254 | 6 | 99.52% | 1 |
| HARMEAN | 19214 | 19214 | - | 19214 | 19214 | 0 | 100.00% | - |
| HEX2BIN | 1214 | 1214 | - | 1214 | 1214 | 0 | 100.00% | - |
| HEX2DEC | 1025 | 1025 | - | 1025 | 1025 | 0 | 100.00% | - |
| HEX2OCT | 1218 | 1218 | - | 1218 | 1218 | 0 | 100.00% | - |
| HLOOKUP | — | — | — | — | — | — | — | — |
| HOUR | 14631 | 14631 | - | 14631 | 14631 | 0 | 100.00% | - |
| HSTACK | — | — | — | — | — | — | — | — |
| HYPERLINK | — | — | — | — | — | — | — | — |
| HYPGEOM.DIST | 3661 | 3572 | 22 | 3661 | 3572 | 89 | 97.57% | 22 |
| HYPGEOMDIST | 3 | 2 | 3 | 2954 | 2917 | 37 | 98.75% | 28 |
| IF | — | — | — | — | — | — | — | — |
| IFERROR | — | — | — | — | — | — | — | — |
| IFNA | — | — | — | — | — | — | — | — |
| IFS | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| IMABS | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMAGINARY | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMARGUMENT | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMCONJUGATE | 3245 | 3245 | - | 3245 | 3245 | 0 | 100.00% | - |
| IMCOS | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMCOSH | 1209 | 1202 | - | 1209 | 1202 | 7 | 99.42% | - |
| IMCOT | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMCSC | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMCSCH | 1209 | 1201 | - | 1209 | 1201 | 8 | 99.34% | - |
| IMDIV | 2067 | 2067 | - | 2067 | 2067 | 0 | 100.00% | - |
| IMEXP | — | — | — | 1049 | 1047 | 2 | 99.81% | - |
| IMLN | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMLOG10 | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMLOG2 | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMPORTCSV | — | — | — | — | — | — | — | — |
| IMPORTTEXT | — | — | — | — | — | — | — | — |
| IMPOWER | — | — | — | 2735 | 2348 | 387 | 85.85% | - |
| IMPRODUCT | 3260 | 3260 | - | 3260 | 3260 | 0 | 100.00% | - |
| IMREAL | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMSEC | — | — | — | 1060 | 1059 | 1 | 99.91% | - |
| IMSECH | 1209 | 1205 | - | 1209 | 1205 | 4 | 99.67% | - |
| IMSIN | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| IMSINH | 1209 | 1200 | - | 1209 | 1200 | 9 | 99.26% | - |
| IMSQRT | 2029 | 2029 | - | 2029 | 2029 | 0 | 100.00% | - |
| IMSUB | 1216 | 1216 | - | 1216 | 1216 | 0 | 100.00% | - |
| IMSUM | 3260 | 3260 | - | 3260 | 3260 | 0 | 100.00% | - |
| IMTAN | 1209 | 1209 | - | 1209 | 1209 | 0 | 100.00% | - |
| INDEX | — | — | — | — | — | — | — | — |
| INDIRECT | — | — | — | — | — | — | — | — |
| INFO | — | — | — | — | — | — | — | — |
| INT | 124403 | 124401 | 4 | 124403 | 124401 | 2 | 100.00% | 4 |
| INTERCEPT | 4 | 4 | - | 2856 | 2783 | 73 | 97.44% | - |
| INTRATE | 2400 | 2400 | - | 2400 | 2400 | 0 | 100.00% | - |
| IPMT | 57 | 57 | - | 3015 | 2977 | 38 | 98.74% | 4503599627370496 |
| IRR | 734 | 549 | 1024 | 3211 | 2603 | 608 | 81.07% | 1024 |
| ISBLANK | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ISERR | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ISERROR | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ISEVEN | 57066 | 57066 | - | 57066 | 57066 | 0 | 100.00% | - |
| ISFORMULA | — | — | — | — | — | — | — | — |
| ISLOGICAL | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ISNA | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ISNONTEXT | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ISNUMBER | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ISO.CEILING | 1335 | 1335 | - | 1335 | 1335 | 0 | 100.00% | - |
| ISODD | 57016 | 57016 | - | 57016 | 57016 | 0 | 100.00% | - |
| ISOMITTED | — | — | — | — | — | — | — | — |
| ISOWEEKNUM | 10153 | 10153 | - | 10153 | 10153 | 0 | 100.00% | - |
| ISPMT | — | — | — | 2947 | 1297 | 1650 | 44.01% | 4503599627370495 |
| ISREF | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ISTEXT | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| JIS | 8 | 8 | - | 8 | 8 | 0 | 100.00% | - |
| KURT | 4508 | 4508 | - | 4508 | 4508 | 0 | 100.00% | - |
| LARGE | — | — | — | 2848 | 2738 | 110 | 96.14% | - |
| LCM | 7967 | 7967 | - | 7967 | 7967 | 0 | 100.00% | - |
| LEFT | — | — | — | 2475 | 2464 | 11 | 99.56% | - |
| LEFTB | — | — | — | 2478 | 2467 | 11 | 99.56% | - |
| LEN | — | — | — | 2087 | 2087 | 0 | 100.00% | - |
| LENB | — | — | — | 2083 | 2083 | 0 | 100.00% | - |
| LINEST | — | — | — | 2960 | 2929 | 31 | 98.95% | 1 |
| LN | 1685 | 1672 | 1 | 1685 | 1672 | 13 | 99.23% | 1 |
| LOG | 25233 | 25233 | - | 25233 | 25233 | 0 | 100.00% | - |
| LOG10 | 2823 | 2823 | - | 2823 | 2823 | 0 | 100.00% | - |
| LOGEST | 360 | 360 | - | 3320 | 3288 | 32 | 99.04% | 1 |
| LOGINV | — | — | — | 2916 | 2617 | 299 | 89.75% | 6562 |
| LOGNORM.DIST | 7809 | 7809 | - | 7809 | 7809 | 0 | 100.00% | - |
| LOGNORM.INV | — | — | — | 2911 | 2614 | 297 | 89.80% | 6978 |
| LOGNORMDIST | 3299 | 3299 | - | 3299 | 3299 | 0 | 100.00% | - |
| LOOKUP | — | — | — | — | — | — | — | — |
| LOWER | 12 | 12 | - | 2112 | 2112 | 0 | 100.00% | - |
| MAKEARRAY | — | — | — | — | — | — | — | — |
| MAP | — | — | — | — | — | — | — | — |
| MATCH | 13 | 13 | - | 13 | 13 | 0 | 100.00% | - |
| MAX | 1211 | 1211 | - | 1211 | 1211 | 0 | 100.00% | - |
| MAXA | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| MAXIFS | — | — | — | — | — | — | — | — |
| MDETERM | — | — | — | 2071 | 1920 | 151 | 92.71% | 4425840583318231482 |
| MDURATION | 1260 | 1238 | 2 | 1260 | 1238 | 22 | 98.25% | 2 |
| MEDIAN | 43412 | 43412 | - | 43412 | 43412 | 0 | 100.00% | - |
| MID | — | — | — | 2910 | 2898 | 12 | 99.59% | - |
| MIDB | — | — | — | 2913 | 2904 | 9 | 99.69% | - |
| MIN | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| MINA | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| MINIFS | — | — | — | — | — | — | — | — |
| MINUTE | 14631 | 14631 | - | 14631 | 14631 | 0 | 100.00% | - |
| MINVERSE | 2243 | 2131 | - | 2243 | 2131 | 112 | 95.01% | - |
| MIRR | — | — | — | 2911 | 2907 | 4 | 99.86% | - |
| MMULT | — | — | — | 1200 | 1200 | 0 | 100.00% | - |
| MOD | 152432 | 152432 | - | 152432 | 152432 | 0 | 100.00% | - |
| MODE | — | — | — | 2799 | 2799 | 0 | 100.00% | - |
| MODE.MULT | — | — | — | 1200 | 1200 | 0 | 100.00% | - |
| MODE.SNGL | — | — | — | 2792 | 2792 | 0 | 100.00% | - |
| MONTH | 10153 | 10153 | - | 10153 | 10153 | 0 | 100.00% | - |
| MROUND | 50782 | 50782 | - | 50782 | 50782 | 0 | 100.00% | - |
| MULTINOMIAL | — | — | — | 2804 | 2511 | 293 | 89.55% | 4672 |
| MUNIT | — | — | — | 1200 | 1200 | 0 | 100.00% | - |
| N | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| NA | 1 | 1 | - | 1 | 1 | 0 | 100.00% | - |
| NEGBINOM.DIST | 510 | 497 | 183 | 3448 | 3277 | 171 | 95.04% | 1955 |
| NEGBINOMDIST | 1804 | 1801 | 2 | 1804 | 1801 | 3 | 99.83% | 2 |
| NETWORKDAYS | 3600 | 3600 | - | 3600 | 3600 | 0 | 100.00% | - |
| NETWORKDAYS.INTL | 3600 | 3600 | - | 3600 | 3600 | 0 | 100.00% | - |
| NOMINAL | 2085 | 2085 | - | 2085 | 2085 | 0 | 100.00% | - |
| NORM.DIST | 24246 | 24246 | - | 24246 | 24246 | 0 | 100.00% | - |
| NORM.INV | — | — | — | 2915 | 2475 | 440 | 84.91% | 1033 |
| NORM.S.DIST | 27497 | 27491 | 2 | 27497 | 27491 | 6 | 99.98% | 2 |
| NORM.S.INV | — | — | — | 2206 | 1072 | 1134 | 48.59% | 75 |
| NORMDIST | 4510 | 4510 | - | 4510 | 4510 | 0 | 100.00% | - |
| NORMINV | — | — | — | 2914 | 2429 | 485 | 83.36% | 960 |
| NORMSDIST | — | — | — | 2121 | 2120 | 1 | 99.95% | - |
| NORMSINV | — | — | — | 2210 | 1110 | 1100 | 50.23% | 581 |
| NOT | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| NOW | — | — | — | — | — | — | — | — |
| NPER | 2509 | 2509 | - | 2509 | 2509 | 0 | 100.00% | - |
| NPV | 284 | 284 | - | 3224 | 3165 | 59 | 98.17% | - |
| NUMBERVALUE | — | — | — | 2715 | 691 | 2024 | 25.45% | 12206042906098239226 |
| OCT2BIN | 1201 | 1201 | - | 1201 | 1201 | 0 | 100.00% | - |
| OCT2DEC | 998 | 998 | - | 2060 | 2060 | 0 | 100.00% | - |
| OCT2HEX | 1198 | 1198 | - | 1198 | 1198 | 0 | 100.00% | - |
| ODD | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| ODDFPRICE | — | — | — | 2956 | 2027 | 929 | 68.57% | - |
| ODDFYIELD | — | — | — | 2952 | 2941 | 11 | 99.63% | - |
| ODDLPRICE | — | — | — | 2952 | 2057 | 895 | 69.68% | - |
| ODDLYIELD | — | — | — | 2957 | 2078 | 879 | 70.27% | - |
| OFFSET | — | — | — | — | — | — | — | — |
| OR | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| PDURATION | 1205 | 1205 | - | 1205 | 1205 | 0 | 100.00% | - |
| PEARSON | — | — | — | 2854 | 2781 | 73 | 97.44% | - |
| PERCENTILE | — | — | — | 2851 | 2841 | 10 | 99.65% | - |
| PERCENTILE.EXC | — | — | — | 2849 | 2841 | 8 | 99.72% | - |
| PERCENTILE.INC | — | — | — | 2855 | 2848 | 7 | 99.75% | - |
| PERCENTOF | — | — | — | — | — | — | — | — |
| PERCENTRANK | — | — | — | 2860 | 2075 | 785 | 72.55% | - |
| PERCENTRANK.EXC | — | — | — | 2864 | 2079 | 785 | 72.59% | - |
| PERCENTRANK.INC | — | — | — | 2865 | 2080 | 785 | 72.60% | - |
| PERMUT | 3432 | 3432 | - | 3432 | 3432 | 0 | 100.00% | - |
| PERMUTATIONA | 71915 | 71915 | - | 71915 | 71915 | 0 | 100.00% | - |
| PHI | 14406 | 14406 | - | 14406 | 14406 | 0 | 100.00% | - |
| PHONETIC | — | — | — | — | — | — | — | — |
| PI | 1 | 1 | - | 1 | 1 | 0 | 100.00% | - |
| PIVOTBY | — | — | — | — | — | — | — | — |
| PMT | 291978 | 291977 | 1 | 291978 | 291977 | 1 | 100.00% | 1 |
| POISSON | — | — | — | 1560 | 1004 | 556 | 64.36% | 42 |
| POISSON.DIST | 49714 | 49162 | 7 | 49714 | 49162 | 552 | 98.89% | 7 |
| POWER | 62635 | 62635 | - | 62635 | 62635 | 0 | 100.00% | - |
| PPMT | 33 | 33 | - | 2987 | 2947 | 40 | 98.66% | 55198092900073692 |
| PRICE | 9153 | 9103 | 1 | 9153 | 9103 | 50 | 99.45% | 1 |
| PRICEDISC | 2400 | 2400 | - | 2400 | 2400 | 0 | 100.00% | - |
| PRICEMAT | 3608 | 3608 | - | 3608 | 3608 | 0 | 100.00% | - |
| PROB | — | — | — | 2919 | 2431 | 488 | 83.28% | - |
| PRODUCT | 1211 | 1211 | - | 1211 | 1211 | 0 | 100.00% | - |
| PROPER | 7 | 7 | - | 2092 | 2091 | 1 | 99.95% | - |
| PV | 1308 | 1306 | 3 | 1308 | 1306 | 2 | 99.85% | 3 |
| QUARTILE | — | — | — | 2856 | 2625 | 231 | 91.91% | - |
| QUARTILE.EXC | — | — | — | 2854 | 2842 | 12 | 99.58% | - |
| QUARTILE.INC | — | — | — | 2851 | 2631 | 220 | 92.28% | - |
| QUOTIENT | 1362 | 1362 | - | 1362 | 1362 | 0 | 100.00% | - |
| RADIANS | 32925 | 32925 | - | 32925 | 32925 | 0 | 100.00% | - |
| RAND | — | — | — | — | — | — | — | — |
| RANDARRAY | — | — | — | — | — | — | — | — |
| RANDBETWEEN | — | — | — | — | — | — | — | — |
| RANK | — | — | — | 2810 | 2671 | 139 | 95.05% | - |
| RANK.AVG | — | — | — | 2812 | 2670 | 142 | 94.95% | - |
| RANK.EQ | — | — | — | 2811 | 2675 | 136 | 95.16% | - |
| RATE | 377 | 376 | 1333068467558 | 3327 | 3039 | 288 | 91.34% | 65348950468097106 |
| RECEIVED | 2400 | 2400 | - | 2400 | 2400 | 0 | 100.00% | - |
| REDUCE | — | — | — | — | — | — | — | — |
| REGEXEXTRACT | — | — | — | — | — | — | — | — |
| REGEXREPLACE | — | — | — | — | — | — | — | — |
| REGEXTEST | 18 | 18 | - | 18 | 18 | 0 | 100.00% | - |
| REGISTER.ID | — | — | — | — | — | — | — | — |
| REPLACE | — | — | — | 2951 | 2563 | 388 | 86.85% | - |
| REPLACEB | — | — | — | 2951 | 2574 | 377 | 87.22% | - |
| REPT | — | — | — | 2859 | 2650 | 209 | 92.69% | - |
| RIGHT | — | — | — | 2486 | 2475 | 11 | 99.56% | - |
| RIGHTB | — | — | — | 2481 | 2463 | 18 | 99.27% | - |
| ROMAN | — | — | — | 2041 | 2023 | 18 | 99.12% | - |
| ROUND | 36945 | 36921 | 14 | 36945 | 36921 | 24 | 99.94% | 14 |
| ROUNDDOWN | 59995 | 59893 | 10 | 59995 | 59893 | 102 | 99.83% | 10 |
| ROUNDUP | 52841 | 52739 | 10 | 52841 | 52739 | 102 | 99.81% | 10 |
| ROW | — | — | — | — | — | — | — | — |
| ROWS | — | — | — | — | — | — | — | — |
| RRI | 5528 | 5528 | - | 5528 | 5528 | 0 | 100.00% | - |
| RSQ | — | — | — | 2852 | 2779 | 73 | 97.44% | - |
| RTD | — | — | — | — | — | — | — | — |
| SCAN | — | — | — | — | — | — | — | — |
| SEARCH | — | — | — | 2857 | 2848 | 9 | 99.68% | - |
| SEARCHB | — | — | — | 2861 | 2850 | 11 | 99.62% | - |
| SEC | 2138 | 2138 | - | 2138 | 2138 | 0 | 100.00% | - |
| SECH | 30424 | 30423 | 1 | 30424 | 30423 | 1 | 100.00% | 1 |
| SECOND | 14631 | 14631 | - | 14631 | 14631 | 0 | 100.00% | - |
| SEQUENCE | — | — | — | 1307 | 731 | 576 | 55.93% | - |
| SERIESSUM | — | — | — | 2952 | 1988 | 964 | 67.34% | 1185697296742875537 |
| SHEET | — | — | — | — | — | — | — | — |
| SHEETS | — | — | — | — | — | — | — | — |
| SIGN | 1226 | 1226 | - | 1226 | 1226 | 0 | 100.00% | - |
| SIN | 2129 | 2129 | - | 2129 | 2129 | 0 | 100.00% | - |
| SINH | 33550 | 33547 | 1 | 33550 | 33547 | 3 | 99.99% | 1 |
| SKEW | 4508 | 4508 | - | 4508 | 4508 | 0 | 100.00% | - |
| SKEW.P | — | — | — | 2800 | 2467 | 333 | 88.11% | - |
| SLN | 1205 | 1205 | - | 1205 | 1205 | 0 | 100.00% | - |
| SLOPE | 4 | 4 | - | 2860 | 2787 | 73 | 97.45% | - |
| SMALL | — | — | — | 2857 | 2744 | 113 | 96.04% | - |
| SORT | — | — | — | 1200 | 1200 | 0 | 100.00% | - |
| SORTBY | — | — | — | — | — | — | — | — |
| SQRT | 42803 | 42803 | - | 42803 | 42803 | 0 | 100.00% | - |
| SQRTPI | 1224 | 1224 | - | 1224 | 1224 | 0 | 100.00% | - |
| STANDARDIZE | 10310 | 10310 | - | 10310 | 10310 | 0 | 100.00% | - |
| STDEV | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| STDEV.P | 1221 | 1221 | - | 1221 | 1221 | 0 | 100.00% | - |
| STDEV.S | 1222 | 1222 | - | 1222 | 1222 | 0 | 100.00% | - |
| STDEVA | 1210 | 1210 | - | 1210 | 1210 | 0 | 100.00% | - |
| STDEVP | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| STDEVPA | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| STEYX | — | — | — | 2858 | 2785 | 73 | 97.45% | - |
| SUBSTITUTE | — | — | — | 2921 | 2817 | 104 | 96.44% | - |
| SUBTOTAL | — | — | — | — | — | — | — | — |
| SUM | 1211 | 1211 | - | 1211 | 1211 | 0 | 100.00% | - |
| SUMIF | — | — | — | — | — | — | — | — |
| SUMIFS | — | — | — | — | — | — | — | — |
| SUMPRODUCT | — | — | — | 2887 | 2826 | 61 | 97.89% | - |
| SUMSQ | 1211 | 1211 | - | 1211 | 1211 | 0 | 100.00% | - |
| SUMX2MY2 | — | — | — | 2850 | 2759 | 91 | 96.81% | 64 |
| SUMX2PY2 | — | — | — | 2853 | 2769 | 84 | 97.06% | 1 |
| SUMXMY2 | — | — | — | 2856 | 2773 | 83 | 97.09% | 1 |
| SWITCH | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| SYD | 1207 | 1207 | - | 1207 | 1207 | 0 | 100.00% | - |
| T | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| T.DIST | — | — | — | 2909 | 2485 | 424 | 85.42% | 385819484 |
| T.DIST.2T | — | — | — | 2855 | 2513 | 342 | 88.02% | 17378036 |
| T.DIST.RT | 6 | 4 | 5 | 2863 | 2077 | 786 | 72.55% | 576139024 |
| T.INV | — | — | — | 2857 | 2316 | 541 | 81.06% | 30138960552455 |
| T.INV.2T | — | — | — | 2857 | 2683 | 174 | 93.91% | 1674322846226204 |
| T.TEST | — | — | — | 2950 | 2529 | 421 | 85.73% | - |
| TAKE | — | — | — | — | — | — | — | — |
| TAN | 2114 | 2114 | - | 2114 | 2114 | 0 | 100.00% | - |
| TANH | 39028 | 39025 | 1 | 39028 | 39025 | 3 | 99.99% | 1 |
| TBILLEQ | 3600 | 3600 | - | 3600 | 3600 | 0 | 100.00% | - |
| TBILLPRICE | 3600 | 3600 | - | 3600 | 3600 | 0 | 100.00% | - |
| TBILLYIELD | 2400 | 2400 | - | 2400 | 2400 | 0 | 100.00% | - |
| TDIST | 92 | 56 | 14 | 3003 | 2904 | 99 | 96.70% | 19164 |
| TEXT | 7 | 7 | - | 2861 | 7 | 2854 | 0.24% | - |
| TEXTAFTER | 11 | 11 | - | 11 | 11 | 0 | 100.00% | - |
| TEXTBEFORE | 11 | 11 | - | 11 | 11 | 0 | 100.00% | - |
| TEXTJOIN | — | — | — | 2949 | 2856 | 93 | 96.85% | - |
| TEXTSPLIT | 11 | 9 | - | 1211 | 1209 | 2 | 99.83% | - |
| TIME | 1296 | 1296 | - | 1296 | 1296 | 0 | 100.00% | - |
| TIMEVALUE | — | — | — | 2114 | 1991 | 123 | 94.18% | - |
| TINV | 32 | 22 | 17 | 2889 | 2704 | 185 | 93.60% | 1672442127014299 |
| TOCOL | — | — | — | — | — | — | — | — |
| TODAY | — | — | — | — | — | — | — | — |
| TOROW | — | — | — | — | — | — | — | — |
| TRANSPOSE | — | — | — | 1200 | 176 | 1024 | 14.67% | - |
| TREND | 12 | 12 | - | 1212 | 12 | 1200 | 0.99% | - |
| TRIM | — | — | — | 2092 | 2092 | 0 | 100.00% | - |
| TRIMMEAN | — | — | — | 2855 | 2843 | 12 | 99.58% | - |
| TRIMRANGE | — | — | — | — | — | — | — | — |
| TRUE | 1 | 1 | - | 1 | 1 | 0 | 100.00% | - |
| TRUNC | 51317 | 51217 | 10 | 51317 | 51217 | 100 | 99.81% | 10 |
| TTEST | — | — | — | 2951 | 2548 | 403 | 86.34% | - |
| TYPE | 1227 | 1227 | - | 1227 | 1227 | 0 | 100.00% | - |
| UNICHAR | — | — | — | 1068 | 1067 | 1 | 99.91% | - |
| UNICODE | — | — | — | 2087 | 2087 | 0 | 100.00% | - |
| UNIQUE | — | — | — | 1200 | 176 | 1024 | 14.67% | - |
| UPPER | 5 | 5 | - | 2114 | 2113 | 1 | 99.95% | - |
| VALUE | — | — | — | 2101 | 2095 | 6 | 99.71% | - |
| VALUETOTEXT | — | — | — | 2475 | 2378 | 97 | 96.08% | - |
| VAR | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| VAR.P | 1222 | 1222 | - | 1222 | 1222 | 0 | 100.00% | - |
| VAR.S | 1323 | 1323 | - | 1323 | 1323 | 0 | 100.00% | - |
| VARA | 1211 | 1211 | - | 1211 | 1211 | 0 | 100.00% | - |
| VARP | 1210 | 1210 | - | 1210 | 1210 | 0 | 100.00% | - |
| VARPA | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| VDB | 2201 | 2201 | - | 2201 | 2201 | 0 | 100.00% | - |
| VLOOKUP | — | — | — | — | — | — | — | — |
| VSTACK | — | — | — | — | — | — | — | — |
| WEEKDAY | 19523 | 19523 | - | 19523 | 19523 | 0 | 100.00% | - |
| WEEKNUM | 13332 | 11382 | - | 13332 | 11382 | 1950 | 85.37% | - |
| WEIBULL | — | — | — | 2952 | 2869 | 83 | 97.19% | 1 |
| WEIBULL.DIST | 31483 | 31480 | 3 | 31483 | 31480 | 3 | 99.99% | 3 |
| WORKDAY | 3600 | 3600 | - | 3600 | 3600 | 0 | 100.00% | - |
| WORKDAY.INTL | 6000 | 6000 | - | 6000 | 6000 | 0 | 100.00% | - |
| WRAPCOLS | — | — | — | — | — | — | — | — |
| WRAPROWS | — | — | — | — | — | — | — | — |
| XIRR | — | — | — | 2861 | 698 | 2163 | 24.40% | - |
| XLOOKUP | — | — | — | — | — | — | — | — |
| XMATCH | 13 | 13 | - | 13 | 13 | 0 | 100.00% | - |
| XNPV | 3406 | 3406 | - | 3406 | 3406 | 0 | 100.00% | - |
| XOR | 1212 | 1212 | - | 1212 | 1212 | 0 | 100.00% | - |
| YEAR | 10153 | 10153 | - | 10153 | 10153 | 0 | 100.00% | - |
| YEARFRAC | 4506 | 4506 | - | 4506 | 4506 | 0 | 100.00% | - |
| YIELD | 533 | 97 | 2844 | 3489 | 2961 | 528 | 84.87% | 2844 |
| YIELDDISC | — | — | — | 2948 | 1173 | 1775 | 39.79% | 839864727122 |
| YIELDMAT | 1085 | 1085 | - | 1085 | 1085 | 0 | 100.00% | - |
| Z.TEST | — | — | — | 2857 | 1386 | 1471 | 48.51% | - |
| ZTEST | — | — | — | 2864 | 1387 | 1477 | 48.43% | - |
| _EUROTOOL.EUROCONVERT | — | — | — | — | — | — | — | — |

## Supplemental array-route type variants

After the full-bank snapshot, CalcFunc-Evidence `82d8f3cf` added 3,780 accepted observations in 12 files, including numbers, text, booleans, blanks and errors. A read-only full-bank recheck of each affected function at DnaCalc `c3046e50` gives the following counts. These are separate from the snapshot totals and the committed full-bank status. The capture rejected 60 SEQUENCE answers with unmapped service error codes.

| Function | Snapshot rows | Snapshot matched | Recheck rows | Recheck matched | Recheck failed | Recheck pass rate | Recheck worst ULP |
|---|---:|---:|---:|---:|---:|---:|---:|
| FREQUENCY | 1200 | 1144 | 1520 | 1378 | 142 | 90.66% | 4611686018427387904 |
| LINEST | 2960 | 2929 | 3225 | 3141 | 84 | 97.40% | 1 |
| LOGEST | 3320 | 3288 | 3585 | 3500 | 85 | 97.63% | 1 |
| MMULT | 1200 | 1200 | 1520 | 1520 | 0 | 100.00% | - |
| MODE.MULT | 1200 | 1200 | 1277 | 1277 | 0 | 100.00% | - |
| MUNIT | 1200 | 1200 | 1277 | 1277 | 0 | 100.00% | - |
| SEQUENCE | 1307 | 731 | 1567 | 918 | 649 | 58.58% | - |
| SORT | 1200 | 1200 | 1464 | 1413 | 51 | 96.52% | - |
| TEXTSPLIT | 1211 | 1209 | 1415 | 1381 | 34 | 97.60% | - |
| TRANSPOSE | 1200 | 176 | 1277 | 253 | 1024 | 19.81% | - |
| TREND | 1212 | 12 | 1532 | 208 | 1324 | 13.58% | - |
| UNIQUE | 1200 | 176 | 1277 | 252 | 1025 | 19.73% | - |

The FREQUENCY recheck newly found a numeric mismatch above 10,000 ULP. The previous full-bank snapshot found 31 functions above that threshold; the follow-up adds FREQUENCY.

## Financial recheck after master advanced

A read-only full-bank recheck of each affected function at `2fd43a49` found FV 5,386 matched / 129 failed of 5,515 (97.66%; worst 14,931,799,827,530,013,823 ULP), PV 1,308 matched / 0 failed of 1,308 (100%; no numeric mismatch), and RATE 3,040 matched / 287 failed of 3,327 (91.37%; worst 65,348,950,468,097,106 ULP). The FV and RATE examples below remain unchanged on that code. These rechecks do not rewrite the full-bank status or its snapshot totals.

The full run also measured six MULTINOMIAL calls exceeding its 20-second per-row limit; all six count as failures. The witness harness now isolates MULTINOMIAL and IRR calculations in child processes so a timed-out call can be stopped.

## Numeric mismatches above 10,000 ULP

Each example is from the completed full-bank trace, with binary64 values encoded as 16-digit hex. A large ULP distance does not imply a large absolute difference.

- AMORLINC: worst 4660922149119197186 ULP; example `["0x34566056f73e5b9c","0x4076d00000000000","0x40da3a4000000000","0x0000000000000000","0x2b2bff2ee48e0530","0x4059000000000000"]`, Excel `0x34c17b43f128b792`, calc-func `0x34566056f73e5b9c` (30147427615529974 ULP; `readings/excel-20527/bank2-answers-amorlinc-more.json`, `bank2-more-AMORLINC-1284`).
- BETA.INV: worst 1188129402768354 ULP; example `["0x30d5b69a71a3153c","0x54b249ad2594c37d",true,"0x3fe6f61580f05cbc","0x3ff0000000000000"]`, Excel `0x3fefffff685aee73`, calc-func `0x3ff0000000000000` (2544177549 ULP; `readings/excel-20527/bank2-answers-beta.inv-scalar.json`, `bank2-scalar-BETA.INV-0698`).
- BETADIST: worst 468382554779545343 ULP; example `["0x3fed1f1c989c3e23","0x3fea7b217a4b4be4","0x32f18c58756b7294"]`, Excel `0x3987e2020ff21f6d`, calc-func `0x3307da8e5b52306e` (468382554779545343 ULP; `readings/excel-20527/bank2-answers-betadist-more.json`, `bank2-more-BETADIST-0390`).
- BETAINV: worst 5480880746509893632 ULP; example `["0x3f787575e5608a00","0x3fd79d28fc087e54","0x4e6cca15bd86f1d4"]`, Excel `0x30297c093281112d`, calc-func `0x30297c0a07e9be05` (3580407000 ULP; `readings/excel-20527/bank2-answers-betainv-more.json`, `bank2-more-BETAINV-1287`).
- CHIINV: worst 2466054876282016 ULP; example `["0x2dfbb2ab46415c20","0x40e5f90000000000"]`, Excel `0x40e8feaee62576f6`, calc-func `0x40eec30000000001` (1623227488700683 ULP; `readings/excel-20527/bank2-answers-chiinv-scalar.json`, `bank2-scalar-CHIINV-0677`).
- CHISQ.DIST: worst 2968641633616463328 ULP; example `["0x4036ffffffffffff","0x4072f00000000000","0x0000000000000000"]`, Excel `0x2932bbd6552505e0`, calc-func `0x0000000000000000` (2968641633616463328 ULP; `readings/excel-20527/bank2-answers-chisq.dist-more.json`, `bank2-more-CHISQ.DIST-0856`).
- CHISQ.INV.RT: worst 2467188620951835 ULP; example `["0.28843613","0x40e5f90000000000"]`, Excel `0x40e60dddac0b90a0`, calc-func `0x40eec30000000001` (2450958855728993 ULP; `readings/excel-20527/bank2-answers-chisq.inv.rt-more.json`, `bank2-more-CHISQ.INV.RT-0561`).
- CONFIDENCE.T: worst 7697584227984561 ULP; example `["0x2b2bff2ee48e0530","0x4059000000000000","0x4088f66169ab758e"]`, Excel `0x40713e7048acbac8`, calc-func `0x4055e586cb231617` (7697584227984561 ULP; `readings/excel-20527/bank2-answers-confidence.t-scalar.json`, `bank2-scalar-CONFIDENCE.T-0007`).
- CUMIPMT: worst 268435456 ULP; example `["0x3e40b25c74007082","0x4076800000000000","0x3fefcf6de32210e3","0x3ff0000000000000","0x3ff0000000000000","0x0000000000000000"]`, Excel `0xbe409904ac800000`, calc-func `0xbe409904ac7c0000` (262144 ULP; `readings/excel-20527/r15-CUMIPMT.json`, `r15-cumipmt-024`).
- F.INV: worst 20007 ULP; example `["0x3fd39d9602219ed2","0x40e5f90000000000","0x3ff1036008809728"]`, Excel `0x3fee999b82a3dd15`, calc-func `0x3fee999b82a42b3c` (20007 ULP; `readings/excel-20527/bank2-answers-f.inv-scalar.json`, `bank2-scalar-F.INV-1199`).
- F.INV.RT: worst 2088116889639963193 ULP; example `["0x2b2bff2ee48e0530","0x4059000000000000","0x3ff55ab5e22df224"]`, Excel `0x696a7b3f707bca39`, calc-func `0x4c70000000000000` (2088116889639963193 ULP; `readings/excel-20527/bank2-answers-f.inv.rt-scalar.json`, `bank2-scalar-F.INV.RT-0007`).
- FINV: worst 1235948291200398959 ULP; example `["0x31110091f2630590",true,"0x3ff0000000000001"]`, Excel `0x5d96f86c29c0366f`, calc-func `0x4c70000000000000` (1235948291200398959 ULP; `readings/excel-20527/bank2-answers-finv-scalar.json`, `bank2-scalar-FINV-0346`).
- FORECAST.ETS.CONFINT: worst 5494198 ULP; example `["0x40e4736000000000",[["0x3ff76f26ac7f4c03"],["0x40012d17320f6f4f"],["0x400c589bee305e6d"],["0x4012446629d8dedd"],["0x4014422493e5c599"]],[["0x3ff0000000000000"],["0x4000000000000000"],["0x4008000000000000"],["0x4010000000000000"],["0x4014000000000000"]],"0x3cebceed4579c0e8","0x0000000000000000"]`, Excel `0x3d914cb8f0f19109`, calc-func `0x3d914cb8f14566bf` (5494198 ULP; `readings/excel-20527/bank2-answers-forecast.ets.confint-scalar.json`, `bank2-scalar-FORECAST.ETS.CONFINT-1277`).
- FV: worst 14931799827530013823 ULP; example `["0.5","0x3fbd6f203f09d758","0xc000000000000000","0x3fefffffffffffff","0x3fe2ecf03d6c8a25"]`, Excel `0xbfe85d455e8ab768`, calc-func `0xbfeb6b299f2007a4` (859698917363772 ULP; `readings/excel-20527/bank2-answers-fv-more.json`, `bank2-more-FV-1121`).
- GAMMA.INV: worst 1291568952534135155 ULP; example `["0x35a3177e167c9220","0x54b249ad2594c37d","0x40d8f84000000000"]`, Excel `0x559c8c340d5136a6`, calc-func `0x559c8a42dada3d62` (2135445403972 ULP; `readings/excel-20527/bank2-answers-gamma.inv-more.json`, `bank2-more-GAMMA.INV-1278`).
- GAMMAINV: worst 1291568952534135155 ULP; example `["0x3f74fa025863b400","0x336376f7d8c9c952","0x4051800000000001"]`, Excel `0x0000000000000000`, calc-func `0x09acc51ecd73a2d8` (697148778422182616 ULP; `readings/excel-20527/bank2-answers-gammainv-more.json`, `bank2-more-GAMMAINV-0883`).
- IPMT: worst 4503599627370496 ULP; example `["0x3fc889469331c448","0x3ff0000000000001","0x3ffe0aaa02d9dae8","0x3ff6e284d04692e4","0xc000000000000000","0x3fefffffffffffff"]`, Excel `0xbfcd72fb4eb3bc0c`, calc-func `0xbfd18c0fdd974d41` (1153475994161461 ULP; `readings/excel-20527/bank2-answers-ipmt-more.json`, `bank2-more-IPMT-0614`).
- ISPMT: worst 4503599627370495 ULP; example `["0x3fbaa54ce14ec218",true,"0x3fefffffffffffff","0xc033ffffffffffff"]`, Excel `0xbcb0a7500cd1394f`, calc-func `0xbcc0a7500cd1394e` (4503599627370495 ULP; `readings/excel-20527/bank2-answers-ispmt-scalar.json`, `bank2-scalar-ISPMT-0490`).
- MDETERM: worst 4425840583318231482 ULP; example `["0x2b2bff2ee48e0530"]`, Excel `0x2b2bff2ee48e0530`, calc-func `0x0000000000000000` (3110860544497550640 ULP; `readings/excel-20527/bank2-answers-mdeterm-scalar.json`, `bank2-scalar-MDETERM-0007`).
- NUMBERVALUE: worst 12206042906098239226 ULP; example `["0x4028000000000001","0xbfe0000000000000","1,234"]`, Excel `0x4000000000000000`, calc-func `0x4028000000000000` (11258999068426240 ULP; `readings/excel-20527/bank2-answers-numbervalue-more.json`, `bank2-more-NUMBERVALUE-1217`).
- PPMT: worst 55198092900073692 ULP; example `["0x3fd10e6863c6eb6e","0x4020000000000001","0x4048000000000001","0xbfe0000000000000","0x4024000000000000","0x2fd5e3051ab8b8b2"]`, Excel `0xbf2045d506628cb6`, calc-func `0xbf249c0b6d86e374` (1220691565500094 ULP; `readings/excel-20527/bank2-answers-ppmt-scalar.json`, `bank2-scalar-PPMT-0623`).
- RATE: worst 65348950468097106 ULP; example `["0x3f98a27184ec4c00","0x3fe8531e450ea63f",true,"0xc000000000000000","0x3fefffffffffffff"]`, Excel `0x419f8c2a5819362a`, calc-func `0x4287b6b08b31aa7c` (65348950468097106 ULP; `readings/excel-20527/bank2-answers-rate-more.json`, `bank2-more-RATE-0418`).
- SERIESSUM: worst 1185697296742875537 ULP; example `["0xb9826701b9d17698","0x4024000000000000","0x3fc6298f3c66f5e8","0xd0b42fd0823737e0"]`, Excel `0x0000000000000000`, calc-func `0x90747168c7800191` (1185697296742875537 ULP; `readings/excel-20527/bank2-answers-seriessum-scalar.json`, `bank2-scalar-SERIESSUM-1079`).
- T.DIST: worst 385819484 ULP; example `["0x3e73429284a7dfc2","0x3ff0000000000000","0xab2bff2ee48e0530"]`, Excel `0x3fe000000c366566`, calc-func `0x3fe000000c42e65c` (819446 ULP; `readings/excel-20527/bank2-answers-t.dist-scalar.json`, `bank2-scalar-T.DIST-0168`).
- T.DIST.2T: worst 17378036 ULP; example `["0x3e2a0864b02e3308",true]`, Excel `0x3ff0000000000000`, calc-func `0x3feffffffef6d50c` (17378036 ULP; `readings/excel-20527/bank2-answers-t.dist.2t-scalar.json`, `bank2-scalar-T.DIST.2T-0586`).
- T.DIST.RT: worst 576139024 ULP; example `["0x3d8e5ca74e06ada0","0x40aa5c0000000000"]`, Excel `0x3fe0000000000000`, calc-func `0x3fdfffffffff9f1b` (24805 ULP; `readings/excel-20527/bank2-answers-t.dist.rt-more.json`, `bank2-more-T.DIST.RT-1205`).
- T.INV: worst 30138960552455 ULP; example `["0.41304068","0x40e5f90000000000"]`, Excel `0xbfcc20286bc9741e`, calc-func `0xbfcc20286bca7ef4` (68310 ULP; `readings/excel-20527/bank2-answers-t.inv-more.json`, `bank2-more-T.INV-0561`).
- T.INV.2T: worst 1674322846226204 ULP; example `["0.5","0x40d8f84000000000"]`, Excel `0x3fe5957fa6abd7bc`, calc-func `0x3fe5957fa6ac6bd0` (37908 ULP; `readings/excel-20527/bank2-answers-t.inv.2t-scalar.json`, `bank2-scalar-T.INV.2T-0241`).
- TDIST: worst 19164 ULP; example `["0x3ed68a156a94720c",true,"0x3ff0000000000000"]`, Excel `0x3fdffff8d353938a`, calc-func `0x3fdffff8d35348ae` (19164 ULP; `readings/excel-20527/bank2-answers-tdist-more.json`, `bank2-more-TDIST-0826`).
- TINV: worst 1672442127014299 ULP; example `["0.5","0x41a8a3919a84e546"]`, Excel `0x3fe5956b7ecabc2c`, calc-func `0x3fe5956b910e8af2` (306433734 ULP; `readings/excel-20527/bank2-answers-tinv-more.json`, `bank2-more-TINV-0193`).
- YIELDDISC: worst 839864727122 ULP; example `["0x4024000000000000","0x40bcc40000000000","0x4014000000000001","0x3fe0000000000000","0x3ff0000000000001"]`, Excel `0xbfa6e2bfb2fa6b98`, calc-func `0xbfa6e3833ed121ea` (839864727122 ULP; `readings/excel-20527/bank2-answers-yielddisc-scalar.json`, `bank2-scalar-YIELDDISC-0013`).

- FREQUENCY (supplemental recheck): worst 4611686018427387904 ULP; example `["-1.25", [["0x400bae129a9932a2", "0x401b4e795267fc03"], ["0x40190577d46f1e82", "0x400bf8eb99f90fcd"]]]`, Excel `0x0000000000000000`, calc-func `0x3ff0000000000000` (4607182418800017408 ULP; `readings/excel-20527/bank2-answers-frequency-variants.json`, `bank2-variants-FREQUENCY-0181`, result cell `[1,1]`).
