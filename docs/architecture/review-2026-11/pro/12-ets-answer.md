# Reconstructing Excel FORECAST.ETS from the supplied oracle evidence

**Scope:** DNA Calc commit 1ece3d0ffeae28f27aa0ff28c2d873e3a6c2af84; Excel 16.0 build 20527; the attached question's appendices A–F.

## Principal finding

The appendices identify a substantial part of the implementation, but they do **not** identify a complete bit-exact algorithm for the 952 refused cells. In particular, a list of reported winners cannot identify all evaluated candidates or their generation, and one forecast plus one horizon-one confidence interval cannot identify a noisy seasonal fit.

My strongest reconstruction is a small deterministic parameter search around an innovations-form AAA engine, with the measured frozen validation, a separate application pass, and the measured nonseasonal trend correction. The automatic detector probably belongs to a spectrum-plus-correlation family. Its thresholds, arithmetic, and selection rules remain unknown. Supplying numerical constants for those gaps would be invention.

There are nevertheless several concrete advances:

1. The 40 cycle-limit refusals have a measured routing correction, but that correction alone yields **zero additional matching cells**: those histories then encounter short initialization.
2. An exactly tied, nonzero objective can be constructed without relying on an affine zero-error shortcut. One example makes every candidate's search SSE exactly 202.
3. A second construction ties precisely the familiar 0.75/0.001 and 0.9/0.001 candidates, with controlled perturbations selecting either side. It also tests whether a supposedly reset-only point is really an optimization candidate.
4. Seven-point histories isolate initialization from extension. Several supplied cases also exclude final-division double rounding as the explanation.
5. The measured confidence-probability fallback provides a direct RMS-error observation that bypasses the inverse-normal kernel.
6. A short seasonal probe batch supplies enough observations to recover terminal states and discriminate initializers.

No numerical programs, optimizer trials, or Excel experiments were run for this answer. The new numerical constructions below are analytical deductions. Sources used are the supplied evidence, public mathematical descriptions, and permitted public documentation; no implementation from a restricted source was inspected or used.

### Evidence labels

- **M** — measured behavior reported in the supplied appendices, within the stated region.
- **I** — a deduction from those measurements or from explicitly stated mathematics.
- **H** — an unverified, falsifiable hypothesis.
- **P** — a public description or mathematical result, not proof of Excel's implementation.

A code block inherits its stated label. A proposed experimental program is not a claim that Excel implements it.

# A. The most plausible algorithm, with its evidence boundaries

## A1. Validation and preparation

The following rules should be preserved from Appendix A and B.1/B.6.

| Status | Stage | Specification |
|---|---|---|
| M | Argument lengths | Different value/timeline lengths produce #N/A before option validation. |
| M | Options | Nonintegral or out-of-range statistic, aggregation, completion, confidence, or seasonality produce #NUM! in the measured order. Statistic is 1–8, aggregation 1–7, completion 0 or 1, confidence in [0,1), seasonality an integer in [0,8784]. |
| M | Observation count | Fewer than two observations produce #DIV/0!; fewer than two distinct timestamps produce #NUM!. |
| M | Sorting | Sort timestamp/value pairs ascending; aggregate duplicate timestamps. Preserve the original-order flag because the before-first-target behavior is not fully order invariant. |
| M | Aggregation codes | 1 average, 2 count, 3 counta, 4 maximum, 5 median, 6 minimum, 7 sum, for the numeric inputs studied. |
| M | Step | For the measured integral-step region, use the smallest consecutive distinct-timestamp gap. |
| M | Missing points | With prepared count N and distinct observed count D, reject when 10(N−D)>3N. Equality is admitted. |
| M | Completion | Option 0 supplies zero. Option 1 uses linear interpolation between the bracketing observed values. |
| M | Future index | Reject an integer future horizon exceeding 2^31−1. |
| M | Target checks | An ascending-input target before the first timestamp gives #NUM!; CONFINT requires target strictly beyond the last timestamp. |
| M | Early outputs | STAT 8 returns the prepared step; confidence zero returns zero, after the preceding applicable validations. |
| H/open | Remaining preparation | Calendar inference, fractional horizons, duplicate accumulation order outside the exact region, and the reversed-input before-first-target exception remain unidentified. |

The interpolation expression used in the supplied implementation is

\[
y_L+(y_R-y_L)\left(\frac{j}{g}\right).
\]

**M within the exact region; H outside it:** this expression preserves the known results, but algebraically equivalent interpolation formulas need not have the same rounding or overflow behavior.

**I:** The limits of 128 prepared points, 32 fitted observations, period at most eight, exact intermediate arithmetic, and magnitudes between 2^-400 and 2^400 are *DNA Calc verification guards*. They are not measured Excel domain limits. Likewise, B.5's complete-cycle and power-of-two guards must not be presented as Excel's seasonal admission rules.

Microsoft's current function documentation contains statements about duplicate timestamps and aggregation defaults that conflict with the supplied measurements. The recorded Excel outputs take precedence here. [P4]

## A2. Routing and automatic seasonality

**M:** Automatic SEASONALITY is zero for prepared lengths two through four. An explicit period greater than floor(n/2) uses the nonseasonal fit. Explicit period two at length four also uses the nonseasonal fit. Exact affine histories and the measured nonzero alternating affine histories retain their established behavior.

**I:** Keep three concepts separate:

1. the integer returned by automatic detection;
2. whether an explicit requested period is admitted;
3. the effective state dimension of the numerical engine.

They differ: a mathematically period-eight series can detect as three, and a nominally nonseasonal fit has two tiny seasonal states.

**H, highest-ranked family:** automatic detection removes a linear trend, obtains spectral candidates, then uses correlation and peak-shape checks. A strict automatic eligibility bound 2p<n is worth testing independently.

**P:** Microsoft's Power View description discusses detrending and normalization, three spectral peaks and neighboring-bin intervals, Pearson/Spearman checks, two-segment hill tests, harmonic handling, power-ratio selection, and common-period preferences. It also describes frozen validation and trend correction. Its 40% missing-data rule differs from the worksheet's measured 30%, and a comparison described as an autocorrelation “value” below six is ambiguous. It supports a family, not a worksheet specification. [P1]

**P/I:** AUTOPERIOD in Vlachos, Yu, Castelli and Meek supplies a concrete published relative: periodogram hints followed by autocorrelation hill validation using two fitted line segments. This is a useful source of experimental hypotheses, not evidence that Excel copied that algorithm. The paper's significance procedure must not be imported without measurements. [P2]

**I, a useful exclusion:** H105 and H127 have zero-error explicit period-eight fits but automatic detection returns zero. Their validation tails are not affine-plus-period-two, so even the hidden two-state nonseasonal frozen forecast cannot predict them with zero error. Therefore detection is not simply exhaustive minimization of the measured SSE over every admitted period with the same initialization. Screening, a distinct score, or a model-selection penalty is necessary to rescue such a hypothesis.

Rejecting the 81 tested ACF programs does not reject every possible ACF hybrid.

## A3. Nonseasonal initialization

Use zero-based observation indices in this answer. The regression coordinate in B.2 is x=j+1.

**M, exact region:** Let r=OLS(y). If n<7, append

\[
E_j=y_{j\bmod2}+(j-j\bmod2)r,\qquad j=n,\ldots,6.
\]

Otherwise E=y. The initial trend is OLS(E), and the initial level is y_0. Both initial seasonal states are zero; gamma is 2^-52.

**M:** Short-history parameter search uses the extended history; the final application pass uses the actual history. For n=7 there is no extension, even though the implementation's exactness guard can still refuse it.

