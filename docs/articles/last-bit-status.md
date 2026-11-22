# The last bit: beta, Bessel and trigonometry

The November 22 measurements use recorded Excel answers. Each result below describes a particular witness set. Exact means the returned binary64 bits matched on every row in that set.

## Beta distributions

The AS 109 inverse and incomplete-beta update shares the solver across tiny shapes and changes where subnormal intermediate values become zero. In that change's 29-function bank, exact matches rose from 172,505 to 173,725 out of 192,972. BETA.INV went from 5,571 to 6,596 out of 7,636, while BETA.DIST went from 3,965 to 4,127 out of 15,845. Near-endpoint and small-shape differences remain. [Solver change and tests](https://github.com/DnaCalc/DnaCalc/commit/a423c5c0)

## Bessel functions

The large-argument seeds and backward recurrence now retain extended precision at the measured arithmetic steps. BESSELJ went from 3,803 to 4,244 exact matches out of 4,244 and BESSELY from 2,901 to 2,905 out of 2,905. BESSELI remained at 1,365 of 1,365 and BESSELK at 1,236 of 1,236. [Bessel change and tests](https://github.com/DnaCalc/DnaCalc/commit/c3500807)

## Circular trigonometry and gamma

SIN, COS, SEC and CSC share the measured x87 FPTAN reduction and reconstruction. Across 54,834 bank rows for 24 functions and callers, exact matches rose from 54,646 to 54,668. SIN reached 2,141 of 2,141 and COS 3,685 of 3,685. GAMMA rose from 12,602 to 12,610 of 12,707, with differences remaining outside the trig path. [Trigonometry change and tests](https://github.com/DnaCalc/DnaCalc/commit/b9b8750c)

The [forward and inverse gamma article](last-bit-gamma.md) explains the shared distribution kernels. Subsequent boundary measurements established where the forward CDF and PDF flush subnormal intermediates and outputs, including the normalized input, the Poisson prefactor and the Temme expansion. [Gamma boundary change and tests](https://github.com/DnaCalc/DnaCalc/commit/9f81c9bd)
