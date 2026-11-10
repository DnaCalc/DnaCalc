# Excel solver reconstruction and oracle experiments

Response to `11-solvers-prompt.md`, pinned to DnaCalc commit `b2a808502f74e1590dcfc682171c8a1db1d93ee2`.

## Main conclusions and evidence standard

The next useful changes are a small set of identifiable arithmetic and state-machine changes. The appendices do not identify one complete Excel implementation for YIELD, IRR, or cumulative principal. In particular, they do not identify any solver's iteration cap. Supplying a single allegedly recovered program for those functions would overstate the evidence.

There are nevertheless several concrete advances available:

1. **YIELD:** forming the total coupon through `n * rate` before multiplying by 100 gives analytically correct predictions for several of the supplied one-ulp failures. Expanding the seed denominator as `n*R + n*d/2 + d/2`, with two stored additions, also explains the exceptional 15-period seed row. These are specific candidates to replay, not claims that all 70 seed rows have been solved.
2. **IRR:** the supplied data already contradict every deterministic implementation that forgets the original guess after computing the stored discount factor. Two guesses collapse to the same stored initial factor but produce different Excel answers. A raw-guess initial residual with an extended denominator is a particularly small candidate that preserves this information.
3. **BINOM.INV and CRITBINOM:** the public-CDF acceptance rule is substantially identified. The remaining prerequisite is reproducing the CDF bits, not devising another mass recurrence or adding an epsilon.
4. **CUMIPMT and CUMPRINC:** preserve cancellation and the backward extended recurrence. The unsolved singleton behaviour points to a second principal-dependent rounding; three analytically specified inputs can distinguish its placement.
5. **DURATION, MDURATION, PRICE:** separate exponent construction, power evaluation, and weighted arithmetic. Schedule clustering alone does not identify a defective power routine. Zero-yield and zero-coupon probes can separate those explanations cheaply.

I use **measured** for results reported in the supplied appendices, **deduced** for consequences of those results or explicit algebra, and **candidate** for an executable hypothesis still requiring replay. No numerical program, Excel experiment, repository test, or candidate search was executed for this response. The exact predictions labelled “analytical” below are hand deductions; approximate trajectory estimates are explicitly distinguished from binary64 forecasts.

Microsoft's public documentation was checked. It describes YIELD and ODDFYIELD as Newton-based calculations involving 100 iterations, IRR as using a default guess of 0.1 with a stated accuracy of 0.00001 percent and failure after 20 tries, and RATE as using a 1e-7 convergence criterion with 20 iterations. These statements are useful priors, not measured descriptions of the installed kernel's loop counter or floating-point stop. [S1–S4]

All implementation evidence comes from the supplied prompt. No Microsoft implementation source, disassembly, decompilation, or restricted third-party implementation was consulted.

## Arithmetic notation

`S(x)` means round to binary64, round-to-nearest, ties-to-even. `E(x)` means round an operation to a 64-bit significand, with the extended exponent range used by the existing x87 emulator. `Xadd`, `Xsub`, `Xmul`, and `Xdiv` mean the existing operation followed by its binary64 store: `S(E(a op b))`. A variable explicitly marked extended is **not** passed through `S` at every assignment. The distinction matters: a loop with extended accumulators cannot be implemented by calling `excel_x87_add` on a binary64 accumulator at every iteration.

Every decimal argument denotes the nearest binary64 value of that decimal literal. Hex strings denote exact binary64 bit patterns. `nextup`, `nextdown`, and integer bit offsets in experiment specifications are operations for the user's harness; they are not extra operations Excel should perform while parsing an input. Load supplied bits directly into the oracle cells and record the bits actually received. Ordinary formula text entry must not silently reduce a designed 17-digit input to 15 significant digits.

# A. Most plausible algorithms and limits of identification

## A1. YIELD

### What the evidence identifies

For the on-coupon, redemption-100 near-par cases, the strongest model remains the landed rational approximation followed by a Newton-like correction, with a small correction causing the **current iterate** to be returned:

```text
y = seed(arguments)
repeat:
    delta = (P(y) - price) / slope(y)
    if abs(delta) < tau:
        return y
    y = y - delta
```

The evidence localizes `tau` near `1e-10` in annual-yield units for the measured frequency-2 designs. It supports pre-update return: otherwise a seed with a correction of order `1e-11` would move millions of yield ulps instead of being returned. The two maturities make a single unscaled price-residual tolerance an inadequate explanation of this near-par switch. They do not distinguish strict `<` from `<=` at exact equality, establish the derivative, or exclude additional tests on other paths. [Prompt B.3, B.8]

For an on-coupon bond, with `c = 100*rate/f`, `d = price-R`, the real-arithmetic seed is

\[
y_a=f\frac{nc-d}{nR+d(n+1)/2}.
\]

Retain the matching single-coupon closed form. Do not replace it with an iterative path merely to share code. The unconditional par shortcut is different: three supplied par rows refute returning `rate` in all cases. [Prompt B.1, B.4]

### A new evaluation order worth replaying

The first untried association to prioritize is the total coupon calculation:

```text
total_coupon = S(S(S(n * rate) * 100) / f)
```

instead of

```text
coupon = S(S(100 * rate) / f)
total_coupon = S(n * coupon)
```

For `n=3`, `rate=0.05`, `f=2`, the former gives `7.5 + 2^-50`; the latter gives exactly `7.5`. For `n=15` the two give exactly `37.5`. Thus this association introduces an upward change in the short design without generally moving the long design. That is the qualitative asymmetry the evidence asks us to explain.

The second association is

```text
den = S(S(n*R + S(S(d*n)/2)) + S(d/2))
```

instead of `S(n*R + S(S(d*(n+1))/2))`. These are mathematically identical. Their intermediate stores are not.

Analytically, the combined candidate reproduces the supplied short-design results at `price=100±2^-46`, the long-design result at `price=100+3*2^-46`, and the relevant factored-numerator results in r18 rows 003, 011, 020, and 021. Section B1 gives bits and derivations. This is a genuine code-shape hypothesis, not an instruction to add an ulp when `n==3`.

**Limit:** neither expression contains any basis-dependent quantity when `A=0`. It therefore cannot reproduce r18 rows 024 and 026, which have opposite one-ulp deviations for bases 2 and 3. I cannot give an expression demonstrated to match all 39+31 seed rows and all r18 rows from these appendices alone. A day-count conversion, a basis-sensitive initialization, or a subsequent operation is still missing.

### General seeds remain underdetermined

The bias table is compatible with premature Newton termination. It does not reconstruct the seed uniquely. Under analytic Newton,