**M, supplied baseline arithmetic:** OLS accumulates sx, sy, sxx, sxy in forward observation order and forms

\[
\frac{n\,sxy-sx\,sy}{n\,sxx-sx^2}.
\]

**H/open:** This expression is not a proved recipe for Excel's nonexact short initialization. R47/R48 already refuted many arithmetic variants. Do not promote another one merely because it fits the printed decimals.

### Exact algebraic invariants

**I:** Eliminating the extension algebraically gives the following *real-arithmetic* initial slopes:

| n | B0 as a weighted sum of the original observations |
|---:|---|
| 3 | (−y0+y2)/2 |
| 4 | (−8y0−3y1+2y2+9y3)/28 |
| 5 | (−26y0−13y1−5y2+13y3+31y4)/140 |
| 6 | (−90y0−124y1−53y2+18y3+89y4+160y5)/980 |
| 7 | (−3y0−2y1−y2+y4+2y5+3y6)/28 |

These are diagnosis tools, **not replacement floating-point kernels**. Rounding the extension and then regressing is different from rounding a simplified rational expression once.

Useful particular values, using the actual observations y16/16:

| History | Exact-real initial slope | What it isolates |
|---|---:|---|
| H90 | 181/112 | Seven points, no extension |
| H95 | −11/7 | Seven points, no extension |
| H100 | −117/448 | Seven points, no extension |
| H97 | 429/224 | Original slope is exactly 9/4; extension is exact; second regression is non-dyadic |
| H87 | −893/224 | Original slope −359/80 is already non-dyadic |

**I:** For H90/H95/H100, the raw-moment sums and products on these small dyadic inputs are exact, and the remaining division has odd denominator seven. H97's exact extension has the same property. A rational with odd denominator seven is at least 1/14 of a binary64 ulp away from a rounding midpoint after scaling into that binade. A PC64 rounding contributes at most about 1/4096 of that ulp. Consequently, changing **only the final division** between RN53 and RN53-after-RN64 cannot change these results.

If their forecasts still differ, investigate normalization, a different initial state, or another operation; do not keep varying only that final division.

## A4. State recurrence and search score

**M for the nonseasonal engine; conditional measured seasonal support in R39:**

~~~text
errors[0] = 0
predictions[0] = y[0]

for j = 1 .. n-1:
    q = j mod period
    prediction = (level + trend) + seasons[q]
    error = y[j] - prediction
    save prediction and error

    if this is the search pass and j >= validation_start:
        level = level + trend
    else:
        level = level + (trend + alpha * error)
        trend = trend + beta * error
        seasons[q] = seasons[q] + gamma * error

    save trend
~~~

Parentheses are part of the supplied arithmetic shape. In particular, level+(trend+alpha*error) must not be rewritten as (level+trend)+alpha*error without evidence. Do not introduce FMA contraction.

**M:** For search-history length m,

\[
w(m)=
\begin{cases}
m,&m\le10,\\
\min(10,\lfloor0.4m\rfloor),&m>10.
\end{cases}
\]

There is no frozen validation segment for m≤10. Otherwise validation starts at m−w(m). The supplied score sums squared errors across **the entire search history**, including training and frozen validation.

**M:** After choosing the parameters, rerun the recurrence on the original observations with updates enabled everywhere. STAT residuals come from this application pass, not from the frozen search run.

**I:** Initialization in the supplied model uses the complete search history. Freezing validation updates therefore does not by itself make the initializer independent of validation observations. Experiments must preserve the regression slope or explicitly model its change.

**I:** With a fixed residual count, SSE, MSE, and the profiled additive Gaussian likelihood have the same minimizer in exact arithmetic. Rounded score transformations can still create additional ties. Separate “which mathematical objective?” from “which scalar expression was compared?”

## A5. Candidate generation, bounds, and stopping

**M:** The supplied evidence contains 30 distinct *reported* alpha/beta pairs. Five additional low-beta pairs can arise from the post-fit reset. This is not a trace of 30 evaluated points.

The 25-point research pool, excluding those five purported reset-only outputs, has this decomposition:

| Group | Points |
|---|---|
| Seven floor points | beta=.001, alpha in {.002,.1,.126,.25,.5,.75,.9} |
| Six near-diagonal points | (.1,.099),(.126,.125),(.25,.249),(.5,.499),(.75,.749),(.9,.899) |
| Five right-edge points | alpha=.998, beta in {.099,.249,.499,.749,.899} |
| Seven interior points | (.166667,.0833333),(.333333,.166667),(.5,.25),(.666667,.166667),(.666667,.5),(.833333,.0833333),(.833333,.75) |

Use the nearest binary64 values of the displayed decimals when reproducing this pool. Do not substitute exact rational sixths or twelfths.

**M:** B.2 contains only 23 of these points. The 25-point program plus a fitted stable-first order still fails the E.2 tie cloud. This refutes that complete program; it does not isolate whether the error lies in the pool, initial states, loads, arithmetic, or comparison order.

**H, plausible bounds:**

\[
.002\le\alpha\le.998,\qquad .001\le\beta\le\alpha-.001.
\]

For actual seasonality, a plausible additional restriction is

\[
.001\le\gamma\le1-\alpha-.001.
\]

The nonseasonal epsilon-gamma branch is an explicit exception. These are candidate domain rules inferred from outputs, not measured admissibility bounds. Conventional innovations-parameter restrictions explain why beta is bounded by alpha and gamma by 1−alpha; they do not establish Excel's offsets. [P3]

**H, most plausible search shape:** a small deterministic design or mesh with bounded adjustment, using decimal-loaded constants. Next come a short adaptive mesh search and a heavily bounded/early-stopped simplex-style method.

The midpoints

\[
\frac{.002+.25}{2}=.126,\qquad
\frac{.001+.249}{2}=.125
\]

are a useful bisection clue. Thirds and sixths are compatible with centroids, but do not prove Nelder–Mead. A two-dimensional simplex has two non-worst vertices in its ordinary centroid calculation; “there are thirds, therefore Nelder–Mead” is not an adequate explanation.

**H:** Six significant decimal digits suggest hardcoded generated constants or a decimal round trip. C's default formatted precision supplies a plausible mechanism. It does not prove that formatting occurs inside Excel. [P5]

**Open:** The evaluated set, candidate generation, stopping rule, and universal tie rule are not identified. The correct answer cannot assign them invented exact constants.

## A6. Nonseasonal correction after fitting

**M:** Preserve B.3's reset literally. Let the final trend array contain n−1 transitions.

~~~text
selected_window = n                 if n <= 10
                  ceil(n/2)         otherwise
count = selected_window - 1
tail = last count fitted trends

divisor_window = n                  if n <= 10
                 floor(n/2)         otherwise
d = divisor_window - 1

total   = forward stored-PC64 sum(tail)
squares = forward stored-PC64 sum(stored-PC64 t*t)
mean    = stored-PC64(total / d)
variance = stored-PC64(
    stored-PC64(squares / d) - stored-PC64(mean * mean))

reset = abs(mean) <
        stored-PC64(stored-PC64(sqrt(variance)*divisor_window) / d)

if reset:
    level = y[n-1] - seasons[(n-1) mod 2]
    trend = initial_slope(original_y)
    reported_beta = .001

center = (seasons[0] + seasons[1]) / 2
level = level + center
seasons[0] = seasons[0] - center
seasons[1] = seasons[1] - center
~~~

