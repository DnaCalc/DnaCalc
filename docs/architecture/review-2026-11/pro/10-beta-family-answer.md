# Excel's incomplete-beta family: computation shapes, decisive probes, and an implementation specification

**Response to `10-beta-family-prompt.md`; repository reference:** `b2a808502f74e1590dcfc682171c8a1db1d93ee2`.

## Scope and principal recommendation

The strongest next step is to expose **the information passed from each distribution wrapper into the beta calculation**, and to test **the already successful gamma/binomial arithmetic inside the beta prefactors**. Another global tolerance change is unlikely to identify the missing computation.

Several conclusions in the prompt need narrowing. The F windows reject particular rounded argument interfaces, not binary64 arithmetic in general. The large TINV error does not identify its stopping rule: converting a rounded beta quantile near one into a t quantile can amplify a sub-ulp beta error into hundreds of t ulps. The eight `bgrat` matches lost by tightening its tolerance can be accidental cancellations of errors; they do not establish that Excel stops the same series earlier.

I have not executed numerical experiments, candidate kernels, or repository code. Numerical evidence below comes from your appendices. New exact-bit predictions are derived explicitly from binary arithmetic; other proposed programs must be evaluated by your implementation agent. References such as **prompt A.6** identify your supplied evidence. External references at the end support mathematical identities and permitted algorithm families, not claims about Excel's provenance.

The exact general beta dispatch, normalizer, and inverse stopping constants are **not recoverable uniquely from the supplied measurements**. Section C therefore separates observed behavior, executable candidate programs, and constants that still need identification. It does not present an invented complete Excel implementation.

---

# A. Most plausible computation shapes

## A.1 BETA.DIST and BETADIST

### What is established

The 294 paired calls support a shared cumulative computation for the two public names. The unit-shape programs have much stronger evidence than the general dispatch. Preserve their order, including the fact that the `b == 1` arm wins when both shapes are one and `x <= RN(0.9)`:

| Condition, after the established boundary handling | Supported program |
|---|---|
| `b == 1 && x <= RN(0.9)` | `EXP(RN(a * LN(x)))` |
| Otherwise, `a == 1 && x >= RN(0.1)` | `RN(0.5 + RN(0.5 - EXP(RN(b * LN(y)))))` |
| Otherwise, `a == 1` | The existing identified `-EXPM1(log1p_times(b,-x))` graph, including its retained product precision |

Here `RN` denotes binary64 rounding to nearest, ties to even, and `y` is the complement supplied to the kernel. For the public standardized beta call it is normally `RN(1-x)`. These statements are empirical program identifications over the probed domain, not proofs over every accepted input. The code shown does not justify replacing the remaining `b == 1, x > 0.9` route with a newly invented shortcut. [Prompt A.3, B.1, C.]

The uniform distribution is an especially valuable anchor. Your recorded

```text
BETA.DIST(2^-30,1,1,TRUE) = 0x3e10000000000004
```

is four ulps above the input. An exact polynomial or identity shortcut is therefore wrong for this case. More generally, the integer examples reject a universal accurately evaluated integer-polynomial implementation. Match percentages alone do not exclude every possible conditional integer shortcut.

### Separate the prefactor question from the summation question

Write

\[
H=\frac{x^a y^b}{B(a,b)},\qquad C=H/a,\qquad y=1-x.
\]

Two legitimate series use different leading factors:

\[
I_x(a,b)=\frac{x^a}{aB(a,b)}S_0,
\quad
S_0={}_2F_1(a,1-b;a+1;x),
\]

\[
I_x(a,b)=C S_1,
\quad
S_1={}_2F_1(a+b,1;a+1;x).
\]

The second series has positive terms for positive shapes and `0 < x < 1`. The first is the representation underlying your `bpser`. A prefactor replacement must account for the missing `y^b` in the first representation. Substituting `C` directly into the current `bpser` would be a mathematical error. [DLMF §8.17, equations 7–8.]

The most productive ranking has independent axes:

| Axis | First hypothesis to test | Main alternative |
|---|---|---|
| Shape normalizer | Shared Stirling-error/deviance machinery, with beta-specific scaling and the existing running exponential | A private DiDonato–Morris-style normalizer with different leaf/staging details |
| Tail evaluator | A hybrid series/recurrence/continued-fraction/asymptotic family, provisionally retaining the current dispatch | A simpler series/continued-fraction hybrid without the current `bgrat` region |
| Precision | Ordinary binary64 stages plus selected retained operands or residuals | Selectively wider intermediates carried through the operations that consume them |

A plain public-GAMMALN prefactor plus a textbook fraction ranks below these: your earlier controlled experiments already reject the tested programs. A private gamma-related normalizer remains possible. Likewise, “use all intermediates wider” is not a justified default: it can destroy the established unit-shape fingerprints.

The TOMS arm labels in A.3 describe **your implementation**. They are not observed Excel branch labels. The error correlation in R43 makes a related hybrid a reasonable baseline; it does not identify its switch constants. Conversely, 8% exact agreement in one arm does not disprove that arm when its prefactor is still unidentified.

### The Loader bridge is more precise than “try another log beta”

Let `n = a+b` in exact mathematics and define a generalized binomial mass

\[
M=\frac{\Gamma(n+1)}{\Gamma(a+1)\Gamma(b+1)}x^a y^b.
\]

Then

\[
\boxed{C=\frac{b}{n}M.}
\]

This follows directly by applying the gamma recurrence to all three factors. It connects the beta continued-fraction prefactor to the arithmetic family that already works for your masses.

With Stirling remainder `s` and deviance `d` defined by

\[
s(z)=\log\Gamma(z+1)-(z+\tfrac12)\log z+z-\tfrac12\log(2\pi),
\]

\[
d(k,\mu)=k\log(k/\mu)+\mu-k,
\]

the exact prefactor is

\[
\boxed{\log C=s(n)-s(a)-s(b)-d(a,nx)-d(b,ny)
+\tfrac12(\log b-\log a-\log n-\log(2\pi)).}
\]

This is an algebraic extension of the saddle-point mass identity discussed by Loader. It avoids public GAMMALN. It does **not** prescribe one floating-point graph: obtaining `M` and multiplying by `b/n`, absorbing that ratio into its exponent, and using shifted mass parameters are different experiments.

There is a particularly useful rounding issue. If `mu_a=RN(n*x)` and `mu_b=RN(n*y)`, while `n=a+b` is exact, replacing the two deviances by `d(a,mu_a),d(b,mu_b)` requires the following additional terms to recover the exact logarithm corresponding to the original operands:

\[
(\mu_a+\mu_b-n)
+a\log\frac{nx}{\mu_a}
+b\log\frac{ny}{\mu_b}.
\]

For `x+y=1`, writing `mu_a=nx(1+delta_a)` and similarly for `b`, their combined first-order contribution is

\[
(nx-a)(\delta_a-\delta_b).
\]

It is suppressed near the mean and can grow with shape in the tails. This is a concrete mechanism for the observed regional error pattern. It is a hypothesis to test, not an accuracy correction to install automatically. Analyze the terms together: treating each as an independent error loses their cancellation. The full correction also accommodates probability operands whose exact real sum differs from one; the simplified first-order cancellation above assumes their sum is one. When `RN(a+b)` differs from the exact shape sum, a separate issue arises. Start with exact-sum integer and half-integer shapes.

The integer mass evidence does not identify a small fractional-shape Stirling helper. Reuse an existing real-shape helper only where you actually have it. An interpolation of the integer table would be unjustified.

### The large-a, small-b block contains a direct gamma experiment

The supplied `bgrat` code defines, mathematically,

\[
\nu=a+(b-1)/2,\qquad z=-\nu\log x,
\]

\[
\boxed{r=\frac{z^b e^{-z}}{\Gamma(b)}
=z f_\Gamma(z;b,1),}
\]

and uses `Q(b,z)`, an upper incomplete-gamma ratio. Its other scaling factor satisfies

\[
\boxed{\frac{u}{r}=\frac{\Gamma(a+b)}{\Gamma(a)\nu^b}.}
\]

Thus three specific substitutions can be tried while retaining the current expansion:

1. Replace the local incomplete-gamma tail with the already successful gamma-family upper-tail evaluator.
2. Replace `r` with the shared gamma-density/Poisson-like prefactor, preserving its established evaluation order.
3. Replace the gamma-ratio correction using shared Stirling pieces.

A useful exact expression for the third quantity is

\[
\log(u/r)=(a+b-\tfrac12)\log1p(b/a)
+b\log(a/\nu)-b+s(a+b)-s(a).
\]

The original code evaluates several exponentials separately and rounds `z`; the identities above do not make a fused exponent bit-equivalent. Test the three substitutions independently before combining them. The 99.8% aggregate GAMMA.DIST agreement is a lead, not a guarantee that each real `b < 1` primitive is already exact.

