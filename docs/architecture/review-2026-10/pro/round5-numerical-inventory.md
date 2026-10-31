Owner correction inventory, 2026-10-31 (dc-j9k). Scope: b0e7b3b bulk numerical import through b952767 and the inherited round-4 working tree; incoming master through dd2d82c was checked for numerical changes (none). All 97 later non-merge source commits were screened (96 in the frozen baseline plus incoming UDF metadata); the 56 rows below split numerical program changes from semantic/metadata/test-only changes. Category a identifies a program element or a published method, not proof that an entire Excel function is identified. Category b uses sampled output agreement to choose a coefficient, cutoff or alternative body. Old restricted/deleted implementations and their code history were not read or restored. Current permitted replacements were inspected.

Count column: effects of this whole rollback on the same frozen full bank, grouped by function; overlapping historical commits are not separately additive. A retained element has zero direct rollback effect even if a shared body changes its caller. No per-commit counterfactual score was manufactured.

| Commit | Function | Kind and basis | Disposition | Count change |
|---|---|---|---|---|
| b0e7b3b | GAMMALN B2 | b: LM-refitted coefficients imported in bulk | removed; published SPECFUN P2/Q2 | GAMMALN family -132 combined with subset removal |
| b0878cc | GAMMA / GAMMALN / ERFC / inverse normal | a/b: published bodies plus seed tables, allowlists, ERFC fit and inverse-region selection | a retained; b removed | family totals below; not separable per commit |
| c82c1d0 | regression baseline | a: published normal-equations method | superseded by bf5472b | 0 direct rollback |
| 87976af | financial baseline | a: published annuity and root methods | retained except later fitted changes | 0 direct rollback |
| 34ea583 | POWER and ^ | a: integer/power-chain dispatch and stores | retained | 0 direct rollback |
| 1687491 | SKEW, Z.TEST, GEOMEAN | a: operation association; product/root method | retained; GEOMEAN superseded by 0ab2535 | 0 direct rollback |
| e0a5b8b | dates and database functions | a: calendar/day-count and arithmetic port | retained | 0 direct rollback |
| db6dbf7 | arithmetic and folds | a: PC64 arithmetic then binary64 stores | retained | 0 direct rollback |
| bf5472b | LINEST/LOGEST/TREND/GROWTH | a: centred pivoted Householder QR and publication | retained | 0 direct rollback |
| 6c614fc | AMOR*, DISC, INTRATE, RECEIVED, PRICEDISC | a: operation order and actual/actual year | retained | 0 direct rollback |
| 427e3e4 | IPMT/PPMT/YIELDDISC | a: backward annuity and grouped year ratio | retained | 0 direct rollback |
| 9557cfe | variance and standard-deviation families | b: one/two-pass switch fitted to a loose cancellation bracket at 1% | removed; original two-pass method | VAR/STDEV direct family -3729 |
| 0ab2535 | GEOMEAN | a: root in resident x87 | retained | 0 direct rollback |
| 4647ad6 | PERCENTRANK forms | a: decimal-place truncation | retained | 0 direct rollback |
| e2f6061 | TRIMMEAN | a: original input accumulation order after selection | retained | 0 direct rollback |
| 9c48f15 | MOD/MROUND/ATAN2/CEILING/FLOOR | a: remainder/multiply/order and directly measured domain predicate | retained | 0 direct rollback |
| 98046b0 | software x87 and GAMMALN stores | a: explicit precision, control word and store model | retained | 0 direct rollback |
| adf6780 | CRT pow callers | a: published Arm pow/configuration; current provenance corrected by 7ade800 | current permitted implementation retained | 0 direct rollback |
| ece3e30 | LINEST/LOGEST statistics and rank | a/b: QR-derived statistics; fitted COLLINEAR=4.5e-13 | statistics retained; fitted rank test removed; exact-zero elimination remains | 0 full-bank change; corpus counted separately |
| 0f411cb | CRT log/log10/exp callers | a: current published AMD routines and operation order | current permitted implementation retained | 0 direct rollback |
| db30f10 | CRT trig/hyperbolic/atan2/hypot callers | a: current published AMD routines and operation order | current permitted implementation retained | 0 direct rollback |
| ad7b366 | CUMIPMT/CUMPRINC | a: shared PMT/PPMT route and accumulation | retained | 0 direct rollback |
| 8703647 | arithmetic underflow and related admission | a: explicit subnormal publication and surface rules | retained | 0 direct rollback |
| cbe24a1 | RATE/YIELD/ODDFYIELD | a: Newton/bisection schedules to binary64 convergence | retained; no comparison of approximation bodies | 0 direct rollback |
| eb28d21 | RRI and error propagation | a: exact sign/domain predicates | retained | 0 direct rollback |
| 7ade800 | EXP/LN/POW substrate provenance | a: published Arm source replacement; no fitted coefficient introduced | current permitted source retained; removed source not read | 0 direct rollback |
| bf8ab31 | CRT library contracts | a: published AMD tables, FMA placements and operation order | retained; removed source not read | 0 direct rollback |
| 98daf86 | INT/decimal rounding/QUOTIENT and endpoints | a: decimal-15 reading, integer boundary and explicit operation order | retained | 0 direct rollback |
| 5540392 | Bessel J/Y and I recurrence | a/b: A&S polynomials/recurrences; Excel-fitted J/Y coefficients/phases and recurrence constants | A&S I0/I1/K0/K1 retained; fitted parts, I_LIMIT and phase cutoff removed; libm and defining I_n series | BESSELI -1030; BESSELJ -793; BESSELY absent from full bank, included in corpus |
| 141f446 | EXP/LN/LOG10/root/binomial exponent chains | a: documented x87 instruction graph | retained; removed source not read | 0 direct rollback |
| 7e7f1ac | discrete distributions | a: published Loader saddle-point and incomplete-gamma methods | retained; removed source not read | 0 direct rollback |
| 617bc56 | IRR | a: bounded ordered-float plateau exploration; 64-bit representation bound | retained | 0 direct rollback |
| 766a0df | ATANH/COMBIN/COMBINA/EXPON/GAMMA.DIST | a: measured admission, internal expm1 graph and PC64 stores | retained | 0 direct rollback |
| c411598 | ERFC/GAUSS/GAMMA.DIST/IRR/PMT/MOD/GAMMALN/rounding | a: admission/flush, shape-one log normalizer, nonzero derivative and stores | retained | 0 direct rollback |
| 4a96776 | GAMMA.DIST CDF | a: final series product stored from PC64 | retained | 0 direct rollback |
| 90b3fa3 | PMT/MOD | a: final product store; binary-power divisor dispatch of raw underflow | retained | 0 direct rollback |
| 7b79331 | lognormal aliases | a: internal CRT logarithm rather than worksheet LN | retained | 0 direct rollback; later caller changes belong to ERFC rollback |
| 6b83980 | tiny ERF and GAUSS | b: data-derived P0/Q0 and unestablished 1e-8 cutoff | removed; store constraints retained as clues | ERF.PRECISE -364549 and GAUSS -651, combined with later ERF fits |
| 6b229e4 | two-cashflow IRR | a: q-space secant/Newton method, exact two-flow condition and fixpoint | retained as scoped program hypothesis | 0 direct rollback |
| 4c44a60 | MOD | b: 2^26/2^-1026 bounds and reversed adjustment chosen from answer agreement | removed | MOD -4974 |
| c677e0b | BINOM.DIST | a: CRT logarithm in normalizer | retained | 0 direct rollback |
| 7ae9012 | small ERF | b: fitted rational coefficients and 0.01 cutoff | removed | included in combined ERF-family count above |
| 9708d03 | small ERF | a/b: stores inside the fitted rational | fitted body removed; store distinctions retained as clues | included in combined ERF-family count above |
| e908063 | binomial/negative-binomial/beta | a: PC64 product/carry, exact equality branch, published 15-digit NSWC constant | retained | 0 direct rollback |
| 4f145a3 | beta and F callers | a: exact existing-branch equality at 2 and 0.375 | retained | 0 direct rollback |
| df1215e | GAMMA | b: two fitted Lanczos coefficients | already withdrawn by 17e42ce; published coefficients remain | 0 this round; historical +2 then -2 |
| 17e42ce | ERFC and PMT | a/b: PMT quotient store; ERFC fitted residual bound 2^-48 | PMT retained; ERFC fit removed | ERFC and normal/lognormal caller totals below |
| 0f2cc3d | GAMMA/PDF and ERFC | b: 16/32-ULP body selection and fitted 2^-49 ERFC bound | removed; public-method provenance retained | GAMMA combined -1175; GAMMA.DIST combined -5596 |
| 2546388 | GAMMA reflection | a/b: reduced reflection candidate selected by disagreement bound | bound-driven dispatch removed; reduction remains a research clue | included in combined GAMMA count |
| 1e636dd | ERFC and normal tails | b: fitted 2^-50 residual bound | removed | ERFC -59; ERFC.PRECISE -100; caller totals below |
| 8db35cb | GAMMA | b: positive library disagreement bound 8 ULP | removed | included in combined GAMMA count |
| cbdbb7f | PMT, ERFC, Gamma contexts | a/b: exact opposing-balance identity; square store/context refinements inside fits | PMT retained; fitted wrappers removed | PMT 0; other effects included in family totals |
| 480481e | GAMMA.DIST CDF | b: shape-2/3 residual correction with fitted strict 2^-51 bound | removed; final PC64 product retained | included in GAMMA.DIST -5596 |
| 2335ccb | Poisson count one | a: published exponential series with analytically derived 2^-27 error bound | retained; no Excel algorithm identification claimed | 0 direct rollback; historical three gains are one semantic input |
| 71b5de2 | PMT | a: finite-overflow rearrangement under exact balance identity | retained; extra controls are mathematical, not Excel | 0 direct rollback |
| uncommitted round 4 | small GAMMA | b: published Taylor series selected by fitted 4-ULP disagreement | removed; patch and coefficients archived | included in combined GAMMA -1175; before includes its +471 |

