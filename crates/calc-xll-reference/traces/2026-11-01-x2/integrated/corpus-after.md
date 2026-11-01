# Corpus summary

Pass = matched Excel from the workbook's own inputs. External links = matched Excel from values another workbook's link caches hold, counted apart (neither pass nor fail, dc-ylr). Circular rule = matched Excel with the value a formula on a circular reference, or one reading it, keeps with iteration off, counted apart (neither pass nor fail, dc-y2o.1). Awaiting capture = formula cells of an oracle workbook or section with no committed capture of its current content, counted apart (neither pass nor fail; `cargo xtask capture --pending` takes them). Fail = mismatched + refused (not modelled) + failed or missing. The pass rate is pass over all formula cells, awaiting ones included (owner ruling 2026-10-23).

- cache: C:\Work\DnaCalc\corpus\captures-cache
- command: cargo xtask corpus --oracle --no-capture --out target/x2-corpus-integrated
- compare_process: a release build of xtask in target/xtask-corpus
- excel_build: not read (no capture needed it)
- finished_utc: 2026-11-01T09:35:08Z
- listing_failures: []
- repository: 053481adc2f42d2d6937236d5e127c0b7c00b579+dirty
- wall_time_s: 167.8656563

## Totals

- workbooks: 21 (21 compared, 0 not compared; 0 compared with the whole run failed)
- formula cells (compared workbooks): 20030; pass 18812; external links 73; circular rule 14; awaiting capture 0; fail 1131 (mismatched 1124, refused 6, failed or missing 1)
- pass rate over all formula cells: 93.91%
- failed or missing by verdict: not_run 1
- wall time (s): open 0.091, recalculation 0.932, project 0.012, judge 0.058, compare processes 2.906, captures -

## Workbooks

| Workbook | Formula cells | Pass | External links | Circular rule | Awaiting capture | Fail | Mismatched | Refused | Failed or missing | Pass rate | Open s | Recalc s | Judge s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| calc-engine/dc_10y/oracle.xlsx | 1436 | 1431 | 0 | 0 | 0 | 5 | 5 | 0 | 0 | 99.65% | 0.008 | 0.024 | 0.004 |
| calc-engine/dc_10y_iteration/oracle.xlsx | 17 | 17 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.001 | 0.003 | 0.000 |
| calc-engine/dc_86f/oracle.xlsx | 22 | 22 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.002 | 0.001 | 0.000 |
| calc-engine/dc_y2o/oracle.xlsx | 17 | 3 | 0 | 14 | 0 | 0 | 0 | 0 | 0 | 17.64% | 0.001 | 0.001 | 0.000 |
| calc-engine/l3/oracle.xlsx | 112 | 112 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.002 | 0.002 | 0.000 |
| calc-fml/dc_51ka/oracle.xlsx | 183 | 183 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.002 | 0.028 | 0.000 |
| calc-fml/dc_ibp2/oracle.xlsx | 83 | 83 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.001 | 0.002 | 0.000 |
| calc-fml/dc_l10u/oracle.xlsx | 57 | 57 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.001 | 0.024 | 0.000 |
| calc-func/catalogue/oracle.xlsx | 13509 | 12392 | 0 | 0 | 0 | 1117 | 1111 | 6 | 0 | 91.73% | 0.049 | 0.629 | 0.042 |
| calc-func/cdoc_jpha/oracle.xlsx | 1526 | 1526 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.008 | 0.047 | 0.004 |
| calc-func/cfunc_m1_2/oracle.xlsx | 234 | 234 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.002 | 0.028 | 0.001 |
| calc-func/cfunc_m3_2/oracle.xlsx | 218 | 215 | 0 | 0 | 0 | 3 | 3 | 0 | 0 | 98.62% | 0.001 | 0.003 | 0.000 |
| calc-func/cfunc_m3_4/oracle.xlsx | 112 | 112 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.001 | 0.002 | 0.000 |
| calc-func/cfunc_m3_5/oracle.xlsx | 136 | 136 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.001 | 0.029 | 0.000 |
| calc-func/cfunc_m3_6/oracle.xlsx | 596 | 596 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.003 | 0.037 | 0.001 |
| calc-func/dc_e00t/oracle.xlsx | 1144 | 1144 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.004 | 0.039 | 0.003 |
| calc-func/dc_fwpz/oracle.xlsx | 115 | 115 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.001 | 0.003 | 0.000 |
| calc-func/dc_nzpw/oracle.xlsx | 186 | 185 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 99.46% | 0.001 | 0.002 | 0.001 |
| calc-func/dc_q9rb/oracle.xlsx | 86 | 81 | 0 | 0 | 0 | 5 | 5 | 0 | 0 | 94.18% | 0.001 | 0.001 | 0.000 |
| calc-func/dc_xrxb/oracle.xlsx | 167 | 167 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.00% | 0.001 | 0.026 | 0.000 |
| calc-host/dc_ylr/oracle.xlsx | 74 | 1 | 73 | 0 | 0 | 0 | 0 | 0 | 0 | 1.35% | 0.002 | 0.002 | 0.000 |
| **Total** | 20030 | 18812 | 73 | 14 | 0 | 1131 | 1124 | 6 | 1 | 93.91% | 0.091 | 0.932 | 0.058 |

