# The last bit: Excel's Bessel functions

*Researched in Carbon round `cb15` and landed and verified in Neon round `nx14` on November 22. The article uses the recorded Excel `16.0.20527.20196` answers and public numerical methods. [Landing and Carbon evidence](https://github.com/DnaCalc/DnaCalc/commit/c3500807) · [Neon corpus close-out](https://github.com/DnaCalc/DnaCalc/commit/8dc25833)*

`BESSELJ` and `BESSELY` evaluate ordinary Bessel functions. `BESSELI` and `BESSELK` evaluate modified Bessel functions. For the ordinary functions, the implementation starts with approximations at orders zero and one and uses recurrence to reach higher orders. A small floating-point difference can enter through either a starting value or a recurrence step. On the final frozen holdout, DNA Calc went from 29 to 240 exact matches out of 240. [Landing and holdout results](https://github.com/DnaCalc/DnaCalc/commit/c3500807)

## Where the work started

The low-argument J and Y seeds use rational approximations adapted from the public SheetJS Bessel implementation. Earlier probes placed binary64 stores at the polynomial, division and logarithmic correction steps. In that run, two previously failing `BESSELY` cells in the oracle corpus matched, and the Y witness bank rose from 2,847 to 2,897 matches out of 2,901. Applying the same stored forward recurrence that `BESSELK` uses recovered the final four Y bank rows on that snapshot. A separate earlier change adapted ITK's public recurrence for `BESSELI`. [Low seeds](https://github.com/DnaCalc/DnaCalc/commit/2651d64a) · [Forward recurrence](https://github.com/DnaCalc/DnaCalc/commit/618df4cf) · [Modified Bessel I](https://github.com/DnaCalc/DnaCalc/commit/d62b1283)

At the start of the final round, all four historical function banks matched on that round's starting rows. The frozen probes and holdout exposed differences in the large-argument seeds and in `BESSELJ`'s backward recurrence. The bank comparison below uses the same expanded denominator on both sides of the final landing. [Carbon evidence and bank comparison](https://github.com/DnaCalc/DnaCalc/commit/c3500807)

## Large arguments change which products matter

The large-argument asymptotic expansions combine an oscillating sine or cosine with an amplitude that decreases as the argument grows. The finite approximation evaluates an amplitude polynomial and a correction polynomial, combines their trigonometric terms, and multiplies by a square-root scale. Near a zero of the result, the leading and correction terms can almost cancel, so changes in the low bits of either term become much easier to see in the output. [NIST asymptotic expansions](https://dlmf.nist.gov/10.17)

In the matching implementation, let `st(op)` mean an arithmetic operation rounded to x87's 64-bit significand precision, then stored as binary64. The seeds form `z = st(8 / |x|)` and `y = st(z * z)`. Horner evaluation stores each multiply and add. After the stored leading product, the correction term uses these associations.

| Seed | Correction term |
|---|---|
| J₀ | `st(st(z * sin(phase)) * correction)` |
| J₁ and Y₁ | `st(st(trig(phase) * correction) * z)` |
| Y₀ | `st(st(cos(phase) * correction) * z)` |

The J seeds subtract the correction, and the Y seeds add it. The implementation stores the combined term, applies the staged square root to the stored scale quotient, and stores the final multiplication. For example, the registered order-zero `BESSELJ` association probe uses argument bits `0x40466492ad084087` and expects `0x3fbe846e744ff5b9`. The regression also includes inputs near the approximation boundary at 8 and near cancellation zeros. [Seed changes and exact probes](https://github.com/DnaCalc/DnaCalc/commit/c3500807)

## Backward recurrence carries rounding into normalization

For ordinary Bessel functions, the relation `F[n-1] + F[n+1] = (2n/x) * F[n]` connects neighboring orders. Backward recurrence starts above the requested order, steps down, rescales the running values when they grow too large, and normalizes the selected result at the end. [NIST recurrence relation](https://dlmf.nist.gov/10.6) · [NIST treatment of backward recurrence](https://dlmf.nist.gov/3.6.iii)

The earlier implementation already stored the initial reciprocal factor, recurrence multiply and subtract, rescale products and final divide. The final round also stored the product of the loop index and `2 / |x|`, the additions to the alternating normalization sum, and the subtraction in `2 * total - current`. The second probe set focused on the backward recurrence. It went from zero to 20 matches out of 20. [Earlier backward changes](https://github.com/DnaCalc/DnaCalc/commit/0de20b30) · [Final changes and probe results](https://github.com/DnaCalc/DnaCalc/commit/c3500807)

## Before and after

The full witness comparison recorded 445 gains and zero losses. Exact means matching the recorded answer on the row, including the binary64 bits for numeric answers. [Full and pinned witness results](https://github.com/DnaCalc/DnaCalc/commit/c3500807)

| Function | Exact before | Exact after |
|---|---:|---:|
| BESSELJ | 3,803 / 4,244 | 4,244 / 4,244 |
| BESSELY | 2,901 / 2,905 | 2,905 / 2,905 |
| BESSELI | 1,365 / 1,365 | 1,365 / 1,365 |
| BESSELK | 1,236 / 1,236 | 1,236 / 1,236 |
| **Total** | **9,305 / 9,750** | **9,750 / 9,750** |

The frozen suites ran through the worksheet function entry point with both native x87 and software x87. The three pinned sets (J, I and K) each matched all 120 rows. [Frozen suite results](https://github.com/DnaCalc/DnaCalc/commit/c3500807)

| Frozen suite | Exact before | Exact after |
|---|---:|---:|
| Unseen holdout | 29 / 240 | 240 / 240 |
| First probe set | 37 / 221 | 221 / 221 |
| Backward recurrence probes | 0 / 20 | 20 / 20 |

## What was checked and what remains open

The Linux close-out found no Bessel mismatch in the 156-workbook oracle corpus, including all 310 cells in the dedicated Bessel workbook and the original 30 discrepant cells. Across all functions, the corpus matched 78,323 of 78,390 cells. Its 67 failures comprised 36 mismatches, 5 missing cells, 14 circular-rule cells and 12 awaiting capture, with zero refusals. [Corpus close-out](https://github.com/DnaCalc/DnaCalc/commit/8dc25833)

Direct probes of the separate order-one `BESSELI` seed and Windows/browser sweeps remain with the parent exactness work (`dc-j9k`). Same-build comparisons across Excel CPU vendors remain part of the CPU study (`dc-ymjf.7`). [Bessel landing tracker record](https://github.com/DnaCalc/DnaCalc/commit/c3500807) · [CPU capture notes](https://github.com/DnaCalc/DnaCalc/blob/master/docs/design/x87-in-calc-func.md)
