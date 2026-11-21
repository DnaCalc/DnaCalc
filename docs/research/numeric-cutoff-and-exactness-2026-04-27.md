# Numeric cutoff, underflow flush, and root cancellation (2026-04-27)

Extracted from [Foundation `EXCEL_NUMERIC_CUTOFF_AND_EXACTNESS_THEORY_NOTE.md`][source]
(April 2026 COM probes). Compare [Foundation evidence notes](../history/foundation/evidence.md)
and DnaCalc's current implementation in `crates/calc-fml/src/eval/compile.rs` and
`crates/calc-fml/src/eval/run.rs`.

## 1. Literal admission cutoff (Behavior A)

Microsoft Support ("Excel specifications and limits") documents 15-digit precision and smallest
allowed positive/negative magnitudes `±2.2251E-308`. In the April 2026 COM probes:

- Entered literals from `2.2250738585072014E-308` through `2.2250738585072099E-308`
  were rewritten by Excel to stored formula `=0` and value `0`:
  - `=2.2250738585072014E-308` -> stored `=0`, value `0`
  - `=2.2250738585072014E-308/10` (`FTC-0630`) -> stored `=0/10`, value `0`
  - `=2.2250738585072090E-308` .. `=2.2250738585072099E-308` -> stored `=0`, value `0`
- First preserved nonzero literal observed:
  - entered `=2.2250738585072100E-308` -> stored `=2.22507385850721E-308`, value `2.22507385850721E-308`.

In DnaCalc (`crates/calc-fml/src/eval/compile.rs:2375-2398`, `2708-2730`), parsed literals are
first truncated to 15 significant digits (`fifteen_digits`), so literals whose 15-digit truncation is
subnormal (including `1E-308`, `1E-309`, `0.1E-307`, `2.2250738585072014E-308`) compile to `0.0`
when scale is readable (measured on Excel `16.0.20527`), while `=1E-310` is refused.

## 2. Post-evaluation zero flush (Behavior B) and November reconciliation

In the April 2026 COM probes, preserved literals whose computed quotient fell into the
subnormal band flushed to `0` (`0.0`):

- `=2.2250738585072100E-308/2` (and `...2102`, `...2104`, `...2106`, `...2108`, `...2110`) -> formula preserved, value `0`
- `=(2.2250738585072100E-308/2)*1E307` -> `0` (intermediate flush)
- `=2.24E-308/2`, `=2.26E-308/2`, `=2.3E-308/2` -> formula preserved, value `0`
- `=POWER(2,-1023)` -> `0`
- `=POWER(2,-1022)` -> `2.2250738585072014E-308` (minimum normal binary64)
- `=5E-308/2` -> `2.5E-308` (nonzero; above flush region)

**Open reconciliation with November build `16.0.20527`:** `crates/calc-fml/src/eval/run.rs:3798-3805`
records a computed subnormal surviving in Excel `16.0.20527`:
`=POWER(2,-1022)*(1+POWER(2,-52))-POWER(2,-1022)` returns `0x0000000000000001`. Both
observations may hold across different operations (division/`POWER` vs subtraction) or builds;
re-running the April `/2` and `POWER` formulas on current Windows Excel (`16.0.20527+`, owned by
`dc-zuwqx.4`) is required before finalizing any general subnormal flush rule.

## 3. Root-only ungrouped add/sub cancellation compensation (`FTC-0533`)

April 2026 COM probe matrix distinguishing root cancellation compensation from ordinary IEEE-754 arithmetic:

| Formula | Observed Excel value | Role |
| --- | --- | --- |
| `=0.1+0.2-0.3` | `0` | Ungrouped root add/sub cancellation zeroed |
| `=(0.1+0.2-0.3)` | `5.551115123125783E-17` | Outer parentheses suppress compensation |
| `=(0.1+0.2-0.3)*1E17` | `5.551115123125783` | Outer multiplication reveals residue |
| `=IF(0.1+0.2-0.3,1,0)` | `1` | Condition argument is nonzero |
| `=(0.1+0.2-0.3)=0` | `FALSE` | Grouped comparison sees nonzero residue |
| `=1E-17+(-2E-17)` | `-1E-17` | Truly small non-cancelling sum preserved |
| `=0.300000000000001-0.3` | `9.992007221626409E-16` | Non-zero-reaching difference preserved |
| `=43.1-43.2+0.1` | `-1.4155343563970746E-15` | Multi-term mismatch residue preserved |
| `=(1E15+1)-1E15` (`FTC-0628`) | `1` | Large meaningful delta preserved |

[source]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/test-corpus/workspace/monitoring/EXCEL_NUMERIC_CUTOFF_AND_EXACTNESS_THEORY_NOTE.md