### Captures

- calc-engine/dc_10y/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-engine\tests\captures\dc_10y\oracle.capture.json
- calc-engine/dc_10y_iteration/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-engine\tests\captures\dc_10y_iteration\oracle.capture.json
- calc-engine/dc_86f/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-engine\tests\captures\dc_86f\oracle.capture.json
- calc-engine/dc_y2o/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-engine\tests\captures\dc_y2o\oracle.capture.json
- calc-engine/l3/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-engine\tests\captures\l3\oracle.capture.json
- calc-fml/dc_51ka/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-fml\tests\captures\dc_51ka\oracle.capture.json
- calc-fml/dc_ibp2/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-fml\tests\captures\dc_ibp2\oracle.capture.json
- calc-fml/dc_l10u/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-fml\tests\captures\dc_l10u\oracle.capture.json
- calc-func/catalogue/oracle.xlsx: 23 committed section captures from 1 capture session(s); 0 section(s) awaiting capture
- calc-func/cdoc_jpha/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\cdoc_jpha\oracle.capture.json
- calc-func/cfunc_m1_2/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\cfunc_m1_2\oracle.capture.json
- calc-func/cfunc_m3_2/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\cfunc_m3_2\oracle.capture.json
- calc-func/cfunc_m3_4/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\cfunc_m3_4\oracle.capture.json
- calc-func/cfunc_m3_5/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\cfunc_m3_5\oracle.capture.json
- calc-func/cfunc_m3_6/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\cfunc_m3_6\oracle.capture.json
- calc-func/dc_e00t/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\dc_e00t\oracle.capture.json
- calc-func/dc_fwpz/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\dc_fwpz\oracle.capture.json
- calc-func/dc_nzpw/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\dc_nzpw\oracle.capture.json
- calc-func/dc_q9rb/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\dc_q9rb\oracle.capture.json
- calc-func/dc_xrxb/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-func\tests\captures\dc_xrxb\oracle.capture.json
- calc-host/dc_ylr/oracle.xlsx: committed capture C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-host\tests\captures\dc_ylr\oracle.capture.json

## Failures by root cause

### Refusals (not modelled), by owner and reason

3 causes, 6 roots, 6 cells.

| Owner | Reason | Roots | Downstream | Whole workbook | Cells | Workbooks |
|---|---|---:|---:|---:|---:|---:|
| exactness dc-uar | not-modelled < | 3 | 0 | 0 | 3 | 1 |
| exactness dc-uar | not-modelled GETPIVOTDATA | 2 | 0 | 0 | 2 | 1 |
| exactness dc-uar | not-modelled > | 1 | 0 | 0 | 1 | 1 |

### Mismatches, by function and shape at the root

112 causes, 1121 roots, 1121 cells.

