# Why QR pivot order changes LINEST's intercept error
Tags: Excel parity, last bit, regression, x87
Status: draft

Fitting a linear model with column-pivoted QR reorders the predictor columns so the largest remaining columns go first. That pivoting keeps the triangular factor stable when predictors are nearly collinear—yet when `LINEST` reports the intercept's standard error, a single change in summation order or reflector square-root rounding can ripple across the matrix inverse into the output.

On our baseline regression witness bank, `221` `LINEST` rows missed Excel—including nine intercept standard errors classified as gross discrepancies, the worst off by `97,184,757,466` ULPs.

## The insight: pivoted QR fit, original-order intercept variance

Two architectural details explained both the `97`-billion-ULP outliers and the remaining single-ULP misses across `LINEST`, `LOGEST`, `TREND`, and `GROWTH`:

- **Staged square roots and original predictor order in `LINEST`:** Under x87 `PC = 64` stored arithmetic (`st(...)`), every Householder norm and standard error uses the staged square root—even a single ULP in a reflector on a near-collinear design (such as repeated rows where column 2 is nearly twice column 1, `0x3fc6a087c5d584f3`) propagates through the triangular inverse into the coefficient covariance. When reconstructing the intercept variance, Excel does not sum that covariance quadratic form in pivot order: it starts with the constant's diagonal variance, adds the active predictor diagonals in **original input order**, then active pairs in row order, and finally each active predictor's covariance with the constant (pinned by a four-predictor holdout at `0x405671480784f433`).
- **Term-by-term rounding in `GROWTH` and `TREND`:** In exact arithmetic, `exp(sum(b_i * x_i) + b_0)` equals `prod(exp(b_i * x_i)) * exp(b_0)`. Excel's `GROWTH` does the second—exponentiating each stored `slope * x` product, multiplying in predictor order, and multiplying by `exp(intercept)` last—recovering all `6` `GROWTH` bank misses. (`TREND` similarly stores each `slope * x` product, sums in predictor order, and adds the intercept.)

## What moved—and what stays open

- **Six-function witness bank (`15,716` rows):** Exact matches rose from **`15,489` to `15,716 / 15,716`** (`+227` gains, `0` losses): `LINEST` `3,224 -> 3,445 / 3,445`, `GROWTH` `1,254 -> 1,260 / 1,260`, alongside `LOGEST` (`3,585 / 3,585`), `TREND` (`1,532 / 1,532`), `FORECAST` (`2,984 / 2,984`), and `FORECAST.LINEAR` (`2,910 / 2,910`).
- **Frozen suites and oracle workbooks:** Pinned banks rose from `327` to `329 / 329`; the unseen holdout jumped from **`16` to `250 / 250`**; `230 / 230` separate probes and `369 / 369` Carbon-retained difference rows matched under both native x87 and `soft-x87`. Five dedicated regression oracle workbooks matched all `6,673 / 6,673` cells and resolved all `108` original catalogue discrepancies.
- **Still open:** Mean calculation at `2,049` or more observations and Windows/browser sweep coverage remain under `dc-j9k`, and the public Function Bank refresh is tracked in `dc-muk9e`.

[Read the full LINEST and regression article](/articles/last-bit-linest/) (`docs/articles/last-bit-linest.md`).
