# The last bit: numerical family status

Each result below describes a particular witness set measured against recorded Excel answers. A numeric result is exact when its returned binary64 bits match the recorded bits.

## Beta distributions

The AS 109 inverse and incomplete-beta update shares the solver across tiny shapes and changes where subnormal intermediate values become zero. In that change's 29-function bank, exact matches rose from 172,505 to 173,725 out of 192,972. BETA.INV went from 5,571 to 6,596 out of 7,636, while BETA.DIST went from 3,965 to 4,127 out of 15,845. [Solver change and tests](https://github.com/DnaCalc/DnaCalc/commit/a423c5c0). Follow-on landings—retaining `dc-i2wk3`'s Ext80 `bgrat` ([bgrat change](https://github.com/DnaCalc/DnaCalc/commit/8ac7c9e8)), fixing the `LowerEndpoint` Newton step ([Newton change](https://github.com/DnaCalc/DnaCalc/commit/b0c75eb2)), and switching `bfrac` to DiDonato & Morris Eq. (2.8) in Modified Lentz ([bfrac change](https://github.com/DnaCalc/DnaCalc/commit/9b35d0fc))—brought the 29-function bank to 182,814 of 192,972, with density-prefactor and `ln B` work remaining under `dc-j9k.55`. See the [incomplete-beta family article](last-bit-beta.md).

## PMT and amortization

Unifying `PMT` onto one `PC = 64` balance formula, switching `alnrel` to the shared x87 logarithm, and storing exponent products from `PC = 64` took all five amortization functions (`PMT`, `IPMT`, `PPMT`, `CUMIPMT`, and `CUMPRINC`) from 308,703 to 308,710 exact matches out of 308,710 (100.000%), matching all 409 original `dc-j9k.12` oracle-corpus cells and 240 of 240 unseen holdout rows. [PMT change and tests](https://github.com/DnaCalc/DnaCalc/commit/b4bd656d). See the [PMT and amortization article](last-bit-pmt.md).

## Bessel functions

The large-argument seeds and backward recurrence now round from extended precision to binary64 at the measured steps. BESSELJ went from 3,803 to 4,244 exact matches out of 4,244 and BESSELY from 2,901 to 2,905 out of 2,905. BESSELI remained at 1,365 of 1,365 and BESSELK at 1,236 of 1,236. The frozen holdout rose from 29 to 240 of 240 exact. [Bessel change and tests](https://github.com/DnaCalc/DnaCalc/commit/c3500807) · [Bessel family article](last-bit-bessel.md)

## LINEST and regression

Staged x87 arithmetic in the QR fit, statistics and prediction loops took LINEST from 3,224 to 3,445 of 3,445 exact and GROWTH from 1,254 to 1,260 of 1,260. Together with LOGEST, TREND, FORECAST and FORECAST.LINEAR, the full witness banks matched all 15,716 rows. The frozen holdout rose from 16 to 250 of 250 exact. [Regression change and tests](https://github.com/DnaCalc/DnaCalc/commit/43853e35) · [LINEST family article](last-bit-linest.md)

## Circular trigonometry and gamma

SIN, COS, SEC and CSC share the measured x87 FPTAN reduction and reconstruction. Across 54,834 bank rows for 24 functions and callers, exact matches rose from 54,646 to 54,668. SIN reached 2,141 of 2,141 and COS 3,685 of 3,685. GAMMA rose from 12,602 to 12,610 of 12,707, with differences remaining outside the trig path. [Trigonometry change and tests](https://github.com/DnaCalc/DnaCalc/commit/b9b8750c)

The [forward and inverse gamma article](last-bit-gamma.md) explains the shared distribution kernels. Subsequent boundary measurements established where the forward CDF and PDF flush subnormal intermediates and outputs, including the normalized input, the Poisson prefactor and the Temme expansion. [Gamma boundary change and tests](https://github.com/DnaCalc/DnaCalc/commit/9f81c9bd)