Additional retained measurement repairs: b820677 (call/result identity, decoding and duplicate lineage) and 485c4b6 (GROWTH prediction-column layout) change the interpretation of recorded calls, not numerical kernels. Their exact metadata conditions are category a; rollback count change 0. Evidence-only source-external commits and capture refreshes were screened separately; no numerical kernel changes were found. 5a9348b adds UDF volatility contract fields, with no arithmetic change.

Other source commits screened (semantic, text/layout, catalogue, licensing metadata, representation, UDF or tests; no fitted numerical kernel change):
db130aa dc-nzpw: IS functions, TYPE, N, NA and ERROR.TYPE as calc-func family 10
058ef2d dc-51ka: INDIRECT text resolved to references through calc_fml
60f54b2 dc-xrxb: file-prefix metadata in calc_func and load-origin compile in calc_fml
7c6677d dc-8dq: file prefixes and Excel's prefix capture for the math family
636d26d dc-mzk: arrays, lifting, reference operators, names, LET and LAMBDA in calc_fml
015f25b dc-ip6: text functions, IFS and SWITCH, and F2's catalogue rows
f44df1f dc-ip6: the calculation environment and host facts through calc-fml, the engine and the host
755fe42 dc-ip6: BAHTTEXT, and 182 F2 rows in the catalogue oracle
770f227 dc-728: dynamic-array functions and the LAMBDA helpers
6d8a7f0 dc-q4y: editor assistance in calc-fml and function help text in calc-func
77b2f75 dc-q4y: OneCalc calculates with a date system, locale and clock; numbers shown as a cell shows them; a smaller bundle
64ca039 dc-m3u: structured references, 3-D references, external refusals, relative names, #NAME? calls, measured legacy places
165ee30 calc-func: rename locale and host-fact identifiers the token check reads as the old reference language
c8c779e dc-ns0: calc-func's MSVC-libm witnesses run on MSVC only; SQRTPI's off-Windows ULP filed as dc-sbo
46540d0 dc-j9k: retag stale owner names in refusals and test notes to the current packages
a55c79c dc-uar: ENCODEURL percent-encodes UTF-8 as Excel does
65214a6 dc-msg: calc-func: run the unit and F1-F3 tests in Chrome and Firefox
84864bb dc-uar: TEXT with fraction codes and with decimal places of seconds
bea7a7e dc-10y: cycles, iteration from stored values, data tables and iteration in the capture, 3-D and legacy-place tables, workbook-qualified INDIRECT, typed entry under a locale
0f9f15d dc-uar: TEXT with literals among grouped digits and among decimal places
6ab91ef dc-6au: function results dc-10y's 3-D and legacy-place sheets exposed, measured in Excel
dd0990c dc-09v: calc-func: cell_text, a cell's value under its number format
702a265 dc-09v: number format colors ([Red]) through calc-func's format sections
3ea199e dc-09v: palette colors, #### for formatted numbers too wide, text overflow
79b7b06 dc-b1l: TEXT writes a group separator beside a literal as Excel does
3381257 dc-6yw: ROW() and COLUMN() without a reference give the caller's row and column
372914d dc-dxm: compact value type step 1 in calc-func and calc-host: every match on a value's core goes through CoreRef
9528e60 dc-96l.6, dc-96l.1: provenance scan in cargo xtask check; the licence ledger in migration/
f358423 dc-96l.5: help.rs descriptions are our own; Microsoft's text replaced
903e693 dc-zsn.1: text compares and sorts beyond ASCII as Excel does: the UCA's root order with Windows' measured departures
2c9f613 dc-fl2: LEN and MID count UTF-16 code units, and CONCATENATE cuts at 32,767, as Excel does
ccf2ea6 dc-o9r, dc-b1l, dc-h9u: legacy formulas intersect TRANSPOSE's and TYPE's references and lift GROUPBY's and PIVOTBY's arrays everywhere
bcf7861 dc-h9u: PIVOTBY subtotals across several fields, function values saved as _xleta., catalogue rows for the legacy places
3f47990 dc-zsn.2: text compares over every UTF-16 code unit as Excel does; dc-zsn.3: the byte functions under en-ZA
3f7a3d5 dc-ylr: an external reference is a reference to the link's cached sheet; names, typed book names and .xlsb
5917aaa dc-96l.5: help.rs keeps Microsoft's official descriptions, attributed with their source
d42d235 dc-96l: source comments for the TOMS 654/708 and AS 241 code, which stays
7eaa840 dc-h9u: a GROUPBY or PIVOTBY cell holds its LAMBDA's array; PIVOTBY column subtotals beside several values; ROWS(GROUPBY(range)) saved ordinary; dc-ylr: INDIRECT naming another workbook is #REF!
0e373c6 dc-b1l: arrays of references from OFFSET and INDIRECT; aggregate functions below an array place in legacy formulas
66c5861 dc-ymjf.2: pin measured hardware-x87 results by CPUID vendor
379d51f dc-snz.3: evaluate VBA value UDFs through engine-free OneCalc
f1f8b44 dc-snz.5 retain VBA UDF state and use native JIT sessions