Stored-PC64 means the supplied RN53(RN64(operation)) primitive, not a continuously extended accumulator. Preserve the separately measured square root and operation boundaries from B.3.

**I:** Do not replace the raw moment variance with a numerically improved centered variance. Do not “fix” the odd-length count mismatch. Do not clamp a negative variance to zero without evidence: the resulting NaN comparison can affect reset.

**M:** Statistics are saved before the reset. The reported beta is changed by reset; the application is not rerun with that new beta. The supplied confidence formula does use the beta after correction.

**H/open:** The corresponding reset and seasonal centering for arbitrary period p are not established by zero-error seasonal examples. B.3 explicitly uses two states. Averaging all p seasonal states is a sensible hypothesis, not a measured generalization.

## A7. Forecasting, STAT outputs, and confidence intervals

### Forecast

**M for the measured nonseasonal integer-horizon region:**

\[
F_h=XADD\!\left(
 \operatorname{fl}(L+\operatorname{fl}(hB)),
 S_{(n-1+h)\bmod2}
\right).
\]

XADD denotes the measured final stored-PC64 addition. The preceding multiplication and addition retain B.4's binary64 shape.

**H for general seasonal publication:** replace two by p in the phase index and preserve the same structure until measured otherwise. Exact affine-seasonal formulas identify the mathematical forecast, but not its cancellation behavior near zero. R48's withdrawn forecasts are a warning against extending the exact region casually.

### STAT

**M for the nonseasonal model:** let w=w(n), start=n−w, and use the application-pass errors e_j and predictions f_j.

| STAT | Value |
|---:|---|
| 1 | Selected alpha |
| 2 | Selected beta, or .001 after reset |
| 3 | 2^-52 |
| 4 | MAE divided by the measured naive scale; zero if the scale is zero |
| 5 | Mean of abs(e_j)/((abs(y_j)+abs(f_j))/2), treating a zero denominator as a zero contribution |
| 6 | Forward sum of abs(e_j), j=start..n−1, divided by w |
| 7 | sqrt((forward sum e_j², j=start..n−1) / w) |
| 8 | Prepared timeline step |

The MASE scale uses lag-one differences in the leading prefix:

\[
u=\begin{cases}n,&start=0,\\start,&start>0,\end{cases}
\qquad
\text{scale}=\frac{\sum_{j=1}^{u-1}|y_j-y_{j-1}|}{u-1}.
\]

**H/open:** For noisy seasonal data, neither lag-one versus lag-p MASE scaling nor tail versus full-sample statistics is established by the supplied zero-error corpus. Measure them.

### CONFINT

**M for the nonseasonal model:** use full application-pass SSE/n, even when STAT 7 uses only a tail. With h>0, g=2^-52 and k=floor((h−1)/2),

\[
V_h=1+(h-1)
 \left[\left(h\alpha\beta+
 \frac{h(2h-1)\beta^2}{6}\right)+\alpha^2\right]
+gk[(2\alpha+g)+2\beta(k+1)].
\]

The displayed grouping follows B.4; its individual binary64 operations must be retained.

\[
q=\operatorname{fl}\!\left(\operatorname{fl}(1+c)/2\right),\qquad
z=\begin{cases}1,&q=1,\\\mathrm{NORM.S.INV\ kernel}(q),&q<1.\end{cases}
\]

Finally,

\[
C_h=XMUL\!\left(z,\sqrt{\operatorname{fl}(\operatorname{fl}(SSE/n)V_h)}\right).
\]

XMUL is the measured stored-PC64 final product. The inverse-normal kernel remains its own bit-exact problem.

**P/H:** The natural general-seasonal variance is

\[
V_h=1+(h-1)\left[\alpha^2+h\alpha\beta+
\frac{h(2h-1)\beta^2}{6}\right]
+\gamma k[2\alpha+\gamma+p\beta(k+1)],
\quad k=\lfloor(h-1)/p\rfloor.
\]

This is standard AAA mathematics and reduces to the measured p=2 expression. It does not establish Excel's noisy-seasonal evaluation order, variance divisor, or use of corrected parameters. [P3]

## A8. Seasonal initial states

**M in the exact zero-innovation region:** B.5's initial states reproduce the accepted observations:

\[
B_0=\frac{\sum_{j=0}^{p-1}(y_{j+p}-y_j)}{p^2},
\quad A_j=y_j-jB_0,
\quad L_0=\frac1p\sum_j A_j,
\quad S_j=A_j-L_0.
\]

The default triple is (.751,.001,.001), and every error statistic is zero.

**H outside that region:** this first-two-cycle construction is the simplest extension to test, but it is not identified by the zero-error data. Phase-wise regression over all observations and multi-cycle phase averaging can agree on every exact affine-periodic series while differing on noisy ones.

**I:** The nine noisy seasonal cases in E.5(b) have only F1 and C1. Under AAA, V1=1; their confidence intervals contain no direct information about gamma's delayed covariance contribution. It is impossible to infer an exact gamma candidate set or initializer from those readings alone.

# B. Ranked hypotheses and discriminating experiments

Unless a probe says otherwise, use timeline 1..n, completion 1, aggregation 1, integer forecast horizons, and explicit seasonality zero for nonseasonal probes. Preserve the full returned bits. Known E-series values are y16 divided by 16.

The rankings below prefer a small, ordinary C implementation. A family without specified thresholds does not make a unique numerical prediction. Where that is the situation, the tables give a precise conditional signature instead of inventing an Excel answer.

## B1. Candidate generation and six significant digits

| Rank | Hypothesis | Reason | Discriminating observation |
|---:|---|---|---|
| 1 | A small fixed/generated design loaded from decimal constants | A tiny repeated family and decimal sixths are a strong fingerprint; inexpensive and simple | Designed winner regions continue to select only the pool; a clearly better unlisted point is never returned |
| 2 | A coarse bounded design plus a small number of adaptive refinements, with quantized loads | Explains midpoint .126/.125 and data-dependent exceptions | New winners or inconsistent pair precedence on well-separated scores refute a fixed pool/order |
| 3 | A bounded simplex or other direct search, stopped early and possibly quantized | Centroid-like coordinates are suggestive | Outputs outside the fixed family, or path-dependent choices unexplained by a fixed argmin, support it; ordinary converged continuous optimization is a poor fit to the observations |

The six-digit origin has a separate ranking:

1. Decimal constants generated offline and embedded.
2. Decimal conversion before scoring/application.
3. Output-only conversion of more precise internal gains.

The first two may be observationally identical. Their source-code distinction need not be recoverable.

**Concrete controls:** use E.2 case 812 and case 1362 exactly as printed, and their power-of-two-scaled versions. Read STAT 1–3 and F1,F2,F3. The baseline E.1 program's published predictions are already different from Excel:

| Case | Baseline program | Recorded Excel |
|---|---|---|
| 812 | .833333/.0833333 | .75/.001 |
| 1362 | .126/.001 | .1/.001 |

A proposed generator plus score arithmetic must reproduce these; matching the ordinary corpus does not waive them.

For an output-only-quantization hypothesis, recover the effective gains from forecast perturbation responses as in D3. It predicts responses corresponding to unrounded internal gains, while a decimal-load hypothesis predicts the displayed decimal gains. R47's load experiments already constrain this in part; do not reopen their established cases without a new discriminator.

## B2. Score, tie rule, and accumulation precision

Rank the unresolved variants as:

1. Direct SSE with strict improvement and first retention.
2. Direct SSE with later replacement on equality, or a data-dependent candidate traversal.
3. A rounded monotone score transformation such as SSE/m or sqrt(SSE/m), possibly compared with a tolerance.