| Function | Shape | Roots | Downstream | Whole workbook | Cells | Workbooks |
|---|---|---:|---:|---:|---:|---:|
| BESSELY | number, differs within 15 significant digits | 80 | 0 | 0 | 80 | 1 |
| BESSELJ | number, differs within 15 significant digits | 77 | 0 | 0 | 77 | 1 |
| CUMPRINC | number, 2 to 16 ulp apart | 65 | 0 | 0 | 65 | 1 |
| PPMT | number, 1 ulp apart | 60 | 0 | 0 | 60 | 1 |
| CUMPRINC | number, 1 ulp apart | 56 | 0 | 0 | 56 | 1 |
| IPMT | number, 1 ulp apart | 56 | 0 | 0 | 56 | 1 |
| BESSELI | number, differs within 15 significant digits | 54 | 0 | 0 | 54 | 1 |
| VAR.S | number, differs within 15 significant digits | 47 | 0 | 0 | 47 | 1 |
| VAR.P | number, differs within 15 significant digits | 40 | 0 | 0 | 40 | 1 |
| GEOMEAN | number, 1 ulp apart | 38 | 0 | 0 | 38 | 1 |
| CUMPRINC | number, differs within 15 significant digits | 35 | 0 | 0 | 35 | 1 |
| BINOM.DIST | number, 2 to 16 ulp apart | 31 | 0 | 0 | 31 | 1 |
| CUMIPMT | number, 2 to 16 ulp apart | 31 | 0 | 0 | 31 | 1 |
| CUMIPMT | number, 1 ulp apart | 29 | 0 | 0 | 29 | 1 |
| BINOM.DIST | number, 1 ulp apart | 25 | 0 | 0 | 25 | 1 |
| INDEX | number, differs within 15 significant digits | 23 | 0 | 0 | 23 | 1 |
| PPMT | number, 2 to 16 ulp apart | 22 | 0 | 0 | 22 | 1 |
| PPMT | number, differs within 15 significant digits | 22 | 0 | 0 | 22 | 1 |
| IPMT | number, 2 to 16 ulp apart | 18 | 0 | 0 | 18 | 1 |
| VAR.P | number, 2 to 16 ulp apart | 16 | 0 | 0 | 16 | 1 |
| VAR.S | number, 2 to 16 ulp apart | 16 | 0 | 0 | 16 | 1 |
| BINOM.DIST | number, differs within 15 significant digits | 14 | 0 | 0 | 14 | 1 |
| INDEX | number, 1 ulp apart | 14 | 0 | 0 | 14 | 1 |
| STDEV | number, differs within 15 significant digits | 13 | 0 | 0 | 13 | 1 |
| VARP | number, differs within 15 significant digits | 13 | 0 | 0 | 13 | 1 |
| Infix & | text expected, text returned | 9 | 0 | 0 | 9 | 2 |
| STDEV.S | number, 1 ulp apart | 9 | 0 | 0 | 9 | 1 |
| STDEVA | number, 1 ulp apart | 9 | 0 | 0 | 9 | 1 |
| VAR.S | number, 1 ulp apart | 9 | 0 | 0 | 9 | 1 |
| VARPA | number, 2 to 16 ulp apart | 9 | 0 | 0 | 9 | 1 |
| INDEX | number, 2 to 16 ulp apart | 7 | 0 | 0 | 7 | 1 |
| CUMIPMT | number, differs within 15 significant digits | 6 | 0 | 0 | 6 | 1 |
| IPMT | number, differs within 15 significant digits | 6 | 0 | 0 | 6 | 1 |
| POISSON.DIST | number, 1 ulp apart | 5 | 0 | 0 | 5 | 1 |
| STDEV.P | number, 2 to 16 ulp apart | 5 | 0 | 0 | 5 | 1 |
| VAR | number, differs within 15 significant digits | 5 | 0 | 0 | 5 | 1 |
| VAR.P | number, 1 ulp apart | 5 | 0 | 0 | 5 | 1 |
| VARPA | number, 1 ulp apart | 5 | 0 | 0 | 5 | 1 |
| BESSELJ | number, 1 ulp apart | 4 | 0 | 0 | 4 | 1 |
| BESSELJ | number, 2 to 16 ulp apart | 4 | 0 | 0 | 4 | 1 |

and 72 more in summary.json

### Other failures, by class and key

2 causes, 4 roots, 4 cells.

| Class | Key | Roots | Downstream | Whole workbook | Cells | Workbooks |
|---|---|---:|---:|---:|---:|---:|
| capture | capture failure at stage reopen | 3 | 0 | 0 | 3 | 1 |
| capture | capture failure at stage value | 1 | 0 | 0 | 1 | 1 |