This is a more focused experiment for the 943-row block than abandoning the entire asymptotic arm. General large-a beta expansions naturally involve incomplete gamma functions; that mathematical relationship is independently documented in DLMF §8.18.

### What the current evidence does not establish

- **Exponent-sized error does not prove an uncompensated final sum.** A rounded logarithm, its multiplication by a shape, a partly compensated sum, or a rounded deviance can all produce this scaling. R44–R45 already caution against the strongest version of O3.
- **Near-one exact matches do not identify the small-tail computation.** Complementing maps many distinct tail values to the same larger output.
- **The five complementary pairs summing below one do not identify the complement spelling.** Independent evaluation, argument complement rounding, and branch changes can also do that. Experiment E3 below distinguishes the actual rounded programs directly.
- **The eight lost `bgrat` rows do not identify an earlier stop.** Earlier truncation can cancel a prefactor error. Compare complete term sequences and prefactors, rather than fitting a tolerance to those eight rows.

## A.2 FDIST, F.DIST, and F.TEST

### The F mapping and the actual information-loss result

For F value `f`, let

\[
a=d_2/2,\quad b=d_1/2,\quad
v=\frac{d_2}{d_2+d_1f},\quad
w=\frac{d_1f}{d_2+d_1f}.
\]

The right tail is `I_v(a,b)` and the left CDF is `I_w(b,a)`.

For any proposed internal state `T(f)`, a collision with different Excel answers proves that Excel is not a deterministic function of **only that state**. A.6 therefore conclusively rejects your current rounded pair and the other tested states with conflicts. It does not prove that no binary64 pair could work, or that the processor must retain x87 intermediates.

My ranking is:

1. A beta-related evaluator retaining original `f`, an unnormalized numerator/denominator, or caller-specific logarithms/prefactor information.
2. A dedicated F series or fraction using its original variable, mathematically equivalent to a beta calculation.
3. Extended or compensated normalized arguments retained through subsequent calculations.
4. Another exclusively normalized binary64 transform, screened by collision tests before implementing a kernel.

A prefactor can use

\[
a\log d_2+b\log(d_1f)-(a+b)\log(d_2+d_1f)-\log B(a,b)
\]

while recurrence coefficients use rounded `v,w`. Such an evaluator can distinguish a normalized-pair collision using entirely binary64 arithmetic.

There is also a useful direct expression for the beta deviance. With `n=a+b`, exact arithmetic gives

\[
\boxed{D=n\log1p\left(\frac{b}{n}(f-1)\right)-b\log f.}
\]

This permits an F-native saddle-point prefactor. Near `f=1` its terms cancel, so the precise graph and any deviance expansion matter. It is a candidate, not an instruction to replace the established deviance everywhere.

Zero conflicts for `RN(d1*f)` in the old windows do not identify that interface: it happens to preserve enough information there. E2 supplies a window in which the multiplication itself must collide.

F.DIST's greater match rate is consistent with larger probabilities concealing small-tail differences. It does not independently establish a more accurate or different kernel. The supplied FDIST/F.DIST.RT equivalence is strong evidence that those two names share a computation. [Prompt A.5–A.6, C.]

### F.TEST: an exact-variance control exposes a remaining discrepancy

Let `S_k` contain `k` copies of `-1`, one zero, and `k` copies of `+1`. Its sample variance is exactly one under both exact arithmetic and your shown two-pass algorithm.

The supplied witnesses give

```text
F.TEST(S_4,S_3)       = 0x3fef0e2c0bbdd312
FDIST(1,6,8)         = 0x3fdf0e2c0bbdd318
2*FDIST(1,6,8)       = 0x3fef0e2c0bbdd318
```

The doubling is exact. Subject to confirming these rows on the same Excel build, the six-ulp difference rejects the literal public composition `2*FDIST(1,6,8)` for this witness. Within the supported two-pass variance family, it isolates a wrapper/tail discrepancy. An unmeasured internal variance algorithm could still introduce different rounding; mathematical variance one does not exclude every such algorithm. Include VAR.S and permutation controls in E9. [Prompt A.5, A.8.]

Keep the landed `2*min` rule. Determine whether the smaller value comes from an internal complement output, public F.DIST, a subtraction from RT, or a separate internal F calculation. Also determine how exactly equal variances select the degree orientation. The translated-array observations support the current variance family but do not prove every sum or division bit-identical.

## A.3 TDIST and low-degree formulas

For `t >= 0`, the two-sided tail is

\[
Q_\nu(t)=I_{\nu/(\nu+t^2)}(\nu/2,1/2).
\]

A beta-related kernel with caller-specific state is the leading hypothesis. Carry the same argument-loss questions from F into t; do not assume the rounded pair is sufficient merely because only 46 paired t inputs were asked.

The exact half relation supports a common calculation with final binary scaling. Over normal outputs, it cannot identify whether Excel starts with the one-tail or two-tail result: multiplication or division by two is exact in either direction.

For `df=1` and `t>0`, the mathematical two-tail formula is

\[
Q_1(t)=\frac{2}{\pi}\arctan(1/t).
\]

At `t=0`, use the endpoint `Q_1(0)=1`. The observed `TDIST(1,1,2)=0.50000000000000011` is a fingerprint against an exact-half special case, but does not rule out every trig program without measuring its constants and leaves.

For `df=2`, the cancelling formula has the stable rationalization

\[
Q_2(t)=1-\frac{t}{\sqrt{t^2+2}}
=\boxed{\frac{2}{\sqrt{t^2+2}(\sqrt{t^2+2}+t)}}.
\]

The eight rejected spellings omit this form. They therefore do not rule out a dedicated closed form.

There is an even more specific explanation of the `t=4.2` versus `t=7.5` behavior. The established beta `a=1` switch is at `v=0.1`; here that corresponds mathematically to `t^2=18`. The former point lies on the cancelling EXP/LN side, the latter on the LOG1P/EXPM1 side. This is consistent with the data and gives a precise boundary experiment. It is not yet proof that TDIST uses those beta arms. [Prompt A.7, B.1.]

## A.4 BETAINV, FINV, and TINV

### Identify the returned coordinate before the stopping tolerance

The straightforward shared-beta architecture is

\[
z=I_p^{-1}(d_2/2,d_1/2),\quad
F=\frac{d_2}{d_1}\frac{1-z}{z},
\]

\[
z=I_p^{-1}(\nu/2,1/2),\quad
t=\sqrt{\nu\frac{1-z}{z}}.
\]

For the t transformation,

\[
\frac{\delta t}{t}\simeq-\frac{\delta z}{2z(1-z)}.
\]

When `z` is close to one, even excellent beta inversion followed by rounding `z` can produce a large t error. Returning the small complementary coordinate separately avoids this mechanism, but compatibility may require reproducing the loss. Boost's permitted implementation documentation explicitly describes returning both coordinates for its t quantile; this establishes the architecture's plausibility, not Excel's implementation.

This is my first inverse hypothesis to test. Second is direct safeguarded Newton/secant inversion of an internal forward; third is a dedicated t approximation with a few corrections. Newton versus Halley, the start, and the stop remain unidentified.

A.4's error versus the exact root cannot distinguish forward error from stopping error. If the computed forward is `Fhat(x)=F(x)+e(x)` and the solver exits with residual `rho=Fhat(x)-p`, then