The first ranking is supported by zero-objective defaults, but a zero-objective shortcut could explain those defaults independently of tie handling.

### Probe T0: a nonzero exact tie across every candidate

Use:

~~~text
y = {10000;11000;12000;13000;14000;15000;
     16000;17000;18000;19000;20011;20991}
t = {1;2;3;4;5;6;7;8;9;10;11;12}
seasonality = 0
read STAT 1 and STAT 2
~~~

The last two observations differ from a slope-1000 line by +11 and −9. They preserve the full-history OLS slope:

\[
12(11\cdot11-12\cdot9)-78(11-9)=0.
\]

The training observations are exactly affine. Validation begins at index eight. Its errors are 0,0,11,−9, so **every candidate has search SSE exactly 202**. This is a non-affine history; the exact-affine shortcut does not apply.

| Complete comparison hypothesis | Predicted STAT 1 / STAT 2 |
|---|---|
| First retained, with default first | .9 / .001 |
| Last retained, B.2's 23-point order | .998 / .899 |
| Last retained, E.1's 25-point research order | .998 / .249 |

The large underlying trend keeps the relevant fitted trend far from a significance reset. An unexpected pair refutes one or more assumptions about initialization, objective, traversal, or default handling. It does not by itself identify a new optimizer.

Repeat with all values multiplied by 2^-100 and by 2^100. Under scale-invariant SSE and the same tie rule, the parameter pair is unchanged. These scalings stay far from underflow and overflow.

### Probe T1: a controlled two-candidate tie

Use:

~~~text
y = {1000000;1100000;1200000;1300000;1400000;1500000;
     1600000;1702000;1826170;1880450;2000000;2100000}
~~~

This is a slope-100000 line with perturbations +2000,+26170,−19550 at zero-based indices 7,8,9. The OLS slope is unchanged because

\[
3(2000)+5(26170)-7(19550)=0.
\]

There is one training error, 2000. For a candidate (a,b), the four frozen-validation errors divided by 2000 are

\[
d_r-a-rb,\quad r=1,\ldots,4,\qquad
d=(13.085,-9.775,0,0).
\]

Thus

\[
S(a,b)=2000^2\left[1+\sum_{r=1}^4(d_r-a-rb)^2\right].
\]

For b=.001, the unrestricted best alpha is .825. Within the stated 25-point pool, the two minima are:

| Candidate | Validation errors |
|---|---|
| A=(.75,.001) | 24668, −21054, −1506, −1508 |
| B=(.9,.001) | 24368, −21354, −1806, −1808 |

The B residual vector is A minus 300 in every coordinate; A's residual sum is 600. Hence the SSE difference is

\[
-600(600)+4(300^2)=0.
\]

The common complete score is **1,060,323,240**. All these squared terms and sums are exact binary64 integers under the supplied stored recurrence. The tiny seasonal correction rounds away in the search predictions at this baseline.

The other points in the 25-point pool have larger real scores. A convenient check is that the score increases with beta throughout the relevant positive domain, while the nearest two available floor-alpha values straddle .825 equally.

Read STAT 1, preferably also STAT 2:

| Input | Strict first using the familiar pool/order | Opposite tie precedence |
|---|---|---|
| T1 | alpha .9 | alpha .75 |
| T1 with index 10 increased by 11 and index 11 decreased by 9 | alpha .9 | alpha .9 |
| T1 with index 10 decreased by 11 and index 11 increased by 9 | alpha .75 | alpha .75 |

The last-two-point perturbations preserve OLS and move the score difference away from equality. The +11/−9 variant favors B by 1200 in SSE; the opposite variant favors A by 1200.

**Additional candidate-set test:** (.833333,.001), if genuinely evaluated, would beat both A and B. A returned .833333 with beta .001 on this strong-trend input therefore challenges the claim that this pair can only be a reset output. Conversely, not returning it establishes only that it did not win; it does not prove no other traversal ever visits it.

### Probe T2: turn the same tie into a precision test

Add K times (1,−2,1,0) to T1's validation tail, where K=2^28. This vector has both zero sum and zero first moment, so it preserves the OLS slope and adds the same *real* score increment to every candidate.

The literal series is:

~~~text
{1000000;1100000;1200000;1300000;1400000;1500000;
 1600000;1702000;270261626;-534990462;270435456;2100000}
~~~

The exact tied score is

\[
S=1,060,323,240+130,540K+6K^2.
\]

At K=2^28, the binary64 score spacing is 64 and S mod 64=40.

For **direct SSE comparison**, the analytically predicted results are:

| Arithmetic program | Score for A | Score for B | Alpha selected if T1 established first-retention |
|---|---:|---:|---:|
| Binary64 squared products, forward binary64 accumulation | S−40 | S+24 | .75 |
| Exact PC64 products and accumulator, with only one final binary64 score store | S+24 | S+24 | .9 |
| Binary64 products, extended accumulator | Smaller for A | Larger for B | .75 |

For A, the relevant product rounding errors are 0,−4,−4,0; subsequent forward summation contributes the rest. For B, they are 0,+28,−4,0. PC64 can represent these integer products and totals exactly before the final store.

Read **alpha**, since the actual-data application pass can reset beta after the large tail excursions.

This probe distinguishes the specified arithmetic programs, not every possible PC64 store schedule. A rounded MSE/RMSE comparison may merge distinct SSE values, so establish score transformation separately before attributing the result solely to accumulator precision.

### Score transformation and tolerance controls

Use T1 and its balanced last-two perturbations scaled by 2^k for k in {−100,0,100}, then reduce the balanced perturbation amplitude through exact powers of two while preserving the initialized slope.

- Direct SSE with exact strict comparison preserves which side wins until numerical rounding creates an actual tie.
- An absolute improvement tolerance can switch to default retention at a scale-dependent boundary.
- A relative tolerance is more nearly scale invariant.
- Comparing rounded SSE/m or sqrt(SSE/m) may create a wider tie interval than comparing SSE.

The hypotheses predict transition structure, not an arbitrary tolerance constant. Locate a boundary, then use adjacent representable inputs to identify its comparison.

## B3. Automatic detection

Rank:

1. Detrended spectral candidates plus Pearson/Spearman and hill checks.
2. A simpler spectral candidate generator with Pearson-only refinement/gating.
3. Spectral screening followed by ETS fit comparison or a penalty.

The following probes separate components of these families without assuming their unknown acceptance thresholds.

### D0: spectrum versus ranks

Repeat each eight-value cycle four times:

~~~text
S = {1;2;-2;-1;-1;-2;2;1}
T = {1;8;-8;-1;-1;-8;8;1}
~~~

T is the coordinatewise cube of S. Both full histories have zero OLS slope and zero mean; their ranks and tied ranks are identical.

Their dominant spectral periods differ. Writing A=cos(pi/8), B=sin(pi/8), the period-eight and period-8/3 amplitudes are proportional to:

| Series | Period 8 | Period 8/3 |
|---|---:|---:|
| S | 4(A+2B) | 4(2A−B) |
| T | 4(A+8B) | 4(8A−B) |

For S the period-eight component is larger; for T the period-8/3 component is larger, by a power ratio about 3.09.

Read SEASONALITY for both:

| Fully stated component hypothesis | Predicted pair |
|---|---|
| Largest spectral peak, round its period to nearest integer | (8,3) |
| Smallest perfect Pearson lag with all lags admitted | (8,8) |
| Smallest perfect Spearman lag with all lags admitted | (8,8) |
| Entire decision depends only on ranks and lag counts | Equal outputs, not necessarily eight |