\[
e_{j+1}\simeq C e_j^2,\qquad C=P''/(2P')<0,
\]

so two updates erase the sign of the initial error. Reconstructing `±|e0|` from the final error presupposes the objective, derivative, and number of updates. A slightly different objective introduces another explanation for the final bias.

A finite-difference slope changes that inference materially. For a positive yield increment `h`, to leading order near the root,

\[
\begin{aligned}
D_+(y)&=[P(y+h)-P(y)]/h,& e_{j+1}&\simeq C e_j(e_j+h),\\
D_-(y)&=[P(y)-P(y-h)]/h,& e_{j+1}&\simeq C e_j(e_j-h).
\end{aligned}
\]

Once `|e_j| << h`, the next error is approximately linear in `e_j`, not quadratic. Predominantly negative final biases are consistent with analytic Newton and with a backward difference; they do not uniquely select either. A forward difference can change the sign near convergence. The actual rounded price differences must also be replayed, because subtracting nearby prices can dominate the asymptotic derivative error.

My ranking is: a price-dependent rational seed with analytic or backward-difference corrections; a day-count or normalized-price variant of that seed; then a simple initial secant construction or a safeguarded variant. I would not nominate a new fixed seed: the supplied fixed-seed searches and the observed near-par rational surface already argue against it.

### Cash scaling imposes a stronger constraint than the prompt states

A finite-difference derivative with a fixed **yield** increment does not, by itself, break cash homogeneity. If cash flows and price are multiplied by `lambda=2^k`,

\[
\frac{\lambda P(y)-\lambda p}
{[\lambda P(y+h)-\lambda P(y)]/h}
=\frac{P(y)-p}{[P(y+h)-P(y)]/h}.
\]

The same conclusion holds bit for bit for the corresponding power-of-two-scaled arithmetic while the relevant intermediates stay normal and finite. Changing the precision from 53 to 64 significand bits does not destroy that property.

Therefore B.7 requires another scale-sensitive element: an absolute cash residual or slope guard, a cash-dependent seed/safeguard, a perturbation in cash units, a branch using a literal face amount, or intermediate overflow/underflow. Fixed annual-yield finite differences alone are not the explanation. A residual stop alone also cannot switch between positive and negative roots if the underlying trajectory never crosses to the negative-base branch; root selection requires a trajectory or domain explanation as well.

For the zero-coupon, two-period case, write `z=1+y/f` and `rho=p/R`. The analytic Newton map is

\[
z_{j+1}=\frac{3z_j-\rho z_j^3}{2}.
\]

At `rho=0.8`, the two algebraic roots are `z=±sqrt(1.25)`, hence yields approximately `0.118` and `-2.118` when `f=1`. The reported negative result is incompatible with an internal guard that always rejects `y<=-f`. Public PRICE's input restriction must not be inherited blindly by the private solver objective. This observation does not authorize negative bases at fractional exponents; those require separate domain probes. [Prompt B.7]

There is a sharper restriction on the proposed homogeneous annuity start. At this same ratio it gives `y0=2/17`, `z0=19/17`, below the positive root. The analytic map remains positive and approaches that root from below. Adding any absolute-residual stop to that particular trajectory cannot produce the negative root. It would merely stop earlier, possibly at approximately 0.117647. The negative result therefore requires more than an additional residual acceptance condition on that unchanged trajectory.

Also distinguish numerical failure from mathematical nonexistence. For an on-coupon positive-cash-flow bond, the price on `y>-f` decreases from infinity to zero. Every positive target price has a root there. A price above the value at the artificial bound `-f+1e-10` has not thereby been proved rootless. This argument extends to positive discount times; it should not be generalized without checking bases whose derived first-period fraction can become nonpositive.

### Objective, cap, errors, and annualization

Use the proven PRICE evaluation as a candidate objective alongside the present CRT-power objective. The fact that a substitution regressed a bisection solver is not evidence that it would regress the correct stopped iteration. Conversely, the public description “based on PRICE” does not prove a bit-identical shared helper.

For the compound multi-coupon formula with redemption 100 and the same internal schedule, a mathematical derivative can also be written `-dirty_price*MDURATION`. This supplies a separately staged candidate, not a binary64 identity. For redemption other than 100, public DURATION must first use the theoretically normalized coupon rate `100*rate/R`, or be replaced by an internal duration calculation with that redemption. The identity does not generally apply to PRICE's single-coupon simple-interest branch.

The iteration cap and exhaustion result remain unmeasured. A research candidate can use `100` and `#NUM!`, explicitly as hypotheses; neither should be represented as recovered behaviour. Keep existing outer coercion and validation. Microsoft documents invalid dates as `#VALUE!`, and negative coupon, nonpositive price/redemption, invalid frequency/basis, or settlement at/after maturity as `#NUM!`; do not infer the precise ordering of overlapping errors from that list. [S1]

## A2. IRR

### Most plausible family

Keep the secant in `v=1/(1+r)` and the final conversion `Xsub(Xdiv(1,v),1)`. Those are much more strongly supported than the initial objective or its storage boundaries. [Prompt C.1–C.3]

For secant errors `e0=d`, `e1≈h`, the first errors satisfy `e2≈Cdh`, `e3≈C^2 d h^2`. The measured `K/C^2=1e-6` therefore supports a second point approximately `0.001` away **in v**, conditional on that returned-iterate interpretation. A rate-space perturbation of `0.001` instead gives `h_v≈-0.001*v_*^2`, hence a factor `v_*^4` in the signature. Do not promote a rate-space second seed simply because it has not yet been tried.

### The current initial state is demonstrably insufficient

For cash flows `[-1000,1210]`:

| Guess | Guess bits | Excel answer |
|---|---|---|
| `0.20999999999999991` | `0x3fcae147ae147ade` | `0x3fcae147ae147ae8` |
| `0.20999999999999994` | `0x3fcae147ae147adf` | `0x3fcae147ae147ae0` |

Both `S(1+guess)` values are `0x3ff35c28f5c28f5c`; their subsequent stored reciprocals must also agree. Any deterministic program whose complete initial state is just that stored factor and the flows must return identical answers. Excel does not. Changing its later secant expression or cap cannot repair this contradiction.

A particularly small candidate computes the first residual from the supplied guess while `1+guess` is still extended:

```text
b_ext = E(1 + guess)
F0 = Xadd(-1000, S(E(1210 / b_ext)))
```

For the first guess this gives `2^-43`; for the second it gives zero. The quotient errors relative to 1000 are approximately `7.52e-14` and `5.23e-14`, on opposite sides of the half-ulp `2^-44`. This is an analytical prediction of this precise candidate, not a measurement of Excel's hidden residual.

The raw guess can therefore survive in `F0` while the later solver still uses a fixed `0.001` v-step. Retaining an extended factor or another initial state is an alternative. Merely avoiding a store in a reciprocal is not enough unless the **complete resulting states** are actually different.

### Two proposed deductions need narrowing

The bank rejects `if F0==0 then return guess`. It does not reject `return Xsub(Xdiv(1,v0),1)` on that branch. Removing the branch and forcing a secant update can create a zero-denominator error. A conversion-only fix is incomplete, because it cannot explain the exceptional pair above with the current collapsed state.

Also, if `b=S(1/v)` lies in `[0.5,2]`, then

```text
S(1 + S(S(1/v) - 1)) == S(1/v)
```

by exact subtraction in this range. A fully stored rate round trip alone cannot fix the common modest-rate cases. In contrast, evaluating public NPV, including its final division, changes the real function from `P(v)` to `vP(v)`. It preserves roots but changes secant trajectories; its local curvature becomes `C_P+1/v_*`.

### Stop and failure behaviour

The existing post-update dual test `abs(delta_v)<1e-7 && abs(F_new)<1e-7` remains a strong candidate, particularly because large cash flows change error outcomes. The two number-versus-error rows do not refute it: an error of about `6e-13` in the d17 rate corresponds to a residual of order `3e-9`, below `1e-7`. A different trajectory could converge under the same stop.

Neither a literal 20-update cap nor the current 200-update cap is measured. Keep RATE's 376/377 implementation isolated while investigating IRR. Mathematical similarity is not evidence that their initialization, counter, and error paths are shared. Preserve established flow coercion and sign validation; the public IRR documentation requires positive and negative flows and says nonnumeric array/reference entries are ignored. [S3]

## A3. BINOM.INV and CRITBINOM

The strongest observational specification is already in D.3. Let `F(k)` be the **Excel-compatible public cumulative** BINOM.DIST result:

```text
accept(0) = F(0) > alpha
accept(k) = F(k) >= alpha, for k >= 1
answer = smallest accepted integer k
```

No epsilon is supported. The 252-boundary experiment supports this rule much more strongly than exact-rational ties or a mass-sum match percentage. Both inverses agree on every shared recorded input. [Prompt D.2–D.3]

A plausible simple code shape is a strict early check at zero, followed by an inclusive search over `1..n`. That explains the asymmetry without inventing a different tolerance at every integer. A separate endpoint routine is also possible, but the straightforward strict comparison against exact `q^n` is already contradicted at `(n,p,alpha)=(3,0.5,0.125)`, where Excel returns zero.

Forward scan, binary lower-bound search, and approximation-plus-correction can be observationally equivalent when they use the same monotone predicate and complete the search. Their exact search order cannot generally be recovered from output values alone. Large-n answers can expose an imperfect correction loop or a changed CDF branch, but cannot distinguish two genuinely equivalent exact searches.

The retained patch's movement from 2290 to 2302 and from 1402 to 1414 is a **net** gain, with regressions. It is not twelve fixes with no losses. The remaining six discrepancies belong to the CDF dependency, especially `(k,n,p)=(2,5,.5),(1,5,.5),(2,6,.5)`. A more accurate mathematical CDF is not a substitute for Excel's bits.

Keep the measured open domains `0<p<1`, `0<alpha<1`, truncation of trials, and recorded errors. Probe validation-before-versus-after-truncation separately for a negative fractional trial count; do not silently alter it from generic documentation. [Prompt D.1–D.3; S5–S6]

## A4. CUMIPMT and CUMPRINC

The later recurrence evidence in E.3 should govern the next candidate, rather than the older public-PPMT sum in E.1. For type 0, with

\[
L=\log(1+r),\quad em_n=\exp(-nL)-1,\quad q=pv/em_n,
\]

the backward shape is

```text
u = EXP(-(nper-end+1)*L)
term = u*rate
sum = 0
for period=end down to start:
    sum += term
    term /= (1+rate)
result = q * stored(sum)
```

Here the recurrence and denominator have the extended storage boundaries specified in E.3. Reuse the existing PMT logarithm/expm1 substrate; the displayed mathematical `em_n` does not license replacing the stable implementation by a fresh `EXP(...)-1` subtraction.

The 600 ranges and 115 singletons at power-of-two `1+r` strongly support this restricted shape. They do not identify the missing operation at ordinary rates. Singleton failures already prove that changing summation order alone is insufficient. The observed failure of every single-rounding `pv*A` representation strongly suggests another principal-dependent rounding within an ordinary arithmetic graph; a principal-dependent branch is a logical alternative. [Prompt E.3]

A natural candidate is to store `q/(1+r)` and shift the exponent by one period. A diagnostic control stores `q*rate` before dividing by `1+r`; an implementation depending only on the public PMT is already disfavoured by E.3. These graphs collapse toward the same behaviour when division by `1+r` is exact scaling. Section B4 supplies a three-point signature that separates them.

Before treating either as a surviving hypothesis, replay the existing 2,000-pv and merging-range certificates against its exact graph. E.3's rejection of dependence on `RN(pv*A)` may already cover the normalized quotient as well as rounded-coefficient variants. The summary does not give those A intervals or collision pairs, so survival cannot be asserted here. If an existing collision certificate applies, reject the graph without new oracle calls; a surviving explanation must retain additional principal information or use an operation outside that certificate's scope.

For type 1, period 1 is the full due payment and contributes zero interest. Later principal payments have the real form `pmt1*v^(nper-period+1)`. Its stored coefficient, recurrence, and combination with period 1 must be measured independently; agreement of a type-0 recurrence does not identify them.

For CUMIPMT, preserve the identified finish `stored(count*payment) - principal`, with the first due period removed from **both** components. Do not replace it with a direct accurate sum of interest. The large ulp counts at tiny rates are cancellation fingerprints, precisely the behaviour required for parity.

There is an additional deduction for the one-period example. Conditional on the principal being exactly `-pv`, Excel's value cannot result from the literal binary64 formula `payment=-S(pv*S(1+r))`. At `pv≈0.051918`, the maximum absolute error in that route relative to `-pv*r` is

\[
pv\,2^{-53}+2^{-58}\approx9.23\times10^{-18},
\]

and the final subtraction is exact. The reported error is approximately `1.566e-17`, larger than this bound. Thus the payment takes another rounding path, or the principal is not exactly `-pv`, or both. “For nper=1 PMT is mathematically `-pv*(1+r)`” must not be mistaken for a recovered execution sequence. [Prompt E.2]

Preserve the measured fractional-period semantics: validate the original bounds, retain fractional `nper`, then include `ceil(start)..floor(end)`; an empty integer interval is zero. Do not truncate all arguments in response to a generic financial-function pattern.

## A5. DURATION, MDURATION, PRICE

Keep the substantial identified arithmetic: the derived complement `(E-A)/E`, the stored integer-power chain, coupon discounting before weighting, redemption weighting before division, and MDURATION's measured final divide. There is no evidence for replacing those wholesale. [Prompt B.1; F.1–F.3]

The remaining power hypothesis is plausible, but schedule clustering does not prove it. The schedule also determines the bits of `(E-A)/E`, `k+off`, `k+1-A/E`, redemption time, and weighted terms. All of those can produce schedule-specific failures without a different transcendental routine.

First resolve a metadata ambiguity. F.2 lists `n=4,A=43,E=90` for the zero-coupon DURATION row `(46845,47168,0,0.083888885163598576,4,3)`. Those literal quantities would give time `(4-43/90)/4≈0.880556`, not the reported `0.88219178082191785`. The basis-3 effective denominator `365/4=91.25` instead gives `322/365≈0.8821917808219178`. At least this row cannot use its tabular E as the effective denominator; a calendar-span column is a natural explanation. Record the actual helper outputs and bits before using the table to reconstruct powers.

My ranked explanations are: exponent construction/storage; retained fractional exponent in the power multiplication; a split integer/fractional power chain; and residual weighting/accumulation differences. A global “one ulp correction for these schedules” is not justified. Nor does the existing PRICE agreement establish that YIELD uses an identical private function.

One exponent association deserves particular priority: `S(S(off+n)-1)` crosses a binade before subtracting one when n is near a power of two, whereas `S(off+(n-1))` can stay below it. The absence of DURATION misses at n=3 and n=5..7, versus their presence at n=1,2,4 and larger groups containing 8, makes this a plausible code-shape explanation. The schedule distribution is confounded, so it is a hypothesis; the two zero-yield calls in B5 test it directly.

For nonzero binary64 inputs, asking for a nonzero exact binary64 `ln(base)` is not a useful experimental condition: mathematically the logarithm is generally not dyadic. Use `base=1` to remove powers completely, or powers of two and dyadic exponents to create exactly understood final powers. Public POWER comparisons remain companion observations, not proof that a financial helper calls that public entry point.

# B. Ranked alternatives and discriminating experiments

The following experiments use explicit input doubles, exact bit ladders, or an explicitly specified adaptive selection rule. A forecast stated as a candidate expression is not an invented Excel answer. Where two implementations are observationally equivalent, I say so rather than promise a separating input that cannot exist.

## B1. YIELD

### Y1. Separate the seed associations before changing the iteration

Define three candidates, all with a stored numerator subtraction, stored division, and final multiplication by `f`:

| Label | Total coupon | Denominator |
|---|---|---|
| Y0 | Present `n*S(100*rate/f)` | Present `n*R+d*(n+1)/2` |
| Y1 | `S(S(S(n*rate)*100)/f)` | Present denominator |
| Y2 | Same as Y1 | `S(S(n*R+S(S(d*n)/2))+S(d/2))` |

Full stores are specified in C1. Y1 versus Y0 is separated by the short witnesses below; Y2 versus Y1 is separated by the long witness; Y2 versus Y0 is separated by either. All three are basis-blind on a coupon date, so rows 024/026 reject them as a complete explanation.

For settlement 44013, maturity 44562, rate 0.05, redemption 100, frequency 2, basis 0:

| Price | Price bits or construction | Y0 forecast | Y1 and Y2 forecast | Status |
|---|---|---|---|---|
| `100.00000000000001` | `0x4059000000000001` | `0x3fa999999999998c` | `0x3fa999999999998d` | Supplied Excel agrees with Y1/Y2 |
| `99.999999999999986` | `0x4058ffffffffffff` | `0x3fa99999999999a7` | `0x3fa99999999999a8` | Supplied Excel agrees with Y1/Y2 |
| `100.00000000000003` | `0x4059000000000002` | `0x3fa999999999997d` | `0x3fa999999999997e` | New analytical forecast |
| `99.999999999999972` | `0x4058fffffffffffe` | `0x3fa99999999999b6` | `0x3fa99999999999b7` | New analytical forecast |

For the long maturity 46753, same remaining arguments, `price=100.00000000000004`, bits `0x4059000000000003`:

| Candidate | Forecast |
|---|---|
| Y0, Y1 | `0x3fa999999999998f` |
| Y2 | `0x3fa9999999999990`, the supplied Excel answer |

The long-denominator calculation is especially diagnostic. Its ulp at 1500 is `2^-42`. The present expression adds `1.5` ulps and rounds to two. Y2 first adds `1.40625` ulps, rounding to one, then adds `0.09375` ulp, leaving one. No fitted numeric constant is involved.

For r18, bypass the unconditional par shortcut **in the candidate replay** and ask:

```text
YIELD(44105,44562,0.0575,100,100,4,0)
YIELD(44105,44562,0.0575,100.00000000000001,100,4,0)
```

The factored total coupon predicts `0x3fad70a3d70a3d72` and `0x3fad70a3d70a3d60`, respectively, the supplied Excel results. Replay all existing 70 seed rows plus all 29 r18 rows before choosing Y1 or Y2. Selected exact matches are insufficient to claim the whole association is recovered.

Follow with the exact new cohort `n=2..20`, `f=1,2,4`, rates `{0,0.03125,0.05,0.0575,0.125}`, and prices whose bits are `0x4059000000000000+j`, `j∈{-4,-2,-1,0,1,2,3,4}`. Construct coupon dates directly from maturity. Freeze the three candidates' predictions before requesting Excel; retain rows where they differ. This tests the operand association rather than learning an `n==3` exception.

An even smaller precision-location test uses adjacent coupon rates `0x3fa9999999999999` (`0.049999999999999996`) and `0x3fa999999999999a` (`0.050000000000000003`). Both give the same stored coupon `S(S(100*rate)/2)=2.5`. Ask the short YIELD tuple at price `100.00000190734863`, exactly `100+2^-19`, and its immediate price neighbours. A program using the original rate only through that stored coupon predicts identical outputs for the two rates. A raw-rate seed can distinguish them. Avoid price exactly 100, where the existing shortcut preserves the raw rate separately.

### Y2. Probe the stopping test on either side of 1e-10

For the three-period design above, treating the nominal coupon as exactly 0.05, the true inverse-price slope at par is

\[
-\frac{68921}{9842000},
\]

whereas the rational seed slope is `-0.007`. Their difference is `27/9842000`. The linearized annual-step boundary is therefore

\[
|p-100|\simeq \frac{9842000}{27}\,10^{-10}
=3.645185185\ldots\times10^{-5}.
\]

Ask the same YIELD tuple at these exact decimal-double prices:

```text
100.0000364
100.00003645
100.00003646
100.0000365
99.9999636
99.99996355
99.99996354
99.9999635
```

For the positive-offset inner pair, the leading step estimates are `9.99949e-11` and `1.000224e-10`. These are forecasts around a linearized boundary, assuming an analytic derivative or a sufficiently small finite-difference spacing such as `1e-7`; they are not asserted exact Excel transition prices. A spacing as large as `1e-4` can move the lower case across the boundary, which is itself discriminating. The outer anchors protect against price-kernel and seed-staging shifts.

| Hypothesis pair | Separating outcome |
|---|---|
| Pre-update versus post-update return | Below the boundary, pre-update returns the rational seed; post-update returns `S(seed-delta)`, displaced by approximately `1e-10` |
| `1e-10` versus substantially different step tolerance | Boundary location shifts proportionally, holding seed/objective fixed |
| Step test versus unscaled price-residual test | Repeat the short/long maturities: a step test aligns in correction size; a residual test aligns in money residual |
| `<` versus `<=` | Only an input producing an exactly equal internal test value separates them; ordinary adjacent frontier prices alone do not establish equality |

After locating a seed/iterate switch, bisect the **price bit lattice** until two adjacent prices straddle it. Evaluate the candidate internal quantities at those exact prices. Select further queries only where candidate branches differ. This is the precise experiment for the stop; do not fit an epsilon to a broad scatter plot of final residuals.

### Y3. Identify whether tolerance is annual or periodic

Preserve the same three-period economic problem, with coupon per period 2.5:

| Frequency | Settlement | Maturity | Coupon rate |
|---|---:|---:|---:|
| 1 | 44197 | 45292 | 0.025 |
| 2 | 44013 | 44562 | 0.05 |
| 4 | 44287 | 44562 | 0.1 |

Use redemption 100, basis 0, and prices `100.00003` and `100.00006`.

An annual `1e-10` correction threshold predicts approximate price boundaries `7.29037e-5`, `3.64519e-5`, and `1.82259e-5`. At `100.00003`, that gives seed/seed/iterate for frequencies 1/2/4. A fixed periodic threshold calibrated to the frequency-2 evidence gives the same price boundary for all three, hence seed/seed/seed. At `100.00006`, the corresponding annual pattern is seed/iterate/iterate and the periodic pattern iterate/iterate/iterate. This probes units without changing periodic coupon geometry.

### Y4. Observe the redemption generalization directly

Use zero coupons, settlement 44013, maturity 44562, frequency 2, basis 0:

| Redemption | Lower price | Upper price |
|---:|---:|---:|
| 97 | 96.9052734375 | 97.0947265625 |
| 100 | 99.90234375 | 100.09765625 |
| 128 | 127.875 | 128.125 |

These prices are exactly `R*(1±2^-10)`. The generalized homogeneous annuity seed predicts the **same rounded yield across all three redemptions**:

```text
lower price: S(1/1535)
upper price: S(-1/1537)
```

Its error against the true zero-coupon root is approximately `4.6e-11`, so the proposed annual pre-update `1e-10` test returns the seed. A converged-root calculation instead returns near `2*((R/p)^(1/3)-1)`, visibly separated by many ulps. A seed that retains literal face 100 in place of R can change across the redemption values; its full trace must establish whether that difference survives iteration. Thus this is a much cleaner general-seed experiment than inferring a start from a `1e-15` final bias.

Repeat at `R*(1±2^-9)`. The leading seed error becomes about `3.7e-10`, so the same model now requires an update. Add coupon-bearing controls `(R,rate)=(50,0.03125),(100,0.0625),(200,0.125)` with prices `R*(1±2^-24)`; these preserve the coupon/redemption ratio and distinguish a general homogeneous seed from a zero-coupon special case.

For the existing annual redemption-97 design, also query

```text
YIELD(44197,45658,0.0325,p,97,1,4)
p = 97 + j*2^-20, j in {-16,-4,-1,0,1,4,16}
```

At price 97, the mathematical coupon-par yield is `3.25/97`. If the neighbouring answers are confirmed to lie in a returned-seed regime, they expose the local seed's intercept and slope directly. Compare the conventional annuity seed, a face-normalized variant, and a converged-root model before spending another round inverting two presumed Newton steps.

### Y5. Rank and separate off-coupon generalizations

The following are exact mathematical candidate shapes. C1 fixes an evaluation graph for the two simplest; none has been demonstrated to reproduce the B.5 biases.

| Rank | Candidate | Why retain it / what argues against it |
|---|---|---|
| 1 | Clean-price annuity with `N=n-A/E` | Best supplied generalization; wrong inferred bias under the assumed analytic two-update model |
| 2 | Integer-n annuity after a first-period simple-interest adjustment | Reduces to the landed on-coupon form and explicitly models the missing fractional period; not yet supported by bits |
| 3 | Same seed with analytic versus backward finite-difference slope | Can change the inferred seed error by changing late convergence; must also survive derivative and scale probes |
| 4 | A first secant construction or cash-sensitive safeguard before Newton-like correction | Can explain scale-dependent trajectories; unconstrained until a frontier/seed observation selects a specific rule |

For candidate 2 let `a=A/E`, `c=100*rate/f`, `m=(n+1)/2`, and `D=p+c*a`. Approximate the dirty price as

\[
D\simeq (1+ai)\frac{R+nc+iR(m-n)}{1+mi},\quad i=y/f.
\]

Solve

\[
aR(m-n)i^2+[R(m-n)+a(R+nc)-Dm]i+(R+nc-D)=0
\]

for the branch continuous with the on-coupon linear solution. C1 specifies the cancellation-avoiding root calculation as a **candidate computation**, not an improvement to Excel. This gives a physically interpretable fractional-period alternative that exactly reduces to the known rational shape when `a=0`.

Use these fixed tuples, each at prices `{90,97,100,103,110}` and the adjacent-double ladders around each:

```text
(44058,44562,0.05,p,100,2,0)
(44094,45658,0.06,p,103,2,2)
(44094,45658,0.06,p,103,2,3)
(44242,45658,0.0875,p,105,4,0)
```

They reproduce the supplied off-coupon dates. Each seed has an exact forecast given by its stated expression and chosen Newton trace. Before buying oracle calls, have the local harness retain inputs where the candidate returned iterates or iteration counts differ. I cannot responsibly print uncomputed output bits or claim that every member of this cohort separates every pair. If a pair never differs on the cohort, expand the price interval or use the adjacent frontier selection from Y2. The precise candidate graph and the frozen prediction table, not a retrospectively chosen winning variant, are the experiment.

### Y6. Derivative, scale, internal domain, and cap

For derivative candidates hold the seed and price leaf fixed. Compare:

```text
analytic: sum the differentiated cash flows
forward:  Xdiv(Xsub(P(Xadd(y,h)),P(y)),h)
backward: Xdiv(Xsub(P(y),P(Xsub(y,h))),h)
central:  Xdiv(Xsub(P(Xadd(y,h)),P(Xsub(y,h))),Xmul(2,h))
```

Start with `h=1e-7` in annual yield, then the distinct candidate `h=f*1e-7` for a periodic perturbation. Test `1e-6` only if the first spacing fails. Reuse the Y2 frontiers and the exact ODDFYIELD witness `(39763,44256,39736,39873,0.0575,84.5,100,2,0)`, whose supplied target is `0x3fb3c65d22ec15b6`. Analytic versus backward/forward candidates have different local error signs and different seed/iterate boundaries; central differences have a quadratic-in-h derivative truncation term. Finite subtraction noise is part of the forecast.

For cash scaling, first replay the original R44/R46 inputs verbatim. Then use the additional two-year on-coupon tuple `(44197,44927,0,p,R,1,0)` with `(p,R)=(25,100)` and `(80,100)`, scaled by exact `2^k` for `k=-500,-40,-20,0,20,40,500`.

For example `(25,100)*2^-500` has bits `(0x20f9000000000000,0x2119000000000000)` and the positive-500 pair has bits `(0x5f79000000000000,0x5f99000000000000)`. Homogeneous finite-difference and analytic candidates predict identical outputs at all safe scales. Absolute-residual or cash-sensitive branches predict transitions. Preserve the measured ordinary-scale root bits from the original corpus rather than assuming date-independent last-bit results for the new dates.

Add an off-coupon settlement and odd `n` controls. A universal `y>-f` guard predicts rejection of every negative-base branch. Integer-power permissiveness can produce negative-base roots for even n; a fractional-power objective may instead fail. Locate whether changes occur at a cash **exponent**, an absolute residual, or a particular initial estimate.

The cap experiment is necessarily conditional on a trajectory. After a seed/slope candidate survives, search exact input grids for traces whose first acceptance is on updates 99, 100, or 101, and traces still finite at update 100 but nonconvergent. Freeze those inputs. A 100-update/#NUM model predicts failure on the latter; a return-last model predicts its specified iterate. An observed #NUM frontier without a validated trace cannot identify the count: domain checks, denominator collapse, and overflow can produce the same error.

## B2. IRR

The leading alternatives are: fixed-v secant with a raw-guess initial residual; fixed-v secant with retained extended state; a rate-space second seed; and a transformed residual such as `vP(v)` or an additional safeguard. The first three must pass the collapsed-state test before their general match percentages matter. A fully stored rate-space second seed can itself collapse the paired guesses, so merely mentioning the raw guess is not sufficient.

### I1. Initial state and zero exit

Ask `IRR([-1000,1210],g)` with guess bits `0x3fcae147ae147ae1+j`, `j=-32..32`. Preserve the two known target rows in A2. Repeat at exact cash scales `2^-20`, `1`, and `2^20`.

| Alternatives | Prediction |
|---|---|
| Any stored-v-only initial state versus a state retaining raw-guess information | Stored-only predicts equality within every complete-state collision group; the supplied pair already contradicts it |
| Verbatim zero return versus converted zero return | The former returns each raw guess; the latter returns the common stored reciprocal conversion when its zero branch is entered |
| Raw-guess initial residual versus later-only objective changes | The former can distinguish the pair through `2^-43` versus zero; later-only changes cannot |

Use a candidate trace to determine exactly which extended-state and raw-residual variants remain distinct. Two different implementation descriptions with identical complete state are the same hypothesis for this experiment.

### I2. P versus vP and iteration-sensitive stopping

Ask

```text
IRR([-1,2],1.0000152587890625)
IRR([-1048576,2097152],1.0000152587890625)
```

The guess is exactly `1+2^-16`. With the fixed signed 0.001 v-step and dual post-update stop, the linear residual `P(v)=-1+2v` reaches the root in one real-arithmetic secant update; only the step criterion can require an additional update. A `Q(v)=vP(v)` residual has, for `e=v-.5`, the exact real-arithmetic recurrence

```text
e_next = 2*e0*e1 / (1 + 2*(e0+e1))
```

and returns approximately `1+6.2e-11` for the unscaled flows under this stop, rather than 1 to last-bit accuracy. Scaling flows by `2^20` makes the same residual too large and requires another iteration. These are analytical separation estimates; use the frozen graphs for exact forecast bits. This also separates an absolute residual condition from a scale-free stop without invoking pathological cash magnitudes.

### I3. Residual-only, OR, AND, and return order

Ask

```text
IRR([-9.0949470177292824e-13,0,3.637978807091713e-12],0)
```

The nonzero flows are exactly `-2^-40` and `2^-38`, bits `0xbd70000000000000` and `0x3d90000000000000`. The true positive-factor root is rate 1.

| Stop hypothesis, holding the stated seed/update fixed | Predicted result |
|---|---|
| Initial absolute-residual check | 0 |
| Post-update OR of residual and step | About `0.5995203837`, the first update; real-arithmetic expression `0.75/1.251` |
| Post-update AND | Continues to near 1 |
| Step-only | Also continues here; distinguish from AND with the cash-scale companion to I2 and the large-flow controls |

I2's scaled separator distinguishes AND from step-only for its nonlinear `vP` residual; the linear P residual can hit an exact zero at both scales. Use a trace-validated nonlinear large-flow case for that pair, retaining the existing d11/d14 failures as controls. First rule out overflow in the particular candidate trace.

### I4. Twenty versus two hundred updates

Ask `IRR([1,-4,4],0)` and the same flows multiplied by `2^-20` and `2^20`; repeat guesses `-0.5` and `0.5`.

Here `P(v)=(1-2v)^2`. For the fixed-v secant, with errors around the double root `v=.5`,

```text
e_next = e0*e1/(e0+e1)
1/e_next = 1/e0 + 1/e1
```

so convergence is linear. Starting around `v=1`, twenty updates leave a step much larger than `1e-7`; thirty-odd updates are needed. A literal 20-update version predicts `#NUM!`, while a 200-update version can return a number near 1. This is a cap discriminator **conditional on the objective and state machine**. Require several related witnesses to distinguish a cap from a changed method, a restart, or an early zero-residual branch.

### I5. Sign, branch, and domain behaviour

Ask `[1,-3,2]` and its negation `[-1,3,-2]` at guess `0.33333333333333331`, bits `0x3fd5555555555555`. The initial stored factor is 0.75, the quadratic vertex. With a signed v-perturbation `h≈.001`, the real-arithmetic next point is approximately `0.75±1/(16h)`, i.e. `-61.75` or `63.25`. A sign-independent seed is invariant under negating all flows; a sign-sensitive seed reverses this jump. A positive-v safeguard can turn the negative jump into an error or a restart. Do not predict a unique final root without specifying that safeguard.

Ask additionally `IRR([1,0,-4],-3)`, with guesses `-2`, `-1`, `nextdown(-1)`, and `nextup(-1)`. An unrestricted integer-power residual with zero acceptance can return -3 at the exact negative-factor root. A positivity guard rejects it. These exact cases separate domain restrictions from difficult convergence.

For the two structural failures, use the original d17/d19 vectors and guesses `{1,1.5,1.75,1.875,1.9375,2,2.0625,2.125,2.25,2.5,3}`, plus 65 adjacent guesses around 2.0 and cash scales `2^-20,1,2^20`. Preserve targets `0x3fa3670ea5237340` and `0x3f8f91595cbb3a00` at guess 2.0. Ask omitted guess versus explicit 0.1 on these vectors and multiple-root controls; documentation predicts equivalence, but the test excludes a private default-initialization path.

## B3. BINOM.INV and CRITBINOM

Rank the alternatives as: public lower CDF with strict-zero/inclusive-interior acceptance; the same predicate with another complete search order; a hidden endpoint/CDF branch; and a complementary-tail or bounded local-correction implementation. Uniform exact-rational `>` or `>=` rules and fixed mass-sum rules are useful refuted controls.

### Q1. Small exact separators

Ask both inverse names on every row:

| `(n,p,alpha)` | Leading prediction | Competing prediction |
|---|---:|---|
| `(1,.5,.5)` | 1 | Uniform `>=` against the exact .5 tail gives 0 |
| `(2,.5,.75)` | 1 | Uniform strict exact-CDF rule gives 2 |
| `(3,.5,.125)` | 0, measured | Strict comparison with exact repeated-product `.5^3` gives 1 |
| `(5,.5,.49999999999999994)` | 3 | Exact-rational quantile or exact mass sum gives 2 |
| `(5,.5,.5)` | 3 | Inclusive complemented-tail comparison gives 2 |
| `(5,.5,.50000000000000011)` | 3 | Inclusive complemented-tail comparison gives 2 |

The last row is a new analytical forecast from the measured CDF. Since `F(2;5,.5)=0x3fdffffffffffffe`, and `S(1-.50000000000000011)` is exactly that value, an inclusive complementary comparison accepts 2. Direct lower-CDF comparison rejects it. This is an especially clean test of an alpha-dependent switch to the upper tail.

To separate the zero-endpoint comparator without assuming an unreported public tail bit pattern, also obtain `T=BINOM.DIST(0,1,.5,TRUE)` and ask each inverse at `(1,.5,T)`. The leading strict-zero rule predicts 1; a uniform inclusive comparison against that same public T predicts 0.

At each CDF leaf `(k,n,p)=(2,5,.5),(1,5,.5),(2,6,.5)`, ask the CDF, its beta companion, and both inverses at the CDF value and three neighbouring doubles on either side. This separates a wrong acceptance predicate from a correct predicate using the wrong CDF bits. Do not replace the directly observed lower CDF by a subtraction from an upper CDF merely because the real probabilities are complementary.

### Q2. Large-n branch and correction probes

Use `n∈{999999,1000000,1000001,1048575,1048576,1048577}`, probabilities `{2^-20,.125,.5,.875,1-2^-20}`, and `k` values around `floor(n*p)` and at `{0,1,n-2,n-1}` where applicable. First ask the public CDF at each k, then ask both inverses at `nextdown(F(k))`, `F(k)`, and `nextup(F(k))`, excluding alpha outside `(0,1)`.

The leading model predicts the first integer satisfying its explicit predicate. A bounded local walk or changed tail branch can violate that certificate near the endpoint, underflow region, or approximation start. A completed bisection and completed local walk using the same monotone predicate predict the same answer; there is no value-only experiment guaranteed to separate them. Timing is supplementary evidence only and must control caches, recalculation, and batching.

Pair both function names throughout. A difference is useful evidence of separate paths; continued equality supports a shared kernel but cannot prove source identity. Add domain probes `(0,.5,.5)`, `(-.5,.5,.5)`, `(1,.5,0)`, `(1,.5,1)`, and probabilities `0`, `1`, `nextup(0)`, `nextdown(1)`. Validation before truncation predicts `#NUM!` at trials -0.5; validation after truncation permits n=0. The current wrapper's established error behaviour, rather than an assumed convention, decides what to retain.

## B4. CUMIPMT and CUMPRINC

### L1. Three points identify the placement of a money rounding

Ask `CUMPRINC(.75,1000,pv,1000,1000,0)` at:

| `pv` | Exact bits |
|---|---|
| `1.7500000000000002` | `0x3ffc000000000001` |
| `1.7500000000000004` | `0x3ffc000000000002` |
| `1.7500000000000007` | `0x3ffc000000000003` |

The supplied saturation evidence gives `em_n=-1`. Set `b=0xbfe8000000000000`, the bits of -0.75. These terminal graphs have analytically distinct signatures:

| Rank / role | Exact terminal graph | Bits on the three inputs |
|---|---|---|
| Leading normalized-quotient candidate | `S(S(-pv/1.75)*.75)` | `b+2,b+2,b+3` |
| Payment-first diagnostic control | `S(S(-pv*.75)/1.75)` | `b+1,b+2,b+2` |
| Rounded fixed-coefficient control | `S(-pv*S(3/7))` | `b,b+1,b+2` |
| Single-rounding exact-coefficient control | `S(-pv*3/7)` with exact rational arithmetic until the final store | `b+1,b+2,b+3` |
| Rounded reciprocal then stored money product | `S(S(-pv*S(4/7))*.75)` | `b,b+2,b+2` |

Every pair has a separating entry in this three-point table. The last two controls are already broadly disfavoured by E.3; they help interpret new results, not justify reopening the entire coefficient search.

This terminal test cannot distinguish graphs that coincide when their shifted exponent is zero, such as the normalized-quotient candidate with r inside versus after the coefficient accumulation. Separate those with the same pv values on `[999,1000]`, `[997,1000]`, and `[1,1000]`, then the unsaturated loan `r=.75,nper=17` on `[1,1]`, `[16,16]`, `[17,17]`, `[2,16]`, `[1,17]`. A rounded exponential reciprocal is a distinct graph, as the fifth row shows. The exact graph forecasts are in C4; request the subset where the local candidate outputs differ.

Repeat the singleton and two-period cases at rates 1 and 3. The extra division becomes binary scaling, enforcing the already measured power-of-two controls. Repeat type 1 separately; do not infer its private coefficient from public PMT equality.

### L2. Resolve the tiny one-period cancellation with its operands

For `r=8.0499406029856276e-09`, `pv=0.05191834857052053`, ask:

```text
PMT(r,1,pv,0,0)
PPMT(r,1,1,pv,0,0)
IPMT(r,1,1,pv,0,0)
CUMPRINC(r,1,pv,1,1,0)
CUMIPMT(r,1,pv,1,1,0)
-pv
-(pv*(1+r))
-(pv+pv*r)
PMT(r,1,pv,0,0)-CUMPRINC(r,1,pv,1,1,0)
PMT(r,1,pv,0,0)+pv
```

Use actual input cells loaded from the exact doubles for all companion expressions. A worksheet expression is an observed companion, not an assumption that a private kernel has the same storage profile.

| Observation | Interpretation |
|---|---|
| Public PMT minus public CUMPRINC equals CUMIPMT, but PMT plus pv does not | The principal is not exactly -pv |
| PMT plus pv equals CUMIPMT and CUMPRINC equals -pv | The payment path contains more than the simple rounded product rejected by A4's bound |
| Neither public-operand difference matches | A private operand, finish-store, or formula branch differs from the public composition |
| CUMIPMT equals public IPMT on this singleton | Supports that local path, but not a general direct-IPMT sum already contradicted elsewhere |

The supplied Excel/current interest difference is exactly two payment ulps (`2^-56`). This is a useful scale for the operand comparison; it does not identify which operand moved.

### L3. Type 1 and fractional bounds

Use `(r,nper,pv)=(0.0013616417420825142,127,220.63364505898258)` and type 1, intervals `[1,1]`, `[2,2]`, `[1,2]`, `[2,127]`, `[1,127]`.

The two due finishes predict respectively `Xmul(pmt1,S(E(1+sum_later)))` and `Xadd(pmt1,Xmul(pmt1,S(sum_later)))`. They are exact competing graphs. The identified interest skip predicts `CUMIPMT(...,1,h,1)` and `CUMIPMT(...,2,h,1)` bit-identically for h>=2. A failure of that identity would expose period 1 participating in a rounded aggregate despite zero mathematical interest.

Keep fractional-bound controls `(r,nper,pv)=(.75,17.5,1.75)`: `[1.25,1.75]` gives zero; `[1.25,2]` includes only period 2; `[1,17.5]` includes periods 1..17; an end of `17.500000000000004` is invalid. These protect established semantics while changing arithmetic.

## B5. DURATION, MDURATION, PRICE

### F1. A two-call control removes the power routine

Ask

```text
DURATION(44256,44562,0,0,2,0)
MDURATION(44256,44562,0,0,2,0)
```

For 2021-03-01 to 2022-01-01, NASD 30/360 gives `n=2,A=60,E=180`. The final exponent differs between two associations:

```text
tA = S(S((180-60)/180)+1) = 0x3ffaaaaaaaaaaaa
tB = S(2-S(60/180))       = 0x3ffaaaaaaaaaaaab
```

At zero yield, every discount is exactly 1. Under explicit stored weighting `w=S(100*t); D=S(S(w/100)/2)`, the resulting duration bits are:

| Graph | Predicted DURATION bits |
|---|---|
| tA, stored weighting | `0x3feaaaaaaaaaaaa` |
| tB, stored weighting | `0x3feaaaaaaaaaaaac` |
| tB, weighting retained extended through cancellation | `0x3feaaaaaaaaaaaab` |

These predictions include the product/division rounding; replacing `100*t/100` by t would erase a discriminator. The measured MDURATION finish predicts equality with DURATION at zero yield. Apply the same zero-yield/coupon-zero controls to the 16 troublesome schedules before refining ln/exp candidates.

Add `DURATION(44256,44927,0,0,2,0)`, which has n=4 and the same accrued fraction. Under that same stored weighting, two calls give a three-way signature:

| Redemption-time association | n=2 result bits | n=4 result bits |
|---|---|---|
| `S(off+(n-1))` | `0x3feaaaaaaaaaaaa` | `0x3ffd555555555554` |
| `S(n-S(A/E))` | `0x3feaaaaaaaaaaaac` | `0x3ffd555555555554` |
| `S(S(off+n)-1)` | `0x3feaaaaaaaaaaaa` | `0x3ffd555555555556` |

This separates every pair of these exponent associations without a transcendental computation. It does not establish that the same expression is used for both discount exponents and redemption weights; preserve that distinction in the trace.

### F2. Recover a private discount factor from several numerators

Ask

```text
PRICE(46463,47859,0,y,R,2,basis)
y in {0,0.20055555505885014,0.20055555505885017}
R in {1,3,7,100,1.0000000000000002,0.99999999999999989}
basis in {2,3}
```

There are no coupon sums or accrued interest: the answer is the staged quotient `R/discount`. For each observed quotient derive the set of possible discount doubles under `Xdiv`, then intersect across R. Non-dyadically related numerators are useful; `1,2,4,8` alone often supply redundant constraints. The inverse of one rounded quotient is not necessarily the original denominator.

This reconstruction assumes a stored binary64 discount and the stated quotient finish. If no common double fits the observations, treat that as evidence against those assumptions, for example an extended discount, rather than forcing the interval intersection to produce a value.

Compare three discount graphs, with the existing fractional and integer leaves fixed:

```text
full:       Dk = bond_pow(base,S(off+k))
split:      Dk = Xmul(frac_pow(base,off),integer_pow(base,k))
recurrent:  D0 = frac_pow(base,off); D(k+1)=Xmul(Dk,base)
```

Each pair is compared on the recovered denominator set, without interference from coupon accumulation. If the graphs coincide for a chosen schedule, select a new schedule where their local predictions differ; do not claim that observing an unchanged final PRICE proves their equality internally.

### F3. A collision test for a stored base

Use zero coupons, settlement 44256, maturity 47119, frequency 2, basis 0, with the redemption ladder from F2 and these yield bits:

```text
0x3fd0000000000000   0.25
0x3fd0000000000001   0.25000000000000006
0x3fd0000000000004   0.25000000000000022
0x3fd0000000000005   0.25000000000000028
```

The first three map to the same stored `S(1+y/2)=1.125`; the fourth maps to its successor. A model using yield only through that stored base predicts identical PRICE outputs for the first three. A difference in Excel refutes that model. Retained extended base precision can distinguish them. Use several R values to expose differences hidden by final quotient rounding.

### F4. Fractional-factor reuse versus full exponents

Fix settlement 44256, frequency 2, basis 0, coupon zero, and maturities `44562,44927,45292,45658,46023,46388,46753,47119`. These preserve coupon phase while increasing the coupon count by two. Use yields 2, 6, and 510, giving stored bases 2, 4, and 256, plus the non-dyadic redemption ladder.

A reused fractional factor with exact dyadic integer multiplication predicts exact power-of-two scaling between maturities while all values remain normal. A full-exponent route that separately rounds `off+k` can violate that scaling, especially at base 256. This is an invariant prediction for the reuse models; no uncomputed full-exponent bits are asserted. A binary split and sequential multiplication are identical on these exact dyadic controls, so separate those two using F2's ordinary bases where the multiplication paths differ.

# C. Implementable specifications and expected bank movement

These specifications distinguish **supported behaviour to preserve** from **named candidate programs to replay**. For an unidentified component, an explicit experimental graph is implementable; it is not evidence that Excel uses that graph. Keep the candidates independently selectable so a gain in one stage cannot conceal a regression in another.

## C1. YIELD

### Seed association candidates

Use binary64 stores at every displayed `S`. In particular, do not allow FMA contraction or algebraic reassociation across the statements.

```text
seed_assoc(n, rate, price, R, f, profile):
    d  = S(price - R)
    nr = S(n * R)

    if profile == Y0:
        cp = S(S(100 * rate) / f)
        nc = S(n * cp)
    else:                              // Y1 or Y2
        nr_rate = S(n * rate)
        total100 = S(nr_rate * 100)
        nc = S(total100 / f)

    if profile == Y2:
        dn = S(d * n)
        half_dn = S(dn / 2)
        partial = S(nr + half_dn)
        half_d = S(d / 2)
        den = S(partial + half_d)
    else:
        np1 = S(n + 1)
        dnp1 = S(d * np1)
        adjustment = S(dnp1 / 2)
        den = S(adjustment + nr)

    num = S(nc - d)
    periodic = S(num / den)
    return S(f * periodic)
```

For the first replay, keep the current eligibility condition `n>1`, settlement on a coupon date, redemption exactly 100, and the current analytic step test. Evaluate par rows through the candidate seed rather than the unconditional `return rate` branch. Compare profiles separately; do not route n=3 to one and n=15 to another after looking at their scores.

A fourth research profile replaces each arithmetic store by its existing x87-then-binary64 primitive while preserving the same graph. This is distinct from retaining extended intermediate expressions. The prompt's negative experiments cover many all-stored variants, but not necessarily these total-coupon and split-denominator associations.

### Two explicit general-seed candidates

For the clean fractional-annuity candidate:

```text
a = Xdiv(A,E)
N = Xsub(n,a)
y0 = seed_assoc(N,rate,price,R,f,Y2)
```

This spells out the reported fractional-n idea using one fixed association. It is a baseline hypothesis, not a claim that this extension supplies the B.5 bias.

For the first-period-adjustment candidate, take all arithmetic below as explicit binary64 stored operations, using separate temporaries in the order written:

```text
a = S(A/E)
c = S(S(100*rate)/f)
m = S(S(n+1)/2)
D = S(price + S(c*a))
L0 = S(R + S(n*c))
L1 = S(R * S(m-n))
qa = S(a*L1)
qb = S(S(L1 + S(a*L0)) - S(D*m))
qc = S(L0-D)

if qa == 0:
    if qb == 0: candidate failure
    i = S(-qc/qb)
else if qc == 0:
    i = 0
else:
    disc = S(S(qb*qb) - S(S(4*qa)*qc))
    if disc < 0 or disc is nonfinite: candidate failure
    root_disc = correctly_rounded_sqrt(disc)
    qroot = S(-0.5 * S(qb + copysign(root_disc,qb)))
    if qroot == 0: candidate failure
    i = S(qc/qroot)                    // branch continuous with -qc/qb
y0 = S(f*i)
```

The quadratic formula and its association are an explicit research choice. Do not smuggle it into production under the name “Excel seed” unless its predictions survive the direct-seed and held-out tests. On the exactly zero-accrual path, use the selected on-coupon association rather than silently switching to a differently rounded algebraic derivation.

### Complete stopped-iteration candidate

Fix one seed, one price provider, one derivative profile, one internal domain policy, and one exhaustion policy for each candidate. Do not mix them per row.

```text
YIELD_candidate(args, profile):
    use existing outer parsing, validation, schedule and single-coupon path
    y = selected_seed(args)
    if y or any required seed operation is invalid/nonfinite: return #NUM!

    for update_index in 0 .. CAP-1:
        p0 = selected_private_price(args,y)
        D  = selected_slope(args,y,p0)
        if p0 or D is invalid/nonfinite or D == 0: return #NUM!
        residual = Xsub(p0,price)
        delta = Xdiv(residual,D)
        if delta is nonfinite: return #NUM!
        if abs(delta) < 1e-10:
            return y
        y = Xsub(y,delta)
        if y is nonfinite: return #NUM!

    if exhaustion_profile == NUM: return #NUM!
    if exhaustion_profile == LAST: return y
```

This definition permits exactly CAP accepted updates and checks convergence before each update. A profile checking once more after the last update is another counter convention and must be named separately. Start with CAP=100 as the documentary prior; do not confuse this declaration with a cap measurement.

For the analytic slope candidate, let `t_k=off+k`, `b=Xadd(1,Xdiv(y,f))`, and use the same power provider as the selected objective. With explicit X operations:

```text
sum = 0
for k=0 .. n-1:
    t = Xadd(off,k)
    numerator = Xmul(t,coupon)
    denominator = PW(b,Xadd(t,1))
    sum = Xadd(sum,Xdiv(numerator,denominator))
tR = Xadd(off,n-1)
redterm = Xdiv(Xmul(tR,R),PW(b,Xadd(tR,1)))
sum = Xadd(sum,redterm)
D = -Xdiv(sum,f)
```

The plain-binary64 derivative in the existing landed code remains a separate baseline. The finite-difference profiles are the exact graphs in B1/Y6, including whether perturbed yields are stored. A public-MDURATION-based slope is another graph and only applies to matched compound cash flows.

Use two explicit private price providers: the present `pcomp_disc(...,false)` graph, and the proven PRICE graph with `excel_bond_pow`. The public PRICE validation is not part of either private numerical provider. For domain experiments separately select: (i) current `y>-f`; (ii) allow a negative base only when all required exponents are integral, rejecting zero bases and invalid powers. The latter is an experiment motivated by the negative-root evidence, not a recovered universal error rule.

**Expected movement:** the association changes target the 26 short seed misses, the single long seed miss, and four of the six r18 misses; some of these sets overlap. The analytical checks do not support an exact gain count. Rows 024/026 remain unexplained. A general solver change targets the remaining stopped-iterate designs, but can regress any current 66 matches if its seed, objective, or return point is wrong. Test the negative-root and cash-scale corpus separately from ordinary price accuracy.

## C2. IRR

### The minimal new candidate

First change only the initialization residual and the zero-return conversion. Keep the current later objective and secant expression so the new hypothesis is isolated.

For two flows, define the raw-guess residual exactly:

```text
raw_initial_F(f0,f1,g):
    b64 = E(1+g)                      // extended, no binary64 store
    term = S(E(f1/b64))
    return Xadd(f0,term)
```

For a concrete multi-flow candidate, use the following specific extension rather than “an NPV-like helper”:

```text
raw_initial_F(flows,g):
    b64 = E(1+g)
    p64 = b64
    total = flows[0]                 // stored
    for k=1 .. len(flows)-1:
        term = S(E(flows[k]/p64))
        total = Xadd(total,term)
        if k+1 < len(flows):
            p64 = E(p64*b64)         // retained extended power
    return total                     // no final extra division
```

The retained power for three or more flows is a candidate, not identified by the two-flow deduction. Test a minimal two-flow-only experimental profile first if necessary; do not infer a production dispatch by flow count without evidence.

```text
convert(v):
    return Xsub(Xdiv(1,v),1)

IRR_candidate(flows,guess,CAP):
    keep existing coercion and sign/error validation
    v0 = Xdiv(1,Xadd(1,guess))
    F0 = raw_initial_F(flows,guess)
    if any required operation is invalid/nonfinite: return #NUM!
    if F0 == 0:
        return convert(v0)

    v1 = Xadd(v0,copysign(0.001,F0))
    F1 = current_baseline_objective(v1)

    repeat CAP times:
        dF = Xsub(F1,F0)
        if dF == 0 or any required operand is invalid/nonfinite:
            return #NUM!
        dv = Xsub(v1,v0)
        numerator = Xmul(F1,dv)
        change = Xdiv(numerator,dF)
        v2 = Xsub(v1,change)
        step = abs(Xsub(v2,v1))
        F2 = current_baseline_objective(v2)
        if any required operation is invalid/nonfinite: return #NUM!
        if step < 1e-7 and abs(F2) < 1e-7:
            return convert(v2)
        v0= v1; F0=F1; v1=v2; F1=F2
    return #NUM!
```

Run CAP=20 and CAP=200 as separate named profiles. A full raw-rate-objective profile instead replaces each later objective with `raw_initial_F(flows,convert(v))`; that is another frozen graph. A `vP(v)` profile adds its stated final multiplication, and a rate-space second-seed profile changes only the seed construction. Do not accidentally implement a zero exit as `return guess`, nor simplify the final conversion to `(1-v)/v`.

Keep the original sign and negative-factor handling while comparing ordinary cases. Broader domain guards require I5's measured answers. A zero secant denominator maps to `#NUM!` in this candidate; a restart is a separate hypothesis needing evidence, not an automatic numerical repair.

**Expected movement:** the converted zero exit targets the verbatim-guess ladder defects; preserving the raw initial residual targets the exceptional collapsed-state pair and potentially other two-flow rows. The eight-ladder group is not proof of eight fixes. The 201 row mismatches include duplicates; report unique-input results as well as the 734-row score. d17/d19 are mandatory structural gates. Changes to the general objective can regress any of the current 533 matches, so do not change RATE or the exact NPV kernel as a side effect.

## C3. BINOM.INV and CRITBINOM

This is the most nearly production-ready specification, conditional on an Excel-compatible CDF over the tested domain.

```text
inverse_binomial(trials,p,alpha):
    preserve established coercion, error ordering and supported trial range
    n = trunc(trials)
    if n < 0 or not (0 < p < 1) or not (0 < alpha < 1):
        return #NUM!
    if n == 0:
        return 0

    F0 = excel_compatible_binom_cumulative(0,n,p)
    propagate any established CDF error
    if F0 > alpha:
        return 0

    lo = 1
    hi = n
    while lo < hi:
        mid = lo + floor((hi-lo)/2)     // exact integer indexing
        Fmid = excel_compatible_binom_cumulative(mid,n,p)
        propagate any established CDF error
        if Fmid >= alpha:
            hi = mid
        else:
            lo = mid+1
    return lo as the function's numeric result
```

The n endpoint is the known cumulative probability 1, so it closes the search. Use checked index conversion consistent with the engine's existing supported trial domain; do not invent a new maximum n or silently cast an unbounded floating input to a machine integer. Error ordering for the negative-fractional-trials probe stays with the existing wrapper until measured.

For diagnostic scans at small n, evaluate every predicate and compare the first accepted k with the binary search result. This tests the required monotonicity of the computed CDF. If a large-n CDF region is not monotone, a lower-bound binary search is no longer observationally guaranteed equivalent to a sequential search; investigate that region before changing production search order.

Both names should call the same kernel as long as paired oracle tests continue to support equivalence. A mass recurrence may provide a starting index for a local correction, but the final acceptance and predecessor certificate must use the full predicate above. At a returned k=1, predecessor rejection is `F(0)<=alpha`; at k>1 it is `F(k-1)<alpha`. A uniform strict predecessor check would mishandle the zero-endpoint tie. No tie epsilon, no rounding alpha to an exact rational probability, and no unverified complemented-CDF substitution.

**Expected movement:** replaying the already retained predicate patch with the present CDF has the measured net results 2302/2308 and 1414/1420, including losses. Reproducing the three named CDF leaves is expected to resolve the remaining six in each replay, but that expectation depends on reproducing all invoked leaf bits and not changing other domain behaviour. It is not evidence that the incomplete-beta kernel as a whole is solved. Protect the 633 recorded BINOM.INV error rows.

## C4. CUMIPMT and CUMPRINC

### Preserve the wrapper exactly

```text
require rate>0, nper>0, pv>0
require type exactly 0 or exactly 1
validate original start>=1, start<=end, end<=nper
lo = ceil(start)
hi = floor(end)
if lo>hi: return positive zero
```

Retain existing conversion and exceptional-value handling. In particular, do not replace nper by an integer or build a remaining exponent from the raw fractional end after selecting integer payments.

### Type 0 restricted recurrence and two generalizations

`PMT_L`, `PMT_EM`, `PMT_QUOTIENT`, and `EXP_leaf` below mean the existing logarithm/exponential/payment substrate described in the prompt. They are dependencies to reuse, not instructions to replace those leaves with host libm. Mathematically `PMT_EM=expm1(-nper*L)` and the stored quotient is `q=pv/PMT_EM`.

```text
coefficient_sum(r,nper,lo,hi,shift,include_rate):
    a64 = E(1+r)
    L = PMT_L(r)                      // established stored leaf
    m = Xsub(nper,hi)
    if shift == ORIGINAL:
        m = Xadd(m,1)
    arg = Xmul(-m,L)
    u = EXP_leaf(arg)
    term64 = E(u*r) if include_rate else u
    sum64 = 0
    for k=hi down to lo:
        sum64 = E(sum64+term64)
        if k>lo:
            term64 = E(term64/a64)
    return S(sum64)
```

The named graphs are:

```text
R0, supported power-of-two control:
    q = PMT_QUOTIENT(r,nper,pv)
    csum = coefficient_sum(...,ORIGINAL,true)
    principal = Xmul(q,csum)

R1, normalized-quotient candidate:
    q = PMT_QUOTIENT(r,nper,pv)
    a64 = E(1+r)
    q1 = S(E(q/a64))
    csum = coefficient_sum(...,SHIFTED,false) // exp(-(nper-hi)*L)
    weighted_sum = Xmul(csum,r)
    principal = Xmul(q1,weighted_sum)

R2, money-product-before-rate candidate:
    q = PMT_QUOTIENT(r,nper,pv)
    csum = coefficient_sum(...,ORIGINAL,false)
    money = Xmul(q,csum)
    principal = Xmul(money,r)
```

For R1, also retain the distinct profile `csum=coefficient_sum(...,SHIFTED,true); principal=Xmul(q1,csum)`. Putting r inside versus after the coefficient loop changes its rounding; the latter profile matches the natural shifted-term derivation most directly. Name them separately, R1a and R1b, instead of letting the implementation choose a common subexpression. At the saturated last singleton, both reduce to the first signature in L1.

R2 has a separate singleton signature determined by `EXP_leaf(-L)`. Even if that leaf is the correctly rounded `S(4/7)`, the signature is `b,b+2,b+2`, distinct from R1; a rounded reciprocal multiplication is not interchangeable with explicit division by 1.75. The graph storing q*r and then explicitly dividing by a64 gives the second L1 signature, but dependence solely on the public PMT is already constrained by E.3. Keep it as a diagnostic control unless a separate private payment coefficient is established.

The explicit R0 graph is supported at `1+r=2` and 4, but even there confirm that its m/L leaves reproduce the established research graph. No claim is made that R1 or R2 solves ordinary rates. They are narrowly specified programs for the next replay.

Run that replay against E.3's existing collision/rejection evidence before requesting L1 or range extensions. R1/R2 are not exempt from the already measured functional-dependence exclusions merely because their arithmetic has been written differently. The new signatures are useful only for graphs not already rejected, or as controls that verify the meaning of the retained certificate.

### Type 1 and interest finish

Freeze the due-payment leaf during a candidate comparison. Initially use the existing `PMT(r,nper,pv,0,1)` result as the chosen leaf, while testing whether Excel's private coefficient agrees through companion observations.

```text
principal_due(lo,hi,finish_profile):
    pmt1 = selected_due_payment_leaf(...)
    later_lo = max(lo,2)
    csum = 0
    if later_lo <= hi:
        csum = coefficient_sum(r,nper,later_lo,hi,ORIGINAL,false)
    if lo != 1:
        return Xmul(pmt1,csum)
    if finish_profile == COEFFICIENT_FIRST:
        return Xmul(pmt1,Xadd(1,csum))
    if finish_profile == PAYMENT_AFTER:
        return Xadd(pmt1,Xmul(pmt1,csum))
```

These profiles differ only in the stated finishing stores. A profile retaining the coefficient sum extended through addition of the unit is distinct and should not be silently substituted for `Xadd(1,csum)`.

```text
interest_range(lo,hi,type):
    if type == 1:
        lo = max(lo,2)
    if lo>hi: return positive zero
    count = hi-lo+1
    payment = selected_payment_leaf(...,type)
    principal = selected_private_principal_range(lo,hi,type)
    return Xsub(Xmul(count,payment),principal)
```

This deliberately removes period 1 before calculating both operands for type 1. Do not compute the whole principal range and subtract period 1 afterward; that is a different rounding graph.

**Expected movement:** the missing singleton rounding is the first target, followed by the 356 CUMPRINC mismatches and cancellation-sensitive CUMIPMT rows. No exact gain count follows from the appendices. The due-loop proposal already known to lose 28 matches is a warning against landing a lower maximum-ulp variant as if it were identified. Protect all 230 current principal and 13 interest matches, distinguish type 0/type 1, and retain the fractional-bound controls.

## C5. DURATION, MDURATION, PRICE

Preserve the current schedule and primitive arithmetic. Add separate choices for time/exponent construction and discount evaluation. Do not make one switch stand for several changes.

```text
off = Xdiv(Xsub(E,A),E)
afrac = Xdiv(A,E)

time_T0(k): Xadd(off,k)
time_T1(k): Xsub(k+1,afrac)

redemption_T0: Xadd(off,n-1)
redemption_T1: Xsub(n,afrac)
redemption_T2: Xsub(Xadd(off,n),1)
```

Discount exponents and weighted times are two independent uses of these expressions. Preserve both in the trace. The zero-yield probes identify time/weighting behaviour without resolving which exponent feeds a nontrivial power.

```text
discount_FULL(k):
    return existing_bond_pow(base,selected_exponent(k))

discount_SPLIT(k):
    fractional = existing_fractional_pow(base,off)
    integer = existing_integer_pow(base,k)
    return Xmul(fractional,integer)

discount_RECURRENT:
    D0 = existing_fractional_pow(base,off)
    D(k+1) = Xmul(Dk,base)
```

The fractional provider itself initially remains unchanged. Only after exponent/weighting probes reject those explanations should one add a specific retained-product profile such as `EXP_leaf(E(exponent*LN_leaf(base)))` versus a profile storing that product at binary64. A name such as “x87 pow” is insufficient; its input precision, product store, range reduction, and output store must be actual existing leaves or newly specified operations.

For DURATION retain the existing finishing graph below. `Wmul_coupon` and `Wmul_red` mean the exact weighted-multiplication leaves already in the pinned implementation; keep ordinary binary64 multiplication if that is what the current code uses. The supplied excerpt establishes the quotient, sum and final-ratio stages, but is not enough to replace every weighted multiplication by Xmul. Ordinary `S(a*b)` and `S(E(a*b))` can differ at rare double-rounding cases.

```text
couponPV = Xdiv(coupon,discount_k)
den = Xadd(den,couponPV)
num = Xadd(num,Wmul_coupon(time_k,couponPV))

redPV = Xdiv(100,discount_R)
redWeighted = Xdiv(Wmul_red(time_R,100),discount_R)
den = Xadd(den,redPV)
num = Xadd(num,redWeighted)
duration = Xdiv(Xdiv(num,den),frequency)

mduration = Xdiv(duration,Xadd(1,Xdiv(yield,frequency)))
```

For PRICE retain its own existing sum and redemption ordering, including the matching single-coupon branch; do not automatically copy DURATION's per-operation store profile into PRICE. The user-provided code is the baseline specification for those already successful stages.

The small-integer weighting products in the two zero-yield signatures are exact within a 64-bit significand before the binary64 store, so their stated predictions are unaffected by this S-versus-X multiplication distinction.

**Expected movement:** a successful fractional/time correction targets the 177 DURATION, 147 MDURATION, and 50 PRICE misses. The banks differ, so a one-for-one propagation count from DURATION to MDURATION cannot be promised. Protect the current 7707, 1113, and 9103 matches and especially the on-coupon controls. The 50 PRICE misses need not have the same cause as the power-of-two-sensitive DURATION time association.

# D. Further black-box experiments and a practical next round

Cost labels refer to Excel cells: **cheap** means roughly fewer than 100 calls, **medium** means roughly 100–2,000, and **expensive** means several thousand or an adaptive campaign. Local prediction and trace generation are for your harness and do not consume oracle calls.

## D1. YIELD

**Recover the stopping frontier as a measurement instrument — cheap to medium.** Once the seed profile is supported, locate adjacent price doubles that switch from the seed to another iterate. The price difference divided by the candidate slope constrains the actual tested quantity. Repeat the same geometry across frequencies and safe cash scales. An annual-step rule moves the price frontier with f; a periodic rule does not for the matched periodic examples; a cash residual rule moves with cash scale. A single frontier leaves strictness-at-equality and hidden extra tests unresolved, so record bounds rather than a fitted exact epsilon.

**Observe the seed where Newton deliberately does nothing — cheap.** Use the redemption-normalized zero-coupon manifold in Y4 and the raw-rate collision pair in Y1. These deliberately make the seed error less than the measured threshold but vastly larger than one output ulp. They expose the seed without backward reconstruction through quadratic convergence. Follow each returned-seed region until the first update appears; this also supplies iteration-count-sensitive inputs.

**Use the negative-root branch as a trajectory certificate — medium.** The small-cash n=2 price/redemption 0.8 observation cannot arise by merely stopping the stated homogeneous positive trajectory earlier. Survey its boundary against cash exponent, price/redemption ratio, and even/odd n. A sharp absolute-money transition suggests a cash-valued guard; an exponent-limit transition suggests range effects; branch-specific failures implicate the internal power/domain path. Do not “repair” the negative yield to the financially conventional branch.

**Infer the derivative only after constraining the seed and objective — medium.** At a returned-seed frontier, the hidden ratio is near the tolerance. A known seed and a constrained private price give a bound on the slope used there. Repeating on both sides and at several f values distinguishes analytic duration, annual finite differences, and periodic finite differences much more efficiently than comparing final roots.

**Cap tomography with a defined counter — expensive until the trajectory is known.** Record separately: accepted updates, convergence-test index, and objective-evaluation count. For a pre-update test, accepting after 100 updates can require test 101. Build traces with nearby acceptance counts and ask their exact inputs. Contrast return-last with `#NUM!` on still-finite exhaustion cases. A grid of #NUM outcomes alone is not a cap measurement.

**Reverse iteration using rounding intervals — expensive, later.** Earlier real-valued inverse-Newton searches already failed; do not repeat them unchanged. Once the derivative, count and final conversion are constrained, invert the actual rounded operation graph into intervals of possible predecessors. Intersect these with the observed smooth seed surface. This may distinguish retained-register seeds from stored rational expressions; without those constraints it remains massively underdetermined.

## D2. IRR

**Partition inputs by complete candidate state — cheap.** For an ulp ladder of guesses, group those mapping to identical `(v0,F0,v1,F1)` under a proposed initializer. If Excel outputs vary within a group, the entire downstream solver is irrelevant: the initializer is refuted. This is stronger and cheaper than scoring thousands of objective/update masks. Repeat grouping after exact cash rescaling to identify which residual/branch retains the raw guess.

**Use exact multiple roots to slow convergence — cheap to medium.** The polynomial `[1,-4,4]` creates a known linear-convergence regime. Related factorizable cash-flow polynomials provide several cap witnesses without requiring enormous or tiny money. Keep rounding-induced zero and residual acceptance visible; the purpose is an identifiable trace, not merely a hard example.

**Use sign reversal as an initialization test — cheap.** Negating all flows preserves the mathematical roots and the ideal secant trajectory if the seed is unchanged. If it changes the selected root or error outcome, a sign-sensitive seed or branch is exposed. The quadratic vertex example amplifies a 0.001 seed choice into a large next-point difference. This can be paired with the negative-factor probes to locate a safeguard.

**Separate the nonlinear residual from its root set — cheap.** `P` and `vP` have the same nonzero roots but different secant curvature. The exact two-flow P-versus-vP example tests the helper's final division much more directly than ordinary multi-flow roots. Use curvature signatures as necessary conditions before broad witness replay.

## D3. BINOM.INV and CRITBINOM

**Recover effective CDF bits through the inverse — cheap locally, medium globally.** For fixed n,p,k, find the largest alpha double for which the inverse answer is at most k. If the observed rule holds and the relevant CDF values are monotone and interior, this threshold is `F(k)` for k>=1 and `nextdown(F(0))` for k=0. A small ulp ladder around the public CDF is usually enough. Bisection over the entire positive-double alpha lattice below 1 takes at most 62 queries per threshold. Handle rounded-zero/one endpoint tails separately, because alpha itself has the open domain `(0,1)`.

This identifies whether a large-n inverse is using the public tail, a complemented tail, or another rounded quantity. A discrepancy cannot be fixed by merely changing the integer-search start if the final acceptance predicate remains the same.

**Request disagreement certificates, not random quantiles — medium.** The useful row is `(n,p,k,F(k),alpha,inverse)` where two frozen candidate tail paths straddle alpha. Centering alpha on a high-precision true CDF can miss Excel's actual binary cutoff. Include the predecessor certificate, with its special zero-endpoint inequality.

**Respect the identifiability limit — no calls can remove it.** Two complete searches over an identical monotone acceptance predicate are functionally indistinguishable. Recover the observable mapping needed for compatibility; use timing only if internal search order affects another required observable and can be measured reliably.

## D4. CUMIPMT and CUMPRINC

**Find collisions in a proposed money-dependent intermediate — cheap.** Adjacent pv values that give the same stored `q/a` must produce the same output in any graph depending on pv only through that value. Different Excel outputs refute the graph before coefficient fitting. Conversely, the three-point L1 signatures locate a store more directly than a single-rounding coefficient fit.

**Use cancellation as an amplifier — cheap.** The tiny one-period CUMIPMT example turns a payment-sized ulp into hundreds of millions of interest ulps. Ask public operand companions and compare on the payment lattice. That is stronger evidence of operation placement than comparing relative errors of the small interest result.

**Use fractional nper to expose shifted exponents — medium.** After the terminal saturated test identifies a plausible money operation, repeat nper values `17`, `17.25`, `17.5`, `17.75` with the same integer ranges. A shifted exponent and a normalized quotient remain mathematically equivalent, but their existing EXP leaves and stores generate different predictions. This uses established fractional semantics rather than creating a new special case.

**Use range partitions as evidence, not identities — medium.** Whole-range versus separately computed prefix/suffix results reveal aggregation stores. The prompt already demonstrates nonadditivity, so do not propose `left+right` as a fix. Use its pattern to discriminate the remaining extended-loop and finishing graphs.

## D5. DURATION, MDURATION, PRICE

**Probe binade crossings in the redemption time — cheap.** The two-call zero-yield signature distinguishes three plausible expressions. If one wins, add n=3,4,7,8,15,16 at the same settlement fraction. A defect appearing only when `off+n` crosses a binade is much more specific than “fractional powers sometimes differ.” Apply the winning expression to the appropriate weighted time or exponent, not automatically both.

**Recover the hidden power by rounded quotients — medium.** Zero-coupon PRICE at multiple non-dyadic redemptions constrains the denominator. Test the assumption that it is a stored double; an empty intersection can reveal a retained extended value or another quotient finish. Then compare full exponent, split factor, and recurrence directly at the hidden stage.

**Use base and exponent collisions — medium.** Select yields or schedules that collapse to the same stored base/exponent in one graph but remain distinct in an extended graph. A changed Excel answer is a one-sided refutation of the stored graph. Several numerators help avoid an inconclusive final rounding collision.

**Use zero-yield controls before transcendental work — cheap.** At base 1 every power is exactly 1. Any remaining discrepancy belongs to time construction, cash weighting, or accumulation. Only after those pass should another fractional ln/exp profile receive a large round of oracle calls.

## Proposed next round

Run the supplied fixed witnesses first, then spend calls on local candidate disagreements. This preserves the requested function priority while keeping the first round within the normal 1,000–5,000-call budget.

| Order | Calls to prioritize | Approximate allocation |
|---|---|---:|
| 1 | YIELD association witnesses, raw-rate collision, threshold brackets, frequency and redemption manifolds; selected new seed disagreements | 300–600 |
| 2 | IRR collapsed-state ladder, P/vP, tiny-flow stop, multiple-root cap, sign/domain and d17/d19 frontier controls | 300–700 |
| 3 | Both inverse names at the three CDF leaves, strict-zero composition and complementary-tail separator; selected large-n cutoffs | 100–250 |
| 4 | Loan three-point signatures, one-period operand companions, type-1 finishes, fractional-bound controls | 100–250 |
| 5 | Two-call duration signature, zero-yield troublesome schedules, private-discount/base-collision probes | 100–250 |

The broad grids in B are candidate-selection pools, not instructions to buy every cross-product in the first round. Exact forecast files should be generated by your harness before the oracle answers are visible. Record both the bank-row score and unique-input score, plus gained/lost row IDs and structural number/error changes. Near-par seed association, initialization state, CDF predicate, loan recurrence, and bond exponent construction should be separate changes so their evidence remains interpretable.

## Questions answered and boundaries still open

| Area | Concrete answer supplied | Still requiring Excel evidence |
|---|---|---|
| YIELD seed evaluation | Two new stored graphs; exact selected witness predictions; direct general-redemption seed probes | All 70 seed rows together, basis-dependent par outputs, general off-coupon seed |
| YIELD iteration | Pre-update step-stop model; explicit derivatives/domain/cap profiles; scaling contradiction and negative-root proof | Actual derivative, cash-sensitive path, strict equality, counter and exhaustion result |
| IRR | Initial-state impossibility proof; raw-guess residual candidate; exact initialization and stop discriminators | Full objective/stores, structural rescue path, cap/domain details |
| Discrete inverses | Strict-zero/inclusive-interior public-CDF specification and implementable lower bound | The three blocking CDF leaves, large-n branches, any paired-name divergence |
| Cumulative loans | Preserved cancellation and extended recurrence; five terminal signatures; one-period bound and companion calls | General second money rounding, due coefficient and finish, exact private operands |
| Bond family | Exact exponent-association signature, staged discount profiles, denominator reconstruction | Winning time/power profile and its held-out schedule coverage |

The implementation work with the best immediate evidence is therefore: replay the two YIELD seed associations; repair IRR's demonstrably insufficient initialization model; use the established inverse predicate together with the required CDF bits; and select the missing loan and bond store graphs with the small exact signatures. None requires changing the project's criterion from Excel's recorded bits to mathematical accuracy.

# Sources

**Primary numerical evidence:** the supplied `11-solvers-prompt.md`, especially Appendices B.3–B.8, C.2–C.3, D.2–D.3, E.2–E.3, and F.1–F.3. References to counts, hexadecimal outputs, previous refutations, and implementation staging in this response cite those sections. The public repository was identified by the supplied commit; no claims here depend on unseen current repository contents.

The following are public user-facing Microsoft documentation, checked on 10 November 2026. They supply syntax, ordinary domain rules, and documentary iteration statements; they do not establish the reconstructed arithmetic graphs.

- **S1:** [YIELD function](https://support.microsoft.com/en-us/excel/functions/yield-function).
- **S2:** [ODDFYIELD function](https://support.microsoft.com/en-us/excel/functions/oddfyield-function).
- **S3:** [IRR function](https://support.microsoft.com/en-us/excel/functions/irr-function).
- **S4:** [RATE function](https://support.microsoft.com/en-us/excel/functions/rate-function).
- **S5:** [BINOM.INV function](https://support.microsoft.com/en-au/excel/functions/binom-inv-function).
- **S6:** [CRITBINOM function](https://support.microsoft.com/en-us/excel/functions/critbinom-function).
- **S7:** [CUMIPMT function](https://support.microsoft.com/en-us/excel/functions/cumipmt-function).
- **S8:** [CUMPRINC function](https://support.microsoft.com/en-gb/excel/functions/cumprinc-function).
- **S9:** [DURATION function](https://support.microsoft.com/en-us/excel/functions/duration-function).
- **S10:** [MDURATION function](https://support.microsoft.com/en-us/excel/functions/mduration-function).
- **S11:** [PRICE function](https://support.microsoft.com/en-au/excel/functions/price-function).

All other mathematical statements are explicit derivations in this response. No restricted implementation material was used.



