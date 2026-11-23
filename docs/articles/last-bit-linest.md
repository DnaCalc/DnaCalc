# The last bit: Excel's LINEST and regression family

*Researched in Carbon round `cb12`, landed in Neon round `nx10`, and verified in round `nx13`. The article uses the recorded Excel `16.0.20527.20196` answers and public least-squares mathematics. [Landing and Carbon evidence](https://github.com/DnaCalc/DnaCalc/commit/43853e35) · [Neon verification](https://github.com/DnaCalc/DnaCalc/commit/ec8eafd6)*

`LINEST` fits a linear model and can return a block of regression statistics. `TREND` uses the fit to predict a response at new predictor values. `LOGEST` and `GROWTH` fit the logarithm of a positive response and express the result on the original scale. The comparison also included the related single-predictor functions `FORECAST` and `FORECAST.LINEAR`. Exact matches across their six witness banks rose from 15,489 to 15,716 out of 15,716. [Full witness results](https://github.com/DnaCalc/DnaCalc/commit/43853e35)

## Where we started

The baseline missed 221 `LINEST` rows, including nine intercept standard errors classified as gross discrepancies. The worst missed by 97,184,757,466 units in the last place (ULPs). `GROWTH` had six one-ULP misses. The frozen holdout tested additional cases in which small changes to least-squares arithmetic or prediction order could change the returned bits. On that set the baseline matched 16 of 250. [Intercept standard error regression test](https://github.com/DnaCalc/DnaCalc/commit/5e2f7daa) · [Frozen suites](https://github.com/DnaCalc/DnaCalc/commit/43853e35)

## Where a QR fit rounds

Column-pivoted QR uses orthogonal transformations to turn the design matrix into an upper triangular system. Pivoting changes the order in which predictors are processed and helps identify columns that are numerically dependent. The triangular solve works from the last unknown back to the first. [LAPACK Users' Guide on column-pivoted QR](https://netlib.org/lapack/lug/node42.html)

The model that matched the observations rounds each selected arithmetic operation first at x87's 64-bit significand precision and then at the binary64 store. DNA Calc uses this sequence for the mean, centering, sums of squares, Householder reflector operations, rank comparisons and intercept reconstruction. The triangular solve sums the already-solved contributions in descending column order before subtracting their sum and dividing by the diagonal. Its square-root sites use the staged square root for QR norms and standard errors. [QR implementation and regression cases](https://github.com/DnaCalc/DnaCalc/commit/43853e35)

The square root matters even when the final discrepancy appears in a different part of the result. One targeted design repeats predictor rows whose second column is almost twice their first. Small changes to a reflector in such a design can affect the triangular factor and propagate through its inverse into the coefficient covariance. The intercept standard error in that case matched Excel's `0x3fc6a087c5d584f3` after the change. The dedicated public-entry regression test pins that case and the other eight gross intercept-error rows. [Staged-root landing](https://github.com/DnaCalc/DnaCalc/commit/43853e35) · [Nine exact intercept-error cases](https://github.com/DnaCalc/DnaCalc/commit/5e2f7daa)

## The intercept variance uses the original predictor order

With centered predictors and a fitted constant, the intercept in the original coordinates is the response mean minus the sum of each fitted slope times its predictor mean. Its variance is a quadratic form in the fitted coefficient covariance, with weight 1 for the constant and the negative predictor means for the slopes. A sum can round differently if it visits the covariance terms in pivot order. [Intercept reconstruction and covariance implementation](https://github.com/DnaCalc/DnaCalc/commit/43853e35)

The measured sequence starts with the constant's diagonal covariance. It adds the active predictor diagonal terms in original input order, then the pairs of active predictors in row order, and finally each active predictor's covariance with the constant. The staged square root is applied to the resulting variance. A four-predictor holdout pins the intercept standard error at `0x405671480784f433`. In a separate boundary case, the measured result block reports `R² = 1` and `F = #NUM!` for a zero total sum of squares. Both `LINEST` and `LOGEST` have exact tests for this boundary. [Variance-order and zero-total tests](https://github.com/DnaCalc/DnaCalc/commit/43853e35)

## Predictions round after the fit

`TREND` stores the individual slope-times-predictor products, adds them in predictor order, then adds the intercept. `GROWTH` exponentiates each stored slope-times-predictor product, multiplies the factors in predictor order, and multiplies by the exponential of the intercept at the end. In exact arithmetic the exponential of a sum gives the same result as the product of exponentials. The intermediate rounding makes this choice observable in worksheet results. The landing recovered all six `GROWTH` bank misses and matched the separate `TREND` prediction probes. [Prediction implementation and tests](https://github.com/DnaCalc/DnaCalc/commit/43853e35)

## Before and after

The full witness comparison recorded 227 gains and zero losses. For an array result a witness can select a particular output cell, such as the intercept standard error. A numeric cell matches when its binary64 bits equal the recorded Excel bits. [Full and pinned witness results](https://github.com/DnaCalc/DnaCalc/commit/43853e35)

| Function | Exact before | Exact after |
|---|---:|---:|
| LINEST | 3,224 / 3,445 | 3,445 / 3,445 |
| LOGEST | 3,585 / 3,585 | 3,585 / 3,585 |
| TREND | 1,532 / 1,532 | 1,532 / 1,532 |
| GROWTH | 1,254 / 1,260 | 1,260 / 1,260 |
| FORECAST | 2,984 / 2,984 | 2,984 / 2,984 |
| FORECAST.LINEAR | 2,910 / 2,910 | 2,910 / 2,910 |
| **Total** | **15,489 / 15,716** | **15,716 / 15,716** |

The pinned banks rose from 327 to 329 exact matches out of 329. The worksheet function entry point matched all 250 frozen unseen holdout rows, up from 16, and all 230 separate probe rows with both native and software x87. The same two paths also matched all 369 recorded rows that the Carbon campaign retained because an earlier result had differed. [Frozen suites and retained differences](https://github.com/DnaCalc/DnaCalc/commit/43853e35)

## What was checked and what remains open

The Linux close-out matched all 6,673 cells in five dedicated regression oracle workbooks. It also found that all 108 original catalogue discrepancies were resolved. [Regression close-out results](https://github.com/DnaCalc/DnaCalc/commit/ec8eafd6)

The mean calculation at 2,049 or more observations remains an unprobed question under the parent exactness work (`dc-j9k`), which also owns Windows/browser sweep coverage. The public Function Bank refresh is tracked in `dc-muk9e`. [Landing tracker record](https://github.com/DnaCalc/DnaCalc/commit/5b9c8b25) · [Close-out tracker record](https://github.com/DnaCalc/DnaCalc/commit/ec8eafd6)