The public-style hybrid may favor (8,3), but that is conditional on its acceptance gates. Do not claim it as a uniquely determined prediction of Excel.

### D1: strict versus inclusive two-cycle eligibility

Read SEASONALITY for:

~~~text
{1;-2;1;1;-2;1}
{1;-2;1;1;-2;1;1}
{1;-1;-1;1;1;-1;-1;1}
{1;-1;-1;1;1;-1;-1;1;1}
~~~

For rules that otherwise recognize these primitive cycles:

| Eligibility rule | Four outputs |
|---|---|
| 2p<n | 0,3,0,4 |
| 2p<=n | 3,3,4,4 |
| Three complete cycles required | 0,0,0,0 |

The existing E.5 data make the strict rule plausible: all exact two-cycle p4/p8 examples fail to return their actual period. Other detector gates can cause the same failures, so this is not yet proof.

### D2: reversal and timeline dependence

Reverse the values of H105, H111 and H139, retaining a fresh ascending timeline. A whole-series, reversal-invariant detrend/spectrum/correlation pipeline predicts SEASONALITY 0,4,3 respectively, matching the originals. A change implicates asymmetric preprocessing, a prefix rule, phase-sensitive processing, or rounding.

For the same D0 value array, compare timelines:

~~~text
t_i=i;  t_i=7*i;  t_i=45000+i;  t_i=45000+i/4
~~~

A detector using only prepared indexed values predicts the same detected period, while STAT 8 gives 1,7,1,1/4. A difference supplies evidence for a timeline-dependent branch; equality does not disprove every business-period hint.

## B4. Short initialization

The structural extension is measured. Rank the remaining uncertainty as follows:

| Rank | Hypothesis | Concrete experiment | Predicted signature |
|---:|---|---|---|
| 1 | Same mathematical initializer through a shared regression routine with additional arithmetic steps | H90/H95/H100, seasonality 0: read F2,F3; compare inferred initial slope with SLOPE, the slope from LINEST, and explicitly evaluated raw-moment expressions on the same data | One common scalar initial trend explains all output bits; a shared helper may agree where the raw formula does not |
| 2 | Normalization or centering before regression changes rounding | H97, plus exactly represented offsets 0,2^20,2^40 and powers-of-two scaling | Inferred slope departures track the preprocessing arithmetic or magnitude; a purely once-rounded rational slope remains unchanged by translation |
| 3 | More than the initial trend differs outside the exact region, or search/application use different loaded states | Same histories and STAT 4–7, F1–F3 jointly | No possible scalar B0 with the stipulated level/seasons/gains explains the observations |

These are not fresh endorsements of the raw-moment/store programs already refuted in R48. They specify which hidden quantity to identify next.

Recommended order:

1. H90, H95, H100: isolate seven-point initialization.
2. H97: original slope 9/4 and extension are exact.
3. H87: add the first nonexact regression/extension stage.
4. Only then the remaining n=4..6 cases.

For example, a once-rounded exact-real hypothesis predicts B0=RN53(429/224) for H97. A centered stored-mean hypothesis instead predicts the result of its explicit centered arithmetic on the seven extended values. Those are precise candidate computations; without evaluating them, I do not claim different numeric bit patterns or a winning alpha for H97.

## B5. Seasonal initialization

Rank:

1. First-two-cycle slope and first-cycle detrended phase offsets, generalized from B.5.
2. First-two-cycle slope with phase averaging across available cycles.
3. A common-slope regression with a separate intercept for each phase, using more of the history.

All can agree on noiseless affine-periodic inputs. The exact seasonal corpus therefore cannot rank them by match count.

Use p=4 and:

~~~text
{0;256;512;768;1024;1280;1536;1792;2096;2304;2560;2816}
~~~

This is y_j=256j+48 at j=8, with no other perturbation. Read STAT 1–7 and F1..F5. Conditional on no reset, recover the initial states using D2.

| Initializer | L0 | B0 | Phase states S0..S3 |
|---|---:|---:|---|
| First two cycles / first-cycle offsets | 0 | 256 | 0,0,0,0 |
| First-two-cycle slope / all-cycle phase averages | 4 | 256 | 12,−4,−4,−4 |
| Full-history common-slope phase regression | −17/4 | 257.5 | 57/4,−13/4,−19/4,−25/4 |

The large linear component reduces reset risk; it does not prove reset is absent. This experiment identifies where data enter initialization, with substantially separated predicted states.

## B6. Seasonal search and gamma

Rank:

1. A small constrained deterministic parameter design, extending the nonseasonal approach.
2. Coarse design plus a bounded amount of adaptive refinement.
3. Continuous bounded optimization with quantized loads or published values.
4. Joint optimization of gains and initial states.

No exact gamma candidate set is supported by the supplied noisy seasonal data.

### S0: a nonzero seasonal exact tie

Use p=2:

~~~text
{0;8;0;8;0;8;0;9}
~~~

Under first-two-cycle initialization, the initial states are L0=4, B0=0, S=(−4,4). Every smoothing triple produces errors 0,0,0,0,0,0,0,1. Hence every ordinary innovation-SSE score is exactly one.

| Combined hypothesis | Prediction |
|---|---|
| Prefix initializer, ordinary SSE, first/default retained | alpha=.751, gamma=.001 |
| Same, all-observation residual statistics | MAE=1/8; RMSE=sqrt(1/8) |
| Later observations influence initialization, or score/traversal differs | The preceding predictions need not hold |

Repeat with final value 8+delta for delta in {1/16,1,16}. Each score remains exactly delta² under the prefix hypothesis. A nondefault alpha/gamma cannot be blamed on rounding of earlier innovations, because they are exactly zero.

### S1: a small parameter survey with a purpose

For p=2,n=10, use

~~~text
{0;8;0;8;u;8;u+v;8+v;u+v;8+v}
u,v in {0,1,4,16}
~~~

Read STAT 1–3. This is a 16-history batch, not a random search. It changes a phase offset and a level shift separately.

- A gamma outside a proposed finite set refutes that set.
- A returned alpha+gamma>1 refutes the usual triangular bound.
- Smooth effective-gain changes inside a fixed reported six-digit value support publication-only quantization.
- Identical parameter families alone cannot distinguish fixed traversal from a short adaptive search.

I cannot honestly assign numerical winners to these noisy histories without a specified and evaluated initializer/search. The purpose is to obtain the missing observables before fitting a candidate table.

## B7. Explicit cycle limits

The economical hypothesis is seasonal admission when n>=max(5,2p), including partial final cycles. Alternatives worth excluding are a complete-final-cycle requirement and a period-dependent extra-cycle requirement.

Use the following exactly centered patterns, repeated or truncated:

~~~text
P3={0;3;9}
P4={0;4;12;24}
P5={0;5;10;20;35}
~~~

Read STAT 3, STAT 1 and the error, if any.

| Complete test rule | p3,n6 | p3,n7 | p3,n9 | p4,n9 | p5,n10 |
|---|---:|---:|---:|---:|---:|
| C1: n>=max(5,2p), partial cycles admitted | .001 | .001 | .001 | .001 | .001 |
| C2: C1 plus a complete final cycle for p>2 | .001 | epsilon | .001 | epsilon | .001 |
| C3: C1 for powers of two; other p>2 require three cycles | epsilon | epsilon | .001 | .001 | epsilon |