\[
x-x_*\simeq\frac{\rho-e(x)}{F'(x_*)}.
\]

Moreover, the df=25 table's error falls from 176.3 ulps at `p=.85` to 2.9 at `.97`; it does not show monotone deterioration toward one. A probability-scale tolerance around `1e-15` is a candidate, not a recovered constant.

Your lattice bisection also needs careful description. Small reversals invalidate the monotonicity premise that would guarantee bisection finds the first crossing; it can select a path-dependent local crossing. Its success is useful evidence, but “bit-converged” does not make its result the unique root that Excel ought to return. The error decomposition deliberately evaluates `e` at the returned point, because rounding error need not vary smoothly between neighboring inputs.

The inverse unit-shape formulas and transforms are listed as concrete candidates in C.6. Do not install more accurate versions without oracle evidence.

## A.5 The discrete distributions

| Function | Mathematical cumulative relation | Most plausible implementation family |
|---|---|---|
| NEGBINOM.DIST(f,s,p,TRUE) | `I_p(s,f+1)` | Beta-related calculation with established special cases; not necessarily a sum of public masses |
| BINOM.DIST(k,n,p,TRUE) | `I_(1-p)(n-k,k+1)` | Beta-related or finite-tail computation retaining original probability information and caller-specific prefix stages |
| HYPGEOM.DIST | A finite hypergeometric sum | Ratio-of-masses/product method plus finite summation or recurrence; no general single-beta identity |

The many exact masses and endpoints do not measure the quality of the beta CDF route. In particular, the BINOM bank has only 14 interior cumulative cases. [Prompt A.1, A.9–A.12.]

### An especially revealing BINOM witness

For `k=1,n=1000`, the adjacent input probabilities

```text
p- = nextdown(0.5) = 0.5 - 2^-54
p0 = 0.5
```

both have `RN(1-p)=0.5`, yet Excel returns respectively

```text
0x020f48000000027c
0x020f48000000027d
```

This directly rejects a CDF depending only on rounded `q=1-p` and shapes. It also reveals a one-ulp increase where the mathematical CDF decreases.

A plausible ordinary-double graph is

\[
F_1=q^{n-1}(q+np),
\]

with rounded `q` in the large exponential but original `p` in the positive correction. Holding `q` fixed while increasing `p` makes this expression increase. A concrete EXP/LN spelling is in C.7. This does not require more accurate transformed arguments, and could arise inside a general beta recurrence rather than an explicit `k=1` shortcut.

At `p=.5`, the exact value is `1001*2^-1000`, exactly represented by `0x020f480000000000`; the stated Excel answer is 637 ulps higher. This bit pattern follows from the integer significand 1001 and exact power-of-two scaling. It rules out the exact dyadic-power polynomial here, but not an EXP-of-rounded-LN prefix or a sum of noisy public mass outputs.

The 4,698/5,000 public BINOM/beta equalities do not prove unrelated routines where they differ. Many differences can come from loss of the original complement; the already observed unit-shape product-precision difference shows a further caller-specific stage. Preserve that distinction. R46's remaining pair-based failures mean that simply keeping `p,q` is insufficient as a complete answer.

### HYPGEOM remains a separate investigation

For support `L=max(0,n+K-N)` through `U=min(n,K)`, its mass has a ratio-of-three-binomial-masses identity and an adjacent-mass recurrence. Both are specified in C.7. Neither requires incomplete beta. The small-N measurements cannot identify behavior for larger N, and do not license replacing it with a binomial/beta approximation. The previously successful `(a/c)*b` ordering should be retained as a control.

## A.6 Beta density

There are no measurements here from which to identify Excel's density program. The leading hypotheses are:

1. Shared mass/prefactor machinery with shifted shapes.
2. The same `H` used by cumulative evaluation, divided by `x*y`.
3. A private log-beta density expression with special boundary handling.

For integer shapes `a,b >= 1`,

\[
\boxed{f_{\rm Beta}(x;a,b)=(a+b-1)
\operatorname{BinomMass}(a-1,a+b-2,x).}
\]

This supplies an unusually direct bit-level probe against the mostly matching mass family. Equality as a mathematical identity does not imply multiplication of the public rounded mass is the actual implementation.

The current generic log expression is not a complete endpoint implementation: unit shapes at endpoints encounter `0*log(0)`. Probe finite, zero, and divergent endpoint cases explicitly before specifying Excel error values. Also determine whether interval-width scaling occurs after exponentiation or inside its exponent. E10 supplies both experiments.

---

# B. Ranked alternatives and discriminating experiments

## Conventions and how to read a negative result

For a positive finite double `c`, define

```text
W(c,h) = { from_bits(bits(c)+j) : j=-h,...,+h }.
```

All powers of two, integers in range, and dyadic fractions below specify exact binary64 inputs. `RN(r)` means the correctly rounded value of the stated rational or expression; decimal anchors such as `.85` mean the exact binary64 value in your supplied row. Send bit patterns to the service, not decimal text reparsed by an uncontrolled path. Hexadecimal floating-point notation, where used, is mathematical input notation rather than Rust source syntax.

For a fully specified candidate graph, the prediction is **its exact returned bits**, using your already characterized leaves. I cannot calculate unspecified last-bit results here. Select witnesses where competing graphs predict different bits before submitting the request. A single contradiction rejects a deterministic graph; agreement merely leaves it alive.

Record Excel build, process bitness, input route, and raw result bits for the batch. Windows x64 does not by itself identify Excel's process bitness or its arithmetic graph. Recheck a few old witnesses, aliases, and repeated calls in the same request before interpreting a historical cross-build discrepancy as a kernel property. These are experimental controls, not a request to repeat the whole bank.

## E1 — Exact F arguments: separate wrapper arithmetic from transform precision

**Highest priority.** For `k=3,...,7`, choose integer degree pairs with `d1+d2=2^k`, including asymmetric pairs and both unit-shape orientations. At `f=1`, request

```text
FDIST(1,d1,d2)
F.DIST(1,d1,d2,TRUE)
BETA.DIST(d2/2^k,d2/2,d1/2,TRUE)
BETA.DIST(d1/2^k,d1/2,d2/2,TRUE)
```

Both normalized arguments are exact dyadics. A shared beta kernel differing **only in argument-formation precision** predicts equality of corresponding outputs, even if it retains wider arguments. A mismatch establishes another difference: prefactor construction, evaluator, orientation, or internal precision beyond argument formation.

The strongest single anchor is

```text
FDIST(2^30-1,2,2)
BETA.DIST(2^-30,1,1,TRUE)
```

The F numerator is exactly `2^31-2`, denominator `2^31`, and beta pair exactly `(2^-30,1-2^-30)`.

| Hypothesis | Predicted FDIST bits |
|---|---|
| Direct rational `RN(1/RN(1+f))` | `0x3e10000000000000` |
| The supplied beta unit graph at this exact pair | `0x3e10000000000004` |

No argument-rounding explanation can reconcile a difference here. Other dedicated F programs may produce a third value; the test distinguishes the stated graphs, not every imaginable implementation. Extend to `f=2^k-1`, `k=2,...,40`.

## E2 — Challenge the supposedly sufficient F numerator

Request

```text
f_i = from_bits(0x3ff8000000000000+i), i=0,...,256
FDIST(f_i,3,5)
```

These inputs lie near `3/2`. The exact product increments are three input ulps, while one product ulp near `9/2` is four input ulps. In particular, indices 2 and 3 have the same rounded `3*f`: index 2 is a midpoint rounding to the even upper product, and index 3 rounds down to it.

**Prediction:** any subsequent computation depending only on `RN(3*f)` must give equal answers on that pair. Unequal outputs reject that entire interface. Equal outputs do not identify it.

Also reuse the original W1 and W13 inputs and screen these complete states offline:

```text
(v,w,LN(f))
(v,w,LN(N))
(v,w,LN(D))
(v,w,LN(d2)-LN(D),LN(N)-LN(D))
(RN(N/d2),RN(d2/N))
```

Here `N=RN(d1*f)`, `D=RN(d2+N)`, and every displayed subtraction is rounded. A conflicting collision eliminates the corresponding state before any series implementation is needed.

For `d1=2`, multiplication by the degree is exact in the relevant range. Windows with `d2` in `{3,5,11,16,31}` around `f=d2/2` isolate unit-shape prefactors and sharply limit a denominator-FMA-only explanation: fusing an already exact multiplication with addition cannot improve that denominator.

## E3 — A complement probe with analytically different bits

Request

```text
w = BETA.DIST(0.5,53.75,1,TRUE)
c = BETA.DIST(0.5,1,53.75,TRUE)
```

and the worksheet expression `1-w`. These are exact dyadic arguments; `53.75 = 0x1.aep+5`.

The small tail is near `2^-53.75`, safely between `1` and `1.5` times `2^-54`. Consequently:

| Complement graph | Predicted `c` |
|---|---|
| `RN(1-w)` | `0x3fefffffffffffff` |
| `RN(0.5+RN(0.5-w))` | `0x3ff0000000000000` |

For the second graph, the inner result is `0.5-2^-54`; adding `0.5` reaches the midpoint below one and rounds to one. The established leaf errors are far too small to cross the broad interval separating these cases.

Include shapes `{53.25,53.5,53.75,54,54.25}` with the same paired calls. This tests the neighboring rounding pattern and helps distinguish the two-stage complement from an unrelated near-one clamp. The pair sums alone are not the classifier.

## E4 — Tiny-x interfaces and the normalizer/product split

Use shape pairs

```text
(2,2), (2,3), (4,7), (8,8), (8,9), (12,12),
(3/2,5/2), (9/4,15/4), (17/8,31/8)
```

and powers `x=2^-e` for `e=40,...,80`. Keep only cases with normal, nonzero outputs and with the series correction provably unable to affect the candidate result; the latter can be certified offline using a bound, not assumed solely from a tiny decimal input. Add adjacent-double windows at selected exponents and at the exponent-edge witnesses already used in R44–R45.

Request the cumulative, density, and LN leaf. Compare collisions successively at

```text
L = LN(x)
P = RN(a*L)
(P, exact_product_remainder(a,L))
```

**Predictions:**

- A graph using only stored `L` and shape constants is constant within `L` collisions once no other x-dependent stage survives.
- A graph using only stored `P` and shape constants is constant within `P` collisions.
- A product-residual graph can separate the latter while remaining constant in the former.

For each candidate prefactor graph, invert the output rounding intervals to obtain allowed intervals for its shape-only constant. Intersect them across exponents, then test held-out exponents and fractional shapes. An empty intersection rejects that graph; a fitted constant for one pair is not evidence of a recovered normalizer. R44–R45 already reject several simplistic versions, so extend their interface analysis rather than restarting the same regrouping search.

## E5 — Actual branch identification, including the Loader fingerprint

First map the current TOMS boundaries against competing complete programs:

| Variable window | Fixed inputs | Current branch distinction being tested |
|---|---|---|
| `a in W(15,128)` | `x=15/16, b=1/2` | Recurrence-plus-asymptotic versus direct asymptotic after swapping |
| `b in W(40,128)` | `x=1/32, a=3` | Current recurrence/series versus fraction |
| `a=b` in `W(100,128)` | `x=511/1024` | Fraction versus large-both-shapes expansion |
| `x in W(RN((7/10)/8),128)` | `a=5,b=8` | Actual rounded predicate `RN(b*x)<=RN(0.7)` |
| `x` near `a/(a+b)` and `(a+1)/(a+b+2)` | `(a,b)=(2,5),(5,2),(3,7)` | Mean-based versus alternative symmetry threshold |

Add exact unit-shape neighbors `prev(1),1,next(1)` and windows about the stored `.1` and `.9`. To isolate the `0.03` large-shape predicate, use shapes such as `(120,140)` and a window about the solution of `lambda=RN(0.03*min(a,b))`, forming lambda in each candidate's actual order.

For the shared Loader hypothesis, the observed deviance rule is

\[
|k-\mu|<0.2(k+\mu),\qquad 2/3<\mu/k<3/2.
\]

For unshifted beta shapes `(3,5)` the resulting candidate boundaries are

```text
x = 1/16, 1/4, 9/16, RN(7/12).
```

Ask a 257-double window about each for both CDF and density, with the relevant binomial masses. The shifted-density route uses effective parameters `n=6,k=2` and predicts different boundaries, including `2/9`, `1/2`, and `5/9`.

**Prediction:** a candidate dispatch must match the bit signatures of its two specific arms on the two sides, including the side containing the equality case. A visible jump alone is not proof of a branch; the absence of a jump does not disprove it. Use asymmetric cases and tail probabilities far enough from one that rounding does not erase the difference. Do not substitute the original Loader paper's `0.1` threshold for the repository's empirically established `0.2`.

## E6 — The `bgrat` gamma substitutions and t/F/beta bridge

For the asymptotic block, request CDFs at

```text
a in {15,16,32,50,96,118,200,320}
b in {1/16,1/4,1/2,3/4,15/16}
x in {3/4,7/8,15/16,31/32,63/64,127/128}
```

plus the eight lost rows from C and short windows around the five fully specified ones. At each candidate `z`, request gamma density and cumulative controls where useful; compute upper tails by the candidate upper-tail evaluator, not by assuming that subtracting a rounded public gamma CDF reproduces it.

**Prediction:** replacing only `Q`, only `r`, or only the gamma-ratio stage yields a complete predicted bit vector with the expansion held fixed. An apparent tolerance match that disappears when the prefactor is corrected is evidence of cancellation, not a stop rule. Trace the final included term to distinguish a real truncation fingerprint from a prefactor fingerprint.

For the t mapping, request, for `nu=2^k-1`, `k=1,...,8`,

```text
TDIST(1,nu,2)
FDIST(1,1,nu)
BETA.DIST(nu/(nu+1),nu/2,1/2,TRUE)
```

All transformed arguments are exact dyadics. A pure difference in normalization precision predicts equality and is rejected by any mismatch. Extend with `t=2^j`, `nu=2^k-2^(2j)>0`.

For `df=2`, query `W(RN(sqrt(18)),128)`. Compare the identified beta unit-shape graph, the stable rationalizations in C.5, and the public F identity at the rounded square. The precise switch is determined by the actual rounded beta predicate, not by hard-coding the real number `sqrt(18)`.

## E7 — Inverse architecture before inverse tolerance

### Near-one quantization

For `p_k=1-2^-k`, `k=20,...,52`, request

```text
TINV(p_k,2)                 TINV(p_k,25)
FINV(p_k,1,2)               FINV(p_k,1,25)
BETAINV(p_k,1,1/2)          BETAINV(p_k,25/2,1/2)
BETAINV(2^-k,1/2,1)         BETAINV(2^-k,1/2,25/2)
```

For `df=2`, the exact large beta coordinate is `z=1-2^(-2k)`.

- Correctly rounding that scalar to binary64 makes it one for `k>=27`, including the tie at 27.
- Retaining a 64-significand-bit scalar until subtraction moves that cutoff to `k>=33`.
- Retaining the small complementary coordinate permits positive quantiles continuing to decrease with `1-p`.

A private inverse can clamp `z` to its predecessor instead of returning one. The unconditional scalar-coordinate prediction is therefore **zero or a positive quantization floor**, not necessarily zero. For binary64 `z<1`, `1-z` cannot be smaller than `2^-53`. A positive t well below the corresponding `sqrt(nu*2^-53)` scale rejects that scalar-z architecture. The df=25 controls prevent a df=2 closed form from hiding the generic architecture.

### Exact image membership: no new oracle answers required

For each fully staged transform in C.6, test whether the already recorded TINV result belongs to its image over **all binary64 `z` in `(0,1)`**. The transforms are monotone, so binary search on the z lattice suffices offline.

If a returned t is absent, that whole scalar-z architecture is impossible, regardless of its beta solver or tolerance. If it is present, compare the recovered z interval with public BETAINV. This is stronger than checking a few nearby beta quantiles.

### The anomalous neighborhood and public forward

For `p in W(0.85,128)` request TINV at 25, FINV at `(1,25)`, and BETAINV in both coordinate orientations. Compare complete staged compositions, including `SQRT(FINV(...))`, rather than approximate real identities.

Use the known quantile bits to request

```text
t_j=from_bits(0x3fc87592efa6421e+j), j=-512,...,512
TDIST(t_j,25,2)
T.DIST.2T(t_j,25)
```

and evaluate at your recorded alternative `0x3fc87592efa642cb`.

This determines whether Excel's quantile is on a public-forward plateau, a local crossing, or neither. Failure of a public-forward stopping condition does not by itself reject a private internal forward.

### Additional sharp controls

`BETAINV(2^-30,1,1)` distinguishes an identity shortcut returning `0x3e10000000000000` from `EXP(LN(p))` returning the recorded `0x3e10000000000004`; a solver of the noisy public forward can return a third value.

For `FINV(p,2,2)` at `p=1-2^-53`, compare `(1-p)/p` with `1/p-1`. The latter rounded program returns `2^-52`, while the former is close to `2^-53`. This isolates transform cancellation without an elaborate beta kernel.

For `p=2^-k`, `k=48,...,80`, query TINV at `{1,2,5,25}` and FINV at `(1,5),(2,5),(5,2)`. A route through `RN(1-p/2)` loses the t-tail input at `k>=53`; a route through `RN(1-p)` loses the F-tail input at `k>=54`. Direct right-tail inversion retains these probabilities.

Only after these tests should residual-scale fitting attempt absolute probability, relative tail, absolute step, or relative step tolerances. Final outputs generally cannot uniquely identify the initial guess when different starts converge to the same last iterate.

## E8 — Exact discrete CDF identities and the BINOM split prefix

For

```text
f,s in {1,2,3,7,11,31,63}
p   in {1/8,3/8,1/2,5/8,7/8}
```

request all 245 triples:

```text
NEGBINOM.DIST(f,s,p,TRUE)
BETA.DIST(p,s,f+1,TRUE)
BINOM.DIST(f,f+s,1-p,TRUE)
```

Every argument complement is exact. NB=beta with BINOM different isolates a BINOM-specific stage; NB=BINOM with beta different isolates a public-beta stage. A discrepancy rejects the literal shared-public-computation hypothesis, not every shared internal helper.

For the more specific BINOM fingerprint, use 257-double windows centered on `.5` for `n=1000`, and `.3` for `n=100`, requesting

```text
BINOM.DIST(1,n,p,TRUE)
BINOM.DIST(0,n,p,FALSE)
BINOM.DIST(1,n,p,FALSE)
BETA.DIST(RN(1-p),n-1,2,TRUE)
```

Compare against the rounded sum of the two returned masses, the three prefix/correction graphs in C.7, their POWER variants, and current beta with one or two probability operands. These are yes/no bit-vector comparisons. Within rounded-q collision groups, a q-only implementation predicts constancy; the split graph permits the observed upward steps.

## E9 — F.TEST with exact two-pass variance controls

For `k,l=1,...,16`, `k!=l`, use the exact-variance arrays `S_k,S_l` and request

```text
F.TEST(S_k,S_l)             F.TEST(S_l,S_k)
FDIST(1,2k,2l)              FDIST(1,2l,2k)
F.DIST(1,2k,2l,TRUE)        F.DIST(1,2l,2k,TRUE)
VAR.S(S_k)                  VAR.S(S_l)
```

Compare the programs `2*min(RT,CDF)`, `2*min(RT,RN(1-RT))`, the two-stage complement version, and the reversed-RT version. Under the supported two-pass variance family, these controls isolate orientation and tail-source differences. A failure of all public compositions still permits an internal paired-tail evaluator or an internal variance stage different from public VAR.S. Include reversed and interleaved array orderings as immediate controls.

Then apply permutations, exact power-of-two scalings, and translations by `2^20`, `2^40`, and `2^52`, requesting `VAR.S` and F.TEST. Include less symmetric dyadic arrays whose mean is not exactly representable, so the experiment actually separates summation orders. Compare ratio staging, including `(sum_sq1/(n1-1))/(sum_sq2/(n2-1))` versus cross-multiplication. Rounded `VAR.S` outputs are useful controls but need not expose an internally retained variance value.

## E10 — Density and interval normalization

For integer `a,b` in `{1,2,3,4,7,8,11,16,31}` and `x` in `{1/8,3/8,1/2,5/8,7/8}`, request

```text
BETA.DIST(x,a,b,FALSE)
BINOM.DIST(a-1,a+b-2,x,FALSE)
```

Compare the first result with `RN((a+b-1)*mass)`. A mismatch rejects that literal rounded-output composition; it does not reject a shared unrounded prefactor. Add windows around selected x and the E5 deviance boundaries.

For width `w=2^m`, `m in {-10,0,10}`, and dyadic z, request

```text
BETA.DIST(z,a,b,FALSE)
BETA.DIST(w*z,a,b,FALSE,0,w)
```

Away from exponent limits, a post-exponentiation division by width predicts exact power-of-two rescaling. An exponent containing `-LN(w)` can violate this. Then use widths 3 and 5, and uniform shapes, to distinguish division, reciprocal multiplication, and an exponential width factor.

For endpoints, request all combinations `x in {0,1}`, `a,b in {1/2,1,2}` on the standard interval. Repeat on `[-2,6]` using raw `X=-2` and `X=6`, which normalize to zero and one. Measure Excel's finite values and error codes rather than inferring them from the mathematical singularities.

For cumulative and inverse affine staging, choose dyadic intervals and standardized inputs for which normalization is exact; then add intervals with cancellation in `X-A` or `A+z*(B-A)`. Compare public standardized compositions using every intermediate's recorded bits.

## E11 — HYPGEOM beyond the small-N bank

For `N in {64,128,256,1024,65536,2^20,2^30}`, set `K=3*N/8` and request

```text
HYPGEOM.DIST(K-1,N-1,K,N,FALSE)
HYPGEOM.DIST(K-1,N-1,K,N,TRUE)
HYPGEOM.DIST(1,1,K,N,FALSE)
```

Every mathematical answer is exactly `3/8`. This separates rational shortcuts, complementary-sample canonicalization, and general mass-ratio graphs. Add nearby odd populations to remove accidental exactness of `n/N`.

Next hold support short with `n in {2,8,31}` while taking N up to `2^30`; request every mass and cumulative for representative K. Separately use `n=K=N/2` with `N in {64,256,1024,4096,65536}` and indices near `N/4` to probe long-support accumulation.

Compare the exact outputs of left/right sums of public masses, a complemented upper sum, and boundary-mass recurrence variants. A difference between a CDF increment and a public mass is not by itself a proof of another mass kernel: subtraction of two rounded CDFs can lose information.

## Recommended first request

Combine the exact controls in E1–E3, the t bridge in E6, the inverse quantization and known-root neighborhood in E7, the 735 calls in E8's identity batch, a compact E9 matrix, and E10's first density controls. Add the E5 Loader windows if request size permits. These tests attack representation and shared-helper questions before requiring a particular global beta dispatch. They can be sent together; none requires this answer to have run a numerical calculation.

---

# C. Implementable specification

## C.0 Arithmetic contract and research structure

Use the existing validators, integer truncation, error mappings, and already observed endpoint behavior. Nothing in this report establishes new general domain rules. Density endpoints require the measurements identified above.

For the programs below:

- `R(v)` is binary64 nearest-even rounding.
- `LN`, `EXP`, `POWER`, and the existing characterized `log1p`/`expm1` graphs are the repository's Excel-compatible leaves.
- Every indicated elementary operation is rounded. No implicit FMA, reassociation, or precision promotion is permitted.
- `W(v)`, when explicitly requested as an alternative, is a 64-significand-bit extended operation. Specify its exponent range and spill locations too. Do not substitute a generic host `long double` and assume it represents this graph.
- Existing constants, Stirling tables, deviance expansions, and running-exponential machinery are reused by their exact current implementation. Replacing them with more accurate published constants defeats the experiment.

Make the kernel accept a research context with shapes, the two probability operands, their provenance, and optional original-variable/logarithm/prefactor information. This is an experimental interface; it is not a claim that Excel has such a data structure. A symmetry swap must exchange the corresponding logs and residual information along with shapes and probabilities.

Keep independent choices for:

```text
argument construction
prefactor construction
tail orientation
series/fraction/asymptotic evaluator
rounding and spill policy
termination rule
final complement
```

First preserve all stages except the one under investigation. A single configuration that changes all seven cannot localize a successful match.

## C.1 Public beta normalization, unit arms, and symmetry

For public bounded input `X` on `[A,B]`, retain the established staging:

```text
width = R(B-A)
offset = R(X-A)
x = R(offset/width)
y = R(1-x)
```

Apply existing endpoint handling before logarithms. Use the unit programs in A.1 without simplifying the `a=b=1` case.

Keep the current complement as the baseline:

```text
complement2(t) = R(0.5 + R(0.5-t))
```

and the following as a distinct candidate:

```text
complement1(t) = R(1-t)
```

E3 distinguishes them directly. If a paired-tail calculation independently computes both outputs, do not overwrite one with a complement until that architecture has been tested.

For the first prefactor experiments, retain the current dispatch exactly, including its current symmetry calculation:

```text
if min(a,b)<=1:
    swap when x>0.5
else:
    n = R(a+b)
    lambda = R(R(n*y)-b) if a>b else R(a-R(n*x))
    swap when lambda<0
```

This is the control architecture, not an identified Excel rule. The alternative generic-fraction orientation uses the rounded comparison with `R(R(a+1)/R(R(a+b)+2))`; test it with E5 rather than silently adopting it.

## C.2 Loader prefactors: small, explicit variants

Define `M(a,b,x,y)` by applying the **current binomial mass core's exact arithmetic sequence** to real operands with `n=R(a+b)`. Initially restrict tests to shapes whose sums and differences are exact and for which the helper is defined. For noninteger shapes, distinguish retaining the supplied `b` from reconstructing `R(n-a)`.

The relevant stages are the ones already documented in the prompt:

```text
n   = R(a+b)
muA = R(n*x)
muB = R(n*y)
e0  = R(s(n)-s(a))
e1  = R(e0-d(a,muA))
e2  = R(e1-s(b))
e3  = R(e2-d(b,muB))
```

Continue with the same existing running-normalizer/exponential sequence used by `binom_mass`: the four normalizer terms are `-LN(n)/2`, `LN(a)/2`, `LN(b)/2`, and the existing `ln(sqrt(2*pi))` constant. Preserve its stores and its retained residual handling. Do not collapse this to `EXP(e3-normalizer)`.

Test these two first:

```text
C_mass_ratio = R(R(b/n) * M(a,b,x,y))
C_mass_scale = R(R(b * M(a,b,x,y)) / n)
```

A third variant absorbs the ratio into the running exponent. Its exact real normalizer for `C` is

```text
0.5*LN(a) - 0.5*LN(b) + 0.5*LN(n) + ln(sqrt(2*pi))
```

Subtract these four terms in the shown order through the same running-exponential machinery. This is a different program from scaling the rounded mass.

For shapes with valid shifted parameters, add the direct mass identities

```text
C_shift1 = R(y * M(a,b-1,x,y))
```

where this invocation has total shape `a+(b-1)=n-1`, and

```text
density_shift = R(R(n-1) * M(a-1,b-1,x,y))
```

with total `n-2`. Preserve the helper's actual treatment of zero shape/endpoints; do not send invalid generalized masses through a logarithm.

The `bpser` prefactor is `P0=C/y^b`, not `C`. In a controlled normal-range experiment, an explicit candidate is

```text
yb = EXP(R(b*LN(y)))
P0 = R(C/yb)
```

A separate candidate appends `-b*LN(y)` to the running log-prefactor **before** exponentiation. These can differ, and the first can underflow or overflow unnecessarily. Use the second or a separately specified scaled version when extending the hypothesis to extreme ranges; do not hide a range failure behind an unrecorded fallback.

The rounded-mean residual terms in A.1 are another independent variant. For product residuals, exact multiplication with final splitting, a specified extended product, and FMA-based extraction are different graphs. Label which one is used. Their purpose here is to discriminate the observed computation, not to improve the answer unconditionally.

For small fractional shapes lacking an established `s`, one executable candidate is to shift upward to `z+m>=7`, apply the existing large-argument series, and recur downward with

\[
s(z)=s(z+1)+(z+\tfrac12)\log1p(1/z)-1.
\]

Choose the smallest nonnegative integer `m` achieving that range; at each descending step round the division, addition, product, and two subsequent additions/subtractions separately. This is a defined candidate. Its shift threshold and its fractional values are not identified Excel facts; E4/E5 and density probes must decide whether it deserves further work.

## C.3 Tail evaluators and their separate prefactors

### Existing power series as the first control

Preserve the supplied `bpser` sequence exactly, changing only `P0`:

```text
sum=0; c=1
for integer j=1,2,...:
    r = R(b/j)
    u = R(0.5 + R(0.5-r))
    c = R(R(c*u)*x)
    term = R(c/R(a+j))
    sum = R(sum+term)
    stop when abs(term)<=R(eps/a)
return R(P0 * R(1+R(a*sum)))
```

Retain `eps=RN(1e-15)` as the baseline, without asserting it is Excel's stopping constant.

### Positive series as a genuinely different candidate

For the `C*S1` representation:

```text
n=R(a+b); term=1; sum=1
for integer j=1,2,...:
    numerator = R(n+(j-1))
    denominator = R(a+j)
    term = R(R(R(term*numerator)/denominator)*x)
    sum = R(sum+term)
    stop when abs(term)<=R(tau*abs(sum))
return R(C*sum)
```

Start with `tau=RN(1e-15)` as a declared trial, then compare other stop families using full traces. This series has a different prefactor and term structure from `bpser`; it is useful specifically where changing a normalizer cannot explain the observed bits.

### Continued fractions

First reuse the current `bfrac` with only its `brcomp` factor replaced by `H=R(a*C)`, retaining the present fraction coefficients, updates, and stop. This is the cleanest test of a shared prefactor.

For an independent evaluator, the DLMF fraction is

\[
I_x=C\bigg/\left(1+\frac{d_1}{1+\frac{d_2}{1+\cdots}}\right),
\]

\[
d_{2m}=\frac{m(b-m)x}{(a+2m-1)(a+2m)},\quad
d_{2m+1}=-\frac{(a+m)(a+b+m)x}{(a+2m)(a+2m+1)}.
\]

One fully specified diagnostic evaluation is modified Lentz: initialize `f=1,c=1,d=0`; for each coefficient `h`, compute

```text
d = guard(R(1+R(h*d)))
c = guard(R(1+R(h/c)))
d = R(1/d)
delta = R(c*d)
f = R(f*delta)
```

Here `guard(v)` preserves `v` unless `abs(v)<2^-1022`, in which case it returns that minimum normal value with the sign of `v` (positive for exact zero). Form each coefficient by left-to-right rounded products followed by one rounded division, with the displayed parenthesized sums evaluated first. After the initial `d1`, test convergence only after a full even/odd coefficient pair, using `abs(R(f-f_old))<=R(tau*abs(f))`; return `R(C/f)`. Set `tau=RN(1e-15)` for the first trial. A diagnostic iteration cap must report “candidate did not converge,” not masquerade as an Excel numerical result.

This fraction and stop are a declared comparison program. Neither the coefficients' equivalence nor its accuracy would identify it as Excel. The independent tail threshold `(a+1)/(a+b+2)` is likewise a candidate, not a deduction from your bank.

For recurrence shifts, retain the current `bup` evaluation order. The exact identity `I_x(a,b)-I_x(a+1,b)=C` provides a useful check on prefactor interpretation, but a difference of rounded public CDFs is not an exact measurement of `C`.

## C.4 `bgrat`: three surgical substitutions

Preserve the current `bm1`, `nu`, `lnx`, `z`, coefficient arrays, recurrence, and `eps=1e-15` initially. Add independently selectable substitutes:

**Gamma tail:** obtain `Q(b,z)` from the established gamma upper-tail core. Use the same rounded z. Do not substitute `R(1-GAMMA.DIST(z,b,1,TRUE))` unless that is the exact candidate being tested.

**Gamma prefactor:** compare the existing split-exponential `r` with `R(z*gamma_density(z,b,1))`, and with a directly supplied prefactor from that helper before public-result scaling. They are distinct graphs. The identity `r=b*PoissonMass_generalized(b,z)` gives another implementation route if the real-shape helper is available; integer POISSON alone does not establish it.

**Gamma ratio:** a concrete binary64 candidate for `g=log(u/r)` is

```text
n  = R(a+b)
l1 = log1p(R(b/a))
v1 = R(R(n-0.5)*l1)
l2 = LN(R(a/nu))
v2 = R(b*l2)
g0 = R(v1+v2)
g1 = R(g0-b)
g2 = R(g1+s(n))
g  = R(g2-s(a))
u  = R(r*EXP(g))
```

Compare this with `l2=-log1p(R(R(b-1)/R(2*a)))` and with the existing running-exponential graph. Do not change `r`, `Q`, and `g` simultaneously on the first pass. If these substitutions do not reproduce the 943-row fingerprint, then compare the complete positive-series/fraction alternatives while holding their prefactors fixed.

The existing large-both-shapes `basym` arm should remain a separate control. The bank has almost no evidence for identifying its thresholds or constants; E5 supplies that missing region.

## C.5 F, F.TEST, and t wrappers

### F candidate state and prefactor programs

For the ordinary-double state:

```text
a=R(d2/2); b=R(d1/2)
N=R(d1*f); D=R(d2+N)
v=R(d2/D); w=R(N/D)
```

Do not discard `f,N,D`. The simple separated-log candidate is

```text
ld = LN(D)
lv = R(LN(d2)-ld)
lw = R(LN(N)-ld)
```

The original-f candidate instead uses

```text
lw = R(R(LN(d1)+LN(f))-ld)
```

Supply `lv,lw` to the prefactor while keeping `v,w` in recurrence coefficients. In an explicit log-beta candidate, form

```text
e1=R(a*lv)
e2=R(b*lw)
e3=R(e1+e2)
e4=R(e3-private_log_beta(a,b))
H=EXP(e4)
```

with the current private normalizer as a control and a shared-Stirling normalizer as a separate variant. This fully defines the new wrapper arithmetic; no public GAMMALN assumption is required.

For the F-native Loader candidate, use

```text
n=R(a+b)
r=R(b/n)
u=R(f-1)
vlog=log1p(R(r*u))
D_beta=R(R(n*vlog)-R(b*LN(f)))
```

in place of the sum of two beta deviances, retaining the chosen Stirling and normalizer sequence. This is a different program; its cancellation near one and its collision behavior must be checked.

Return the requested tail from a common paired-tail evaluator where that hypothesis survives. Keep FDIST and F.DIST.RT aliases. Test F.DIST's relation to the paired complement explicitly.

For a wider candidate, specify exactly which of `N,D,v,w` are formed and retained with 64 significand bits, and every subsequent consuming operation. A.6 already contradicts the tested construction using a 64-significand-bit numerator/denominator followed by a binary64 quotient pair, with 260 conflicts. That result does not cover every conceivable mixed-width construction. A wide quotient whose first consumer narrows it to call LN also loses its extra information on that path.

### F.TEST

The variance baseline remains

```text
sum = left-to-right binary64 sum of samples
mean = R(sum/n)
ss = left-to-right sum of R(R(sample-mean)*R(sample-mean))
variance = R(ss/R(n-1))
```

Use the larger variance as numerator with corresponding degrees. Keep the current exact-tie convention as a named control; E9 identifies whether it matches Excel. Set `f=R(var_large/var_small)`, obtain the candidate paired F tails, and return `R(2*min(left,right))`.

Do not change the variance method merely to improve the exact-variance witness. First use E9 to separate evidence for variance staging from evidence for tail construction.

### TDIST

Retain

```text
t2=R(t*t)
D=R(nu+t2)
v=R(nu/D); w=R(t2/D)
```

as one argument candidate, with original t and `t2,D` available to the prefix. Return `Q` for two tails and `R(Q/2)` for one, preserving existing endpoint behavior.

For df=2, compare the beta unit arms with these precise rationalized programs:

```text
s=SQRT(R(R(t*t)+2))
Q_A=R(2/R(s*R(s+t)))
Q_B=R(R(2/R(s+t))/s)
Q_C=R(2/R(R(R(t*t)+2)+R(t*s)))
```

For df=1 and `t>0`, screen `R(R(2*ATAN(R(1/t)))/pi)` and a separately staged `R(R(2/pi)*ATAN(R(1/t)))` using measured trig leaves. Their constants and branches are not identified by the supplied evidence. At zero, return the established endpoint.

## C.6 Inverses: transforms, then a specified solver harness

### Beta inverse and interval scaling

Given a standardized quantile z, retain as the baseline

```text
width=R(B-A)
scaled=R(z*width)
result=R(A+scaled)
```

Compare wider affine evaluation separately in cancellation-sensitive bounded tests. Do not assume inversion occurs in the bounded x coordinate.

### FINV and TINV transform candidates

For right-tail FINV, solve beta with shapes `(d2/2,d1/2)` and target p. Compare a single rounded z with an independently returned pair `(z,w)`:

```text
w=R(1-z)                         # scalar-coordinate variant only
F_A=R(R(d2*w)/R(d1*z))
F_B=R(R(d2/d1)*R(w/z))
F_C=R(R(d2/d1)*R(R(1/z)-1))
```

For TINV, solve beta with `(nu/2,1/2)`:

```text
w=R(1-z)                         # omit this overwrite for a returned pair
t_A=SQRT(R(R(nu*w)/z))
t_B=SQRT(R(nu*R(w/z)))
t_C=SQRT(R(nu*R(R(1/z)-1)))
```

Apply E7's image test before attempting to fit an inverse-beta solver behind any one of these transforms.

### Unit-shape and low-degree inverse candidates

For beta `b=1`, compare identity handling at `a=1` with

```text
z=EXP(R(LN(p)/a))
```

For beta `a=1`, compare `R(1-EXP(R(LN(R(1-p))/b)))` with the corresponding LOG1P/EXPM1 graph. The forward unit-shape evidence does not identify either inverse.

For F numerator degree two, the mathematical expression is

\[
F=\frac{d_2}{2}\operatorname{expm1}\left(-\frac{2}{d_2}\log p\right).
\]

A concrete candidate is `R(R(d2/2)*EXPM1(R(R(-2/d2)*LN(p))))`; compare the EXP-then-subtract version and the beta-coordinate transforms. Do not infer the retained product precision from the beta forward arm.

For TINV at df=1, compare

```text
R(1/TAN(R(R(pi*p)/2)))
TAN(R(R(pi*R(1-p))/2))
```

and any empirically observed switch between them. The current DNA Calc special case does not by itself identify Excel's program.

For df=2, the exact expression is

\[
t=\frac{\sqrt2(1-p)}{\sqrt{p(2-p)}}.
\]

A specified stable candidate is

```text
q=R(1-p)
den=SQRT(R(p*R(2-p)))
num=R(SQRT(2)*q)
t=R(num/den)
```

Compare it with `SQRT(R(R(R(2*q)*q)/R(p*R(2-p))))` and the scalar-beta variants. Better accuracy alone is not a selection criterion.

### Iteration candidates, with all missing choices exposed

After selecting a forward graph, derivative graph, and returned coordinate, implement safeguarded Newton and Halley as separate configurations. For increasing beta CDF:

```text
r=R(F(x)-p)
h=R(r/pdf(x))
```

Newton proposes `R(x-h)`. For Halley, form

```text
gA=R(R(a-1)/x)
gB=R(R(b-1)/R(1-x))
g=R(gA-gB)
den=R(1-R(0.5*R(h*g)))
hH=R(h/den)
proposal=R(x-hH)
```

Maintain the bracket from residual signs. If a proposal is nonfinite or outside the bracket, use the explicitly rounded midpoint `R(lo+R(R(hi-lo)/2))`. Record whether the stop tests the raw derivative step or the safeguarded actual step. Record whether it publishes current x or the proposal. Those choices alter the final bits.

Start candidates are:

1. `R(a/R(a+b))`.
2. A normal approximation: `n=R(a+b)`, `mu=R(a/n)`, `var=R(R(a*b)/R(R(n*n)*R(n+1)))`, `zN=NORM_S_INV(p)`, and `x0=R(mu+R(SQRT(var)*zN))`.
3. The small-tail expression `EXP(R(R(R(LN(p)+LN(a))+log_beta(a,b))/a))`.

Use a separate optional reflection policy `p>0.5`, swapping shapes and solving at `R(1-p)`; do not silently mix policies across configurations. For a concrete interior-clamp candidate, replace a finite `x0<=0` with the least positive representable value admitted by the kernel and a finite `x0>=1` with `nextdown(1)`; flag a nonfinite start as failure of that start program. Declare the admitted lower endpoint explicitly in the harness. These are research choices, not recovered Excel rules.

For direct t inversion, use the increasing function `-Q_nu(t)`, derivative `2*f_t(t)`, and logarithmic derivative `-(nu+1)*t/(nu+t*t)`. For direct right-tail F inversion, use `-Q_F(f)` and its positive F density; with `a=d2/2,b=d1/2`, its logarithmic derivative is `(b-1)/f-(a+b)*d1/(d2+d1*f)`. Round each product, sum, and quotient separately in the displayed parenthesization for the ordinary-double candidate. Preserve the current upper-bound search as a control and keep its iterations separate from polishing.

**Identify tolerances by interval rejection, not a small list of guesses.** For a complete candidate graph, generate its iteration trace offline without its early tolerance stop. For a chosen measure `m_j`—absolute residual, relative tail residual, absolute step, or relative step—first stopping at iteration k under `m_j<=tau` requires

\[
\boxed{\tau\in[m_k,\min_{j<k}m_j).}
\]

Union these intervals over all publication points that match Excel's bits, then intersect across inputs and with the permitted binary64 constants. The minimum over an empty prefix is positive infinity. An empty intersection rejects the entire graph and stop family for **every** tau. A surviving interval constrains the constant. This interval formula applies literally to `m_j<=tau`: for a program testing `abs(r_j)<=R(tau*scale_j)`, invert that rounded-product condition directly. Replacing it by a rounded division comparison can change threshold ties. Handle unconditional zero-residual, unchanged-iterate, adjacency, and iteration-cap stops separately. The historical 100-iteration statement in the prompt is a possible control cap, not an identified fact about these current builds.

## C.7 Discrete wrappers and hypergeometric recurrence

### NEGBINOM.DIST

Keep the 129/129 mass route and the established `f=0` behavior. For interior cumulative values, the first candidate uses beta context `(a=s,b=f+1,x=p,y=R(1-p))`. Compare it with caller-specific prefixes through E8. Do not route it through a sum of masses merely because the masses match.

### BINOM.DIST

Preserve the working mass core, endpoints, and the distinct `k=n-1,q<0.1` stored-product EXPM1 arm described in B.3 of the prompt. For other cumulatives retain original p together with `q=R(1-p)`.

For the `k=1` diagnostic, implement exactly these three candidates:

```text
# A: rounded q in power, original p in coefficient
q=R(1-p)
h=EXP(R(R(n-1)*LN(q)))
c=R(q+R(n*p))
ansA=R(h*c)

# B: use 1+(n-1)*p as the coefficient
ansB=R(h*R(1+R(R(n-1)*p)))

# C: factor out q^n
hC=EXP(R(n*LN(q)))
cC=R(1+R(R(n*p)/q))
ansC=R(hC*cC)
```

Compare each against replacing the EXP/LN prefix by POWER, and against summing the two public masses. These formulas are candidate fingerprints, not a recommendation to add a special case before measurement. A matching general beta recurrence may produce the same pattern.

### HYPGEOM.DIST mass

For integerized inputs in support, define `r=R(n/N)` and `q=R(1-r)` and compute

```text
A = mass(j,   K,   r,q)
B = mass(n-j, N-K, r,q)
C = mass(n,   N,   r,q)
h = R(R(A/C)*B)
```

This is the centered-denominator candidate, with the established multiplication/division order retained. Compare it with the equivalent orientation interchanging n and K, which uses `R(K/N)` as auxiliary probability. Reuse existing support and degenerate-case handling, including N=0; do not form `n/N` before it.

### HYPGEOM.DIST cumulative

The current sum of masses is the baseline. A concrete recurrence alternative initializes a boundary mass and uses

```text
num=R(R(K-j)*R(n-j))
den=R(R(j+1)*R(R(R(N-K)-n)+R(j+1)))
ratio=R(num/den)
next_mass=R(mass_j*ratio)
sum=R(sum+next_mass)
```

The exact recurrence is

\[
h(j+1)/h(j)=\frac{(K-j)(n-j)}{(j+1)(N-K-n+j+1)}.
\]

Use ascending accumulation from L for the lower-tail candidate. For descending accumulation, the exact ratio is `h(j-1)/h(j)=j*(N-K-n+j)/((K-j+1)*(n-j+1))`; evaluate the parenthesized differences and sums left to right, each product separately, then the quotient, then multiplication by the previous mass, all with R. Sum masses from U down through `k+1` and apply the selected complement graph. A relative-term sum with the boundary mass multiplied only once is another distinct graph. The choice of seed, orientation, and crossover is unmeasured; E11 identifies them. Do not invent a population threshold for switching to a beta approximation.

## C.8 Density programs and endpoint completion

In the interior, compare three exact candidate graphs:

```text
pdf_shift = R(R(R(a+b)-1) * M(a-1,b-1,x,y))
pdf_H     = R(H/R(x*y))
la=R(R(a-1)*LN(x)); lb=R(R(b-1)*LN(y))
pdf_log   = EXP(R(R(la+lb)-private_log_beta(a,b)))
```

The shifted-mass program initially applies where its effective shapes are supported. For width `w=R(B-A)`, compare `R(pdf/w)` against the corresponding log program with `-LN(w)` inside its exponent. E10 separates them.

Mathematical endpoint expectations provide the experiment design: at zero, the density is zero for `a>1`, `b` for `a=1`, and divergent for `a<1`; at one, exchange a and b. Divide finite results by interval width. Excel's exact finite staging and error code for divergent cases are still unknown. Do not leave endpoint behavior to `0*LN(0)`.

## C.9 Constants and boundaries: what is known versus pending

| Item | Current value or candidate | Status / discriminator |
|---|---|---|
| Unit comparisons | `a==1`, `b==1`, stored `.1`, `.9` | Strong empirical evidence; preserve order and neighboring-shape tests |
| Final two-stage complement | `.5+(.5-t)` | Strong unit-arm evidence; E3 identifies the graph directly |
| Deviance threshold | `abs(k-mu)<.2*(k+mu)` | Preserve the empirically identified mass helper, including strict inequality |
| Small Stirling entries and large series | Existing cut-to-16-decimal table; existing ten-term series | Reuse exact existing constants; fractional extension remains a hypothesis |
| `bgrat` tolerance | Current `1e-15` | Successful landed baseline; not proof of Excel's tolerance |
| `bfrac` tolerance | Current `15*1e-15` | Control only; needs a separate test |
| `15`, shift `20`, `40`, `100`, `.7`, `.03` | Current TOMS dispatch constants | Not identified as Excel's; E5 maps them against alternatives |
| Series/fraction stops | Explicit candidates in C.3 | Trace and interval tests; no fitted value asserted |
| Wider intermediate locations | Only already identified unit behavior is retained by default | E1/E2/E4/E7 identify where information must survive |
| Inverse start, stop, cap | Configurable candidates | E7 plus trace-interval intersection |
| HYPGEOM crossover | Unknown | E11; no assumed large-N beta conversion |
| Density endpoint codes | Unknown | E10's 18 endpoint combinations |

## C.10 What I would implement first

1. Add the non-destructive representation/collision and inverse-image diagnostics, then run the sharp E1–E3/E7 controls.
2. Add the three isolated gamma substitutions inside `bgrat` and the generalized-mass prefactor variants, preserving current dispatch.
3. Run density/mass and exact discrete-identity probes to determine which public routines share a final rounded stage.
4. Only then test replacement tail evaluators or recover a different dispatch; keep the unit-shape programs intact.
5. Identify the inverse coordinate and transform before fitting iteration stops.

Report exact counts on **nontrivial numeric rows**, separately by function, directly evaluated tail, shape class, and input transformation. Retain the largest mismatches as witnesses. Error-versus-truth summaries help diagnose a graph but are not the acceptance criterion. No candidate in this report has been validated against your recorded bank yet.

---

# D. Bold ideas worth pursuing

## D.1 The common missing ingredient may be original operands, not a wider floating-point type

The F collision evidence, BINOM's rounded-q collision, and R44–R45's product-prefix difficulties all say that certain proposed compressed states discard information the observed result still depends on. That information could be an original scalar, a second probability operand, a product remainder, or a separately formed logarithm.

The BINOM upward-step witness is especially important: retaining information can preserve a less mathematically coherent floating-point graph. Excel may use rounded q in one stage and original p in another. Searching only for a more accurate beta argument could therefore miss the correct implementation completely.

## D.2 The 943-row block may be a gamma-helper mismatch inside a correct beta expansion

The outer `bgrat` expansion can be substantially right while its local `Q`, gamma-density prefactor, and gamma-ratio implementation differ from the ones Excel shares with GAMMA.DIST. This explains why changing the tolerance removes a broad bias without identifying the remaining last bits. It also explains why more work on a beta log-normalizer alone might fail.

The `r`, `Q`, and `u/r` substitutions are small enough to make this hypothesis decisively testable. A wholesale new beta algorithm would obscure that opportunity.

## D.3 TINV may expose a coordinate quantizer rather than a loose solver

A single binary64 beta coordinate near one imposes a sparse output grid after conversion to t. Image-membership and positive-floor tests can reject that whole architecture without knowing any inverse iteration constant. If it survives, reproducing the apparently poor t result may require reproducing the quantization, not degrading a well-converged t-space solve with an arbitrary probability tolerance.

## D.4 There may be a common mathematical core with several small public front ends

BETA versus BINOM's unit product precision is already an example. F.TEST's exact-variance witness suggests another orientation or tail-output distinction. The absence of bitwise equality between public mathematical identities does not require wholly independent implementations, and treating every name as a separate approximation problem loses valuable cross-function evidence.

Model shared primitives and small caller differences explicitly. Conversely, do not force every public function through one narrow beta API merely because their real-valued identities are equivalent.

## D.5 Search for invariants that reject whole programs

The strongest tests here do not rank percentages:

- A collision rejects a proposed information interface.
- An exact dyadic transform removes argument precision as an explanation.
- A transform-image failure rejects every solver hidden behind that scalar coordinate.
- An empty intersection of stop-tolerance intervals rejects every constant for that iteration graph.
- A carefully chosen complement midpoint gives two distinct, analytically predicted result bits.

These tests make the next implementation round substantially more constrained. The exact-match bank then confirms the surviving program shapes; it need not serve as an undirected optimization target.

---

## Source and evidence notes

The supplied prompt is the sole empirical source for Excel outputs and match counts. Its own branch labels and historical source comments are hypotheses where indicated, not additional observations. The row listings take precedence over summaries: for example, A.11's listed cumulative witnesses include roughly `1e-14` and `1e-299`, so the earlier prose summary placing all seven in `1e-150..1e-130` should not guide experiment selection.

Permitted references consulted:

1. [NIST DLMF §8.17, Incomplete Beta Functions](https://dlmf.nist.gov/8.17): definitions, symmetry, series, recurrences, and fraction coefficients.
2. [NIST DLMF §8.18, Asymptotic Expansions](https://dlmf.nist.gov/8.18): the role of incomplete-gamma functions in large-shape beta expansions.
3. [Catherine Loader, Fast and Accurate Computation of Binomial Probabilities, 2002](https://people.math.ethz.ch/~maechler/doc/CLoader-dbinom-2002.pdf): saddle-point mass decomposition. Its published deviance threshold is not substituted for the empirical repository threshold.
4. [Boost.Math incomplete-beta implementation notes, version 1.60](https://www.boost.org/doc/libs/1_60_0/libs/math/doc/html/math_toolkit/sf_beta/ibeta_function.html): an allowed example of a hybrid DiDonato–Morris-related implementation, not evidence that Excel uses it.
5. [Boost.Math Student t distribution implementation notes](https://www.boost.org/doc/libs/latest/libs/math/doc/html/math_toolkit/dist_ref/dists/students_t_dist.html) and [inverse-beta documentation](https://www.boost.org/doc/libs/latest/libs/math/doc/html/math_toolkit/sf_beta/ibeta_inv_function.html): separately returned inverse coordinates and the conditioning issue discussed above.
6. [NIST DLMF §3.10, Continued Fractions](https://dlmf.nist.gov/3.10): numerical evaluation framework.

No Microsoft implementation source, runtime disassembly, decompilation, or prohibited source-code structure was used. The new residual corrections, transform-image tests, exact-rounding witnesses, and tolerance-interval method are derivations in this report. They remain unexecuted proposals until evaluated by your permitted implementation/oracle workflow.


