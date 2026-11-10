# Question for GPT-6 Pro: reconstruct Excel's FORECAST.ETS family (FORECAST.ETS, .CONFINT, .SEASONALITY, .STAT) exactly

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet that must reproduce Excel
bit for bit (identical binary64 results, the same error values). The repository is public:
https://github.com/DnaCalc/DnaCalc, MIT. This question is pinned to commit
`1ece3d0ffeae28f27aa0ff28c2d873e3a6c2af84`
(https://github.com/DnaCalc/DnaCalc/tree/1ece3d0ffeae28f27aa0ff28c2d873e3a6c2af84). Every
`file:line` below refers to that commit. You may read the repository if you can; everything you
need is in the appendices, so do not depend on browsing.

The four ETS functions are the largest block of cells we still refuse. In our oracle corpus
(5,240 ETS formula cells in three recorded workbooks, every answer an Excel bit pattern) **952
cells are refused** (FORECAST.ETS 256, FORECAST.ETS.CONFINT 256, FORECAST.ETS.SEASONALITY 368,
FORECAST.ETS.STAT 72), 31 more are last-bit mismatches, and the other 4,257 match Excel exactly.
A refused cell counts as a failure, exactly like a wrong answer. Research rounds 46 to 49
(automated agents probing Excel for hours each) found no further gain: they reconstructed a lot
(appendix D) but stalled on four stages (automatic seasonality detection, the short-history
initialiser, the seasonal initialiser and search, and a few cycle limits). The counts are recorded
in `docs/generated/problem-functions.md` lines 18 to 22 (`XLFN.FORECAST.ETS*` rows) and
reproduced by `cargo xtask corpus`; the refusal reasons are decided in
`crates/calc-func/src/catalogue/statistics/ets.rs` (appendix B). The recorded Excel answers are in
`crates/calc-func/tests/captures/dcj9k_r38_ets`, `dcj9k_r39_ets`, `dcj9k_r39_seasonal` (JSON
`observations` with `excel_raw` bit patterns; formulas in `crates/calc-func/tests/fixtures/<name>/sheet-<name>.txt`).
The oracle is Excel 16.0, build 20527, 64-bit Windows, on an AMD Ryzen 7 4800H; our answer service
returns the bits of any single formula in about 0.2 s, so hundreds of designed questions per
experiment are cheap.

## What we ask

Please produce, in this order:

**A. The most plausible complete algorithm Excel uses.** Reconstruct the full AAA (additive error,
additive trend, additive seasonal) exponential-smoothing machinery end to end: timeline and
aggregation preparation, seasonality detection (what `FORECAST.ETS.SEASONALITY` and the automatic
mode of the others compute), initial states for the nonseasonal and the seasonal model, the
objective, the optimiser (method, parameter bounds, starting points, candidate set or iteration
scheme, stopping rule, tie rule), the post-fit corrections (the "reset"), forecasting, the
confidence interval, and every `FORECAST.ETS.STAT` output. Separate, line by line, **what the data
in the appendices pins down**, **what is inferred** (and from which datum), and **what you
recall from public sources** (say which; see the source rules). Keep the parts that are already
measured (appendix D, tagged MEASURED) unless the data contradicts them, and say so when it does.

**B. Ranked alternative hypotheses for every uncertain stage, with discriminating experiments.**
For each uncertain stage (at least: how the visited (alpha, beta) point set of appendix E.1 is
generated and why every reported value has at most six significant digits; the score and its tie
rule; seasonality detection; the short-history initial states; the seasonal initial states; the
seasonal gamma search; the cycle limits for explicit periods), give 2 to 4 ranked hypotheses
(ranked by "what a C programmer in the 1990s to 2010s would have typed", with reasons), and for
each ranking step a concrete experiment: a specific input series (numbers, timeline, options) we
can enter in Excel, the quantity we read (SEASONALITY integer, STAT 1 to 8, a forecast at chosen
horizons, a confidence interval) and the outcome each hypothesis predicts. Prefer experiments whose
outcome is a discrete, easy-to-read fact (a detected period, which of the table's points wins,
which side of a tie wins) over last-bit arithmetic.

**C. An implementable specification for the refused cells**, for the four first-refusal blocks of
appendix C: automatic detection (528 cells), short initialisation (204), nonzero seasonal
innovations (180), cycle limits (40). For each: the exact computation (formulas, evaluation order,
constants, branch thresholds, bounds, candidate sets, tie rule) in pseudocode precise enough for a
Sonnet-class agent to implement and test bit for bit against recorded answers. Wherever the
specification is only a hypothesis, mark it, and state the observation in appendix E that it must
reproduce. Predict, for at least ten of the histories listed in E.3 to E.5, the discrete outputs
your specification gives (detected period, alpha, beta), so that we can score it before
implementing anything.

**D. Bold ideas.** In particular: how to use `FORECAST.ETS.STAT` (alpha, beta, gamma, MASE, SMAPE,
MAE, RMSE), which exposes the chosen parameters, and the forecasts at chosen horizons (which expose
final level, trend and seasonal states), to recover the optimiser's path or the set of points it
visits; how to build series whose candidate scores tie exactly or differ by one ulp to read the
tie rule and the accumulation order; how to exploit timeline and aggregation options, magnitude
extremes, and `#NUM!` thresholds as side channels; and anything you think would shortcut this.

## Constraints that bind the answer

1. **Bit-exact parity with Excel is the goal**; the answer is judged by recorded Excel answers. "Close"
   or "statistically equivalent" is worth nothing, except as a step to an exact shape.
2. **Source rules** (`docs/plan/RESEARCH_SOURCES.md`): tier 1 (free to read): Abramowitz and Stegun,
   DLMF, Hart et al., published papers, textbook mathematics (Hyndman and Athanasopoulos, Holt-Winters,
   Hyndman et al. 2008 state-space ETS), Intel SDM and IEEE 754, and **Excel's own outputs**; tier 2
   (permissive code): Boost.Math, fdlibm, openlibm, SLATEC, SPECFUN and similar; tier 3 (facts only):
   Numerical Recipes, R nmath, glibc libm, other ACM TOMS code; tier 4 (never used): Microsoft source,
   disassembly or decompilation of Excel or Microsoft runtime DLLs. **For this answer: you may use
   tier 1 and 2 freely. Treat R's `forecast`/`ets`/`HoltWinters`, statsmodels, LibreOffice Calc's
   FORECAST.ETS implementation and any other copyleft or unlisted implementation as tier 3: give
   facts only (the method family, published origin, constants as numbers, observed behaviour), never
   code, structure, identifiers or comments.** Microsoft's public documentation and blog posts
   (for example the 8 May 2014 Power View article on its forecasting models, which the notes below
   mention) are fine to use as published descriptions. Never use or request tier 4 material.
   Black-box reasoning from Excel's outputs is the main method.
3. **Method: the "code shape" hypothesis.** Excel's numeric kernels are mostly simple hand-written
   C (double arithmetic, some x87 habits: several kernels are measured to compute in a 64-bit
   mantissa and store to binary64, which we write PC64 then RN53). Rank candidate computations by what
   someone would plausibly have typed; find the shape first, then fit constants. A match percentage
   alone is not evidence of the right shape (earlier rounds fitted tables to answers and were
   refuted by fresh answers).
4. **You cannot run anything.** So ask for experiments; we run them and feed the results back.
5. **Implementable:** for each proposed change, the exact computation (formula, evaluation order,
   constants, branch thresholds) in prose or pseudocode.
6. The recorded data are real; none was invented. Series in E.2 to E.5 are integers in units of
   1/16 (every value is a multiple of 1/16; timelines are 1..n unless stated).

---

# Appendix A: the functions as measured

Signatures (Excel): `FORECAST.ETS(target_date, values, timeline, [seasonality], [data_completion],
[aggregation])`; `FORECAST.ETS.CONFINT(target_date, values, timeline, [confidence_level=0.95],
[seasonality], [data_completion], [aggregation])`; `FORECAST.ETS.SEASONALITY(values, timeline,
[data_completion], [aggregation])`; `FORECAST.ETS.STAT(values, timeline, statistic_type,
[seasonality], [data_completion], [aggregation])`. Seasonality 1 (default) = automatic, 0 = none,
integer >= 2 = that period. STAT types: 1 alpha, 2 beta, 3 gamma, 4 MASE, 5 SMAPE, 6 MAE, 7 RMSE,
8 step size.

MEASURED (rounds 37 to 48, Excel 20527; "R" numbers are research rounds, appendix D):

- Domain and error order: lengths of values and timeline differ -> `#N/A` (before option checks);
  statistic not an integer 1..8, confidence not in [0,1), seasonality not an integer in [0, 8784] (8784
  last admitted, 8785 refused, bisected), completion not 0/1, aggregation not an integer 1..7 -> `#NUM!`;
  fewer than 2 observations -> `#DIV/0!`; fewer than 2 distinct timestamps -> `#NUM!`; more than 30%
  of the prepared timeline missing -> `#NUM!`; target before the first timestamp -> `#NUM!` (ascending
  input); CONFINT target <= last timestamp -> `#NUM!`; future index above 2^31-1 steps -> `#NUM!`.
- Timeline: sorted ascending (unordered input is accepted), duplicate timestamps aggregated with
  aggregation 1 AVERAGE, 2 COUNT, 3 COUNTA, 4 MAX, 5 MEDIAN, 6 MIN, 7 SUM; step = smallest gap; all gaps
  integer multiples of it (non-integral: calendar-month inference, NOT measured, we refuse); missing
  points filled by data_completion 1 = linear interpolation, 0 = zeros. STAT 8 = step.
- Nonseasonal fit (seasonality 0, or the automatic mode when detection returns 0 or 1): the reported
  gamma is always exactly 2^-52 (DBL_EPSILON); reported alpha and beta are always one of 30 pairs
  (E.1); the recorded behaviour is reproduced by the recurrence and candidate table of appendix B.
- SEASONALITY returns 0 for 2 to 4 prepared observations (R46: every case, 1,653 short controls). For
  FORECAST/CONFINT/STAT, an explicit period 2 at length 4 gives the same answers as seasonality 0
  (R47: 256 paired histories), and our code treats an explicit period greater than half the prepared
  length as nonseasonal (`season > n/2`, R38).
- Zero-noise seasonal histories (exact affine-plus-periodic) return alpha 0.751, beta 0.001, gamma
  0.001 and all error statistics 0 (so with SSE exactly 0 the optimiser returns its first or default
  point: 0.751/0.001/0.001 seasonal, 0.9/0.001/2^-52 nonseasonal).

GUESSED / OPEN: everything about automatic detection beyond length 4, the short initialiser, the
seasonal initialiser and the seasonal gamma/alpha/beta search, the generating procedure of the
candidate points, calendar timelines, more than 128 prepared points (never tested).

---

# Appendix B: our implementation (what it does, what it refuses, and why)

`ets.rs` is 919 lines. Routing in `evaluate` (lines 765 to 919) validates options, prepares the
series, then chooses: exact-affine shortcut (`exact_trend`, lines 230 to 268), the nonseasonal fit
(`nonseasonal`, lines 662 to 763), or the exact seasonal region (`exact_seasonal`, lines 536 to 660).
Helper predicates not reproduced (they only decide "is every intermediate sum and product exact in
binary64", so that results can be returned without knowing Excel's rounding order): `exact_ratio`
(61 to 64), `exact_total` (68 to 83: all subset sums on one binary lattice), `quantum_exponent`
(217 to 228), `exact_sum` (273 to 278, TwoSum), `exact_regression_slope` and `exact_short_history`
(342 to 378). A refusal is `NotHeld{owner: "dc-j9k"}`; it is a failure in every statistic.

How refusals arise (this is the census used throughout; I reproduced the control flow offline and it
gives exactly 256/256/368/72 refusals on the three recorded workbooks and exactly the tracker's four
first-refusal blocks 528/204/180/40):

| block | cells | where decided | meaning |
|---|---:|---|---|
| automatic detection | 528 (SEASONALITY 368, FORECAST 80, CONFINT 80) | `evaluate` line 884-891: affine test failed, `season == 1` (automatic) and `exact_seasonal(period 2, automatic)` refused or fell through | the series is not exactly affine and has at least 5 prepared points; detection is unknown |
| short initialisation | 204 (FORECAST 66, CONFINT 66, STAT 72) | `nonseasonal` line 687: `exact_short_history` is `None` | fewer than 8 prepared points, nonexact regression/extension, nonseasonal fit needed |
| nonzero seasonal innovations | 180 (FORECAST 90, CONFINT 90) | `exact_seasonal` line 622 | explicit period, nonexact seasonal noise: initial seasonal states and gamma/alpha/beta search unknown |
| cycle limits | 40 (FORECAST 20, CONFINT 20) | `exact_seasonal` line 545-555 | explicit period 2 at length 4 (the histories {2,7,-1,4} and {2,3.5,3.75,5.75}) with non-affine data |

All histories in the refused cells are at most 32 prepared observations; there are 106 distinct
histories behind the 952 refused cells (of 142 distinct histories in the 5,240 cells).

## B.1 Preparation (lines 85 to 212)

```rust
// crates/calc-func/src/catalogue/statistics/ets.rs lines 85-212
  85  fn prepare(
  86      site: &Site<'_>,
  87      y: Vec<f64>,
  88      t: Vec<f64>,
  89      completion: f64,
  90      aggregation: f64,
  91  ) -> Step<Series> {
  92      if y.len() != t.len() {
  93          return fail(WorksheetErrorCode::NA);
  94      }
  95      if y.len() < 2 {
  96          return fail(WorksheetErrorCode::Div0);
  97      }
  98      let ordered = t.windows(2).all(|p| p[0] <= p[1]);
  99      let mut pairs: Vec<_> = t.into_iter().zip(y).collect();
 100      pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
 101      let mut times = Vec::new();
 102      let mut values = Vec::new();
 103      let mut preparation_exact = true;
 104      let mut i = 0;
 105      while i < pairs.len() {
 106          let start = i;
 107          let time = pairs[i].0;
 108          while i < pairs.len() && pairs[i].0 == time {
 109              i += 1;
 110          }
 111          let mut group: Vec<_> = pairs[start..i].iter().map(|p| p.1).collect();
 112          if group.len() > 1 {
 113              let exact = match aggregation as u8 {
 114                  1 => exact_total(&group)
 115                      .and_then(|s| exact_ratio(s, group.len() as f64))
 116                      .is_some(),
 117                  7 => exact_total(&group).is_some(),
 118                  _ => true,
 119              };
 120              preparation_exact &= exact;
 121          }
 122          let value = match aggregation as u8 {
 123              1 => group.iter().sum::<f64>() / group.len() as f64,
 124              2 | 3 => group.len() as f64,
 125              4 => group.iter().copied().fold(f64::NEG_INFINITY, f64::max),
 126              5 => {
 127                  group.sort_by(f64::total_cmp);
 128                  let m = group.len() / 2;
 129                  if group.len() % 2 == 0 {
 130                      preparation_exact &= exact_total(&group[m - 1..=m])
 131                          .and_then(|s| exact_ratio(s, 2.0))
 132                          .is_some();
 133                      (group[m - 1] + group[m]) / 2.0
 134                  } else {
 135                      group[m]
 136                  }
 137              }
 138              6 => group.iter().copied().fold(f64::INFINITY, f64::min),
 139              7 => group.iter().sum(),
 140              _ => unreachable!(),
 141          };
 142          times.push(time);
 143          values.push(value);
 144      }
 145      if values.iter().any(|v| !v.is_finite()) {
 146          return Err(unfinished(site));
 147      }
 148      if times.len() < 2 {
 149          return fail(WorksheetErrorCode::Num);
 150      }
 151      let step = times
 152          .windows(2)
 153          .map(|p| p[1] - p[0])
 154          .fold(f64::INFINITY, f64::min);
 155      if !step.is_finite() || step <= 0.0 {
 156          return Err(unfinished(site));
 157      }
 158      let mut gaps = Vec::new();
 159      let mut count = 1usize;
 160      for pair in times.windows(2) {
 161          let gap = (pair[1] - pair[0]) / step;
 162          // Calendar-month inference and almost-integral timestamps need separate
 163          // measurements. Never turn an unidentified timeline into a worksheet error.
 164          if gap.fract() != 0.0 || gap < 1.0 || gap > 1_000_000.0 {
 165              return Err(unfinished(site));
 166          }
 167          count = count
 168              .checked_add(gap as usize)
 169              .ok_or_else(|| unfinished(site))?;
 170          gaps.push(gap as usize);
 171      }
 172      if count > 128 {
 173          return Err(unfinished(site));
 174      }
 175      if (count - times.len()) * 10 > count * 3 {
 176          return fail(WorksheetErrorCode::Num);
 177      }
 178      let mut completed = Vec::with_capacity(count);
 179      completed.push(values[0]);
 180      for (i, &gap) in gaps.iter().enumerate() {
 181          for j in 1..gap {
 182              if completion != 0.0 {
 183                  let exact = exact_sum(values[i + 1], -values[i]).and_then(|difference| {
 184                      let fraction = exact_ratio(j as f64, gap as f64)?;
 185                      let product = difference * fraction;
 186                      if difference.mul_add(fraction, -product) != 0.0 {
 187                          return None;
 188                      }
 189                      exact_sum(values[i], product)
 190                  });
 191                  preparation_exact &= exact.is_some();
 192              }
 193              completed.push(if completion == 0.0 {
 194                  0.0
 195              } else {
 196                  values[i] + (values[i + 1] - values[i]) * (j as f64 / gap as f64)
 197              });
 198          }
 199          completed.push(values[i + 1]);
 200      }
 201      if completed.iter().any(|v| !v.is_finite()) {
 202          return Err(unfinished(site));
 203      }
 204      Ok(Series {
 205          values: completed,
 206          first: times[0],
 207          last: *times.last().unwrap(),
 208          step,
 209          ordered,
 210          preparation_exact,
 211      })
 212  }
```


## B.2 Candidate table, regression slope, short extension (lines 280 to 338)

```rust
// crates/calc-func/src/catalogue/statistics/ets.rs lines 280-338
 280  // R39's fixed parameter family, measured before the independent random fits.
 281  // The later trend correction can report beta .001 for other alpha values;
 282  // those corrected pairs must not become additional optimisation candidates.
 283  const PARAMETERS: &[(f64, f64)] = &[
 284      (0.9, 0.001),
 285      (0.9, 0.899),
 286      (0.833333, 0.75),
 287      (0.75, 0.001),
 288      (0.75, 0.749),
 289      (0.666667, 0.166667),
 290      (0.666667, 0.5),
 291      (0.5, 0.001),
 292      (0.5, 0.25),
 293      (0.5, 0.499),
 294      (0.333333, 0.166667),
 295      (0.25, 0.001),
 296      (0.25, 0.249),
 297      (0.166667, 0.0833333),
 298      (0.126, 0.001),
 299      (0.126, 0.125),
 300      (0.1, 0.001),
 301      (0.1, 0.099),
 302      (0.002, 0.001),
 303      (0.998, 0.099),
 304      (0.998, 0.499),
 305      (0.998, 0.749),
 306      (0.998, 0.899),
 307  ];
 308  
 309  fn regression_slope(y: &[f64]) -> f64 {
 310      let mut sx = 0.0;
 311      let mut sy = 0.0;
 312      let mut sxx = 0.0;
 313      let mut sxy = 0.0;
 314      for (i, &v) in y.iter().enumerate() {
 315          let x = (i + 1) as f64;
 316          sx += x;
 317          sy += v;
 318          sxx += x * x;
 319          sxy += x * v;
 320      }
 321      let n = y.len() as f64;
 322      (n * sxy - sx * sy) / (n * sxx - sx * sx)
 323  }
 324  
 325  /// R40's short-history impulse response identifies the first two phases plus
 326  /// the original regression trend as the extension to seven observations.
 327  /// The search runs on that extension; final state updates use the real history.
 328  fn initial_slope(y: &[f64]) -> f64 {
 329      if y.len() >= 7 {
 330          return regression_slope(y);
 331      }
 332      let trend = regression_slope(y);
 333      let mut extended = y.to_vec();
 334      for j in y.len()..7 {
 335          extended.push(y[j % 2] + (j - j % 2) as f64 * trend);
 336      }
 337      regression_slope(&extended)
 338  }
```


## B.3 The state recursion, reset ("finish") and statistics (lines 380 to 529)

Note: `Model::run` (lines 409-423) uses initial level `y[0]`, trend `initial_slope(y)`, two
seasonal states (period 2, all zero), gamma = 2^-52. The `period` in `Model` is 2 for the
nonseasonal fit: the "nonseasonal" model is a period-2 seasonal model whose seasonal states are
numerically negligible; this formulation reproduces Excel's forecast bits (see the centering at the end of `finish`). `excel_x87_add/sub/mul/div(a,b)` compute RN53(RN64(a op b))
(x87 precision control 64-bit mantissa then store to binary64).

```rust
// crates/calc-func/src/catalogue/statistics/ets.rs lines 380-529
 380  fn statistic_window(n: usize) -> usize {
 381      if n <= 10 {
 382          n
 383      } else {
 384          ((n as f64 * 0.4) as usize).min(10)
 385      }
 386  }
 387  
 388  struct Model {
 389      level: f64,
 390      trend: f64,
 391      seasons: [f64; 8],
 392      period: usize,
 393      errors: Vec<f64>,
 394      predictions: Vec<f64>,
 395      trends: Vec<f64>,
 396  }
 397  
 398  struct Initial {
 399      level: f64,
 400      trend: f64,
 401      seasons: [f64; 8],
 402      period: usize,
 403      gamma: f64,
 404  }
 405  
 406  impl Model {
 407      /// The application and parameter search use this same recurrence. During
 408      /// validation only the transition runs; observations no longer update it.
 409      fn run(y: &[f64], alpha: f64, beta: f64, validation: Option<usize>) -> Self {
 410          Self::run_initialized(
 411              y,
 412              alpha,
 413              beta,
 414              validation,
 415              Initial {
 416                  level: y[0],
 417                  trend: initial_slope(y),
 418                  seasons: [0.0; 8],
 419                  period: 2,
 420                  gamma: f64::EPSILON,
 421              },
 422          )
 423      }
 424  
 425      fn run_initialized(
 426          y: &[f64],
 427          alpha: f64,
 428          beta: f64,
 429          validation: Option<usize>,
 430          initial: Initial,
 431      ) -> Self {
 432          let mut model = Self {
 433              level: initial.level,
 434              trend: initial.trend,
 435              seasons: initial.seasons,
 436              period: initial.period,
 437              errors: vec![0.0],
 438              predictions: vec![y[0]],
 439              trends: Vec::with_capacity(y.len() - 1),
 440          };
 441          for (j, &v) in y.iter().enumerate().skip(1) {
 442              let q = j % model.period;
 443              let prediction = (model.level + model.trend) + model.seasons[q];
 444              let error = v - prediction;
 445              model.errors.push(error);
 446              model.predictions.push(prediction);
 447              if validation.is_some_and(|start| j >= start) {
 448                  model.level += model.trend;
 449              } else {
 450                  model.level += model.trend + alpha * error;
 451                  model.trend += beta * error;
 452                  model.seasons[q] += initial.gamma * error;
 453              }
 454              model.trends.push(model.trend);
 455          }
 456          model
 457      }
 458  
 459      fn squared_errors(&self) -> f64 {
 460          self.errors.iter().fold(0.0, |sum, &v| sum + v * v)
 461      }
 462  
 463      fn finish(&mut self, y: &[f64], beta: &mut f64) -> bool {
 464          // This window is independent of STAT's tail. It retains the transitions
 465          // within the final half of the observations, including its first state.
 466          let window = if y.len() <= 10 {
 467              y.len()
 468          } else {
 469              y.len().div_ceil(2)
 470          };
 471          let count = window - 1;
 472          let trends = &self.trends[self.trends.len() - count..];
 473          use crate::ported::excel_numeric::{
 474              excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul, excel_x87_sub as sub,
 475          };
 476          // R46 trend-shift questions separate the selected tail from the
 477          // divisors. Odd histories retain ceil(n/2)-1 transitions but use
 478          // floor(n/2)-1 in both raw moments. Centered variance is a different
 479          // computation here, including its cancellation on near-flat trends.
 480          let divisor_window = if y.len() <= 10 { y.len() } else { y.len() / 2 };
 481          let divisor = (divisor_window - 1) as f64;
 482          let total = trends.iter().fold(0.0, |sum, &v| add(sum, v));
 483          let squares = trends.iter().fold(0.0, |sum, &v| add(sum, mul(v, v)));
 484          let mean = div(total, divisor);
 485          let variance = sub(div(squares, divisor), mul(mean, mean));
 486          let reset = mean.abs() < div(mul(variance.sqrt(), divisor_window as f64), divisor);
 487          if reset {
 488              self.level = *y.last().unwrap() - self.seasons[(y.len() - 1) % 2];
 489              self.trend = initial_slope(y);
 490              *beta = 0.001;
 491          }
 492          let center = (self.seasons[0] + self.seasons[1]) / 2.0;
 493          self.level += center;
 494          for season in &mut self.seasons[..self.period] {
 495              *season -= center;
 496          }
 497          reset
 498      }
 499  
 500      fn statistics(&self, y: &[f64]) -> [f64; 4] {
 501          let count = statistic_window(y.len());
 502          let start = y.len() - count;
 503          let mae = self.errors[start..]
 504              .iter()
 505              .fold(0.0, |sum, &v| sum + v.abs())
 506              / count as f64;
 507          let rmse =
 508              (self.errors[start..].iter().fold(0.0, |sum, &v| sum + v * v) / count as f64).sqrt();
 509          let smape = (start..y.len()).fold(0.0, |sum, j| {
 510              let denominator = (y[j].abs() + self.predictions[j].abs()) / 2.0;
 511              sum + if denominator == 0.0 {
 512                  0.0
 513              } else {
 514                  self.errors[j].abs() / denominator
 515              }
 516          }) / count as f64;
 517          let prefix = if start == 0 { y.len() } else { start };
 518          let scale = y[..prefix]
 519              .windows(2)
 520              .fold(0.0, |sum, v| sum + (v[1] - v[0]).abs())
 521              / (prefix - 1) as f64;
 522          [
 523              if scale == 0.0 { 0.0 } else { mae / scale },
 524              smape,
 525              mae,
 526              rmse,
 527          ]
 528      }
 529  }
```


## B.4 The nonseasonal fit, forecast and confidence interval (lines 662 to 763)

```rust
// crates/calc-func/src/catalogue/statistics/ets.rs lines 662-763
 662  fn nonseasonal(
 663      site: &Site<'_>,
 664      y: &[f64],
 665      output: Output,
 666      statistic: f64,
 667      horizon: f64,
 668      confidence: f64,
 669  ) -> Step<CalcValue> {
 670      if y.iter()
 671          .any(|&v| v != 0.0 && !(2.0_f64.powi(-400)..=2.0_f64.powi(400)).contains(&v.abs()))
 672      {
 673          return Err(unfinished(site));
 674      }
 675      // Gamma does not require the unidentified initial states. Fresh short and
 676      // rounded-constant selections retain the same two epsilon seasonal states.
 677      if output == Output::Statistic && statistic == 3.0 && (3..=32).contains(&y.len()) {
 678          return site.number(f64::EPSILON);
 679      }
 680      if !(3..=32).contains(&y.len()) {
 681          return Err(unfinished(site));
 682      }
 683      // Outside the exact short-history region, the private initial regression's
 684      // rounding remains unidentified. Every refusal remains a counted failure.
 685      let extended;
 686      let search_history = if y.len() < 8 {
 687          extended = exact_short_history(y).ok_or_else(|| unfinished(site))?;
 688          &extended[..]
 689      } else {
 690          y
 691      };
 692      // The exact-affine route has already failed here. Rounded constant
 693      // initialisation changes the search parameters in retained adjacent probes.
 694      if y.iter().all(|&v| v == y[0]) {
 695          return Err(unfinished(site));
 696      }
 697      let start = search_history.len() - statistic_window(search_history.len());
 698      let validation = (start != 0).then_some(start);
 699      let mut best = f64::INFINITY;
 700      let mut chosen = PARAMETERS[0];
 701      for &(alpha, beta) in PARAMETERS {
 702          let score = Model::run(search_history, alpha, beta, validation).squared_errors();
 703          if score < best {
 704              best = score;
 705              chosen = (alpha, beta);
 706          }
 707      }
 708      if !best.is_finite() {
 709          return Err(unfinished(site));
 710      }
 711      let (alpha, mut beta) = chosen;
 712      let mut model = Model::run(y, alpha, beta, None);
 713      let squared_errors = model.squared_errors();
 714      let statistics = model.statistics(y);
 715      model.finish(y, &mut beta);
 716      match output {
 717          Output::Forecast => {
 718              if horizon < 0.0 {
 719                  return Err(unfinished(site));
 720              }
 721              let phase = ((y.len() - 1) as u64 + horizon as u64) % 2;
 722              // Fresh midpoint questions distinguish the final seasonal addition
 723              // at PC64 followed by a binary64 store from strict binary64 addition.
 724              site.number(crate::ported::excel_numeric::excel_x87_add(
 725                  model.level + horizon * model.trend,
 726                  model.seasons[phase as usize],
 727              ))
 728          }
 729          Output::Statistic => site.number(match statistic as u8 {
 730              1 => alpha,
 731              2 => beta,
 732              3 => f64::EPSILON,
 733              4..=7 => statistics[statistic as usize - 4],
 734              _ => unreachable!(),
 735          }),
 736          Output::Confidence => {
 737              let h = horizon;
 738              let k = ((h - 1.0) / 2.0).floor();
 739              let g = f64::EPSILON;
 740              let variance = 1.0
 741                  + (h - 1.0)
 742                      * ((h * alpha * beta + h * (2.0 * h - 1.0) * beta * beta / 6.0)
 743                          + alpha * alpha);
 744              let variance = variance + g * k * ((2.0 * alpha + g) + 2.0 * beta * (k + 1.0));
 745              let probability = (1.0 + confidence) / 2.0;
 746              // Valid confidence immediately below one rounds this probability
 747              // to one. Sixteen fresh selections identify the internal fallback.
 748              let z = if probability == 1.0 {
 749                  1.0
 750              } else {
 751                  crate::ported::normal_log_family::norm_s_inv_kernel(probability)
 752                      .map_err(Exit::Error)?
 753              };
 754              // R40 independently certified product-midpoint questions identify
 755              // the same PC64-then-binary64 publication used by the forecast.
 756              site.number(crate::ported::excel_numeric::excel_x87_mul(
 757                  z,
 758                  ((squared_errors / y.len() as f64) * variance).sqrt(),
 759              ))
 760          }
 761          Output::Seasonality => Err(unfinished(site)),
 762      }
 763  }
```


## B.5 The exact seasonal region (lines 531 to 629; lines 630 to 660 only format the outputs)

The seasonal region is entered only when every intermediate is exactly representable; the initial
states below are derived from the first two cycles and have only been verified in that exact region.

```rust
// crates/calc-func/src/catalogue/statistics/ets.rs lines 531-629
 531  /// Seasonal affine histories in the measured exact-arithmetic region. Period
 532  /// two starts at five observations and permits an unfinished final cycle.
 533  /// Every innovation is calculated by the application's shared loop.
 534  /// The zero objective retains the measured starting parameters; a nonzero
 535  /// innovation needs the still unidentified seasonal initialisation/search.
 536  fn exact_seasonal(
 537      site: &Site<'_>,
 538      y: &[f64],
 539      period: usize,
 540      output: Output,
 541      statistic: f64,
 542      horizon: f64,
 543      automatic: bool,
 544  ) -> Step<CalcValue> {
 545      if !(5..=32).contains(&y.len())
 546          || !(2..=8).contains(&period)
 547          || (period != 2
 548              && (y.len() % period != 0 || y.len() < 2 * period
 549          // Three cycles give every phase a measured initial mean. R39 also
 550          // proved the shorter power-of-two histories starting at eight values.
 551          || (y.len() < 3 * period && (!period.is_power_of_two() || y.len() < 8))))
 552          || y.iter()
 553              .any(|&v| v != 0.0 && !(2.0_f64.powi(-400)..=2.0_f64.powi(400)).contains(&v.abs()))
 554      {
 555          return Err(unfinished(site));
 556      }
 557      // R48's short/partial-cycle proofs identify detection, fitted parameters
 558      // and zero variance. Fresh lattice-edge forecasts expose an unidentified
 559      // publication step when a large affine component cancels near zero.
 560      // Keep the entire newly extended forecast consumer refused; the previously
 561      // measured complete-cycle forecast region retains its existing checks.
 562      if output == Output::Forecast
 563          && period == 2
 564          && (y.len() < 6 || y.len() % 2 != 0 || (automatic && y.len() < 8))
 565      {
 566          return Err(unfinished(site));
 567      }
 568      let differences: Vec<_> = (0..period)
 569          .map(|j| exact_sum(y[j + period], -y[j]))
 570          .collect::<Option<_>>()
 571          .ok_or_else(|| unfinished(site))?;
 572      let trend = exact_total(&differences)
 573          .and_then(|v| exact_ratio(v, (period * period) as f64))
 574          .ok_or_else(|| unfinished(site))?;
 575      let mut adjusted = Vec::with_capacity(period);
 576      for (j, &v) in y.iter().take(period).enumerate() {
 577          let product = j as f64 * trend;
 578          if (j as f64).mul_add(trend, -product) != 0.0 {
 579              return Err(unfinished(site));
 580          }
 581          adjusted.push(exact_sum(v, -product).ok_or_else(|| unfinished(site))?);
 582      }
 583      let level = exact_total(&adjusted)
 584          .and_then(|v| exact_ratio(v, period as f64))
 585          .ok_or_else(|| unfinished(site))?;
 586      let mut seasons = [0.0; 8];
 587      for (target, &v) in seasons.iter_mut().zip(&adjusted) {
 588          *target = exact_sum(v, -level).ok_or_else(|| unfinished(site))?;
 589      }
 590      if exact_total(&seasons[..period]) != Some(0.0) {
 591          return Err(unfinished(site));
 592      }
 593      let mut lattice = y.to_vec();
 594      lattice.extend_from_slice(&adjusted);
 595      lattice.extend_from_slice(&seasons[..period]);
 596      lattice.extend([level, trend]);
 597      let minimum = quantum_exponent(&lattice);
 598      if !(-900..=900).contains(&minimum) {
 599          return Err(unfinished(site));
 600      }
 601      let quantum = 2.0_f64.powi(minimum);
 602      let bound = (1u64 << 50) as f64 / (y.len() as f64).powi(3);
 603      if lattice.iter().any(|v| (v / quantum).abs() > bound) {
 604          return Err(unfinished(site));
 605      }
 606      if exact_sum(level, seasons[0]) != Some(y[0]) {
 607          return Err(unfinished(site));
 608      }
 609      let model = Model::run_initialized(
 610          y,
 611          0.751,
 612          0.001,
 613          None,
 614          Initial {
 615              level,
 616              trend,
 617              seasons,
 618              period,
 619              gamma: 0.001,
 620          },
 621      );
 622      if model.errors.iter().any(|&v| v != 0.0) {
 623          return Err(unfinished(site));
 624      }
 625      // Fresh automatic profiles distinguish a nonzero alternating component
 626      // from the affine case. Other detector shapes remain unidentified.
 627      if automatic && (period != 2 || model.seasons[0] == model.seasons[1]) {
 628          return Err(unfinished(site));
 629      }
```


## B.6 Routing and option handling (lines 801 to 919)

Option parsing before line 801 is trivial (argument positions per function, defaults seasonality 1, completion 1, aggregation 1, confidence 0.95).

```rust
// crates/calc-func/src/catalogue/statistics/ets.rs lines 801-919
 801      if statistic.fract() != 0.0 || !(1.0..=8.0).contains(&statistic)
 802          || !(0.0..1.0).contains(&confidence)
 803          // Bisection locates the last admitted integer period at 8784; the
 804          // error applies to step statistics and confidence zero as well.
 805          || season < 0.0 || season > 8_784.0 || season.fract() != 0.0
 806          || (completion != 0.0 && completion != 1.0)
 807          || aggregation.fract() != 0.0 || !(1.0..=7.0).contains(&aggregation)
 808      {
 809          return fail(WorksheetErrorCode::Num);
 810      }
 811      let series = prepare(site, y, t, completion, aggregation)?;
 812      let target = if offset == 1 {
 813          num(site, args, 0)?
 814      } else {
 815          series.last
 816      };
 817      if target < series.first {
 818          // A reversed input timeline can admit extrapolation before its minimum
 819          // where an ascending one returns NUM. Its admission rule is still open.
 820          return if series.ordered {
 821              fail(WorksheetErrorCode::Num)
 822          } else {
 823              Err(unfinished(site))
 824          };
 825      }
 826      if output == Output::Confidence && target <= series.last {
 827          return fail(WorksheetErrorCode::Num);
 828      }
 829      let horizon = if offset == 1 {
 830          let distance = exact_sum(target, -series.last).ok_or_else(|| unfinished(site))?;
 831          let h = distance / series.step;
 832          if h.fract() != 0.0 || h * series.step != distance {
 833              return Err(unfinished(site));
 834          }
 835          // Both target functions share this signed-32-bit future-index limit,
 836          // including the confidence-zero case (three translated/scaled timelines).
 837          if h > i32::MAX as f64 {
 838              return fail(WorksheetErrorCode::Num);
 839          }
 840          h
 841      } else {
 842          0.0
 843      };
 844      if output == Output::Statistic && statistic == 8.0 {
 845          return site.number(series.step);
 846      }
 847      if output == Output::Confidence && confidence == 0.0 {
 848          return site.number(0.0);
 849      }
 850      if !series.preparation_exact {
 851          return Err(unfinished(site));
 852      }
 853      // R46's adjacent/exponent controls identify the detector's minimum
 854      // history: lag two first becomes available at five prepared observations.
 855      // This does not identify the detector or fitted model for longer histories.
 856      if output == Output::Seasonality && series.values.len() <= 4 {
 857          return site.number(0.0);
 858      }
 859      // R47 paired questions identify the nonseasonal fit before five prepared
 860      // observations, including explicit period two at length four. Admit its
 861      // proved zero-innovation region. Rounded short initialization and nonzero
 862      // confidence/statistic differences still keep the other short fits open.
 863      let short_affine = series.values.len() <= 4 && exact_trend(&series.values).is_some();
 864      let nonseasonal_mode =
 865          season == 0.0 || short_affine || season > (series.values.len() / 2) as f64;
 866      if season == 1.0 && !nonseasonal_mode {
 867          match exact_seasonal(site, &series.values, 2, output, statistic, horizon, true) {
 868              Ok(value) => return Ok(value),
 869              Err(Exit::Fault(FunctionFault::NotHeld { .. })) => {}
 870              Err(why) => return Err(why),
 871          }
 872      }
 873      if season > 1.0 && !nonseasonal_mode {
 874          return exact_seasonal(
 875              site,
 876              &series.values,
 877              season as usize,
 878              output,
 879              statistic,
 880              horizon,
 881              false,
 882          );
 883      }
 884      let trend = match exact_trend(&series.values) {
 885          Some(trend) => trend,
 886          None => {
 887              if nonseasonal_mode {
 888                  return nonseasonal(site, &series.values, output, statistic, horizon, confidence);
 889              }
 890              return Err(unfinished(site));
 891          }
 892      };
 893      if output == Output::Seasonality {
 894          return site.number(0.0);
 895      }
 896      // An explicit period longer than half the data uses the nonseasonal model.
 897      match output {
 898          Output::Forecast => {
 899              let product = horizon * trend;
 900              // The trend's quantum is at least 2^-900 and h is an integer, so
 901              // a nonzero multiplication residue cannot underflow here.
 902              if !product.is_finite() || horizon.mul_add(trend, -product) != 0.0 {
 903                  return Err(unfinished(site));
 904              }
 905              site.number(
 906                  exact_sum(*series.values.last().unwrap(), product)
 907                      .ok_or_else(|| unfinished(site))?,
 908              )
 909          }
 910          Output::Confidence => site.number(0.0),
 911          Output::Statistic => site.number(match statistic as u8 {
 912              1 => 0.9,
 913              2 => 0.001,
 914              3 => f64::EPSILON,
 915              _ => 0.0,
 916          }),
 917          Output::Seasonality => unreachable!(),
 918      }
 919  }
```


---

# Appendix C: what is refused, by block, with the data to look at

The three recorded workbooks: `dcj9k_r38_ets` (22 base series x all option combinations: STAT type 8
only, FORECAST/CONFINT/SEASONALITY with seasonality 0..7, completion 0/1, aggregation 1,4,5,6,7;
targets at n+1), `dcj9k_r39_ets` (80 designed random histories, lengths 3 to 31: SEASONALITY
automatic; FORECAST, CONFINT, STAT 1..8 all with seasonality 0 (nonseasonal fit), completion 1,
aggregation 1) and `dcj9k_r39_seasonal` (40 exact seasonal-plus-trend histories with explicit period
2, 4 or 8: FORECAST/CONFINT/STAT with that period, plus automatic SEASONALITY).

1. **Automatic detection (528 cells).** Every refused SEASONALITY cell is a request for the
   automatic detector's integer. The FORECAST/CONFINT cells in this block are the r38 histories with
   seasonality left automatic. Excel's SEASONALITY answers are listed in E.3 (all 142 histories)
   and are the only direct view of the detector; they are not what ACF peaks suggest (R47: all 81
   fixed ACF programs fail; R48: 30 spectrum/ACF/error-minimising trees fail 220 histories).
2. **Short initialisation (204 cells):** n = 4..7 prepared points (E.4): alpha, beta, STAT 4..7, forecast and
   CI are known for 16 histories; our nonseasonal fit refuses unless the OLS slope and the seven-point
   extension are exact in binary64. (n = 3 matches.)
3. **Nonzero seasonal innovations (180 cells), cycle limits (40 cells):** E.5 lists the distinct
   (history, period, target) triples with Excel's forecast and confidence interval. No alpha/beta/gamma
   or error statistic exists in the recorded workbooks for these histories (the r38 workbook asked STAT 8 only).

---

# Appendix D: what each research round established (and which parts were guesses)

All rounds were automated, original-research implementers working from Excel's answers only; the
numbers are from their reports and design notes. "M" = measured and proved on fresh answers, "C" =
conditional (true given an assumption stated), "G" = guess, "X" = refuted.

- **R16 (2026-11-02):** a prior round on variance functions, no ETS.
- **R37:** the 980-case design (22 base series x options x four functions) was built; the question
  service was extended to accept the array arguments. Nothing numerical.
- **R38:** all 19,600 selections answered. M: option and timeline errors (appendix A), period 8784/8785,
  future index 2^31, step statistic, confidence zero (`CONFINT(…, 0)` = 0), exact affine histories
  (alpha 0.9, beta 0.001, gamma 2^-52, zero errors, forecast = last + h*slope), duplicate reduction,
  interpolation. G: the optimiser; they found "general optimisation stopped yielding an exact model".
- **R39 (2026-11-08/09):** M: the nonseasonal recurrence ((level + trend) + season; level += trend +
  alpha*e; trend += beta*e; gamma 2^-52), reported parameters taken from a table of about 23 points
  (all in six significant digits), the objective is a frozen-validation SSE ("transition only, no
  observation updates in the last window"; the window is min(10, floor(0.4 n)) points for n > 10, none for n <= 10), STAT 4..7
  over the same window (MASE with scale = mean absolute first difference of the leading history,
  SMAPE with denominator (|y| + |yhat|)/2, MAE, RMSE divided by the count), the CI variance
  expression and a rounding of the confidence probability; C: seasonal recursion exact given
  measured parameters (1,536/1,536) and a "forward 2p-point means" initialiser that fits only a
  positive-trend period-two family; X: two complete-cycle count rules, 24 fixed correlation
  detectors, a 21-triple and a 25-point search candidate (fails 25/384 outside the fitted region).
- **R40:** M: short-history initialisation = extend to seven points with the first two phases plus
  the OLS trend, then slope again (guarded, exact region only); PC64 stores for the final forecast
  addition and the confidence product; exact affine seasonal periods 2 to 8. 10,000 histories gave 28
  distinct (alpha, beta, gamma) triples; 332 predictions of the table failed there (later partly
  repaired by the six-significant-digit decimals). X: Cartesian grid and twelve projected Nelder-Mead
  programs; AS 241/Newton-Halley normal quantile families (inverse normal is its own function).
- **R41:** 491,715 histories reconciled to 30 triples; 27/28 target paths show adjacent departures;
  the 25-pair table survives necessary constraints on 128 fresh histories and 54 boundary sides but
  one complete search/reset program has a contradiction.
- **R46:** M: the "reset" (trend significance test on the final half of the trends with raw moments
  and separate odd/even counts, stored-PC64 arithmetic: 512 fresh triples, 460 minimum-alpha
  forecasts) ; SEASONALITY = 0 for 2..4 points. The detector can return 2 at length 5. Individual
  stores are "nonunique" (1,228 store programs survive).
- **R47:** M: explicit period 2 at length 4 behaves like seasonality 0. C: the fixed 25-pair
  program fits 34 retained tie constraints but fails 47/4,608 fresh parameter readings (E.2);
  61 plateau forecasts and 122 parameter readings agree; 62/100 fitted-load neighbours separate.
  X: all 81 fixed ACF programs on 312 designed histories, 768 short-initialiser form/store programs
  (27/1,280 forecast contradictions), seasonal search 17/384 parameter contradictions, forward 2p
  initialiser 44/512 forecast contradictions (even given the measured parameters).
- **R48:** X: 35 of 36 alternative search trees (sparse mesh, coarse mesh plus four halved coordinate
  refinements, bounded Nelder-Mead, multiplicative-error likelihood) on 96 histories, 30 detector trees
  (spectrum and ACF) on 220 histories, 3,072 raw-moment and 2,048 centred short-initialiser trees on
  318 histories, 12 seasonal initialisers on 96 histories. Note the identifiability remark: with fixed
  window and additive errors, MSE and the profiled Gaussian likelihood have the same minimiser, so
  that choice cannot be separated by data. Microsoft's May 2014 Power View article is recalled in the
  notes as describing: frozen validation states, a trend correction, and a detector that removes
  the linear trend, takes candidate intervals from spectrum peaks, ranks correlation peaks within
  them and considers business-period hints ("a method-family hypothesis, not proof of an identical
  worksheet implementation"). A zero-innovation period-two subset (length >= 5) is calculated for
  detection/STAT/zero confidence; its forecasts were withdrawn because Excel returned exactly 0 where
  the candidate returned +-2^-400 when the final affine component cancels a much larger scale (an
  unidentified zero threshold, or a different last-step arithmetic).
- **R49:** no ETS work.
- **Fable-1 note (2026-11-09):** reported alphas {0.002, 0.126, 0.25, 0.5, 0.75, 0.9, 0.998} and betas
  {0.001, 0.125, 0.249, 0.499, 0.749} with gamma exactly 2^-52 on 28 readings; suggests a coarse grid
  with 0.001/0.002 boundary offsets and DBL_EPSILON as a lower clamp. Open questions it raised: which
  metric (SSE vs absolute error vs AIC), accumulation order, tie rule, and "detection is not ACF:
  perhaps refit-based".

Where the reports disagree: R39 says the nonseasonal search matches 512/512 forecasts on fresh
answers, R47/R48 say the combined score/load/initialiser/order program fails 47/4,608 parameter
readings in constructed tie clouds (E.2). These are not in conflict: the table works on typical
data and fails on designed ties (and the extra points in E.2 prove the true visited set is larger
than our table).

---

# Appendix E: the data

## E.1 The visited (alpha, beta) pairs (STAT types 1 and 2, nonseasonal, gamma always 2^-52)

Union over the 2,664 fresh nonseasonal histories of R47 (tie clouds, single-bit boundary paths, random;
counts = histories ending on each pair; values are the nearest doubles of the decimals):

```
alpha      beta values seen (count)
0.002      0.001 (357)
0.1        0.001 (311)  0.099 (36)
0.126      0.001 (425)  0.125 (53)
0.166667   0.001 (25)   0.0833333 (43)
0.25       0.001 (451)  0.249 (70)
0.333333   0.001 (28)   0.166667 (22)
0.5        0.001 (191)  0.25 (7)  0.499 (47)
0.666667   0.001 (46)   0.166667 (9)  0.5 (12)
0.75       0.001 (85)   0.749 (31)
0.833333   0.001 (6)    0.0833333 (13)  0.75 (22)
0.9        0.001 (134)  0.899 (48)
0.998      0.001 (55)  0.099 (20)  0.249 (42)  0.499 (27)  0.749 (16)  0.899 (32)
```

Total 30 distinct pairs. The reported beta of a history that triggers the reset (B.3) is 0.001
whatever alpha was chosen. Our `PARAMETERS` table (B.2) has 23 of these 30; the research trial pool
added (0.833333, 0.0833333) and (0.998, 0.249) (25 points); the other five (0.166667/0.001,
0.333333/0.001, 0.666667/0.001, 0.833333/0.001, 0.998/0.001) are taken to be reset outputs (our code's
comment at line 280 says so) rather than search points. Observations:
(a) beta is either the floor 0.001 or a value at most alpha - 0.001; for alpha in {0.1, 0.126, 0.25,
0.5, 0.75, 0.9} the top value is exactly alpha - 0.001 (a clamp "beta <= alpha - 0.001"?); (b) alpha has
twelve values: tenths-and-quarters-and-eighths {0.002, 0.1, 0.126 (= 0.125 + 0.001), 0.25, 0.5,
0.75, 0.9, 0.998 (= 1 - 0.002)} and sixths {1/6, 1/3, 2/3, 5/6 to six significant digits}; (c)
beta values: {0.001, 1/12 = 0.0833333, 0.099, 0.125, 1/6 = 0.166667, 0.249, 0.25, 0.499, 0.5, 0.749,
0.75, 0.899}; for alpha = 0.998 the betas {0.099, 0.249, 0.499, 0.749, 0.899} are {0.1, 0.25, 0.5,
0.75, 0.9} minus 0.001 and are not clamped by alpha; (d) beta/alpha for the unclamped interior points
is {0.5 for 1/6, 1/3, 0.5; 0.25 and 0.75 for 2/3; 0.1 and 0.9 for 5/6}.
The reported values are RN53 of decimals with at most six significant digits ("0.0833333").

Strict-less first-minimum order of the research's 25-point pool (indices into the pool listed in
this order) that satisfied 34 retained tie constraints: `[0, 3, 7, 11, 13, 14, 16, 17, 15, 5, 8, 10,
12, 18, 19, 20, 6, 9, 21, 2, 4, 22, 1, 23, 24]` where pool index 0 = (0.9, 0.001), 1 = (0.9, 0.899),
2 = (0.833333, 0.75), 3 = (0.75, 0.001), 4 = (0.75, 0.749), 5 = (0.666667, 0.166667), 6 = (0.666667, 0.5),
7 = (0.5, 0.001), 8 = (0.5, 0.25), 9 = (0.5, 0.499), 10 = (0.333333, 0.166667), 11 = (0.25, 0.001),
12 = (0.25, 0.249), 13 = (0.166667, 0.0833333), 14 = (0.126, 0.001), 15 = (0.126, 0.125), 16 = (0.1, 0.001),
17 = (0.1, 0.099), 18 = (0.002, 0.001), 19 = (0.998, 0.099), 20 = (0.998, 0.499), 21 = (0.998, 0.749),
22 = (0.998, 0.899), 23 = (0.833333, 0.0833333), 24 = (0.998, 0.249). Even this ordering fails 47
of 4,608 fresh readings.

## E.2 Where the candidate table fails: designed exact-score tie clouds (R47)

1,536 histories of 8 to 32 points: a constant (e.g. 3.944304526105058e-31, 0.6666666666666666, 4.0e30 or
1.0e42) plus single-bit noise (a few ulps), at several exponents; so the candidate scores are equal
or differ by an ulp or two and the choice depends on score arithmetic and tie order. Examples (exact
doubles): case 812 y = [3.944304526105061e-31, 3.94430452610506e-31, 3.944304526105058e-31,
3.944304526105059e-31, 3.9443045261050586e-31, 3.94430452610506e-31, 3.94430452610506e-31,
3.944304526105058e-31]; case 1362 y = [0.666666666666666, 0.6666666666666654, 0.6666666666666683,
0.666666666666667, 0.6666666666666649, 0.6666666666666651, 0.6666666666666654, 0.6666666666666669].
Of 4,608 parameter readings (1,536 histories x alpha, beta, gamma) our best table program (25-point pool,
stable-first order of E.1, strict less) matches Excel on 4,561 and fails 47 readings on the 31 histories below
(program = what the program picks; Excel = what Excel reported; alpha/beta):

```
case  n   program               excel              case  n   program               excel
 520 16 0.126/0.001          0.833333/0.0833333   1136 32 0.166667/0.001       0.126/0.001
 568 14 0.166667/0.0833333   0.126/0.001          1148 19 0.75/0.001          0.666667/0.001
 604 25 0.166667/0.0833333   0.1/0.001            1176 22 0.166667/0.0833333  0.002/0.001
 605 26 0.166667/0.0833333   0.126/0.001          1208 29 0.166667/0.001       0.126/0.001
 660 31 0.1/0.099            0.166667/0.001       1232 28 0.166667/0.001       0.25/0.001
 672 18 0.5/0.001            0.333333/0.001       1256 27 0.333333/0.001       0.126/0.125
 721 17 0.126/0.125          0.833333/0.0833333   1308 29 0.1/0.001           0.166667/0.001
 757 28 0.1/0.001            0.126/0.001          1324 20 0.5/0.001           0.333333/0.001
 812  8 0.833333/0.0833333   0.75/0.001           1348 19 0.166667/0.0833333  0.126/0.001
 845 16 0.166667/0.0833333   0.126/0.001          1362  8 0.126/0.001          0.1/0.001
 848 19 0.166667/0.001       0.126/0.001          1373 19 0.166667/0.0833333  0.126/0.001
 916 12 0.666667/0.001       0.833333/0.001       1376 22 0.75/0.001          0.666667/0.001
 968 14 0.5/0.001            0.333333/0.166667    1412  8 0.9/0.001           0.833333/0.001
1028 24 0.166667/0.0833333   0.126/0.001          1416 12 0.5/0.25            0.333333/0.166667
                                                  1418 14 0.25/0.001          0.166667/0.001
                                                  1493 14 0.166667/0.0833333  0.126/0.001
                                                  1520 16 0.666667/0.001      0.25/0.001
```

Many failures are "the program picked the sixth point (0.166667, 0.0833333) or a 0.001-beta point
where Excel picked a neighbouring alpha", which is what an ulp-level difference in the score
accumulation order or in the candidate order would do. Points with beta 0.001 and alpha 1/6, 1/3,
2/3, 5/6, 0.998 are not search points of our table: our code's comment (line 280) says a later "trend
correction" (the reset of B.3) can report beta 0.001 for any alpha, and these may be such outputs.
The R47 plateau experiment (61 forecast questions on 12,000 constructed exact-OLS histories, 122
parameter readings, all agreeing) shows that fitted loads at neighbouring decimals separate in 62 of
100 cases and are undetermined in 38.

## E.3 Seasonality detection: Excel's SEASONALITY (0 = none) for the recorded histories

Timelines are 1..n unless noted. Format: `H<id> n=<points> y16=[series x 16] SEAS=<Excel's integer>
 a, b = alpha, beta (seasonality 0 fit) FC(target) = forecast of the nonseasonal fit, CI = 95%
interval half-width`. These are the 64 designed histories of length 8 to 31 and the 16 short ones
(E.4) from the `dcj9k_r39_ets` workbook (our nonseasonal fit matches a, b and FC of all those with n >= 8, `st=match` marks our FC result; a few STAT 4..7 and CI
last-bit mismatches exist (E.6) but are not about the shape). The series are
trend + noise, random walks, seasonal-noise mixtures and pure noise (the generator is not in the
repository). Detection results over the 64 histories n >= 8: 0 (55 times), 2 (4), 4 (2),
7 (1), 8 (2).

```
H22 n=24 y16=[-5,-21,-40,-58,-78,-87,-104,-120,-123,-143,-137,-151,-138,-150,-143,-136,-130,-122,-106,-92,-80,-55,-38,-29] SEAS=0.0 a=0.998 b=0.899 FC(25)=-1.1947416282224714 CI=1.479393874549574 st=match
H23 n=20 y16=[-14,8,-164,-210,-76,-14,152,124,-278,-158,-22,-202,-220,-116,142,146,-250,30,-298,-230] SEAS=8.0 a=0.002 b=0.001 FC(21)=-7.141440820092876 CI=17.9047795771417 st=match
H24 n=23 y16=[7,-4,-7,-3,12,9,30,33,54,55,84,96,121,138,174,190,228,257,281,317,354,403,436] SEAS=0.0 a=0.25 b=0.249 FC(24)=29.422806950822974 CI=2.077077834148402 st=match
H25 n=28 y16=[-226,38,-22,152,-18,282,-144,20,188,-18,194,206,170,-34,-12,266,-20,34,-4,-28,60,-6,-70,208,322,-18,264,64] SEAS=0.0 a=0.25 b=0.001 FC(29)=8.28942098394685 CI=18.857942090172653 st=match
H26 n=11 y16=[-8,22,59,79,125,162,187,238,264,320,366] SEAS=0.0 a=0.25 b=0.001 FC(12)=24.30551336116661 CI=1.373771597392273 st=match
H27 n=31 y16=[-18,102,2,-80,-258,-162,-300,-350,-202,-320,-182,-380,-256,-134,-176,-580,-134,-118,-196,-658,-192,-520,-546,-324,-486,-708,-630,-402,-684,-698,-534] SEAS=0.0 a=0.002 b=0.001 FC(32)=-38.748257829285826 CI=17.763920296246773 st=match
H28 n=30 y16=[1,-1,9,17,23,20,28,46,69,81,103,117,145,168,202,217,261,291,330,367,404,445,486,523,569,632,673,734,778,839] SEAS=0.0 a=0.5 b=0.499 FC(31)=55.70195551860585 CI=1.3233102528065661 st=match
H29 n=11 y16=[96,-142,284,74,68,-128,180,374,390,436,418] SEAS=0.0 a=0.126 b=0.001 FC(12)=30.374505097679542 CI=20.749602851059674 st=match
H30 n=20 y16=[1,-10,-7,-16,-2,-8,-3,7,11,31,43,50,72,98,115,130,155,185,213,250] SEAS=0.0 a=0.25 b=0.249 FC(21)=16.62672295554367 CI=1.839668369008629 st=match
H31 n=26 y16=[238,-62,214,-62,-28,14,180,84,-66,116,-178,-366,-12,-278,-164,-370,-328,-70,-58,-8,-138,-12,-400,-174,-328,-202] SEAS=0.0 a=0.1 b=0.001 FC(27)=-17.549004294414623 CI=19.756872254218063 st=match
H32 n=24 y16=[8,-18,-43,-54,-69,-93,-90,-110,-112,-115,-122,-128,-121,-124,-120,-101,-88,-86,-65,-59,-32,-28,-2,24] SEAS=0.0 a=0.25 b=0.249 FC(25)=2.2335437135744978 CI=2.0728738656459194 st=match
H33 n=18 y16=[20,16,-10,-86,-74,316,-132,240,-136,120,-56,62,364,-2,200,-46,-48,66] SEAS=2.0 a=0.002 b=0.001 FC(19)=5.588120044949882 CI=17.50373144450816 st=match
H34 n=26 y16=[-4,-32,-47,-76,-99,-122,-125,-140,-166,-167,-181,-185,-196,-200,-190,-187,-195,-180,-179,-164,-154,-140,-125,-122,-99,-73] SEAS=0.0 a=0.75 b=0.749 FC(27)=-3.122382411059722 CI=1.3405519952521534 st=match
H35 n=30 y16=[230,150,-186,-160,-260,-98,206,-306,-110,134,-206,-118,-226,-16,-216,-218,-76,52,-84,-36,-116,-282,78,48,-416,-214,-134,-194,-42,-428] SEAS=0.0 a=0.25 b=0.001 FC(31)=-14.672093977775617 CI=22.12791546725228 st=match
H36 n=27 y16=[-4,-22,-49,-66,-75,-100,-103,-113,-136,-131,-144,-145,-142,-140,-133,-134,-126,-122,-110,-88,-88,-61,-50,-19,0,32,45] SEAS=0.0 a=0.126 b=0.125 FC(28)=3.5984531395337545 CI=3.6633121125142782 st=match
H37 n=17 y16=[-44,-58,248,0,172,140,214,-114,-218,226,26,246,194,-120,186,-144,252] SEAS=2.0 a=0.126 b=0.001 FC(18)=5.67832532010962 CI=21.10287786527075 st=match
H38 n=22 y16=[-4,-6,-14,-16,-14,-12,-7,1,3,4,17,32,46,65,84,106,136,145,172,210,235,269] SEAS=0.0 a=0.998 b=0.899 FC(23)=18.88603500560692 CI=1.0611997418707066 st=match
H39 n=24 y16=[-120,266,114,10,-122,308,312,126,238,536,364,300,184,218,530,342,258,670,372,836,592,552,846,938] SEAS=0.0 a=0.126 b=0.001 FC(25)=49.0996605994278 CI=21.36307734543796 st=match
H40 n=30 y16=[3,-1,-18,-25,-26,-21,-16,-17,-19,-5,1,3,22,32,60,69,102,117,139,176,195,235,267,291,333,379,418,463,499,558] SEAS=0.0 a=0.9 b=0.899 FC(31)=38.25019232809183 CI=1.5022364085227466 st=match
H41 n=11 y16=[124,-78,46,-104,-52,264,164,80,-44,168,344] SEAS=0.0 a=0.25 b=0.001 FC(12)=14.416296462537128 CI=18.26980033406069 st=match
H42 n=26 y16=[4,0,-7,-13,-24,-19,-14,-3,0,4,27,41,46,61,91,109,135,158,179,217,242,274,314,344,392,420] SEAS=0.0 a=0.75 b=0.749 FC(27)=28.582761698573627 CI=1.2436153784547457 st=match
H43 n=17 y16=[-122,60,-72,256,88,30,4,40,-32,324,256,358,378,506,340,298,226] SEAS=0.0 a=0.126 b=0.001 FC(18)=25.60686009539885 CI=16.137640606351987 st=match
H44 n=22 y16=[-4,-26,-66,-88,-104,-140,-164,-173,-194,-210,-225,-233,-240,-254,-257,-254,-250,-254,-245,-246,-239,-224] SEAS=0.0 a=0.25 b=0.001 FC(23)=-14.67186617730096 CI=1.8547471850711512 st=match
H45 n=21 y16=[66,-282,-224,-200,42,-144,-124,-426,-254,-132,-130,-548,-420,-540,-614,-678,-584,-538,-556,-772,-820] SEAS=0.0 a=0.1 b=0.001 FC(22)=-48.24458199553301 CI=17.262248579571086 st=match
H46 n=26 y16=[8,-1,1,-10,-6,-2,11,23,26,41,53,73,101,124,144,164,184,222,245,282,320,360,390,442,485,522] SEAS=0.0 a=0.126 b=0.125 FC(27)=34.42662339569754 CI=4.080698608520103 st=match
H47 n=31 y16=[-62,58,-264,-218,-156,-180,20,-96,96,130,-270,-230,-236,-224,-42,-202,34,-206,-130,152,24,-158,-250,-174,22,-126,-206,-122,-122,114,-216] SEAS=0.0 a=0.002 b=0.001 FC(32)=-5.124744753055292 CI=16.255205481496382 st=match
H48 n=30 y16=[-2,-8,-18,-19,-16,-19,-9,-2,-7,6,19,37,49,68,81,101,126,148,185,210,247,273,307,338,383,423,472,511,565,607] SEAS=0.0 a=0.5 b=0.499 FC(31)=41.01937838092377 CI=1.3367128484422588 st=match
H49 n=21 y16=[80,-236,40,-90,216,156,-46,300,38,28,-80,-146,138,8,378,176,288,330,-4,24,-6] SEAS=0.0 a=0.1 b=0.001 FC(22)=9.547944190712506 CI=20.15454574731128 st=match
H50 n=28 y16=[5,-39,-67,-89,-107,-136,-149,-180,-185,-202,-212,-233,-241,-243,-245,-258,-263,-253,-244,-254,-246,-237,-217,-208,-199,-168,-159,-139] SEAS=0.0 a=0.9 b=0.001 FC(29)=-9.006020799124247 CI=1.6704991401108418 st=match
H51 n=26 y16=[-42,90,-74,194,-172,-168,22,192,-106,-122,-168,36,-62,180,70,140,274,218,260,-50,34,-44,16,4,348,104] SEAS=0.0 a=0.002 b=0.001 FC(27)=8.716460597306902 CI=16.485694528177184 st=match
H52 n=23 y16=[-6,26,53,90,112,153,197,234,264,317,359,403,463,507,567,622,665,732,787,857,928,985,1059] SEAS=0.0 a=0.25 b=0.249 FC(24)=70.01876862186951 CI=1.9411595133422177 st=match
H53 n=20 y16=[256,-82,-22,-36,-44,238,226,310,116,-46,238,236,232,422,372,126,134,490,406,492] SEAS=7.0 a=0.75 b=0.001 FC(21)=31.095199165642406 CI=20.38226439058934 st=match
H54 n=23 y16=[-4,-29,-57,-74,-111,-131,-141,-157,-173,-182,-197,-210,-216,-218,-218,-218,-223,-229,-210,-215,-197,-187,-168] SEAS=0.0 a=0.9 b=0.001 FC(24)=-10.989933300395258 CI=1.313703965915155 st=match
H55 n=17 y16=[-238,6,26,-60,294,56,112,52,436,388,306,336,386,350,200,468,632] SEAS=0.0 a=0.25 b=0.001 FC(18)=34.496245984970415 CI=17.554209000532705 st=match
H56 n=27 y16=[7,-8,-1,-6,-2,-1,-2,8,22,34,34,56,73,91,116,140,166,191,224,243,284,319,350,383,432,480,519] SEAS=0.0 a=0.5 b=0.499 FC(28)=35.419808110776444 CI=1.2772411401775368 st=match
H57 n=30 y16=[-30,154,-172,-14,-54,50,-82,-106,-68,-346,-310,-396,-106,-422,-466,-146,-322,-234,-318,-602,-188,-196,-262,-440,-628,-472,-540,-658,-788,-470] SEAS=0.0 a=0.002 b=0.001 FC(31)=-39.92366785399513 CI=17.285701267058577 st=match
H58 n=14 y16=[2,35,63,93,137,171,211,265,308,345,393,457,506,556] SEAS=0.0 a=0.75 b=0.749 FC(15)=38.016026381700804 CI=1.2436801756710354 st=match
H59 n=19 y16=[250,128,92,36,-118,324,-52,162,138,118,266,356,150,570,278,570,314,614,350] SEAS=2.0 a=0.5 b=0.001 FC(20)=29.45907472582571 CI=21.7154043876158 st=match
H60 n=17 y16=[-7,-4,-3,11,12,29,43,41,59,86,100,124,149,173,190,230,250] SEAS=0.0 a=0.5 b=0.499 FC(18)=17.414493751981816 CI=1.1498228477311827 st=match
H61 n=19 y16=[-208,92,-80,-206,-184,-180,-284,-204,-116,-62,-196,60,-70,-50,86,-126,-120,-162,-64] SEAS=0.0 a=0.126 b=0.001 FC(20)=-4.298262137437559 CI=13.31166392255713 st=match
H62 n=27 y16=[-5,18,16,36,49,72,94,116,137,178,194,236,256,293,331,374,421,455,506,553,602,648,698,754,819,881,944] SEAS=0.0 a=0.75 b=0.749 FC(28)=63.00324584645868 CI=1.2939465750609442 st=match
H63 n=25 y16=[-124,-136,32,-168,-94,90,128,48,176,142,30,-34,388,-66,-24,-20,130,444,250,420,208,454,336,250,116] SEAS=0.0 a=0.002 b=0.001 FC(26)=19.95964819154321 CI=17.117541707604317 st=match
H64 n=17 y16=[0,-10,-11,-16,-24,-7,-14,-10,5,10,25,34,44,73,76,105,121] SEAS=0.0 a=0.75 b=0.749 FC(18)=8.84234739846091 CI=1.341761859878319 st=match
H65 n=17 y16=[150,112,36,-16,110,-174,62,64,-14,-256,-396,-52,-194,-442,-566,-314,-642] SEAS=4.0 a=0.1 b=0.001 FC(18)=-33.44330970806759 CI=16.57719233293295 st=match
H66 n=9 y16=[7,17,54,84,107,144,175,218,259] SEAS=0.0 a=0.9 b=0.001 FC(10)=18.115927734998223 CI=1.1676080679032699 st=match
H67 n=17 y16=[164,110,78,36,134,166,-56,-66,-210,-70,66,-20,-120,36,-162,92,-346] SEAS=0.0 a=0.1 b=0.001 FC(18)=-9.963649851187505 CI=14.221992558842977 st=match
H68 n=30 y16=[5,16,43,79,110,132,168,200,242,280,325,355,408,461,504,563,612,669,716,787,832,903,969,1033,1110,1168,1255,1317,1393,1473] SEAS=0.0 a=0.998 b=0.749 FC(31)=96.96288356730646 CI=1.725956885280776 st=match
H69 n=8 y16=[220,-12,242,-36,12,42,304,242] SEAS=2.0 a=0.25 b=0.001 FC(9)=13.371628877284007 CI=19.028798452326495 st=match
H70 n=19 y16=[-2,21,36,63,90,112,152,173,201,250,279,312,357,405,447,493,547,593,647] SEAS=0.0 a=0.25 b=0.249 FC(20)=43.398422823660596 CI=1.651124038210716 st=match
H71 n=24 y16=[28,20,94,22,208,-98,34,116,250,-36,-78,260,144,70,274,216,60,288,10,194,386,220,366,-62] SEAS=0.0 a=0.002 b=0.001 FC(25)=14.11830018400045 CI=15.301584308281203 st=match
H72 n=12 y16=[1,-22,-58,-73,-101,-120,-143,-164,-173,-183,-201,-216] SEAS=0.0 a=0.25 b=0.001 FC(13)=-15.304781538359249 CI=1.3701835203867239 st=match
H73 n=24 y16=[-48,214,-202,-88,-102,-202,-210,-90,-204,62,106,18,-282,-164,-124,-86,-28,-10,-160,-362,-468,-496,-464,-362] SEAS=8.0 a=0.1 b=0.001 FC(25)=-22.52239019439066 CI=18.52621524124124 st=match
H74 n=8 y16=[5,-27,-50,-61,-86,-89,-109,-125] SEAS=0.0 a=0.75 b=0.001 FC(9)=-8.925561416818823 CI=0.9919518391731963 st=match
H75 n=27 y16=[-172,-214,-54,26,-122,54,252,-124,-210,-96,248,184,274,-172,294,236,166,280,200,-136,130,-136,160,34,272,-34,40] SEAS=0.0 a=0.1 b=0.001 FC(28)=8.920157935296512 CI=21.524689982923793 st=match
H76 n=17 y16=[5,-20,-31,-37,-61,-64,-74,-75,-82,-77,-83,-69,-80,-57,-55,-38,-34] SEAS=0.0 a=0.25 b=0.249 FC(18)=-1.3039267316507464 CI=1.7660792869274675 st=match
H77 n=19 y16=[112,196,72,74,14,370,374,56,458,376,250,38,482,572,548,174,374,534,684] SEAS=4.0 a=0.002 b=0.001 FC(20)=35.98888813841113 CI=19.130087185225673 st=match
H78 n=14 y16=[-4,19,54,67,96,143,174,197,236,271,312,371,412,460] SEAS=0.0 a=0.998 b=0.899 FC(15)=31.71383769120925 CI=1.6234131010306327 st=match
H79 n=18 y16=[-74,184,-206,8,-124,130,-156,-92,-170,32,2,16,-216,-130,192,-98,-202,92] SEAS=0.0 a=0.002 b=0.001 FC(19)=-4.978249762144141 CI=16.30470269353484 st=match
H80 n=21 y16=[1,-9,-36,-47,-55,-70,-68,-71,-72,-84,-83,-80,-78,-66,-50,-43,-31,-22,-6,27,44] SEAS=0.0 a=0.25 b=0.249 FC(22)=3.520035824270763 CI=1.969873328630909 st=match
H81 n=17 y16=[-132,70,192,114,130,390,362,240,228,326,94,500,300,466,440,460,722] SEAS=0.0 a=0.25 b=0.001 FC(18)=37.46053279017929 CI=16.943620885956673 st=match
H82 n=29 y16=[-5,-12,-7,-13,-22,-12,-12,-8,-1,15,19,25,56,57,85,97,133,154,184,202,240,271,300,340,384,424,466,521,558] SEAS=0.0 a=0.5 b=0.499 FC(30)=37.967466054521566 CI=1.3013536139107378 st=match
H83 n=30 y16=[-230,-124,124,36,66,-38,-102,-72,-450,-220,-152,-112,-478,-636,-206,-204,-714,-752,-318,-728,-710,-604,-564,-680,-566,-1002,-902,-878,-906,-860] SEAS=0.0 a=0.25 b=0.001 FC(31)=-59.57139658604917 CI=23.220518032033922 st=match
H84 n=19 y16=[-6,-9,5,-3,12,12,21,39,46,61,73,93,123,140,161,196,229,262,284] SEAS=0.0 a=0.998 b=0.899 FC(20)=19.195850524346664 CI=1.425019046289249 st=match
H85 n=14 y16=[34,-222,98,-108,-36,-222,-132,-108,-26,114,-240,-376,-388,-266] SEAS=0.0 a=0.002 b=0.001 FC(15)=-16.500873488804157 CI=16.25572552872944 st=match
H86 n=3 y16=[-4,-21,-43] SEAS=0.0 a=0.1 b=0.099 FC(4)=-3.8658751562500004 CI=0.1802772123781471 st=match
H87 n=4 y16=[216,-184,-152,-34] SEAS=0.0 a=0.25 b=0.001 FC(5)=-9.196836064540175 CI=23.05152185688889 st=refused
H88 n=5 y16=[0,35,49,90,121] SEAS=2.0 a=0.002 b=0.001 FC(6)=9.420128273019193 CI=0.6752049114314308 st=refused
H89 n=6 y16=[196,46,-20,-178,12,-96] SEAS=0.0 a=0.5 b=0.001 FC(7)=-8.82827216253369 CI=12.88390874408478 st=refused
H90 n=7 y16=[1,18,39,70,89,127,153] SEAS=0.0 a=0.75 b=0.001 FC(8)=11.134251596205962 CI=0.7984726350221377 st=refused
H91 n=3 y16=[30,-184,-72] SEAS=0.0 a=0.1 b=0.099 FC(4)=-10.319939812500001 CI=11.754074247055188 st=match
H92 n=4 y16=[-2,10,28,26] SEAS=0.0 a=0.126 b=0.001 FC(5)=2.411926408437499 CI=0.6556258405374181 st=refused
H93 n=5 y16=[220,-12,206,-82,186] SEAS=2.0 a=0.126 b=0.001 FC(6)=6.7496182330435746 CI=18.31122599845753 st=refused
H94 n=6 y16=[2,13,51,67,102,132] SEAS=2.0 a=0.5 b=0.001 FC(7)=9.846963524674747 CI=1.0028337226030182 st=refused
H95 n=7 y16=[232,172,-28,-76,-260,146,92] SEAS=0.0 a=0.25 b=0.001 FC(8)=-2.089250409692619 CI=21.66970801027544 st=refused
H96 n=3 y16=[0,9,27] SEAS=0.0 a=0.1 b=0.099 FC(4)=2.45857528125 CI=0.3244989822806647 st=match
H97 n=4 y16=[10,154,232,104] SEAS=0.0 a=0.25 b=0.001 FC(5)=11.224872907754467 CI=11.278136309344658 st=refused
H98 n=5 y16=[-1,-27,-40,-61,-77] SEAS=2.0 a=0.25 b=0.001 FC(6)=-6.013922823688826 CI=0.428398337152761 st=refused
H99 n=6 y16=[-60,-118,226,70,56,-92] SEAS=0.0 a=0.126 b=0.001 FC(7)=-5.776913265306116 CI=16.776053993495513 st=refused
H100 n=7 y16=[8,-4,-24,-20,-16,-26,-19] SEAS=0.0 a=0.75 b=0.001 FC(8)=-1.6101840134248235 CI=1.1278637751671117 st=refused
H101 n=3 y16=[-58,38,170] SEAS=0.0 a=0.1 b=0.099 FC(4)=17.459301125000003 CI=1.2979959291226588 st=match
```

## E.4 Short histories (n = 3..7): complete Excel readings (the short-initialisation block)

n = 3 matches (it is included for the model); n = 4..7 are refused (204 cells). STAT 1..7 in order
(alpha, beta, gamma, MASE, SMAPE, MAE, RMSE) then FC at n+1 and CI(95%) at n+1, all nonseasonal
(seasonality 0 fit; SEAS is automatic detection for the same series).

```
H86 n=3 y16=[-4,-21,-43] SEAS=0.0 STAT1..7=[0.1, 0.099, 2.220446049250313e-16, 0.051239316239316275, 0.04133221239889717, 0.062447916666666714, 0.09197985973219447] FC(4)=-3.8658751562500004 CI=0.1802772123781471 st(fc)=match
H87 n=4 y16=[216,-184,-152,-34] SEAS=0.0 STAT1..7=[0.25, 0.001, 2.220446049250313e-16, 0.7602299311363638, 1.2448383769372215, 8.710967960937502, 11.761196653977501] FC(5)=-9.196836064540175 CI=23.05152185688889 st(fc)=refused
H88 n=5 y16=[0,35,49,90,121] SEAS=2.0 STAT1..7=[0.002, 0.001, 2.220446049250313e-16, 0.11254427053917355, 0.07284385089015988, 0.212779011488125, 0.3444986319939352] FC(6)=9.420128273019193 CI=0.6752049114314308 st(fc)=refused
H89 n=6 y16=[196,46,-20,-178,12,-96] SEAS=0.0 STAT1..7=[0.5, 0.001, 2.220446049250313e-16, 0.6298485665164815, 1.1262274414321995, 5.290727958738445, 6.573543619021273] FC(7)=-8.82827216253369 CI=12.88390874408478 st(fc)=refused
H90 n=7 y16=[1,18,39,70,89,127,153] SEAS=0.0 STAT1..7=[0.75, 0.001, 2.220446049250313e-16, 0.21872802574489408, 0.11171834589806928, 0.34631937409608227, 0.4073914833743825] FC(8)=11.134251596205962 CI=0.7984726350221377 st(fc)=refused
H91 n=3 y16=[30,-184,-72] SEAS=0.0 STAT1..7=[0.1, 0.099, 2.220446049250313e-16, 0.39966666666666667, 0.6526444064352227, 4.071604166666667, 5.997086854539078] FC(4)=-10.319939812500001 CI=11.754074247055188 st(fc)=match
H92 n=4 y16=[-2,10,28,26] SEAS=0.0 STAT1..7=[0.126, 0.001, 2.220446049250313e-16, 0.3579812444196429, 0.19715524912413113, 0.23865416294642858, 0.3345091265497281] FC(5)=2.411926408437499 CI=0.6556258405374181 st(fc)=refused
H93 n=5 y16=[220,-12,206,-82,186] SEAS=2.0 STAT1..7=[0.126, 0.001, 2.220446049250313e-16, 0.4543037274435557, 0.9422622153176452, 7.141086715753391, 9.342633917201617] FC(6)=6.7496182330435746 CI=18.31122599845753 st(fc)=refused
H94 n=6 y16=[2,13,51,67,102,132] SEAS=2.0 STAT1..7=[0.5, 0.001, 2.220446049250313e-16, 0.22609243398042167, 0.16960032155022328, 0.3674002052181852, 0.5116592603299055] FC(7)=9.846963524674747 CI=1.0028337226030182 st(fc)=refused
H95 n=7 y16=[232,172,-28,-76,-260,146,92] SEAS=0.0 STAT1..7=[0.25, 0.001, 2.220446049250313e-16, 0.9421103132795159, 1.4548588666235724, 9.342593940021866, 11.0561766344705] FC(8)=-2.089250409692619 CI=21.66970801027544 st(fc)=refused
H96 n=3 y16=[0,9,27] SEAS=0.0 STAT1..7=[0.1, 0.099, 2.220446049250313e-16, 0.1332222222222222, 0.14457531847583535, 0.11240624999999997, 0.16556374751795] FC(4)=2.45857528125 CI=0.3244989822806647 st(fc)=match
H97 n=4 y16=[10,154,232,104] SEAS=0.0 STAT1..7=[0.25, 0.001, 2.220446049250313e-16, 0.6542379203571429, 0.6020331428738402, 4.7704848359375, 5.754256914058198] FC(5)=11.224872907754467 CI=11.278136309344658 st(fc)=refused
H98 n=5 y16=[-1,-27,-40,-61,-77] SEAS=2.0 STAT1..7=[0.25, 0.001, 2.220446049250313e-16, 0.11206484784112816, 0.07386509310524303, 0.1330770068113397, 0.21857459653948366] FC(6)=-6.013922823688826 CI=0.428398337152761 st(fc)=refused
H99 n=6 y16=[-60,-118,226,70,56,-92] SEAS=0.0 STAT1..7=[0.126, 0.001, 2.220446049250313e-16, 0.7065138694927412, 1.3800093925708274, 6.358624825434671, 8.559368501575994] FC(7)=-5.776913265306116 CI=16.776053993495513 st(fc)=refused
H100 n=7 y16=[8,-4,-24,-20,-16,-26,-19] SEAS=0.0 STAT1..7=[0.75, 0.001, 2.220446049250313e-16, 0.7869265707502694, 0.6234521477551551, 0.46723765138297246, 0.5754512756681028] FC(8)=-1.6101840134248235 CI=1.1278637751671117 st(fc)=refused
H101 n=3 y16=[-58,38,170] SEAS=0.0 STAT1..7=[0.1, 0.099, 2.220446049250313e-16, 0.06310526315789472, 0.134757886686813, 0.4496249999999999, 0.6622549900718] FC(4)=17.459301125000003 CI=1.2979959291226588 st(fc)=match
```

## E.5 Seasonal histories

(a) `dcj9k_r39_seasonal`: exact seasonal-plus-trend series (every cell except SEASONALITY is
reproduced exactly; zero innovations). `p` is the explicit period used for FC/STAT (alpha 0.751, beta 0.001,
gamma 0.001, STAT 4 to 7 all 0 for every one of these); `SEAS` is Excel's automatic detection of the same
series (`0.0` = none). Detection fails to return the true period often.

```
H102 n=20 p=2 y16=[-518,-421,-536,-439,-554,-457,-572,-475,-590,-493,-608,-511,-626,-529,-644,-547,-662,-565,-680,-583] SEAS=2.0 FC(21)=-43.625 stat1,4=['0.751', '0.0']
H103 n=20 p=2 y16=[266,397,264,395,262,393,260,391,258,389,256,387,254,385,252,383,250,381,248,379] SEAS=2.0 FC(21)=15.375 stat1,4=['0.751', '0.0']
H104 n=24 p=4 y16=[150,153,172,91,106,109,128,47,62,65,84,3,18,21,40,-41,-26,-23,-4,-85,-70,-67,-48,-129] SEAS=4.0 FC(25)=-7.125 stat1,4=['0.751', '0.0']
H105 n=32 p=8 y16=[100,87,188,93,40,221,170,57,92,79,180,85,32,213,162,49,84,71,172,77,24,205,154,41,76,63,164,69,16,197,146,33] SEAS=0.0 FC(33)=4.25 stat1,4=['0.751', '0.0']
H106 n=8 p=2 y16=[-442,-475,-464,-497,-486,-519,-508,-541] SEAS=2.0 FC(9)=-33.125 stat1,4=['0.751', '0.0']
H107 n=8 p=4 y16=[116,61,-10,33,80,25,-46,-3] SEAS=0.0 FC(9)=2.75 stat1,4=['0.751', '0.0']
H108 n=22 p=2 y16=[-328,-299,-322,-293,-316,-287,-310,-281,-304,-275,-298,-269,-292,-263,-286,-257,-280,-251,-274,-245,-268,-239] SEAS=2.0 FC(23)=-16.375 stat1,4=['0.751', '0.0']
H109 n=10 p=2 y16=[-282,-160,-302,-180,-322,-200,-342,-220,-362,-240] SEAS=2.0 FC(11)=-23.875 stat1,4=['0.751', '0.0']
H110 n=16 p=4 y16=[-122,-117,-16,-131,-174,-169,-68,-183,-226,-221,-120,-235,-278,-273,-172,-287] SEAS=4.0 FC(17)=-20.625 stat1,4=['0.751', '0.0']
H111 n=16 p=8 y16=[204,174,252,364,374,222,226,410,316,286,364,476,486,334,338,522] SEAS=4.0 FC(17)=26.75 stat1,4=['0.751', '0.0']
H112 n=26 p=2 y16=[-380,-557,-354,-531,-328,-505,-302,-479,-276,-453,-250,-427,-224,-401,-198,-375,-172,-349,-146,-323,-120,-297,-94,-271,-68,-245] SEAS=2.0 FC(27)=-2.625 stat1,4=['0.751', '0.0']
H113 n=12 p=4 y16=[536,440,556,530,600,504,620,594,664,568,684,658] SEAS=2.0 FC(13)=45.5 stat1,4=['0.751', '0.0']
H114 n=16 p=8 y16=[-30,115,78,1,-12,249,100,95,74,219,182,105,92,353,204,199] SEAS=4.0 FC(17)=11.125 stat1,4=['0.751', '0.0']
H115 n=32 p=8 y16=[466,352,416,424,306,254,368,280,450,336,400,408,290,238,352,264,434,320,384,392,274,222,336,248,418,304,368,376,258,206,320,232] SEAS=8.0 FC(33)=25.125 stat1,4=['0.751', '0.0']
H116 n=12 p=2 y16=[460,233,486,259,512,285,538,311,564,337,590,363] SEAS=2.0 FC(13)=38.5 stat1,4=['0.751', '0.0']
H117 n=12 p=2 y16=[472,342,472,342,472,342,472,342,472,342,472,342] SEAS=2.0 FC(13)=29.5 stat1,4=['0.751', '0.0']
H118 n=26 p=2 y16=[-502,-280,-502,-280,-502,-280,-502,-280,-502,-280,-502,-280,-502,-280,-502,-280,-502,-280,-502,-280,-502,-280,-502,-280,-502,-280] SEAS=2.0 FC(27)=-31.375 stat1,4=['0.751', '0.0']
H119 n=8 p=2 y16=[184,-31,170,-45,156,-59,142,-73] SEAS=2.0 FC(9)=8.0 stat1,4=['0.751', '0.0']
H120 n=28 p=2 y16=[248,392,224,368,200,344,176,320,152,296,128,272,104,248,80,224,56,200,32,176,8,152,-16,128,-40,104,-64,80] SEAS=2.0 FC(29)=-5.5 stat1,4=['0.751', '0.0']
H121 n=24 p=8 y16=[528,409,440,467,492,467,384,503,552,433,464,491,516,491,408,527,576,457,488,515,540,515,432,551] SEAS=4.0 FC(25)=37.5 stat1,4=['0.751', '0.0']
H122 n=28 p=4 y16=[342,254,424,430,366,278,448,454,390,302,472,478,414,326,496,502,438,350,520,526,462,374,544,550,486,398,568,574] SEAS=4.0 FC(29)=31.875 stat1,4=['0.751', '0.0']
H123 n=30 p=2 y16=[-80,16,-108,-12,-136,-40,-164,-68,-192,-96,-220,-124,-248,-152,-276,-180,-304,-208,-332,-236,-360,-264,-388,-292,-416,-320,-444,-348,-472,-376] SEAS=2.0 FC(31)=-31.25 stat1,4=['0.751', '0.0']
H124 n=32 p=2 y16=[84,-12,72,-24,60,-36,48,-48,36,-60,24,-72,12,-84,0,-96,-12,-108,-24,-120,-36,-132,-48,-144,-60,-156,-72,-168,-84,-180,-96,-192] SEAS=2.0 FC(33)=-6.75 stat1,4=['0.751', '0.0']
H125 n=12 p=4 y16=[-166,-380,-246,-424,-198,-412,-278,-456,-230,-444,-310,-488] SEAS=2.0 FC(13)=-16.375 stat1,4=['0.751', '0.0']
H126 n=14 p=2 y16=[-2,130,30,162,62,194,94,226,126,258,158,290,190,322] SEAS=2.0 FC(15)=13.875 stat1,4=['0.751', '0.0']
H127 n=24 p=8 y16=[108,211,282,107,334,233,346,271,212,315,386,211,438,337,450,375,316,419,490,315,542,441,554,479] SEAS=0.0 FC(25)=26.25 stat1,4=['0.751', '0.0']
H128 n=22 p=2 y16=[210,149,200,139,190,129,180,119,170,109,160,99,150,89,140,79,130,69,120,59,110,49] SEAS=2.0 FC(23)=6.25 stat1,4=['0.751', '0.0']
H129 n=28 p=2 y16=[-186,-327,-208,-349,-230,-371,-252,-393,-274,-415,-296,-437,-318,-459,-340,-481,-362,-503,-384,-525,-406,-547,-428,-569,-450,-591,-472,-613] SEAS=2.0 FC(29)=-30.875 stat1,4=['0.751', '0.0']
H130 n=8 p=2 y16=[-488,-404,-496,-412,-504,-420,-512,-428] SEAS=2.0 FC(9)=-32.5 stat1,4=['0.751', '0.0']
H131 n=30 p=2 y16=[62,-58,50,-70,38,-82,26,-94,14,-106,2,-118,-10,-130,-22,-142,-34,-154,-46,-166,-58,-178,-70,-190,-82,-202,-94,-214,-106,-226] SEAS=2.0 FC(31)=-7.375 stat1,4=['0.751', '0.0']
H132 n=32 p=2 y16=[100,191,110,201,120,211,130,221,140,231,150,241,160,251,170,261,180,271,190,281,200,291,210,301,220,311,230,321,240,331,250,341] SEAS=2.0 FC(33)=16.25 stat1,4=['0.751', '0.0']
H133 n=30 p=2 y16=[-194,-162,-210,-178,-226,-194,-242,-210,-258,-226,-274,-242,-290,-258,-306,-274,-322,-290,-338,-306,-354,-322,-370,-338,-386,-354,-402,-370,-418,-386] SEAS=2.0 FC(31)=-27.125 stat1,4=['0.751', '0.0']
H134 n=28 p=4 y16=[522,470,386,576,562,510,426,616,602,550,466,656,642,590,506,696,682,630,546,736,722,670,586,776,762,710,626,816] SEAS=4.0 FC(29)=50.125 stat1,4=['0.751', '0.0']
H135 n=16 p=8 y16=[-134,-85,-212,-229,-82,-283,-68,-137,-174,-125,-252,-269,-122,-323,-108,-177] SEAS=3.0 FC(17)=-13.375 stat1,4=['0.751', '0.0']
H136 n=12 p=4 y16=[-358,-396,-408,-482,-302,-340,-352,-426,-246,-284,-296,-370] SEAS=4.0 FC(13)=-11.875 stat1,4=['0.751', '0.0']
H137 n=8 p=4 y16=[280,264,192,278,320,304,232,318] SEAS=0.0 FC(9)=22.5 stat1,4=['0.751', '0.0']
H138 n=32 p=8 y16=[378,408,354,426,500,412,524,410,346,376,322,394,468,380,492,378,314,344,290,362,436,348,460,346,282,312,258,330,404,316,428,314] SEAS=8.0 FC(33)=15.625 stat1,4=['0.751', '0.0']
H139 n=24 p=8 y16=[-38,32,-42,12,40,-132,108,76,-54,16,-58,-4,24,-148,92,60,-70,0,-74,-20,8,-164,76,44] SEAS=3.0 FC(25)=-5.375 stat1,4=['0.751', '0.0']
H140 n=8 p=2 y16=[-256,-296,-272,-312,-288,-328,-304,-344] SEAS=2.0 FC(9)=-20.0 stat1,4=['0.751', '0.0']
H141 n=32 p=8 y16=[256,164,238,250,104,340,260,248,320,228,302,314,168,404,324,312,384,292,366,378,232,468,388,376,448,356,430,442,296,532,452,440] SEAS=3.0 FC(33)=32.0 stat1,4=['0.751', '0.0']
```

(b) The refused seasonal cells (explicit period; deduped; completion and aggregation options make no
difference, since the timelines have no gaps). FC and CI are at the target n+1 (CI at 95%).

```
y={2;7;-1;4} target=5 period=2 FC=['1.5926883521785709'] CI=['6.140512107538674'] why=['seas:length_cycle_limits']
y={2;3.5;3.75;5.75} target=5 period=2 FC=['6.678586669633923'] CI=['0.7038800985387762'] why=['seas:length_cycle_limits']
y={2;7;-1;4;2;7;-1;4} target=9 period=2 FC=['-0.6284500063736136'] CI=['2.8926394216792626'] why=['seas:nonzero_innovations']
y={2;3.5;3.75;5.75;5.5;7;8.5;8.75} target=9 period=2 FC=['9.578221436991228'] CI=['0.8247073546568308'] why=['seas:nonzero_innovations']
y={2;3.5;3.75;5.75;5.5;7;8.5;8.75} target=9 period=4 FC=['9.202832547517541'] CI=['0.8278620082743089'] why=['seas:nonzero_innovations']
y={2;7;-1;4;2;7;-1;4;2;7;-1;4} target=13 period=2 FC=['-0.18957383005877881'] CI=['3.1281816983529667'] why=['seas:nonzero_innovations']
y={2;3.5;3.75;5.75;5.5;7;8.5;8.75;10.75;10.5;12;13.5} target=13 period=2 FC=['14.131841913720057'] CI=['1.0312125552012186'] why=['seas:nonzero_innovations']
y={2;3.5;3.75;5.75;5.5;7;8.5;8.75;10.75;10.5;12;13.5} target=13 period=4 FC=['14.176743590968009'] CI=['1.258746918616139'] why=['seas:nonzero_innovations']
y={2;7;-1;4;2;7;-1;4;2;7;-1;4;2;7;-1;4;2;7;-1;4;2;7;-1;4} target=25 period=2 FC=['0.07223649233009377'] CI=['3.2232921779553756'] why=['seas:nonzero_innovations']
y={2;3.5;3.75;5.75;5.5;7;8.5;8.75;10.75;10.5;12;13.5;13.75;15.75;15.5;17;18.5;18.75;20.75;20.5;22;23.5;23.75;25.75} target=25 period=2 FC=['26.047053204770208'] CI=['0.9778435126421993'] why=['seas:nonzero_innovations']
y={2;3.5;3.75;5.75;5.5;7;8.5;8.75;10.75;10.5;12;13.5;13.75;15.75;15.5;17;18.5;18.75;20.75;20.5;22;23.5;23.75;25.75} target=25 period=4 FC=['26.06664507379056'] CI=['0.9549771734176575'] why=['seas:nonzero_innovations']
```

The r38 base series of the other 20 histories are: constants 5, affine 2+3k (n = 4, 8, 12, 24), series
{2;3;5;7;11;13;17;19} on timelines 1,2,3,5,6,7,8,9 (missing 4), the reverse timeline, duplicate
timestamp 2, the sparse timeline {1,2,4,7,11,16,22,29}, constant timestamps, and a length mismatch.
SEASONALITY for the missing-point and reverse timelines is 0, for the duplicate timeline 2.

## E.6 A worked nonseasonal example (to check the recurrence)

H74, y = {0.3125; -1.6875; -3.125; -3.8125; -5.375; -5.5625; -6.8125; -7.8125}, timeline 1..8.
n = 8 <= 10, so there is no validation window (the whole series is training). Initial slope
(OLS on index 1..8) = -1.087797619047619. Training SSE (sequence of squared one-step errors from the
recurrence with y[0] as level, errors[0] = 0) for the best candidates in the table order of B.2:
index 3 (0.75, 0.001) 2.049155796622682; index 0 (0.9, 0.001) 2.0937728304991023; index 7 (0.5, 0.001)
2.2076891615297307; index 19 (0.998, 0.099) 2.379147813355; index 5 (0.666667, 0.166667) 2.4476594594216143;
index 11 (0.25, 0.001) 2.7534734699039562. Excel: alpha 0.75, beta 0.001, gamma 2^-52, MASE
0.3330018450406527, SMAPE 0.15510931040434275, MAE 0.3865199987079005, RMSE 0.506107176967325.
Our code reproduces all of them exactly.

The last-bit mismatches we still have (31 cells; none about the shape): the 10 FORECAST cells of
{2;7;-1;4;2;7;-1;4} at target 9 with seasonality 0 (Excel 1.678952143543242, ours 1.6789521435432422,
one ulp, for every completion/aggregation option); 9 STAT cells: H38 STAT 4..7 (3 to 5 ulp), H50 STAT
4..7 (3 to 11 ulp), H83 STAT 4 (1 ulp); and 12 CONFINT cells one or two ulp off (z-quantile or product
arithmetic). H38 and H50 are 22- and 28-point trend-plus-noise histories.

---

# Appendix F: what we can do with Excel (for your experiments)

Our answer service evaluates any single-cell formula in Excel, returns the double (as bits) or the
error, about 0.2 s per question, several thousand per hour. It accepts array literals up to 128
numbers. It can call all four ETS functions with any options. We can build any number series (we can
generate exact decimal text, subnormals, huge exponents), unordered or duplicated timelines, missing
points. We cannot observe internal state except through the outputs: SEASONALITY (an integer),
STAT 1 to 8 (alpha, beta, gamma, four error scores, step), FORECAST at any horizon, CONFINT at any
horizon and confidence. Each function recomputes the whole fit, so alpha and beta are readable for any
history; for forecasts at horizons 1, 2, 3 ... the differences reveal trend (and seasonal) states.
Experiments should be small designed batches (tens to a few hundred questions) with a hypothesis
table per question, not statistical fits.