Entries are predicted gamma, conditional on exact zero-innovation initialization and default retention for an admitted seasonal pattern, and on a failed admission test falling back to nonseasonal fitting. Eligibility alone does not establish those numerical behaviors. An error would refute the fallback assumption. C3 is particularly weak: its resemblance to B.5's guard is not evidence that Excel uses it.

## B8. Seasonal statistics, variance, and reset

Rank the seasonal error-statistic conventions:

1. Reuse the measured nonseasonal residual window and lag-one MASE scale.
2. Reuse its window but use seasonal-lag MASE scaling.
3. Use a season-dependent residual exclusion/window or variance divisor.

Read STAT 4–7 on the nine noisy cases already listed in E.5(b), then compare them with the recovered application-pass innovations. Zero-error cases cannot distinguish any of these choices.

For covariance, compare:

1. Reported gamma is the innovation gain.
2. Reported gamma is a traditional coefficient whose innovation gain is (1−alpha)gamma.
3. A different interval construction or correction is used.

Using one of the period-four n=12 or n=24 histories, read same-confidence C1,C4,C5. The predicted normalized jump is:

\[
\frac{C_5^2-C_4^2}{C_1^2}
=
\begin{cases}
(\alpha+4\beta+\gamma)^2,&\text{direct innovation gain},\\
(\alpha+4\beta+(1-\alpha)\gamma)^2,&\text{traditional gamma}.
\end{cases}
\]

These are coarse convention tests before last-bit arithmetic. Account for the beta used by the confidence calculation and any reset.

For a seasonal reset, compare a noisy series y with y+256j and y−256j, keeping the explicit period. Under affine-equivariant initialization and scoring, the pre-reset innovations and selected gains are unchanged. A change only in published beta or terminal-state behavior points toward the correction stage; changes in alpha/gamma require examining initialization/search or numerical effects as well.

# C. Implementable specification for the four refusal blocks

## C1. What is and is not ready to implement

| First-refusal block | Defensible change now | Missing for a bit-exact implementation |
|---|---|---|
| Automatic detection, 528 cells | Preserve measured short/affine/alternating branches; independently record detector and model outcomes | The general detector, its thresholds and arithmetic; an automatic forecast may additionally need noisy seasonal fitting |
| Short initialization, 204 cells | Isolate initialization as a separately observable component; apply the scalar-state identification procedure below | The nonexact initial states and extension loads, plus their interaction with score selection |
| Nonzero seasonal innovations, 180 cells | Acquire gains, statistics and sufficient horizons; use the exact forward/inverse equations to discriminate initialization | General initializer, candidate/search procedure, statistics convention, reset and publication arithmetic |
| Cycle limits, 40 cells | Route explicit p2,n4 through nonseasonal fitting | The same nonexact short-initialization problem |

This is a research specification and one measured routing correction. It is **not** a claimed implementation of all 952 cells. In particular, the general automatic detector and seasonal search cannot be filled with exact constants from the supplied evidence.

### Proven routing correction

**M/I:** Insert the following after the existing validation and early-output rules, before the seasonal fit:

~~~text
if requested_seasonality == 2 and prepared_length == 4:
    return nonseasonal(prepared_values, requested_output, ...)
~~~

The more general short-history routing can be consolidated when preserving the already measured branches, but this targeted condition is sufficient for the recorded cycle-limit block.

For its two histories:

\[
OLS(2,7,-1,4)=-1/5,\qquad
OLS(2,3.5,3.75,5.75)=23/20.
\]

Both enter the unresolved nonexact short initializer. Therefore this patch changes their first-refusal classification; **it does not establish forty new matches**. Resolving short initialization potentially addresses 204+40 recorded refusals, before considering further dependencies in automatic mode.

### Specification of the unchanged exact seasonal branch

**M:** Retain the existing exactness guards and arithmetic from B.5. Within them:

~~~text
differences[j] = exact(y[j+p] - y[j]), j=0..p-1
B0 = exact(sum_exact(differences) / (p*p))
A[j] = exact(y[j] - exact(j*B0))
L0 = exact(sum_exact(A) / p)
S[j] = exact(A[j] - L0)
require sum_exact(S) == 0

run the established recurrence with (.751,.001,.001)
require every innovation == 0
publish only in the previously measured output regions
~~~

Do not remove the near-cancellation forecast guard based on this algebra. R48 supplies counterexamples to that extension.

## C2. Short-history identification procedure

This procedure is implementable by an agent, but it identifies a kernel rather than pretending an unidentified kernel has already been solved.

**I/H, with every output compared by raw bits:**

~~~text
inputs:
    history y
    measured alpha, reported beta, STAT4..7
    measured forecasts F1,F2,F3

1. Compute exact-real slope candidates from A3 as diagnostic centers.

2. Consider both reset statuses.
   If reported beta != .001, reset is excluded by the measured rule.
   If reported beta == .001, enumerate plausible pre-reset beta values
   for that alpha; do not insert the reported beta automatically.

3. Treat B0 as an unknown binary64 initial slope.
   Start near each exact-real center, but do not assume the answer must
   lie within a fixed number of ulps.

4. For every candidate B0 and pre-reset beta:
       initialize L0=y[0], S0=S1=0, gamma=2^-52
       run the measured application recurrence on the real history
       calculate STAT4..7 before correction
       run the measured reset and centering
       publish F1,F2,F3 with the measured operation boundaries
       compare all outputs jointly

5. If no scalar B0 works:
       test the initial level / initial seasonal-state assumptions
       and the recurrence's load/store boundaries separately.
       Do not claim that a different OLS division alone is the cause.

6. If B0 is identified:
       for n<7, separately identify the original regression result r
       and every extended observation used by parameter search.
       require the selected parameter to win under the proposed
       score, candidate traversal, and comparison rule.
~~~

A production implementation must compute B0 from the input through a general arithmetic procedure. A per-history recovered seed or table is evidence for identifying that procedure, not a clean solution.

The exact real recurrence yields e_j=A_j+C_j B0 when the other initial values and gains are fixed. Thus SSE is quadratic in B0, MAE is piecewise linear, and final trend is affine. These constraints are stronger than trying thousands of regression-expression trees with only a forecast as a check.

Use intervals consistent with the rounded observations rather than treating (F3−F1)/2 as an exact recovered float. Midpoint boundaries, the extra PC64 rounding step, and ill-conditioning matter.

## C3. Seasonal identification procedure

**I/H:** For each of the nine noisy seasonal histories in E.5(b) with n>=8:

~~~text
read STAT1..7
read forecasts F2..F(p+1)       # F1 is already recorded
read confidence C_p,C_(p+1)   # C1 at 95% is already recorded

establish:
    effective period and parameter convention
    whether correction has replaced the fitted state or beta

recover terminal-state constraints in the zero-mean seasonal gauge
use inverse recurrence to distinguish initializer families
validate any selected family by forward evaluation

only after identifying initialization:
    compare proposed search scores and candidate generation
    identify the seasonal reset and error-statistic conventions
    refine operation boundaries for bit-level publication
~~~

This batch is 63 new parameter/statistic readings, 24 new forecasts, and 18 new confidence readings: **105 formulas**. It addresses the actual missing observables. There are six period-two and three period-four cases, so these counts do not require a large survey.

The first-two-cycle initializer hypothesis must reproduce all nine recorded F1/C1 pairs as well as these new readings. The broader seasonal specification must preserve the exact-zero E.5(a) cases. A match on their defaults alone is insufficient.

## C4. Predictions on existing histories

The following are predictions of the **preserved measured branches**, not a claimed validation of an unknown general detector. For the seasonal rows, the data have an exact nonzero alternating component; the real initialization gives zero innovations, so the measured default is retained.

