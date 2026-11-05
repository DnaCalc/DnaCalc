# Numerical fingerprint map

Regenerate from DnaCalc-Research, using retained service replies (or asking Excel for missing replies):

```sh
python3 racer/fingerprint.py map ../DnaCalc ../DnaCalc/docs/generated/numeric-fingerprints --cache evidence/round26/fingerprints
```

Add `--offline` to require existing answers. The JSON retains exact windows, spectra, request identities and worker provenance.

Exact single/double-rounding identification requires discriminating computation-tree predictions; output spectra alone cannot supply it.

Detected spectral lines are not identified stores. Absence of a line cannot exclude a stored stage.

Two local windows do not establish global route boundaries. Each unmeasured edge remains empty.

The EXP double-rounding label applies to its listed local discriminants. R27 broader windows contain74 ideal-leaf contradictions among2048 inputs; the registered x87 chain matches every measured input. An ideal mathematical EXP leaf is not a global identification.

Separate-cell scalar observations do not establish range, array, reference or workbook behavior.

All problem-list entries are reported, including workbook-only, text/discrete and unidentifiable cases.

| Function | Status | Rounding | Stored stages | Window peaks | Route edges |
|---|---|---|---|---|---|
| BINOM.DIST | observed | ideal single/double leaf rejected on this window | unresolved | 8, 8 | unmeasured |
| POISSON.DIST | observed | ideal single/double leaf rejected on this window | unresolved | 6, 3 | unmeasured |
| BETA.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 5 | unmeasured |
| LOGNORM.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 4 | unmeasured |
| ROUNDDOWN | workbook-only | unresolved | unresolved | — | unmeasured |
| TRUNC | workbook-only | unresolved | unresolved | — | unmeasured |
| YIELD | observed | not identifiable on non-numeric/error output | unresolved | 1, 0 | unmeasured |
| ROUNDUP | workbook-only | unresolved | unresolved | — | unmeasured |
| RATE | observed | unresolved: no complete scalar mathematical reference | unresolved | 0, 0 | unmeasured |
| NEGBINOMDIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 5 | unmeasured |
| IRR | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| PMT | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 8 | unmeasured |
| GAMMA.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| BINOM.DIST.RANGE | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 5 | unmeasured |
| KURT | observed | unresolved: no complete scalar mathematical reference | unresolved | 7, 4 | unmeasured |
| LINEST | workbook-only | unresolved | unresolved | — | unmeasured |
| LOGEST | workbook-only | unresolved | unresolved | — | unmeasured |
| NEGBINOM.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 5 | unmeasured |
| ODDFYIELD | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| GAMMA | observed | unresolved: no complete scalar mathematical reference | unresolved | 7, 5 | unmeasured |
| CUMIPMT | observed | unresolved: no complete scalar mathematical reference | unresolved | 3, 4 | unmeasured |
| BETADIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 5 | unmeasured |
| PPMT | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 6 | unmeasured |
| FV | observed | unresolved: flat quantized windows | unresolved | 0, 0 | unmeasured |
| PV | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| FDIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 7 | unmeasured |
| CUMPRINC | observed | unresolved: no complete scalar mathematical reference | unresolved | 6, 3 | unmeasured |
| F.TEST | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| IPMT | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| GAMMA.INV | observed | unresolved: no complete scalar mathematical reference | unresolved | 2, 0 | unmeasured |
| HYPGEOM.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 0 | unmeasured |
| TINV | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| CHIINV | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 0 | unmeasured |
| FINV | observed | unresolved: no complete scalar mathematical reference | unresolved | 6, 6 | unmeasured |
| BINOM.INV | observed | unresolved: flat quantized windows | unresolved | 0, 0 | unmeasured |
| TDIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 7, 7 | unmeasured |
| CEILING | workbook-only | unresolved | unresolved | — | unmeasured |
| CEILING.PRECISE | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 0 | unmeasured |
| FLOOR.PRECISE | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 0 | unmeasured |
| ISO.CEILING | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 0 | unmeasured |
| SWITCH | workbook-only | unresolved | unresolved | — | unmeasured |
| CHIDIST | observed | unresolved: flat quantized windows | unresolved | 0, 0 | unmeasured |
| CHISQ.INV | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 1 | unmeasured |
| SKEW | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| T.DIST.RT | observed | unresolved: no complete scalar mathematical reference | unresolved | 3, 3 | unmeasured |
| BESSELY | observed | ideal single/double leaf rejected on this window | unresolved | 3, 3 | 5 |
| CRITBINOM | observed | unresolved: flat quantized windows | unresolved | 0, 0 | unmeasured |
| FLOOR | workbook-only | unresolved | unresolved | — | unmeasured |
| NORM.S.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 5, 2 | unmeasured |
| GAMMALN.PRECISE | observed | unresolved: no complete scalar mathematical reference | unresolved | 2, 2 | unmeasured |
| GAMMALN | observed | unresolved: no complete scalar mathematical reference | unresolved | 2, 2 | unmeasured |
| LOGNORMDIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 4 | unmeasured |
| ERFC.PRECISE | observed | unresolved: no complete scalar mathematical reference | unresolved | 3, 6 | unmeasured |
| NORMDIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 5, 2 | unmeasured |
| GAUSS | observed | unresolved: no complete scalar mathematical reference | unresolved | 5, 2 | unmeasured |
| DURATION | observed | not identifiable on non-numeric/error output | unresolved | 1, 0 | unmeasured |
| ERFC | observed | unresolved: no complete scalar mathematical reference | unresolved | 3, 6 | unmeasured |
| NORM.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 5, 2 | unmeasured |
| F.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 6, 8 | unmeasured |
| MDURATION | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| GEOMEAN | observed | ideal single/double leaf rejected on this window | unresolved | 6, 5 | unmeasured |
| SINH | observed | ideal single/double leaf rejected on this window | unresolved | 8, 8 | unmeasured |
| ROUND | workbook-only | unresolved | unresolved | — | unmeasured |
| NPV | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| TANH | observed | ideal single/double leaf rejected on this window | unresolved | 8, 8 | unmeasured |
| COTH | observed | ideal single/double leaf rejected on this window | unresolved | 5, 8 | unmeasured |
| PRICE | observed | not identifiable on non-numeric/error output | unresolved | 1, 0 | unmeasured |
| BETAINV | observed | unresolved: no complete scalar mathematical reference | unresolved | 3, 1 | unmeasured |
| ERF.PRECISE | observed | unresolved: no complete scalar mathematical reference | unresolved | 2, 3 | unmeasured |
| CSCH | observed | ideal single/double leaf rejected on this window | unresolved | 8, 8 | unmeasured |
| GROWTH | workbook-only | unresolved | unresolved | — | unmeasured |
| HYPGEOMDIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 0 | unmeasured |
| PERMUTATIONA | observed | unresolved: flat quantized windows | unresolved | 0, 0 | unmeasured |
| ZTEST | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| GAMMADIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| Z.TEST | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| BINOMDIST | observed | ideal single/double leaf rejected on this window | unresolved | 8, 8 | unmeasured |
| CHISQ.TEST | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| CONFIDENCE | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| F.DIST.RT | observed | unresolved: no complete scalar mathematical reference | unresolved | 3, 8 | unmeasured |
| GAMMAINV | observed | unresolved: no complete scalar mathematical reference | unresolved | 2, 0 | unmeasured |
| INT | workbook-only | unresolved | unresolved | — | unmeasured |
| LOGINV | observed | unresolved: no complete scalar mathematical reference | unresolved | 2, 3 | unmeasured |
| POISSON | observed | ideal single/double leaf rejected on this window | unresolved | 6, 3 | unmeasured |
| POWER | observed | ideal single/double leaf rejected on this window | unresolved | 8, 8 | unmeasured |
| TREND | workbook-only | unresolved | unresolved | — | unmeasured |
| TTEST | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| WEIBULL.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| BESSELJ | observed | ideal single/double leaf rejected on this window | unresolved | 6, 7 | 5 |
| BETA.INV | observed | unresolved: no complete scalar mathematical reference | unresolved | 3, 1 | unmeasured |
| CHISQ.DIST.RT | observed | unresolved: no complete scalar mathematical reference | unresolved | 5, 8 | unmeasured |
| CONFIDENCE.NORM | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| CONFIDENCE.T | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| EXPON.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| F.INV | observed | unresolved: no complete scalar mathematical reference | unresolved | 0, 4 | unmeasured |
| F.INV.RT | observed | unresolved: no complete scalar mathematical reference | unresolved | 3, 5 | unmeasured |
| LOGNORM.INV | observed | unresolved: no complete scalar mathematical reference | unresolved | 2, 3 | unmeasured |
| SECH | observed | ideal single/double leaf rejected on this window | unresolved | 7, 8 | unmeasured |
| T.DIST | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 3 | unmeasured |
| T.DIST.2T | observed | unresolved: no complete scalar mathematical reference | unresolved | 4, 6 | unmeasured |
| T.INV.2T | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 6 | unmeasured |
| T.TEST | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| ABS | workbook-only | unresolved | unresolved | — | unmeasured |
| CEILING.MATH | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 0 | unmeasured |
| COLUMNS | workbook-only | unresolved | unresolved | — | unmeasured |
| COMPLEX | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| CONCAT | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| CONCATENATE | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| Cell reference (unresolved) | workbook-only | unresolved | unresolved | — | unmeasured |
| DEGREES | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| DELTA | observed | unresolved: flat quantized windows | unresolved | 0, 0 | unmeasured |
| EUROCONVERT | workbook-only | unresolved | unresolved | — | unmeasured |
| EVEN | workbook-only | unresolved | unresolved | — | unmeasured |
| EXP | observed | double-round mathematical leaf consistent | unresolved | 8, 8 | 1 |
| FIXED | service-refused | unresolved | unresolved | — | unmeasured |
| FLOOR.MATH | observed | unresolved: no complete scalar mathematical reference | unresolved | 1, 0 | unmeasured |
| GESTEP | observed | unresolved: flat quantized windows | unresolved | 0, 0 | unmeasured |
| IMCOSH | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| IMCOT | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| IMCSCH | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| IMDIV | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| IMLOG10 | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| IMLOG2 | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| IMSECH | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| IMSINH | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| IMTAN | observed | not identifiable on non-numeric/error output | unresolved | 0, 0 | unmeasured |
| ISEVEN | workbook-only | unresolved | unresolved | — | unmeasured |
| ISODD | workbook-only | unresolved | unresolved | — | unmeasured |
| Infix & | workbook-only | unresolved | unresolved | — | unmeasured |
| Infix + | workbook-only | unresolved | unresolved | — | unmeasured |
| Infix - | workbook-only | unresolved | unresolved | — | unmeasured |
| JIS | workbook-only | unresolved | unresolved | — | unmeasured |
| MEDIAN | observed | unresolved: flat quantized windows | unresolved | 0, 0 | unmeasured |
| MINVERSE | workbook-only | unresolved | unresolved | — | unmeasured |
| N | workbook-only | unresolved | unresolved | — | unmeasured |
| NOT | workbook-only | unresolved | unresolved | — | unmeasured |
| ODD | workbook-only | unresolved | unresolved | — | unmeasured |
| QUOTIENT | workbook-only | unresolved | unresolved | — | unmeasured |
| RADIANS | observed | unresolved: no complete scalar mathematical reference | unresolved | 8, 8 | unmeasured |
| ROWS | workbook-only | unresolved | unresolved | — | unmeasured |
| RRI | observed | unresolved: no complete scalar mathematical reference | unresolved | 6, 5 | unmeasured |
| SIGN | workbook-only | unresolved | unresolved | — | unmeasured |
| SQRT | observed | double-round mathematical leaf consistent | unresolved | 8, 8 | unmeasured |
| STANDARDIZE | observed | single/double indistinguishable | unresolved | 8, 8 | unmeasured |
| TEXTSPLIT | workbook-only | unresolved | unresolved | — | unmeasured |
| WEEKNUM | observed | unresolved: flat quantized windows | unresolved | 0, 0 | unmeasured |
| LN | observed | single/double indistinguishable | unresolved | 7, 7 | unmeasured |
| LOG | observed | double division in the public-LN quotient model; ideal leaf rejected | unresolved | 8, 8 | unmeasured |
| LOG10 | observed | ideal single/double leaf rejected on this window | unresolved | 8, 8 | unmeasured |
