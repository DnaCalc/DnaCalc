# Why large arguments expose Bessel rounding
Tags: Excel parity, last bit, Bessel, x87
Status: draft

Away from zeros, a single bit in an intermediate product barely moves a Bessel function. Near a cancellation zero of the large-argument asymptotic expansion (`|x| >= 8`), the leading amplitude term and the correction term nearly cancel—so changing the order of two multiplications or storing one step early to `binary64` shows up straight away in the returned bits.

## The insight: association and store points at `|x| >= 8` and in backward recurrence

Excel's ordinary Bessel functions (`BESSELJ`, `BESSELY`) start from rational approximations at low arguments and switch at `8` to an asymptotic expansion pairing an oscillating `sin` or `cos` with amplitude and correction polynomials in `z = st(8 / |x|)` and `y = st(z * z)`. For higher orders, they step backward (`F[n-1] + F[n+1] = (2n / x) * F[n]`), rescaling along the way and normalizing at the end.

Probing inputs near `8`, near cancellation zeros, and across higher-order backward recurrences pinned down the exact `PC = 64` store boundaries:

- **Correction-term association:** After forming `z = st(8 / |x|)` and evaluating Horner polynomials with stored multiplies and adds, the seeds associate their correction product differently: `J_0` forms `st(st(z * sin(phase)) * correction)`, while `J_1` and `Y_1` form `st(st(trig(phase) * correction) * z)` and `Y_0` forms `st(st(cos(phase) * correction) * z)`. `J` subtracts the correction; `Y` adds it, stores the combination, applies the staged square root to the scale quotient, and stores the final product. On the registered `BESSELJ` order-zero probe (`0x40466492ad084087`), that order matches Excel's `0x3fbe846e744ff5b9`.
- **Backward recurrence normalization:** Beyond storing the initial reciprocal factor, recurrence multiply/subtract, rescale products, and final division, storing `loop_index * (2 / |x|)`, the additions into the alternating normalization sum, and the subtraction in `2 * total - current` took the backward-recurrence probe set from `0 / 20` to `20 / 20`.

## What moved—and what stays open

- **Four-function full witness bank (`9,750` rows):** Exact matches rose from **`9,305` to `9,750 / 9,750`** (`+445` gains, `0` losses): `BESSELJ` `3,803 -> 4,244 / 4,244`, `BESSELY` `2,901 -> 2,905 / 2,905`, `BESSELI` `1,365 / 1,365`, and `BESSELK` `1,236 / 1,236`.
- **Frozen suites and oracle corpus:** The unseen holdout jumped from **`29` to `240 / 240`** exact, the first probe set from **`37` to `221 / 221`**, and the pinned `J`, `I`, and `K` sets matched `120 / 120` each across both native x87 and `soft-x87`. The `156`-workbook Linux oracle corpus had `0` Bessel mismatches (all `310` dedicated Bessel cells and all `30` original discrepant cells matching; `78,323 / 78,390` corpus cells overall, `0` refusals).
- **Still open:** Direct probes of the separate order-one `BESSELI` seed and Windows/browser sweeps remain under `dc-j9k`, and same-build comparisons across CPU vendors remain under `dc-ymjf.7`.

[Read the full Bessel article](/articles/last-bit-bessel/) (`docs/articles/last-bit-bessel.md`).