| History | Automatic period | Alpha for the recorded fit mode | Beta for that mode | Fit mode |
|---|---:|---:|---:|---|
| H86 | 0 | .1 | .099 | Explicit nonseasonal |
| H91 | 0 | .1 | .099 | Explicit nonseasonal |
| H96 | 0 | .1 | .099 | Explicit nonseasonal |
| H101 | 0 | .1 | .099 | Explicit nonseasonal |
| H102 | 2 | .751 | .001 | Explicit p2 |
| H103 | 2 | .751 | .001 | Explicit p2 |
| H106 | 2 | .751 | .001 | Explicit p2 |
| H108 | 2 | .751 | .001 | Explicit p2 |
| H109 | 2 | .751 | .001 | Explicit p2 |
| H112 | 2 | .751 | .001 | Explicit p2 |
| H116 | 2 | .751 | .001 | Explicit p2 |
| H117 | 2 | .751 | .001 | Explicit p2 |
| H118 | 2 | .751 | .001 | Explicit p2 |
| H119 | 2 | .751 | .001 | Explicit p2 |
| H120 | 2 | .751 | .001 | Explicit p2 |
| H123 | 2 | .751 | .001 | Explicit p2 |

These reproduce the supplied results. They do not justify a new alpha/beta prediction for H87–H100's refused cases or a general detector prediction for H105/H127.

### A fully specified detector control that is already disproved

For comparison, this simple proposed detector has no unspecified constants:

~~~text
if n<=4: return 0
if all consecutive differences equal: return 0
for p=2..floor((n-1)/2), ascending:
    d=y[p]-y[0]
    if all y[j+p]-y[j] equal d:
        return p
return 0
~~~

On these small dyadic inputs its comparisons are exact.

| History | This rule predicts | Excel |
|---|---:|---:|
| H102 | 2 | 2 |
| H103 | 2 | 2 |
| H104 | 4 | 4 |
| H105 | **8** | **0** |
| H106 | 2 | 2 |
| H107 | 0 | 0 |
| H108 | 2 | 2 |
| H109 | 2 | 2 |
| H110 | 4 | 4 |
| H111 | **0** | **4** |
| H112 | 2 | 2 |
| H113 | **4** | **2** |
| H114 | **0** | **4** |
| H115 | 8 | 8 |

This is an explicit negative control, **not a recommended implementation**. It demonstrates why an exact mathematical period finder cannot fill the automatic-detection block.

The observed p8→3 outputs could reflect a strong harmonic near 8/3; p8→4 and p4→2 similarly suggest harmonic selection. Those inferences do not identify an exact FFT, bin-rounding instruction, or acceptance threshold.

# D. Methods that can shorten the reconstruction

## D1. Use the confidence endpoint as an RMS oracle

**I from the measured B.4 fallback:** choose

\[
c_*=1-2^{-53}.
\]

Then binary64 addition rounds 1+c* to 2, so q=1 and the measured internal fallback uses z=1.

At horizon one,

\[
\boxed{C_1(c_*)=\sqrt{SSE_{\rm application}/n}}
\]

for the measured nonseasonal branch, with its specified arithmetic.

Use the Excel expression (1-2^-53) for the confidence argument, and verify the passed confidence value where the service allows it.

This has three uses:

1. It separates residual-variance arithmetic from the inverse-normal kernel and final nontrivial quantile product.
2. At n<=10 it should agree with STAT 7 under the measured formula.
3. At n>10 it observes full application SSE/n while STAT 7 observes the tail. That separates variance normalization from the statistic window.

Do not assume the seasonal branch shares this endpoint behavior until one noisy seasonal control confirms it.

## D2. Recover seasonal state from forecasts

Assume no post-fit state replacement, the standard additive forecast, and period p. In exact real arithmetic,

\[
F_h=L+hB+S_{(n-1+h)\bmod p}.
\]

Therefore,

\[
B=\frac{F_{p+1}-F_1}{p}.
\]

Level and season have a gauge freedom:

\[
L\mapsto L+c,\qquad S_q\mapsto S_q-c.
\]

Choose zero-mean seasons:

\[
L=\frac1p\sum_{h=1}^{p}(F_h-hB),\qquad
S_{(n-1+h)\bmod p}=F_h-L-hB.
\]

Extra forecasts test consistency through

\[
F_{h+p}-F_h=pB.
\]

For the hidden nonseasonal p2 model, use F1,F2,F3:

\[
B=(F_3-F_1)/2,\qquad L=(F_1+F_2-3B)/2.
\]

Using F2−F1 alone confounds the tiny alternating seasonal state.

### Invert the application recurrence

Given the updated state after observation y_j, for phase q=j mod p:

\[
e_j=\frac{y_j-L_j-S_q^{new}}{1-\alpha-\gamma},
\]

\[
B_{j-1}=B_j-\beta e_j,\qquad
S_q^{old}=S_q^{new}-\gamma e_j,
\]

\[
L_{j-1}=L_j-B_{j-1}-\alpha e_j.
\]

The key identity is

\[
L_j+S_q^{new}=y_j-(1-\alpha-\gamma)e_j.
\]

This recovers innovations and initial-state constraints without guessing a dozen initializer formulas first.

The recovered initial level and seasons still carry the gauge chosen at the terminal state. Seasonal innovations change the sum of the phase states, so zero-mean terminal seasons do not imply zero-mean initial seasons. Before comparing against the initializer table in B5, normalize the recovered initial state separately:

~~~text
c0 = mean(recovered_initial_seasons)
recovered_initial_level += c0
for each phase q:
    recovered_initial_seasons[q] -= c0
~~~

The initial trend and all innovations are unchanged by this adjustment.

**Limits:** the formula is singular at alpha+gamma=1 and ill-conditioned near it; forecast publication is rounded; reset can destroy the original terminal trend/level and overwrite beta. Use interval constraints or approximate recovery to distinguish states, then validate forward in the proposed arithmetic. Do not claim exact inversion of published binary64 outputs.

## D3. Distinguish internal gains from reported gains

Suppose a last-observation perturbation delta leaves initialization and selected gains unchanged and does not trigger a reset. Then its effect on forecasts is

\[
\Delta F_h=\delta\left(\alpha+h\beta+
\gamma\,1_{\{p\mid h\}}\right).
\]

For p>2, h=1,2 give

\[
\beta=(\Delta F_2-\Delta F_1)/\delta,\qquad
\alpha=(2\Delta F_1-\Delta F_2)/\delta.
\]

At h=p, recover gamma from the additional same-phase response. For p=2, use h=1,3 for beta, then h=2 for gamma.

This is a direct parameter-load test. Decimal-loaded gains and unrounded internal rational gains predict different response slopes, even when STAT displays the same six digits.

The initialization condition is essential. Nonseasonal full-history OLS generally changes when the last observation changes. Either account for that influence using the identified initializer or use a balanced perturbation that preserves its required moments. S0 supplies a particularly simple conditional seasonal example.

## D4. Use confidence increments to recover covariance gains

The standard AAA variance also has the impulse-response form

\[
V_h=1+\sum_{i=1}^{h-1}
\left(\alpha+i\beta+\gamma\,1_{\{p\mid i\}}\right)^2.
\]

At fixed confidence and fixed fit:

\[
\frac{C_{h+1}^2-C_h^2}{C_1^2}
=\left(\alpha+h\beta+\gamma\,1_{\{p\mid h\}}\right)^2.
\]

