# High-precision witness triage

Each function table contains the decoded calls on which the full witness judge and the selected
Excel reading differ. The study discrepancies admitted by dc-j9k.65 go through the same judge and
appear with their original source locator. The reference is an independent mathematical calculation.
Excel's observed bits remain the target for CalcFunc, including cases where Excel's answer deviates
from the mathematical result.

## Snapshot

The dev-profile judge used DNA Calc `334fd20a`, CalcFunc-Evidence `4ef2c244`, and an AMD EPYC
x86-64 host. It matched 4,763,471 of 4,794,193 counted rows (99.35918%). Of 30,722 failures,
30,718 have trace rows across 80 functions. This includes 28,221 failures from the 44,352
loaded study readings. The four remaining failures are POISSON.DIST input changes during
Value2 writes, recorded as `excluded_without_trace` in the summary. The judge reported zero
unread outcomes and zero typed refusals. The native Intel replay is tracked in dc-j9k.62.

The traced pairs contain 2,193 ours-wrong, 17,451 Excel-quirk, 10,701 large-deviation,
105 structural and 268 ETS rows requiring a reference fit. The thresholds and classifications
below apply to this snapshot. The 288 complex text comparisons are included in these totals.

## Reproduce

```sh
cargo xtask witness judge --bank full --report judge.json --trace judge-trace.jsonl
python3 crates/calc-func/witness-reference/reference.py judge-trace.jsonl new-tables --jobs 5
python3 crates/calc-func/witness-reference/summarize.py new-tables --judge judge.json
python3 -m unittest discover -s crates/calc-func/witness-reference
```

Run the Python steps on Linux with mpmath 1.3.0 available. Use a fresh output directory
so a function that now matches every row does not retain an old table. The summary reconciles
decoded mismatches and unread outcomes with the actual judge report, function by function.
The optional function names after the output directory select a subset for diagnosis.

## Reading a function table

| Column | Contents |
|---|---|
| `id` | Row identity in the judge trace |
| `args` | Typed arguments as JSON, including binary64 bit patterns and array layout |
| `excel`, `ours` | Excel's recorded answer and CalcFunc's calculated answer |
| `reference` | Reference published to 25 significant digits |
| `excel_ulp`, `ours_ulp` | Absolute distance in units of the last place of the reference binade, or in subnormal spacing at and near zero |
| `class`, `detail` | Triage result and its qualification |
| `source`, `result_index`, `reading` | Source locator, selected output cell and recorded Excel environment |

Text fields use JSON string escaping without enclosing quotes. A missing distance indicates that
there is no numeric answer or applicable reference for that side.

- `ours-wrong` means Excel has the nearest-even binary64 value and CalcFunc differs.
- `excel-quirk` means Excel differs from the nearest-even value. The `both-inexact` detail counts
  cases where both answers miss that value and stay within 16 ULP.
- `large-deviation` takes precedence when either answer is more than 16 ULP from the reference.
  Cancellation can make a tiny absolute error large in ULP.
- `structural:...` records differing answer kinds, a transport exclusion, a refusal or a
  representation-only difference. A finite answer may still have a reference distance.
- `no-reference` and `reference-failed:...` record numeric pairs without a usable mathematical
  comparison. Unread outcomes have no typed pair and are counted in the summary's `unread` column.
  Transport exclusions without a trace record have their own `excluded_without_trace` column.

`text15` compares the components of complex-number text at 15 significant digits. Those rows
have no binary64 ULP distance and the summary counts them explicitly within their classes.

## Reference arithmetic

The calculations use mpmath at 60 decimal digits, with extra guard digits for cancellation.
The ROUND family starts from exact rational
values. Discrete distributions use their mathematical mass and CDF, and inverse distributions
solve the reference CDF or the smaller tail with arbitrary-precision arithmetic. The incomplete
beta uses symmetry and a continued fraction. Large-shape gamma tails near the mode integrate the
density in standardized coordinates. [NIST DLMF incomplete beta](https://dlmf.nist.gov/8.17),
[NIST DLMF incomplete gamma](https://dlmf.nist.gov/8.6)

RATE and YIELD use the bank answer as a root-search starting point. An iteration's stopping
value can differ from the mathematical root. The finance references use the annuity balance,
the MIRR cash-flow equation and coupon arithmetic. [Annuity balance](https://support.microsoft.com/en-us/excel/functions/pv-function),
[MIRR](https://support.microsoft.com/en-us/excel/functions/mirr-function),
[YIELD](https://support.microsoft.com/en-us/excel/functions/yield-function)

Two RATE pairs have no real annuity root for a periodic rate above −1. Their structural
classification records the balance constraint that rules out a root.

Excel specifies the AAA family for FORECAST.ETS. The seed, automatic period selection, fitted
parameters and forecast error model also affect its results. Numeric pairs from that family
require an independently specified fit before they can be classified against a true value.
[Microsoft forecasting functions](https://support.microsoft.com/en-us/excel/forecasting-functions-reference)