Full-bank per-function effects (same answers and identities):
[
  {
    "function": "BESSELI",
    "rows": 1236,
    "before": 1230,
    "after": 200,
    "delta": -1030
  },
  {
    "function": "BESSELJ",
    "rows": 794,
    "before": 793,
    "after": 0,
    "delta": -793
  },
  {
    "function": "CHIDIST",
    "rows": 388,
    "before": 239,
    "after": 234,
    "delta": -5
  },
  {
    "function": "ERF.PRECISE",
    "rows": 618437,
    "before": 617129,
    "after": 252580,
    "delta": -364549
  },
  {
    "function": "ERFC",
    "rows": 544,
    "before": 184,
    "after": 125,
    "delta": -59
  },
  {
    "function": "ERFC.PRECISE",
    "rows": 1863,
    "before": 834,
    "after": 734,
    "delta": -100
  },
  {
    "function": "GAMMA",
    "rows": 12506,
    "before": 1611,
    "after": 436,
    "delta": -1175
  },
  {
    "function": "GAMMA.DIST",
    "rows": 77975,
    "before": 28808,
    "after": 23212,
    "delta": -5596
  },
  {
    "function": "GAMMALN",
    "rows": 4525,
    "before": 2015,
    "after": 1999,
    "delta": -16
  },
  {
    "function": "GAMMALN.PRECISE",
    "rows": 14278,
    "before": 10309,
    "after": 10193,
    "delta": -116
  },
  {
    "function": "GAUSS",
    "rows": 9728,
    "before": 9220,
    "after": 8569,
    "delta": -651
  },
  {
    "function": "LOGNORM.DIST",
    "rows": 6304,
    "before": 2578,
    "after": 2440,
    "delta": -138
  },
  {
    "function": "LOGNORMDIST",
    "rows": 3299,
    "before": 799,
    "after": 666,
    "delta": -133
  },
  {
    "function": "MOD",
    "rows": 152432,
    "before": 152432,
    "after": 147458,
    "delta": -4974
  },
  {
    "function": "NORM.DIST",
    "rows": 24246,
    "before": 23965,
    "after": 23913,
    "delta": -52
  },
  {
    "function": "NORM.S.DIST",
    "rows": 27473,
    "before": 14522,
    "after": 13757,
    "delta": -765
  },
  {
    "function": "NORMDIST",
    "rows": 3005,
    "before": 2719,
    "after": 2703,
    "delta": -16
  },
  {
    "function": "STDEV",
    "rows": 1212,
    "before": 1181,
    "after": 944,
    "delta": -237
  },
  {
    "function": "STDEV.P",
    "rows": 1211,
    "before": 1190,
    "after": 919,
    "delta": -271
  },
  {
    "function": "STDEV.S",
    "rows": 1212,
    "before": 1178,
    "after": 950,
    "delta": -228
  },
  {
    "function": "STDEVA",
    "rows": 1210,
    "before": 1181,
    "after": 949,
    "delta": -232
  },
  {
    "function": "STDEVP",
    "rows": 1212,
    "before": 1182,
    "after": 936,
    "delta": -246
  },
  {
    "function": "STDEVPA",
    "rows": 1212,
    "before": 1181,
    "after": 926,
    "delta": -255
  },
  {
    "function": "VAR",
    "rows": 1212,
    "before": 1171,
    "after": 800,
    "delta": -371
  },
  {
    "function": "VAR.P",
    "rows": 1212,
    "before": 1155,
    "after": 770,
    "delta": -385
  },
  {
    "function": "VAR.S",
    "rows": 1313,
    "before": 1262,
    "after": 910,
    "delta": -352
  },
  {
    "function": "VARA",
    "rows": 1211,
    "before": 1173,
    "after": 792,
    "delta": -381
  },
  {
    "function": "VARP",
    "rows": 1210,
    "before": 1161,
    "after": 791,
    "delta": -370
  },
  {
    "function": "VARPA",
    "rows": 1212,
    "before": 1167,
    "after": 766,
    "delta": -401
  }
]