The innovation variance and normal quantile cancel in real arithmetic. The first seasonal jump is at h=p.

For p>2, the positive square roots of the h=1 and h=2 increments identify alpha+beta and alpha+2beta, hence alpha and beta. The h=p increment then identifies gamma. Compare these effective covariance gains with STAT and the forecast-response gains.

Disagreement can locate the use of corrected beta, a different gamma convention, or a separate interval implementation. Output rounding means this is initially a shape test, not a way to obtain exact gain bits by naive subtraction.

## D5. Recover observable winner regions, not an imaginary optimizer trace

With fixed candidate gains, a linear initializer, and a fixed window, the real error vector is linear in y:

\[
e_\theta=R_\theta y,\qquad S_\theta(y)=y^TQ_\theta y,
\quad Q_\theta=R_\theta^TR_\theta.
\]

Candidate equality is a quadratic surface. Search for pairwise witnesses on a small affine family

\[
y=y^{(0)}+u\,a+v\,b,
\]

where a,b preserve the initializer's required moments. T1 is one explicitly solved witness.

For each pair:

1. Construct a tie well separated from other candidates.
2. Query one point on each side to verify the pair is genuinely competitive.
3. Query the tie to infer precedence.
4. Change only a score-orthogonal component, as in T2, to investigate arithmetic.

A consistent precedence relation is compatible with a fixed traversal. An apparent cycle could also come from miscomputed ties, transformed scores, or state differences; exclude those before declaring adaptive order.

Final winners cannot reveal all losing evaluations or uniquely reconstruct a hidden optimizer path. What can be recovered is an observationally adequate candidate-generation and comparison procedure. That is sufficient for parity if it survives independent counterexamples.

## D6. Preparation as a controlled numerical channel

### Duplicate aggregation order

Use the first timestamp three times with values:

~~~text
{9007199254740992;1;-9007199254740992}
~~~

followed at timestamps 2..8 by:

~~~text
{1;0;1;0;1;0;1}
~~~

Request aggregation 7 (SUM), explicit nonseasonal fitting, and compare several outputs with direct prepared controls whose first value is zero or one.

- Forward binary64 accumulation of the three duplicate values gives zero.
- A sufficiently wide accumulator retaining the unit gives one.
- Permuting equal-timestamp values tests whether aggregation preserves input order or changes its reduction order.

The downstream comparison should use multiple outputs; one forecast can coincide accidentally. This isolates preparation separately from candidate fitting.

### Missing-point and timeline invariance

For an exact linear-interpolation control, compare:

~~~text
values   {0;2;3;4;5;6;7}
timeline {1;3;4;5;6;7;8}
~~~

with the fully prepared history {0;1;2;3;4;5;6;7} on 1..8, completion 1. All numerical function outputs should agree under the measured preparation model, with appropriately corresponding target dates.

At a 30% missing boundary, use timestamps {1;2;4;5;7;8;10}, with values equal to timestamps and completion 1. Compare with timestamps {1;2;4;7;8;10}, again using values equal to timestamps. Both inferred steps are one and both grids contain ten points. The first is admitted; the second gives #NUM!. Test STAT 8 and confidence zero too, to establish validation order without needing a fit.

### Magnitude and signed-zero boundaries

Scale a non-tied probe such as T1's negative-side variant by powers of two.

- At moderate exponents, a homogeneous SSE search preserves its winner.
- At a scale such as 2^-600, direct binary64 squaring underflows the scores to zero and predicts default tie retention; a scale-normalized objective may preserve the ordinary winner.
- Large scaling can make squared errors overflow while input values remain finite, separating normalization, infinity handling and internal guards.

Do not label an observed error as an Excel domain limit until identifying which stage produces it. The project's 2^±400 guard does not settle the question.

For the R48 near-zero seasonal forecast cases, use exact affine shifts and several forecast horizons. A magnitude-dependent threshold can break homogeneity; a different arithmetic association often produces a scale-covariant rounding pattern. Inspect signed zero as well as equality to zero.

## D7. Minimal order of work

1. **Nine optimizer readings:** T0 STAT1/2; T1 and its two neighbors STAT1/2; T2 STAT1. These test nonzero ties, a real pair boundary, and score precision.
2. **Ten extra short-history forecasts:** F2,F3 for H90,H95,H100,H97,H87. Combine with their already recorded statistics and F1.
3. **Six detector readings:** the two D0 histories and four D1 histories.
4. **105 seasonal readings:** the focused batch in C3.
5. **Confidence endpoint controls:** test the direct-RMS observation first on a nonseasonal short case and a long case, then on noisy seasonal data.

The initial set is roughly 130 formulas before the endpoint controls, with a distinct hypothesis attached to each batch. Subsequent experiments should be chosen from the resulting disagreements.

The first implementation target should be the short initializer, because it also blocks the recorded p2,n4 cases. The largest information gain comes from obtaining the currently absent seasonal parameter/state observations. The general detector is a separate identification problem; preserving its failures as explicit unknowns is preferable to falsely claiming an ACF or spectrum heuristic is bit exact.

# Sources and provenance

**Supplied evidence.** The attached 12-ets-prompt.md, especially B.1–B.6, D, E.1–E.6 and F. Corpus counts, existing match claims, measured operation order, and recorded outputs in this answer come from that document. No new parity count is claimed.

**[P1] Microsoft, “Describing the forecasting models in Power View,” original article dated 8 May 2014.** [Original address](https://powerbi.microsoft.com/en-us/blog/describing-the-forecasting-models-in-power-view/) and [current Microsoft-hosted copy](https://community.fabric.microsoft.com/blog/fbc_pbiupdatesblog/describing-the-forecasting-models-in-power-view/5174199). Used only for the published method family and its limitations. Its different-code-base discussion compares Power View with the older Data Mining Add-ins; it is not a statement establishing or denying identity with worksheet FORECAST.ETS.

**[P2] Michail Vlachos, Philip S. Yu, Vittorio Castelli and Christopher Meek, “Structural Periodic Measures for Time-Series Data,” Data Mining and Knowledge Discovery 12, 1–28 (2006), DOI 10.1007/s10618-005-0016-4.** [IBM publication record](https://research.ibm.com/publications/structural-periodic-measures-for-time-series-data); [author-uploaded paper](https://www.researchgate.net/publication/220451935_Structural_Periodic_Measures_for_Time-Series_Data). Used for the published AUTOPERIOD family, especially its periodogram/correlation relationship and hill-validation idea.

**[P3] Rob J. Hyndman and George Athanasopoulos, Forecasting: Principles and Practice, exponential-smoothing chapter.** [Public textbook](https://otexts.com/fpp3/exponential-smoothing.html). Used for innovations versus traditional gain conventions and the general AAA forecast-variance identity. Excel's arithmetic and correction behavior are taken from the supplied observations, not from an implementation associated with the book.

**[P4] Microsoft, FORECAST.ETS.SEASONALITY documentation.** [Public documentation](https://support.microsoft.com/en-us/excel/functions/forecast-ets-seasonality-function). Used to check the public interface description; conflicts with recorded behavior are not adopted.

**[P5] Microsoft, printf/wprintf format specification documentation.** [Public formatting documentation](https://learn.microsoft.com/en-us/cpp/c-runtime-library/format-specification-syntax-printf-and-wprintf-functions). Used only for the documented six-significant-digit formatting mechanism, not runtime source or binary inspection.

All new probe constructions, weighted short-slope identities, state inversions, and numerical rounding predictions are analytical deductions in this answer. They remain proposals for the user's oracle to test.

