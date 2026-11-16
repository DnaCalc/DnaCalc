# Last-bit triage against a high-precision reference (dc-dnbcq)

One table per function, next to `../witness-status.tsv`: every row of the full bank that Excel and DNA Calc
answer differently (from `cargo xtask witness judge --bank full --trace`), with a reference value computed
by mpmath at 60 significant digits (or exact rational arithmetic for ROUND, ROUNDUP, ROUNDDOWN, TRUNC),
and the distance of each answer from that true value.

## Files

- `<FUNCTION>.tsv`: the rows of one function, tab separated.
- `summary.tsv`: one line per function with the class counts and the worst distances.
- `reference.py`: the reference implementations (`R`, one per function) and the classifier.
  `PYTHONPATH=<mpmath dir> python3 reference.py <trace.jsonl> <out-dir> [FUNCTION ...]`
  (mpmath: `pip3 install --target <dir> mpmath`). It reads only rows with expected != actual.
- `summarize.py <dir>`: writes `summary.tsv` from the tables.

## Columns of `<FUNCTION>.tsv`

| column | meaning |
|---|---|
| `id` | the bank row identity (as in the judge trace) |
| `args` | the call arguments (JSON; doubles as `0x` bit patterns) |
| `excel` | Excel's answer (double bits, `error:...` or `text:...`) |
| `ours` | DNA Calc's answer |
| `reference` | the true value, 25 significant digits (empty when there is none) |
| `excel_ulp`, `ours_ulp` | distance of each answer from the true value in ulps of the true value's binade |
| `class` | see below |

## Classes

A double is *correctly rounded* when it is within 0.5000001 ulp of the true value.

- `ours-wrong`: Excel is correctly rounded, we are not. A fix for the calc-func kernel.
- `excel-quirk`: we are correctly rounded, Excel is not. The bit-match rule still applies (the engine does no
  more than Excel): the row needs Excel's own arithmetic, not a better one.
- `both-inexact`: neither is correctly rounded and they differ, both within 16 ulp of the true value.
  Last-bit work against Excel's algorithm.
- `large-deviation`: either answer is more than 16 ulp from the true value. Excel's answer is the
  recorded behaviour; the row shows how far each side is from the mathematics.
- `structural:<excel>/<ours>`: one side is not a number (error, text, refusal `fault:...`, `excluded:...`).
  There is no distance; the kinds are in the class.
- `no-reference`: numeric rows of a function with no reference implementation yet; `reference` holds the
  distance between Excel's and our answer in ulps.
- `non-scalar-argument`: an argument is blank or text, so the reference does not apply.
- `reference-failed:<kind>`: the reference did not compute (outside its domain, no convergence, over 4 s).
- `reference-zero-or-range`: the true value is zero or outside the double range, so ulps are undefined.

Inverse distributions are solved from the definition (a bracketing root find on the reference CDF), started
at Excel's answer. Bond functions (PRICE, YIELD, DURATION, MDURATION) use the documented coupon
arithmetic with the 1900 date system; the bank rows do not record more than that.

Note on `large-deviation`: a distance of 1e15 ulp on an inverse function means the answer is near zero by
cancellation (for example `mean + sd * z` close to 0) and the two doubles differ at the scale of the operands,
not of the result. Read such rows by the absolute difference to the reference.
