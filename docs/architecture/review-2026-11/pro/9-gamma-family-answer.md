# 9 — The last bits of Excel’s gamma-function family

**Analytical review of the evidence supplied for DnaCalc commit b2a808502f74e1590dcfc682171c8a1db1d93ee2.**

No numerical candidate was executed, no Excel calls were made, and no repository code was run for this review. Hexadecimal answers identified as observations come from the prompt. New exact-bit deductions are marked as such; other numerical estimates are analytical predictions to test. I checked public mathematical and product documentation only, principally to resolve the claimed iteration limit. No Microsoft implementation source or restricted implementation was consulted.

References such as **[P:G.2]** identify the corresponding appendix of the supplied prompt. External references are listed at the end. Confidence labels are subjective assessments of the evidence, not statistical posterior probabilities.

## A. The most plausible computation shape

### A.1 Overall verdict

**Preserve the identified outer computations. Concentrate identification on the root-preserving GAMMALN cores, the generic GAMMA seed, the internal normalization boundary of the incomplete-gamma functions, and the precision of the residual used by the inverses.**

The data justify a fairly precise architecture. They do not identify complete B1/B2 coefficient tables, a particular Lanczos parameter pair for the Gamma seed, or a particular inverse stopping rule. Supplying invented values for those missing pieces would make an apparently complete specification less useful than an explicitly incomplete one.

| Component | Most plausible shape | Confidence |
|---|---|---|
| GAMMALN and GAMMALN.PRECISE | Shared positive-argument numeric body on the tested domain | Very high |
| GAMMALN below 0.7 | Stored shifted-core result minus worksheet LN | Very high |
| GAMMALN on [0.7,1.5) | Root-preserving rational, most likely (x−1)R₁(x), with stored stages | High for the factor; medium-high for raw-x rational evaluation |
| GAMMALN on [1.5,2) | Root-preserving rational around 2; coefficients and degree unresolved | High for the factor; medium for a separate coefficient table |
| GAMMALN on [2,3] | The reported 8/8 rational and stored-Horner graph, with incompletely recovered coefficients | High for the graph, conditional on the reported stage evidence |
| GAMMALN on (3,8) | Ascending running product, one LN, then one addition to the lower-band value | Very high |
| GAMMALN at and above 8 | The supplied ten-term stored Stirling computation | Very high |
| GAMMA below 10 | Generic seed on [1,2), followed by the measured recurrences, with qualifications below 1 | High |
| Generic GAMMA seed | Multiplicative normalization/product; a Stirling-normalized correction or sufficiently accurate Lanczos product leads | Medium for the family; low for any exact formula |
| GAMMA at and above 10 | The measured split-power Stirling product | Very high |
| GAMMA at and below −10 | Folded-sine reflection using positive Gamma | High for the identity; medium for the last store sites |
| Incomplete gamma | A hybrid of small-argument series, continued fractions, finite polynomials, and a large-shape transition method | High for the hybrid; variable for individual dispatch details |
| Poisson mass | The measured compensated log route and Loader route | Very high on the sampled regions |
| Inverses | A safeguarded iterative method using a sufficiently resolved tail/residual; Newton is a leading candidate | Medium for this family; low for its exact iteration and stop |

These are assessments of the supplied observations, not replacements for them. [P:B–H]

### A.2 GAMMALN: root-preserving rationals are the right starting point

The decisive evidence is the behavior at 1 and 2. The absolute error shrinks with the distance from the root. This strongly favors computations of the form

\[
L_1(x)=(x-1)R_1(x),\qquad L_2(x)=(x-2)R_2(x).
\]

The subtractions themselves are exact in binary arithmetic over the proposed bands: x−1 on [0.7,1.5), and x−2 on [1.5,3], satisfy the usual exact-subtraction condition. Extra precision is not needed to explain their exactness. [P:D.2; P:H/R49]

**LN of a stored binary64 Gamma is excluded near the roots.** At x=1+2⁻⁵², a stored Gamma near 1 lies on a lattice whose spacing below 1 is 2⁻⁵³. Taking LN of that stored value cannot recover the supplied tiny log-Gamma value to a handful of its own ulps. The resulting error would be of order 10⁻¹⁷ in absolute terms, roughly 10¹⁴–10¹⁵ ulps of this particular tiny result. That is qualitatively different from the observed few-ulp error. An internally accurate log1p formulation of a factored correction is not excluded by this argument.

An ordinary cancellation-prone Lanczos log expression is also strongly disfavored. A specially root-preserving reformulation derived from Lanczos remains possible. It is the evaluated graph, rather than the ancestry of the approximation, that the root evidence constrains.

For B3, retain the reported raw-x 8/8 graph as the leading hypothesis. Its central-window phase agreement is much stronger evidence than its overall match percentage. However, the current coefficients are not an exact Excel table: the prompt explicitly says that some low bits remain unread and that the constant terms were obtained partly from identities and match minima. [P:B.1; P:I]

There are three important qualifications.

1. **The evaluated bias is not the approximation error alone.** Appendix D measures coefficient error, approximation error, input rounding, correlated intermediate roundings, and final rounding together. It does not prove that Excel’s real-arithmetic rational is intrinsically less accurate than the published SPECFUN approximation. A very accurate approximation can become biased when rewritten into an ill-conditioned basis and evaluated with stored stages. The failed stage fingerprints of the tested tables are stronger evidence against them than their different mean errors.

2. **A separate evaluation regime at 2 need not mean a separate coefficient table.** An incompletely recovered B3 table failing below 2 does not conclusively reject one common table. Different graphs using one table remain possible. Test this with equal argument spacing across 2, rather than allowing the change in binary64 input spacing to masquerade as a branch.

3. **An 8/8 rational does not establish a particular Hart or Cody table.** For example, a centered form t(D₂+tP₇(t)/Q₈(t)) can be cleared into t(D₂Q₈(t)+tP₇(t))/Q₈(t), another 8/8 ratio, and then expressed in x. These algebraically equivalent forms have different floating-point behavior. Degree alone cannot establish historical provenance.

I would not name a Hart table number or assert degrees for B1/B2 from the supplied summaries. Start by recovering their graph signatures. For B1, 7/7 and 8/8 raw-x rationals are sensible candidate classes; for B2, an 8/8 raw-x ratio and a cleared centered rational are sensible candidate classes. These are search priorities, not identifications.

The outer dispatch is much firmer: literal binary64 0.7, product recurrence on (3,8), and the ten-term expansion at 8. Preserve these. The explicit x=3 constant is a valid compatibility pin in the present implementation, but does not prove that Excel has an explicit equality branch at 3. An inclusive direct approximation could naturally produce the observed endpoint. [P:G.1–G.3]

**What would make this interpretation wrong?** Root-neighbor normalized ratios inconsistent with any fixed coefficient/stage model; a coherent log1p graph predicting those same stages better; or equal-spacing traces across a supposed seam showing one common graph. A bin-average sign change alone would not settle these questions.

### A.3 GAMMA: the generic seed is partly hidden by public special cases

The large-positive graph is already unusually well constrained: the split power, stored squares, stored logarithm, reciprocal square root, normalization, and correction explain both individual stage lines and the growth in error variance. Its nine misses are not sufficient reason to replace its method family. The seed below 10 is a separate problem. [P:B.2; P:G.5–G.6]

The relative bias supports a common multiplicative normalization. Binary64 sqrt(2π) is a plausible source because its representation error has the observed magnitude and the same constant occurs in the identified large-positive graph. That is meaningful evidence, but not a proof: a normalized rational table or biased coefficient combination can have the same effect.

Likewise, the standard deviation does not uniquely count stored operations. The proposed “six to nine independent roundings” assumes independence; the visible stage lines demonstrate structured dependencies. Treat the variance as a fingerprint to reproduce, not an operation count.

My ranking of seed families is:

1. A product with a separately approximated Stirling normalization/correction on [1,2).
2. A sufficiently accurate Lanczos product with a stored multiplicative normalization.
3. A direct rational for Gamma, possibly normalized by sqrt(2π).
4. EXP of the public GAMMALN core.
5. The unmodified ten-term correction shifted down from [6,7), unless its exact arithmetic and stage errors pass the test below.

The first two are close. No particular g or N is identified. Candidate pairs such as g=7 with nine coefficients and g=607/128 with fifteen coefficients are reasonable hypotheses to distinguish, not conclusions. An additional published comparison is Boost’s N=13, g=6.024680040776729583740234375. The coefficient-count convention must be made explicit; publications differ in whether the leading constant is counted. [R1]

#### A concrete hidden-seed deduction

The supplied positive half-integer path gives

    public GAMMA(1.5) = 0x3fec5bf891b4ef6b

but the supplied negative value is

    GAMMA(-0.5) = 0xc00c5bf891b4ef6a.

Under the proposed generic recurrence, if S is the seed at y=0.5,

\[
\Gamma(-0.5)=\operatorname{st}\bigl(S/0.5/(-0.5)\bigr)=\operatorname{st}(-4S).
\]

Both divisions are exact power-of-two scalings. Thus the binary64 rounding of the generic seed must be

\[
\boxed{\operatorname{RN53}(S)=\texttt{0x3fec5bf891b4ef6a}}.
\]

It differs by one ulp from the public positive-half result.

For a **stored** seed with these bits, the same recurrence predicts

\[
\boxed{\Gamma(-1.5)=\texttt{0x4002e7fb0bcdf4f1}},
\]

exactly the supplied Excel value, whereas the public positive-half seed produces the current answer ending in f2.

This is an exact-bit deduction, conditional on the recurrence and stored seed. One integer check establishes the last rounding: with M=0x1c5bf891b4ef6a and N=0x12e7fb0bcdf4f1, 2M=3N+1. The division by 1.5 therefore produces N+1/3 in output-significand units and rounds to N. A resident seed has additional low bits, so the second observation constrains those bits rather than following solely from its binary64 rounding.

**Public GAMMA(1+y) is therefore not automatically the generic internal seed.** Exact positive half-integer overrides can conceal it. A separate negative-half branch remains an alternative; neighboring non-half inputs distinguish that alternative.

#### Resolve the below-one contradiction before fitting a seed

Appendix G.6 reports an exact seed recurrence, while H/R33 reports incompatible common-seed groups even with a resident seed. Both cannot be universal statements with the same argument and branch conventions.

Two concrete confounders are available:

- Internal y and the public call’s recovered fraction st(1+y)−1 may differ.
- Public integer and half-integer overrides may replace the generic seed.

An exact-translation lattice, specified in B.4, removes both. If the common-seed intervals still have empty intersections, the below-one graph itself is wrong. Do not absorb that failure into a fitted seed.

#### The recurrence-from-6 correction has a substantive truncation problem

For the ten Bernoulli terms in the prompt, the next two log-Stirling terms are

\[
T_{11}(u)=\frac{854513}{63756u^{21}},\qquad
T_{12}(u)=-\frac{236364091}{1506960u^{23}}.
\]

Let S₁₀ here mean the exact-real ten-term sum with exact rational coefficients, before any implementation roundings. For positive real u, its remainder satisfies

\[
T_{11}(u)+T_{12}(u)<S_{\rm true}(u)-S_{10}(u)<T_{11}(u).
\]

Near u=6, this is approximately 4.1×10⁻¹⁶ to 6.1×10⁻¹⁶; near 7, the first omitted term is approximately 2.4×10⁻¹⁷. These are analytical estimates from the standard remainder bound. [R2]

Shifting t in (1,2) to t+5 and recurring down preserves this log-error in exact arithmetic. It predicts a negative relative tilt of several units of 2⁻⁵³ near t=1, rapidly diminishing toward 2. That is a poor match to the reported broadly flat positive bias. Stored recurrence errors might compensate it, but this requires evidence; it cannot be presumed. Integer table exceptions do not remove the prediction on adjacent noninteger inputs.

This specifically lowers the rank of the **unmodified shared helper**. It does not reject a Stirling-normalized seed with an independently accurate rational correction.

For negative arguments at or below −10, the folded-sine reflection identity is strongly plausible. The current precise store graph is not fully established by 71–75% agreement. Isolate the positive Gamma leaf and the common folded sine before changing that graph. [P:G.7]

The reciprocal plateau below about 10⁻¹⁶ is established over the sampled inputs. An explicit literal branch at exactly 10⁻¹⁶ is less well established: a rounded seed plateau can imitate it. Preserve the working behavior and test the boundary rather than treating the observed last agreeing point as a proven dispatch instruction.


### A.4 Incomplete gamma: retain the hybrid, identify the normalization boundaries

The evidence supports a shared incomplete-gamma family, but it does not establish that Excel copied one complete named implementation. The public-domain port supplies a useful structure. The measured series order, finite-polynomial evidence, prefactor routing, and continued-fraction behavior point to modifications or a different combination of familiar methods. [P:A; P:B.5; P:F.4; P:H]

For the lower series, retain the identified form

\[
P(a,y)=\operatorname{st}(M(a,y)A(a,y)),
\quad
M(a,y)=y^ae^{-y}/\Gamma(a+1),
\]

with stored forward terms starting at one and the measured 2⁻⁵³ stopping predicate. Here M denotes the identified floating-point mass computation, not an instruction to evaluate the mathematical expression anew.

For the upper tail, distinguish four questions:

- Which normalization is used?
- Is its result stored before entering the tail computation?
- Which finite sum, continued fraction, or asymptotic expression is used?
- How are P and Q separately published?

Public equality of POISSON.DIST cumulative and CHIDIST establishes an excellent paired oracle. It does not imply that their internal prefactor is the stored public mass, or that their companion P is obtained by ordinary subtraction from Q. [P:F.4]

#### The small-shape normalization is unresolved

For a<1, the leading alternatives are a dedicated approximation to a reciprocal-Gamma correction, and the same compensated GAMMALN-based mass used by the other lower-series branches. A special a<1 formula can also use the shared normalization but a different stable expression for the tail. These possibilities are distinct.

Do not “repair” all 15-digit constants together. The supplied experiment with sqrt(pi) identifies one specific defect; it is not evidence that every old literal should be replaced by its nearest mathematical value. The unchanged and regressed rows after other constant replacements reinforce this point. [P:F.2; P:H]

B.1 below supplies a direct public-function test of whether the small-shape normalization is shared.

#### The integer finite-polynomial result deserves its narrow, exact scope

The broad rejection of finite sums in F.4 must be read alongside R35/R37: a stored Horner polynomial with correctly rounded reciprocal-factorial coefficients matched a large fresh sample for integer shapes 2 through 20 with shape<y≤30, and failed outside the dispatch. This is strong positive evidence for that **specific graph on that specific region**. It is not a reason to rerun unrestricted finite-sum variants, nor to extend the result to shape 21 or arbitrarily large y. [P:F.4; P:H/R35–R37]

Loss of four inverse pins when applying a forward-function change does not by itself refute the forward identification. An inverse that currently selects the wrong point on an imperfect forward function can lose accidental matches when that forward function is corrected. Evaluate the forward identification and inverse search separately.

For half-integer finite sums, preserve the landed correction literal; the remaining possibilities concern the first recurrence term, its normalization, and store order. The half-shape public ERFC path is a useful fixed leaf. There is no reason to reopen its accuracy while testing those surrounding operations. [P:F.2; P:G.10]

**There are two different sqrt(pi) constants in the supplied material.** The positive-Gamma half-integer path names 0x3ffc5bf891b4ef6b, represented by 1.7724538509055159. The B.5 landed literal 1.772453850905516 rounds instead to 0x3ffc5bf891b4ef6c. The mathematical sqrt(pi)=1.772453850905516027298… also rounds to 6c; the 6b value is consistent with taking the square root after first rounding pi to binary64. [R8] The midpoint of these adjacent doubles is approximately 1.77245385090551599294, which makes the decimal-literal distinction explicit. This is an analytical constant audit, not a newly executed test. Preserve the observed Gamma constant and the separate landed incomplete-gamma constant; do not unify them under the label “RN(sqrt(pi)).”

#### Two corrections to the supplied causal summary

The scaled density identified by A/B and H/R30 is

\[
\boxed{
f(x;a,b)=\operatorname{st}\left(M(a,y)\,
                   \operatorname{st}(a/x)\right),\quad
y=\operatorname{st}(x/b).
}
\]

Here x is the **original argument**. Replacing a/x by a/y omits the scale factor. Computing a/y and then dividing by b introduces another graph; it is not a bitwise identity. G.9’s a/y wording needs this qualification.

Also, the statement that the 99 GAMMALN-route density misses are all at a<2 conflicts with the supplied bins, which include shapes above 2. The relevant leaf is GAMMALN(a+1). For example:

- a=1.75 uses GAMMALN(2.75);
- a=2.75 uses GAMMALN(3.75), and therefore the B3 value at 2.75 through recurrence;
- a=5.5 uses GAMMALN(6.5), and therefore B3 at 2.5.

The lower core can affect shapes up to a<7 through this route. It cannot explain the independent Loader discrepancies at 1<a<2 merely because both occur in density rows. [P:B.4; P:F.1; P:G.2]

The 4,000 matching nonunit-scale rows also do not establish a scale-one defect: the summary does not establish paired sampling at the same shapes and normalized arguments. Paired power-of-two scale controls are needed.

#### Large-mean Poisson: distinguish exponent error from final staging

The fact that EXP(−mean) is subnormal is not evidence that Excel uses that quantity in the relevant branch. Nonzero results at means around 796, 909, and 1,010 already exclude any unscaled graph that first stores binary64 EXP(−mean) and then multiplies a polynomial: that stored exponential has rounded to zero.

In the identified Loader-style route, the stored exponential is EXP(E), with E=−s(a)−dev(a,y), not −y. At Q near the smallest normal value and y≫a,

\[
\operatorname{EXP}(E)/Q
\approx \sqrt{2\pi a}\,y/a.
\]

This factor is hundreds in representative rows, so the exponential and mass can remain normal even when the final Q is near the publication threshold.

There is a second useful scale comparison. If |E| lies in [512,1024), one ulp of E is 2⁻⁴³. Moving that exponent by one ulp changes EXP(E), and hence an otherwise fixed Q, by roughly 512–1,024 result ulps. The observed differences of only one to four ulps near the normal threshold therefore favor a shared effective exponent with different normalization/tail/publication staging over a generic one-ulp exponent discrepancy. This is an analytical prediction, not a proof that every row shares the exponent.

A particularly useful remaining hypothesis is that Q consumes a resident normalized prefactor before the public mass is stored. Existing experiments starting from the public mass cannot test that boundary. B.8 and C.6 specify this candidate.

### A.5 Inverses: a rounded-CDF plateau is a major part of the problem

The lattice-ceiling model of the public CDF is already refuted. More importantly, its large errors are sometimes the predictable consequence of returning an edge of a rounded probability plateau. They need not signal an inaccurate elementary leaf. [P:F.3; P:H/R35]

Consider the supplied CHIINV(0.99,2) row. Its forward function is

\[
Q_2(x)=\exp(-x/2).
\]

At this probability, ulp(Q)=2⁻⁵³, |Q′|≈0.495, and the root has ulp(x)=2⁻⁵⁸. A single rounded Q value therefore occupies an x interval approximately

\[
\frac{2^{-53}}{0.495\,2^{-58}}\approx64.6
\]

root ulps wide. Its lower edge is about 32 ulps below its center. The supplied implementation is about 31.8 ulps below the true root. That agreement in sign and scale is a strong explanation of this particular failure without changing EXP.

For a decreasing Q, the current upper-end publication after bisection of −Q chooses the first x where Q(x)≤p: the low-x edge of the relevant plateau. This is exactly the direction observed.

Excel’s closer result could arise from any of these:

1. Solving the smaller tail, P=1−p, when Q is close to one.
2. Retaining an internal tail/residual with more than binary64 precision.
3. Using a compensated residual rather than a rounded public CDF value.
4. Selecting an interior point by interpolation or iteration rather than a plateau edge.
5. A special formula such as −2 LN(p) for two degrees of freedom.

These alternatives must be separated before choosing a stopping tolerance. For p≥0.5, 1−p is exact in binary64, so complementary-tail tests can be particularly clean.

My method ranking is a safeguarded Newton-type method first, a safeguarded secant/interpolation method second, Halley or another higher-order correction next, and a tolerance-based bisection/interpolation hybrid after that. The relative ranking of Newton and secant is weak: output accuracy and an iteration cap do not identify the update. An exact lattice edge on the public rounded forward is below these because it is contradicted by the data.

For shape 20, p=0.07, plateau selection alone is much less persuasive: this is a smaller, well-resolved lower-tail probability, and the current root is about 54 ulps above truth. The first task is to compare **Excel’s public lower tail** and the implementation’s lower tail around Excel’s inverse output, then distinguish a defective Temme dispatch from an internal forward calculation. Three scales of the same unit quantile are one underlying observation, not three independent pieces of solver evidence.

#### The documentation does not establish a universal 64-iteration solver

The currently published pages say:

| Public documentation | Stated limit |
|---|---|
| GAMMA.INV worksheet function | 64 iterations |
| CHISQ.INV.RT worksheet function | 64 iterations |
| Legacy CHIINV worksheet function | 100 iterations |
| VBA WorksheetFunction.ChiInv | 64 iterations |
| CHISQ.INV worksheet page examined | No iteration limit stated |

[R3–R7]

This supports iterative search as a broad description. It gives little weight to a specific loop, termination rule, or common legacy/new implementation. The conflicting CHIINV descriptions are a reason not to use the number as an architectural fingerprint. Public documentation is not Microsoft implementation source and was used only for this descriptive check.


## B. Ranked alternatives and discriminating experiments

### Experimental contract

For a positive finite binary64 bit pattern b, define **W(b)** to be the 1,024 inputs with bit patterns b−512 through b+511. For negative values, traverse predecessors/successors in numeric order rather than incrementing the unsigned representation blindly.

Inject the input bits through the existing oracle mechanism. Do not round displayed decimal values to 15 digits or reconstruct the nearby arguments by an unverified worksheet expression. Where a rule uses an exact power-of-two scaling, preserve it exactly. Evaluate candidate st operations in the explicit arithmetic harness; an ordinary worksheet formula is not automatically a substitute for a specified store graph.

Every proposed test has a reference: a fully specified candidate graph, a public-function equality, or the existing frozen baseline. Exact equality predictions are often more informative here than an estimated direction against mathematical truth. For hypotheses whose coefficients are still unknown, there is no honest unconditional prediction of the Excel bit pattern; the test must first freeze a finite candidate and then select inputs on which it disagrees with another candidate.

The following ranking maps the unresolved alternatives to experiments.

| Region/problem | Ranked alternatives | Main discriminants |
|---|---|---|
| GAMMALN core | Raw-x factored rationals; centered/cleared factored rationals; stable log1p form; ordinary cancellation-prone log form | B.2–B.3 |
| B2 versus B3 | Different tables; shared table/different graph; shared table/shared graph | B.3 |
| GAMMA seed | Independently corrected normalized product; accurate Lanczos product; normalized/direct rational; EXP(public GAMMALN); shared correction shifted from 6 | B.4–B.5 |
| a<1 lower gamma | Shared mass normalization; separate reciprocal-Gamma correction; other small-shape approximation | B.1 |
| Integer upper branch | Narrow R35 reciprocal-factorial Horner; current recurrence; another finite-sum graph outside the demonstrated dispatch | B.6 |
| Half-integer upper branch | Recurrence with a different first-term graph; reciprocal constant; resident term/sum | B.7 |
| General continued fraction | Current resident convergent recurrence with a different termination/publication; another convergent recurrence; modified-Lentz evaluation | B.7 |
| Large-mean upper tail | Shared exponent with a resident normalization boundary; different tail staging; genuinely different compensated/scaled exponent | B.8 |
| Inverses | Conditioned Newton/hybrid; conditioned secant/interpolation; interior-selecting bisection/hybrid; public-CDF edge | B.9–B.10 |

The priority order below reflects family-wide information, with the first three tests particularly valuable for GAMMALN and its downstream callers.

### B.1 Recover the lower-series mass from the density without a truth library

Take shapes from W around:

    0.75: 0x3fe8000000000000
    1.75: 0x3ffc000000000000
    2.75: 0x4006000000000000
    5.5:  0x4016000000000000

For each a, set x=ldexp(a,−80), b=1. Obtain

    C = GAMMA.DIST(x,a,1,TRUE)
    D = GAMMA.DIST(x,a,1,FALSE)
    M_observed = ldexp(D,−80).

The established density multiplier a/x is exactly 2⁸⁰. Thus the power-of-two rescaling recovers the stored mass exactly. At these x, the correction terms in the forward series are too small to change its sum from one, even at PC64 precision. All relevant outputs are normal for the listed shapes.

**Prediction under the shared stored-mass hypothesis:**

\[
\boxed{C=M_{\rm observed}\quad\text{bit for bit}.}
\]

Repeat at exponents −96 and −112. The a≥1 cases are controls; the a≈0.75 cases decide whether the small-shape branch shares this normalization.

Under an independent reciprocal-Gamma approximation, equality is not required. Freeze its existing 15-digit table as one named candidate: it makes exact computed predictions for every input. A consistent normalized discrepancy across the three exponents points to a shape-dependent normalization difference; a discrepancy tied to rounding phases can instead indicate a resident intermediate. Neither signature alone identifies the coefficients.

Then probe W(0x3ff199999999999a), the mean threshold 1.1, at fixed a=2.75 and 5.5, asking both density and CDF. Both sides remain in the same lower-series region. If GAMMALN(a+1) is the main cause, the predominantly negative current-minus-Excel error should persist below 1.1 and collapse toward the accurate Loader baseline above it.

Finally repeat each failing unit-scale density row at b=1/2, 2, and 4, with x=b·y exactly. With normal intermediates, the identified graph predicts exact reciprocal power-of-two scaling of the density. The same normalized failures should reappear. This tests the apparent scale-one concentration.

### B.2 Use power-of-two root offsets to expose GAMMALN’s normalized ratios

Probe x=1±2⁻ᵏ and x=2±2⁻ᵏ whenever the argument is distinct, representable, and lies in the relevant band. For a compact initial set:

- Around 1: positive offsets k=2…52; negative offsets k=2…53.
- Around 2: positive offsets k=1…51; negative offsets k=1…52.

Because the factor x−r is a signed power of two, rescaling the output removes the final multiplication without additional rounding noise.

Two supplied observations already imply:

| Input | Supplied GAMMALN output | Exactly rescaled ratio |
|---|---|---|
| 0x3ff0000000000001 = 1+2⁻⁵² | 0xbca2788cfc6fb61a | 0xbfe2788cfc6fb61a |
| 0x3fffffffffffffff = 2−2⁻⁵² | 0xbc9b0ee6072093cc | 0x3fdb0ee6072093cc |

These are deductions by binary scaling. They do not identify the derivative at the root: curvature terms at these nearby arguments can still matter in ratio ulps.

**Predictions:**

- A factored-ratio candidate must reproduce these ratio bits directly.
- A stored Gamma followed by LN cannot reproduce the small-result lattice.
- A cancellation-prone candidate predicts an absolute error floor unless it has an additional root-preserving mechanism.
- A stored ratio and a resident ratio are not distinguished by power-of-two factors alone. Follow with non-power-of-two offsets and use exact rounding preimages.

For B1/B2 recovery, use these ratios as hard constraints, not fitted derivative constants.

### B.3 Recover B3 globally and test whether B2 really requires another table

First collect the equal-spacing grid

\[
x_j=2+j\,2^{-51},\quad j=-512,\ldots,511,\ j\ne0.
\]

The negative side intentionally skips alternate binary64 values. This removes the input-binade spacing change at 2 as a confounder. Add W around 1.625, 2.5, 0.7, 1.5, 3, and 8:

    1.625  0x3ffa000000000000
    2.5    0x4004000000000000
    0.7    0x3fe6666666666666
    1.5    0x3ff8000000000000
    3      0x4008000000000000
    8      0x4020000000000000.

Fit no residual correction. Instead, require a shared-table hypothesis to satisfy the same coefficient-bit constraints on both sides of 2. A shared graph cannot arbitrarily reset its phases at 2; actual intermediate binade changes must be accounted for.

For each stored B3 product and sum, use the current approximate table to locate candidate crossings of |H_j(x)|=2ᵉ in [2,3]. Ask centered windows there and refine the locations from the observed trace. These windows expose coefficient bits hidden by the binade used near 2.5.

**Predictions:**

- Correct coefficients plus the reported graph predict crossing location, phase, and signed amplitude.
- An erroneous coefficient constrained only near 2.5 drifts elsewhere.
- Removing a store removes its binary64 quantization signature.
- A stored final polynomial sum and a resident final sum produce different quotient disturbances at selected discriminants.

Use candidate disagreement to choose the next window, then freeze the candidates before reading it. A continuum of unknown coefficient tables cannot be assigned a unique predicted output in advance; exact-bit prediction becomes possible once each finite candidate tuple is specified.

At 0.7 and 3, continue using the public-leaf recurrences as controls. At 8, retain the established Stirling program as the control. Distinguish an observed equality interval from a proven branch condition: two graphs may coincide for some points outside their dispatch.

### B.4 Establish the generic Gamma seed’s argument and storage precision

Use

\[
y_j=\tfrac14+j\,2^{-49},\quad j=0,\ldots,1023,
\]

and query GAMMA(n+y_j) for integers n=−9,…,9. Every translation is exact, and none is an integer or half-integer.

Start with the public seed at 1+y_j. Predict the descending stored product above one and resident divisions below one. For each candidate graph, invert the final roundings to determine the set of possible common seed values.

**Predictions:**

- A common stored seed gives exact equality in every applicable row.
- A common resident seed can survive cases that a stored seed cannot, but must round to the observed public seed.
- An empty intersection across exact translations refutes that common-seed graph.
- Failure only when translations are inexact implicates argument normalization rather than the seed approximation.

Next use 1,024 consecutive doubles starting at 0x3fd0000000000000, y=0.25. Several nearby y values map to the same stored 1+y. This specifically tests dependence on original y versus the rounded shifted argument.

Use neighboring windows around 1.5, −0.5, and −1.5 to test the hidden-half anchor. Under the stored generic recurrence:

    required internal seed at y=.5: 0x3fec5bf891b4ef6a
    predicted GAMMA(-.5):           0xc00c5bf891b4ef6a
    predicted GAMMA(-1.5):          0x4002e7fb0bcdf4f1.

A public positive-half seed instead predicts the conflicting endings 6b and f2. Neighboring non-half calls decide whether a special negative-half branch is responsible.

For the tiny plateau, use W around:

    RN53(1e−16): 0x3c9cd2b297d889bc
    2⁻⁵³:       0x3ca0000000000000
    2⁻⁵²:       0x3cb0000000000000.

A hard reciprocal branch guarantees equality through its threshold; a shifted-argument plateau instead predicts common-seed cells associated with rounding 1+x. The first Gamma/reciprocal disagreement alone does not prove the location of a branch.

### B.5 Separate seed product families by their leaves, not their average bias

On W around 1.25, 1.5 excluding the override, and 1.75, compare GAMMA(t) with EXP(GAMMALN(t)). This public-composition hypothesis predicts exact equality and has no free constants. Failure excludes the public composition, not every private log-gamma formulation.

For a direct Stirling-normalized product, locate the least binary64 t in (1,2) for which trusted LN(t) reaches 2⁻ʲ, j=1,2,3,4. Ask W around each. A stored unshifted LN(t) stage predicts quantization changes there.

For each frozen Lanczos candidate, use T=st(t+g−0.5) and examine binade crossings of its stored shifted argument and LN(T) wherever they exist. Include g=7 and g=607/128, with explicitly defined coefficient counts. For g=7, T crosses 8 at t=1.5 but LN(T) stays within [2,4). For g=607/128, both T and LN(T) remain in single binades throughout 1<t<2. Their ordinary phase tracks are still discriminants; do not search for nonexistent shifted-log crossings. A direct-product candidate and shifted-Lanczos candidate predict different tracks. Test those tracks, not whether total standard deviation doubles: other stages can mask the change.

A useful correction probe is

\[
D(t)=\operatorname{GAMMA.DIST}(t,t,1,\mathrm{FALSE}),\qquad1.1\le t<2.
\]

Here the density’s final factor is one and the deviance is zero. It exposes the existing Loader correction through EXP(−s(t))/sqrt(2πt), subject to the known normalizer graph. Invert its output-rounding intervals, adding other means at the same shape if necessary. A seed claiming to share that correction must be compatible with those intervals and its own fixed product graph.

The recurrence-from-6 candidate predicts the negative truncation tilt in A.3 before implementation errors. Freeze its exact stages in C.4 and check whether those errors actually compensate it. Do not normalize its mean afterward.

### B.6 Recover the integer-polynomial dispatch precisely

For integer a=2,…,20, collect windows around y=a, y=30, and y=31. Add interior dyadics such as y=a+1/4 whenever y≤30. Include shapes immediately below 20, exactly 20, immediately above 20, and 21.

Compare:

- The archived R35 graph, with its exact arithmetic convention.
- Candidate H53 in C.5.
- Candidate H64 in C.5.
- The current recurrence.

A coefficient-table Horner branch predicts the corresponding complete output, including phase and last bit, in its dispatch. A running-term sum generally disagrees on some selected rows. The existing POISSON.DIST(1,5,TRUE) answer 0x3fa4b2efe809fe98 is a useful simple control, not a new prediction.

Do not interpolate the documented successful interval a<y≤30 into an assumed boundary at 31. The windows must establish whether the cutoff is 30, 31, or another predicate, and whether equality is included. Similarly, a<20 in the present generic dispatcher is not proof that the observed shape-20 finite polynomial is impossible.

### B.7 Separate the remaining fractional-tail graph from its prefactor

For half-integer shape, start with exact-square means:

    25/16, 9/4, 49/16, 25/4, 49/4.

Their square roots are exactly representable and are not just powers of two. Ask W at those means for CHIDIST(2y,3), CHIDIST(2y,5), the half-shape public ERFC base, and corresponding GAMMA.DIST cumulative outputs. For each shape, use only y≥a as finite-sum probes; the smaller means are lower-series controls.

For a=1.5, freeze the two first-term graphs in C.5. They differ specifically in whether the algebraically removable factor y/sqrt(y) is evaluated by the current divide-then-recur path or by a direct sqrt(y) term. Each predicts exact bits relative to the same public ERFC/EXP leaves. Compare the signed residual against the predicted first-term difference before changing accumulation.

For a general continued-fraction region, use a=2.3, 2.75, and 5.3, and windows at y=a+1/4, y=a+1, and y=30.5, avoiding integer and half-integer overrides. Use GAMMA.DIST's public P for these arbitrary fractional shapes and compare the candidate's full publication graph. CHIDIST(2y,2a) is a Q oracle only when 2a is an integer; its degrees-of-freedom truncation otherwise changes the shape. For a<1, use a=0.75 and windows around 1.1 as a separate dispatcher test.

First substitute an observed/independently constrained prefactor. Then compare the existing resident fraction against its exact truncations one iteration earlier and later. A stopping error predicts changes correlated with the last convergent correction and its sign. A persistent earlier phase mismatch points to the recurrence or stores instead.

The prompt does not supply the full fraction recurrence or its complete stopping expression. I cannot honestly supply a distinct fully specified “Excel continued fraction” from that excerpt. If the current graph with a corrected prefactor and identified stopping threshold fails, compare a published permissible modified-Lentz or convergent implementation as an explicitly new candidate, with all its constants frozen before asking discriminants. A generic instruction to tune epsilon would not identify anything.


### B.8 Test the large-tail normalization boundary and publication separately

Use W around the nearest-even binary64 conversions of these supplied mean/count pairs:

    k=1,  mean=714.9686572379662
    k=4,  mean=731.5992965217341
    k=7,  mean=746.1854892200472
    k=19, mean=795.9928765080592
    k=53, mean=909.1269836788059
    k=89, mean=1010.5608861606299.

Include the recorded means below 700 as controls. Ask the three upper-tail aliases together. Also inspect the existing candidate’s stored exponent E, exponential V, and normalizer D.

Compare only the current mass-store boundary with the resident-quotient candidate in C.6, retaining the same tail sum.

**Prediction if the resident-quotient candidate is Excel's graph:** for positive tail factors, current Q minus Excel Q must have the same sign as st(V/D)−RN64(V/D), or be zero where later rounding hides the difference. The predicted difference is exactly Q_old−Q_new from C.6, ordinarily one or a few result ulps. Opposite-sign discrepancies refute this as a complete explanation of those rows; do not fit another constant to compensate it.

If the discrepancy instead tracks a changed E, use the exponent sensitivity in A.4. A one-ulp E difference should ordinarily move hundreds of result ulps near these tails, not merely one. A compensating factor must be explicitly demonstrated before accepting a changed-exponent explanation.

Locate the actual Q publication boundary around 0x0010000000000000. Compare:

- Store Q to binary64, then flush if the stored value is below the minimum normal.
- Test the resident Q against the minimum normal before storing.

The two differ only for resident values just below the boundary that round up to the smallest normal. Ask points selected specifically to make the two predictions disagree. A generic window far from such a crossing cannot identify this order. Do not assume the different k=0 behavior extends to every public caller; ask the aliases at the boundary.

For the isolated k=0 anomaly, repeat the exact recorded mean 60.01467269596327 and a window around it, asking POISSON.DIST(0,m,TRUE), POISSON.DIST(0,m,FALSE), EXP(−m), and CHIDIST(2m,2) together. The shared-leaf hypothesis predicts four identical bit patterns. A reproducible disagreement is a routing fact; a stale/unpaired record is not a reason to alter EXP.

### B.9 Map inverse plateaus using already identified elementary forwards

For x in W around the following observed Excel inverse outputs, ask the appropriate forward functions:

    CHIINV(.99,2):       0x3f9495453e6fd4bc
    CHIINV(.99,1):       0x3f2496fe6cb55211
    GAMMA.INV(.99,.5,1): 0x400a8a2255a6e921.

The EXP/ERFC/ERF special cases isolate search behavior from the unknown gamma core.

Record plateau endpoints, Excel’s position within them, and the smaller-tail values. The public-lattice model predicts the specified edge. A smaller-tail or more precise internal-residual method can lie well inside the public plateau. For df=2, the width should be about 65 input ulps, subject to the exact placement of rounding boundaries; the current answer should be near the low-x edge.

At df=2, compare the inverse directly with st(−2·LN(p)). This special-form hypothesis predicts exact equality. Failure leaves a shared solver entirely possible.

Then take W around p=0.99, whose center bits are 0x3fefae147ae147ae, with p≥1/2, and let q=1−p, an exact subtraction. Ask:

    CHIINV(p,k)
    CHISQ.INV(q,k)
    st(2 * GAMMA.INV(q,k/2,1))

for k=1,2,3,4,10,40,50.

A common solver that canonicalizes to exactly the same smaller-tail problem predicts identical bits. A separate right-tail solver need not. Equality is evidence for common canonicalization, not proof of Newton specifically. Around 1/2, use p=1/2+j·2⁻⁵³, j=−512,…,511, so the complement is exact on both sides. Consecutive doubles immediately below 1/2 do not all have exact binary64 complements and could create a false branch signature. A branch can also exist when its outputs join smoothly.

### B.10 Separate the shape-20 forward defect from inverse termination

At a=20, p=0.07, the unit-scale Excel root deduced by exact rescaling is

    xE = 0x402bac06c70d46b8.

Ask public GAMMA.DIST over W(xE) at shape 20, and at the immediately adjacent shape values. Add shape 25 as a control. Compare the current Temme path with the known lower series using the same established prefactor.

If the current monotone forward selects a root about 55 ulps higher, its value at xE should be below the target. Determine whether Excel’s public forward shares that defect. If not, fix the forward discrepancy before inferring a solver tolerance. If it does, the inverse is using a different residual/forward or a materially different selection criterion.

At observed inverse answers, compute the fixed one-step Newton and Halley probes in C.7 with public leaves. A zero-step stopping model predicts that its last update rounds back to the same x. A reproducible multi-ulp step refutes that exact public-leaf zero-step model. It does not refute a resident residual or an earlier width/tolerance stop.

These probes cannot generally distinguish Newton from Halley near convergence: their difference is second-order in an already tiny correction. To identify the update, first freeze complete candidate initializers and safeguards, then ask inputs selected from their different final predictions. The supplied final-output summaries do not determine those trajectories.

## C. Implementable specifications and their limits

### C.1 Arithmetic and constant conventions

Use:

- RN53: a single nearest-even binary64 rounding.
- R64: a nearest-even operation with 64-bit significand in the x87 register format.
- st: R64 evaluation of the indicated graph, followed by a binary64 store.
- LN and EXP: the supplied reproduced worksheet leaves.
- ext: exact loading of a binary64 value into the register format.

An expression with several operators is not fused: each operator rounds to the stated working precision. No FMA is implied. Powers of two used for exact rescaling must avoid overflow and underflow.

| Constant | Value/bit specification | Status |
|---|---|---|
| GAMMALN lower seam | 0x3fe6666666666666 = RN53(0.7) | Observed dispatch |
| Other rational seams | 1.5, 2, 3 | Current leading partition; qualifications in A.2 |
| GAMMALN Stirling seam | 8 | Strongly observed |
| GAMMA large-positive seam | 10 | Strongly observed for generic branch |
| GAMMA reflection seam | −10 | Leading observed generic partition |
| Working tiny reciprocal cutoff | RN53(1e−16), 0x3c9cd2b297d889bc | Retained behavior; literal branch not proved |
| Half log 2π | 0x3fed67f1c864beb5 = 0.91893853320467278 | Mathematical constant, observed usage |
| sqrt(2π) | 0x40040d931ff62706 = 2.5066282746310007 | Mathematical constant, observed high-Gamma usage |
| Positive-Gamma half constant | 0x3ffc5bf891b4ef6b = 1.7724538509055159 | Observed special-case constant; not RN53 of mathematical sqrt(π) |
| Landed incomplete-gamma half constant | 0x3ffc5bf891b4ef6c = RN53(1.772453850905516) | B.5 literal; also RN53 of mathematical sqrt(π) |
| π | 0x400921fb54442d18 | Binary64 mathematical constant |
| GAMMALN(3) | 0x3fe62e42fefa39f0 | Observed compatibility pin |
| Series stop | 2⁻⁵³ = 1.1102230246251565e−16 | Observed lower-series predicate |
| Mass route seam | RN53(1.1), 0x3ff199999999999a | Observed |
| Existing series/CF split constant | RN53(2.30258509299405) | Keep the supplied literal |
| Minimum normal | 0x0010000000000000 = 2⁻¹⁰²² | Exact binary value |

Do not replace a literal such as 2.30258509299405 by a newly computed LN(10). That is a different candidate.

### C.2 GAMMALN: exact shell, provisional coefficients

Apply the following only after the existing worksheet-domain guard has rejected non-finite or non-positive x. Preserve the existing error mapping.

For S10, load each of the following exact rational numbers rounded once to binary64, in ascending powers of z²:

    c0 = 1/12
    c1 = −1/360
    c2 = 1/1260
    c3 = −1/1680
    c4 = 1/1188
    c5 = −691/360360
    c6 = 1/156
    c7 = −3617/122400
    c8 = 43867/244188
    c9 = −174611/125400.

The operational series is:

    z = st(1/x)
    t = st(z*z)
    w = c9
    for j = 8 down to 0:
        w = st(st(w*t) + cj)
    S10 = st(z*w).

For x≥8:

    q1 = st((x−0.5)*LN(x))
    q2 = st(q1−x)
    q3 = st(q2 + 0x3fed67f1c864beb5)
    answer = st(q3 + S10(x)).

Here x−0.5 is inside the indicated register expression. Do not introduce an additional stored subtraction without treating it as a different graph. This specification preserves the supplied successful G.1 computation; individual high-order coefficient bits are not independently proved by whole-program agreement.

For 3<x<8:

    n = floor(x)
    y = x−n
    z = y+2
    for k = 3 through n−1:
        z = st(z*(y+k))
    answer = st(LN(z) + core(2+y)).

For x<0.7:

    u = RN53(x+1)
    inner = core(u)
    answer = RN53(inner−LN(x)).

The last line deliberately retains the current B.1 plain-double subtraction. A variant using st for that subtraction remains observationally unresolved in the supplied controls. B.3’s candidate-disagreement selection is the way to choose between them, not an assumed universal x87 rule.

Preserve +0 at x=1 and x=2 and the measured x=3 pin.

For the reported B3 graph, each polynomial is evaluated as:

    v = c8
    for j = 7 down to 1:
        v = st(st(v*x) + cj)
    p = st(v*x)
    return R64(ext(p)+ext(c0))     // no binary64 store of the last sum

Then:

    N = polynomial(P,x)           // resident
    D = polynomial(Q,x)           // resident
    r = st(N/D)
    answer = st(r*(x−2)).

Retain the supplied provisional P and Q values as the starting candidate, in ascending powers:

    P:
    −2.266786438167228
    −29.907511646102087
    −66.721979913050035
    −2.0639757251365638
     62.082398083771913
     33.248586310798345
      5.3620587366274837
      0.26453758918906523
      0.0026730135451372965

    Q:
      1.0000000000157898
     25.696574388696767
    123.4678964737532
    189.61325507101793
    113.27059743840934
     27.704029458859928
      2.648541159441014
      0.082760427448873825
      0.00049551480844350372.

These are **supplied inferred coefficients, not a newly identified exact table**. Preserve their existing repository bits when testing; do not introduce decimal conversion as another uncontrolled variable.

No complete replacement coefficients for B1 or B2 follow from the evidence. The precise implementation task is to solve their staged rounding constraints and then freeze a table for independent windows. Inventing coefficients or selecting them by total match count would violate the stated method.

### C.3 GAMMA: preserve the outer graph, distinguish public and generic seeds

These generic formulas apply after the existing pole, special-case, domain, and overflow handling. No new behavior outside the analyzed finite ranges is specified here.

Let S(y) denote the generic seed, with y the fraction used by the internal branch. This notation deliberately does not equate it with every public GAMMA(1+y).

For generic 1<x<10:

    n = floor(x)
    y = x−n
    product = 1
    for k = 1 through n−1:
        product = st(product*(x−k))
    answer = st(product*S(y)).

For the candidate below-one graph:

    n = floor(x)
    y = x−n
    value = ext(S(y))
    for k = −n down to 0:
        value = R64(value / ext(RN53(x+k)))
    answer = RN53(value).

This is the existing graph to validate with exact translations, not a resolution of the conflicting R33 evidence. Keep the working positive-tiny reciprocal behavior separately. Preserve the existing tested integer and positive-half overrides; do not use their public values as generic seed observations.

For generic x≥10, retain:

    n = floor(x/2)
    f = x/2−n
    root = EXP(st(f*LN(x)))
    v = st(integer_power(x,n)*root)
    reciprocal = st(1/x)
    h = st(sqrt_R64(ext(reciprocal)))
    e = EXP(−x)
    u = st(v*st(e*h))
    body = st(st(u*v)*0x40040d931ff62706)
    answer = st(body*EXP(S10(x))).

integer_power is the measured low-bit-first binary powering: store every square and partial product, and initialize the first selected partial product from that square without an unnecessary multiplication by one.

The leading negative-large reflection candidate is:

    m = −x
    n = floor(m)
    f = m−n
    t = min(f,1−f)
    theta = st(0x400921fb54442d18*t)
    s = stored sine leaf(theta)
    z = st(m*s)
    g = positive generic Gamma(m)
    denominator = R64(ext(g)*ext(z))
    magnitude = st(ext(0x400921fb54442d18)/denominator)
    answer = −magnitude if n is even, otherwise magnitude.

No unknown sine bits should be “corrected” from the negative output alone. Query positive partners and intersect common-sine preimages at fixed fractions, for example t=1/4+j·2⁻⁴⁵, j=0,…,1023, with m=n+t and n in {10,16,32,64,128,170}. These translations preserve the fraction exactly. A correct graph must admit a common sine across them.

A precise low-priority candidate for the nine positive-large exceptions changes only the last correction boundary: retain R64(z*w) from the final S10 multiplication as the argument of the existing register EXP body instead of storing that argument first. All coefficients and preceding stores remain fixed. It predicts differences only near a rounding boundary of EXP(S10), plausibly one correction-factor ulp propagated into a few final ulps. Check the predicted boundary proximity at the failing inputs before pursuing it. This is unproved and may explain none of the nine.

### C.4 A complete diagnostic seed candidate, not a proposed production replacement

To test the specifically suggested “shared ten-term correction from 6” family, define the following complete candidate on noninteger 1<t<2:

    u = RN53(t+5)
    s = S10(u)
    for j = 4 down to 0:
        v = RN53(t+j)
        r = st(1/v)
        l = LN(st(1+r))
        h = st(v+0.5)
        d = st(st(h*l)−1)
        s = st(s+d)

    L = LN(t)
    p = EXP(st(st(t−0.5)*L))
    a = st(0x40040d931ff62706*p)
    b = st(a*EXP(−t))
    answer = st(b*EXP(s)).

Every constant and store is specified. This uses no fitted coefficient. It may not equal the existing helper’s precise recurrence schedule, so compare that schedule separately rather than silently identifying them.

Its purpose is to test a concrete hypothesis that the prompt raises. A.3 gives the exact-real truncation prediction. I do **not** expect it to be an automatic improvement or to recover the hidden half-seed anchor. The leading normalized-product family still lacks an identified accurate correction table.

A second fully specified outer graph can use the same s but replace the last product by the measured split-half-power structure in C.3, with n=0 and f=t/2. This separates a direct power from a squared half-power without fitting the correction. Any success must also reproduce its stage-line signature.


### C.5 Incomplete gamma: precise lower series and narrow finite-sum candidates

The established lower-series graph is:

    rr = a
    c = 1
    A = 1
    repeat:
        rr = RN53(rr+1)
        c = st(c*st(y/rr))
        A = st(A+c)
    until st(c/A) <= 2⁻⁵³
    P = st(M(a,y)*A).

Preserve the branch’s existing companion publication

    Q = RN53(0.5 + RN53(0.5−P))

unless a selected discriminant demonstrates a different precision convention. Do not impose this complement on every branch of the whole family.

For the restricted integer polynomial, define coefficients Cj=RN53(1/j!) loaded as independently rounded constants. The exact factorial denominators j=0…19 are:

    1, 1, 2, 6, 24, 120, 720, 5040, 40320, 362880,
    3628800, 39916800, 479001600, 6227020800, 87178291200,
    1307674368000, 20922789888000, 355687428096000,
    6402373705728000, 121645100408832000.

Do not generate these coefficients by a running floating-point reciprocal recurrence.

**Candidate H53**, provisionally dispatched only at integer a=2…20 and a<y≤30:

    H = C[a−1]
    for j = a−2 down to 0:
        H = RN53(RN53(y*H)+Cj)
    Q = RN53(EXP(−y)*H)
    P = RN53(0.5 + RN53(0.5−Q)).

**Candidate H64** has the same coefficients and provisional dispatch, but replaces each arithmetic RN53 operation in H53 with st. Constant loading remains RN53. These are explicit alternatives because “stored Horner” in the summary does not alone settle single versus double rounding. The archived R35 program, if available to the implementation agents, takes priority over this reconstructed pair as the original frozen hypothesis.

For half-shape recurrence, let h=RN53(sqrt(y)), e=EXP(−y), C=0x3ffc5bf891b4ef6c (the landed B.5 literal), and Q0 be the established public half-shape upper tail at exactly the same y. This C is distinct from the positive-Gamma half constant ending in 6b.

**Candidate T53, divide first:**

    z = RN53(C*h)
    t = RN53(e/z)
    term = RN53(RN53(y*t)/0.5)
    Q = RN53(Q0+term).

**Candidate D53, direct first increment:**

    u = RN53(2*h)
    term = RN53(RN53(u*e)/C)
    Q = RN53(Q0+term).

Both now give a candidate Q at shape 1.5. To reach a larger half-integer shape, successively divide the term recurrence by 1.5, 2.5, and so on:

    term = RN53(RN53(y*term)/c)
    Q = RN53(Q+term),

where c runs from 1.5 through a−1. These two candidates isolate the algebraically different first term before reopening the precision of every accumulation. Neither is claimed to be an identified Excel graph.

Do not infer a complete a<1 replacement from these formulas. B.1 must first establish whether its mass normalization is shared.

### C.6 A precise one-boundary large-Poisson candidate

Reuse the existing Loader quantities for a=k+1 and y=mean:

    E = the existing stored value st(−s(a)−dev(a,y))
    V = EXP(E)
    D = the existing binary64 sqrt(2*pi*a) normalizer
    u = st(a/y)
    A = the existing stored asymptotic-tail factor (1+sum).

The present mass-first graph is:

    M = st(V/D)
    T_old = st(M*u)
    Q_old = st(T_old*A).

The proposed diagnostic changes exactly one boundary:

    M_resident = R64(ext(V)/ext(D))
    T_new = st(M_resident*ext(u))
    Q_new = st(T_new*A).

Use the same E, V, D, u, A, and final publication policy in both. This is a complete local change: no new constants, no fitted residual, no new tail sum, and no changed exponent.

For positive u and A, monotonicity of the subsequent roundings means Q_old−Q_new cannot take the opposite sign to the mass-store error; it may be zero. Any failure of that directional prediction indicates that the comparison changed another stage or that the proposed causal explanation is wrong.

This candidate may fix only a subset, or none, of the 542 large-mean misses. It should not be accepted just because its total match count improves. Under the hypothesis that Q_new is Excel's graph, require Q_old−Excel to equal Q_old−Q_new, including the stated sign restriction, on independent windows.

### C.7 Exact inverse diagnostics without an invented full solver

Keep the observed outer relationships:

    GAMMA.INV(p,a,b) = st(b * unit_inverse(p,a))
    CHISQ.INV(p,k) = GAMMA.INV(p,k/2,2)

with the established argument/domain conventions.

Use unit-gamma coordinates for the following probes. Query GAMMA.INV with scale 1 directly; divide a CHIINV or CHISQ.INV result by 2 before taking a=k/2. For a public upper-tail leaf through CHIDIST(2x,2a), require integer 2a; otherwise its degree truncation changes the problem, and an appropriate internal upper-tail probe is needed instead.

To test a **public-leaf Newton zero-step hypothesis**, at an observed candidate x and fixed selected tail define:

    R = RN53(P_public(a,x)−p)          // lower-tail formulation
    or
    R = RN53(q−Q_public(a,x))          // upper-tail formulation
    d = GAMMA.DIST(x,a,1,FALSE)
    delta = st(R/d)
    next_N = st(x−delta).

The chosen upper-tail residual has derivative +d, so the same subtraction direction applies. Keep the two residual definitions separate; they need not have the same rounding information.

For a precisely defined Halley probe:

    u = st(st(a−1)/x)
    h = st(u−1)
    v = st(st(0.5*delta)*h)
    denominator = st(1−v)
    delta_H = st(delta/denominator)
    next_H = st(x−delta_H).

These specify probes, not a complete inverse algorithm. They contain no guessed tolerance. A model claiming to stop only when that public-leaf update rounds to the same point predicts next=x at termination, unless a separately specified cycle/bracket rule intervenes.

The following details remain unidentifiable from the supplied summaries: initial approximation, bracket expansion, exact safeguard, relative tolerance constant, cycle handling, and endpoint tie rule. I would not fabricate them and label the resulting solver “most likely Excel.” Complete candidate solvers can be compared only after those choices are frozen and inputs are selected on which their predictions differ.

### C.8 Which supplied rows should change?

The distinction between a deduction, a conditional consequence, and an untested candidate matters here.

| Supplied row or class | Prediction |
|---|---|
| GAMMA(−1.5), current ending f2 versus Excel f1 | The stored generic seed ending 6a and the stated recurrence predict f1 exactly. This is a conditional exact-bit deduction. |
| GAMMA(−0.5) | The same seed explains the observed ending 6a; the public positive-half seed cannot. |
| GAMMALN at 0.12639999389648438 | Exact recovery of B1 at stored 1+x should fix the −1-ulp miss through the established lower recurrence. |
| GAMMALN at 0.7682 and 1.4048 | Exact B1 recovery should fix the −5 and −7 misses. No coefficients supplied here establish that recovery yet. |
| GAMMALN at 1.515389574256078 | Requires exact B2 recovery; expected −5 miss removal conditional on that recovery. |
| GAMMALN at 2.7566582032331577 | Requires exact B3 recovery; expected −6 miss removal conditional on that recovery. |
| GAMMALN at 3.6269 | Exact B3 at the actual reduced binary64 argument should fix the −2 miss through the established recurrence. |
| Predecessor of 0.7 | Depends on B2 at stored 1+x; expected +1 miss removal after exact recovery. |
| Exact stored 0.7 and predecessor of 1.5 | Depend on B1; expected +2 and −2 miss removal. |
| GAMMALN at 2.9997717401389883 | Depends on B3; expected −1 miss removal. |
| Existing matched GAMMALN anchors and all x≥8 rows | Must remain matched; no new correction is proposed there. |
| Other below-ten Gamma samples, including 1.1790527, 1.9200133, 2.7712475, and 0.06083395 | No exact new bit prediction from a method-family ranking alone. A recovered seed fixes them only if their reduction graph is also established. |
| Gamma at 19.81385692846423 | A seed change has no effect. Test the isolated large-positive staging candidate separately. |
| Gamma at −143.00098710784383 | A below-ten seed change has no effect. Isolate positive Gamma and reflection. |

For Appendix F, exact log-gamma leaves should remove or reduce errors on the small-mean route **when leaf injection actually removes the discrepancy**. This includes the plausible a=1.75, 2.75, and 5.5 chains. It does not automatically fix the Loader 1<a<2 rows, half-integer upper sums, the a=12 tiny-input miss, or the large-shape Temme errors.

The incomplete-gamma and inverse diagnostic candidates do not directly change Appendix E’s GAMMA/GAMMALN rows.

## D. Bold ideas worth pursuing

### D.1 Treat rounded outputs as constraints on hidden intermediates

For a fixed graph, an observed binary64 result constrains its pre-store value to a rounding preimage. Back-propagate those constraints through the measured recurrence. Intersect them across exact translations, scales, and public aliases.

This can reveal whether:

- a generic seed was stored;
- a public special case concealed it;
- an input translation changed the seed argument;
- an internal mass remained resident;
- a presumed common sine is inconsistent across reflected arguments.

Model R64 and RN53 separately, including midpoint parity. A real-valued interval with a single RN53 endpoint rule is not enough when the subject is double rounding. If branches of the preimage become relevant, preserve them instead of replacing them by an over-broad interval.

This is a stronger objective than minimizing error: the candidate either has an admissible intermediate or it does not.

### D.2 Use signed stage gains, not only Fourier frequencies

For L=(x−r)P/Q, a numerator perturbation contributes approximately

\[
\delta L=(x-r)\delta P/Q,
\]

whereas a denominator perturbation contributes

\[
\delta L=-(x-r)P\delta Q/Q^2.
\]

A perturbation at a Horner stage acquires additional powers of x from the remaining multiplications. Use these signed amplitudes to label numerator and denominator lines. Frequency matching alone discards this information.

A narrow-window stage frequency is aliased on the input lattice. The number of visible peaks is not the polynomial degree. Link phases across independent centers and intermediate binades; require one global staged computation to explain them.

### D.3 Recognize the coefficient scale that outputs cannot identify

Multiplying every numerator and denominator coefficient by the same exact power of two leaves a normal-range stored-Horner ratio unchanged, provided no stage overflows or underflows. Rounding commutes with that scaling, and the quotient is the same.

Thus some absolute coefficient scales are unidentifiable from output bits. Fix one representative of that equivalence class. Do not confuse this legitimate ambiguity with permission to normalize an arbitrary coefficient to 1: a non-power-of-two rescaling generally changes the rounding graph.

This is especially relevant to B3’s inferred Q(0). Recover the observable coefficient relationships first.

### D.4 Look for table values formed as separate literal constants

The restricted reciprocal-factorial result is consistent with a programmer loading a short table, rather than obtaining each coefficient by repeated division. Those constructions agree mathematically and can differ at the last bit. Similarly, a rational may have been algebraically cleared offline and typed as raw-x coefficients, or a constant may have been copied at a particular decimal precision.

Use these as graph hypotheses only when their quantization signatures are observed. A higher match rate after changing decimal digits is not evidence of the table’s origin. The half-shape sqrt(pi) correction succeeded because its effect had an identified scale and branch.

### D.5 Do not interpret every seam as a dispatch or every plateau as a special case

A stage crossing a power of two can change the error statistics without a branch. Conversely, two dispatched graphs can agree over many adjacent inputs. This distinction matters at 1.5 and 2 for GAMMALN, at tiny Gamma arguments, and at inverse tail-selection seams.

Design paired predicates for two complete graphs. Search for their disagreements, then query those exact inputs. A “first observed mismatch” is not necessarily a dispatch threshold.

### D.6 Use the inverse as a second oracle for probability resolution

The inverse contains information about how Excel represents a probability internally. A root lying inside a wide public-CDF plateau demonstrates that exact inversion of that published lattice is not the whole computation.

The special EXP/ERF/ERFC cases make this unusually clean: the forward family is already identified. Compare the smaller tail, the public plateau, and candidate resident residuals before attributing a many-ulp root discrepancy to an inaccurate Gamma kernel.

Do not demand that a correct inverse satisfy public_forward(inverse(p))=p on every row. Rounded forward values can skip probabilities or flatten over many x values.

### D.7 Keep a short evidence ledger before another four-hour search

Several supplied summaries need explicit qualifications: the below-one Gamma recurrence, the original-x density factor, the shapes affected by GAMMALN, the two distinct sqrt(pi) constants, the restricted polynomial result, and the iteration-limit claim. Appendix C’s prose also gives a different high/low split for the nine large-positive Gamma misses from its regional histogram. Use the individual bit records for signed analysis.

This is a small bookkeeping task with large value: it prevents a fresh round from treating an earlier provisional conclusion as a proven exclusion.

For error statistics, retain window identities. Consecutive-double errors are correlated and the bank is deliberately nonuniform. Report dispersion across independently selected window centers as well as pooled point means. The existing means describe the bank; their nominal pointwise standard errors do not automatically establish uncertainty over an arbitrary distribution of x.

### D.8 The next sequence I would actually run

1. Run the exact density-to-CDF prefactor probe and inject public log-gamma leaves into the existing small-mean prefactor.
2. Recover B3 coefficient constraints at stage binade crossings, while collecting power-of-two normalized ratios for B1/B2.
3. Run the exact-translation Gamma lattice and hidden-half neighborhoods before more seed fitting.
4. Restore the narrow R35 polynomial as a separately tested forward candidate and determine its real dispatch.
5. Map the elementary inverse plateaus and exact-complement inverse identities.
6. Test the single withheld mass store in the large-Poisson tail, with signed predictions and ordinary-range controls.
7. Only then spend another broad search on seed coefficient families or inverse control flow.

The expected gain is not merely more matching rows. Each step removes a specific ambiguity that currently allows mutually incompatible computation shapes to survive.

## References and provenance

**P. Supplied evidence:** 9-gamma-family-prompt.md, pinned to DnaCalc commit b2a808502f74e1590dcfc682171c8a1db1d93ee2. Appendix references throughout this report refer to that supplied text. No claim of fresh numerical validation is made.

**R1. Boost.Math, “The Lanczos Approximation.”** Used only for published parameter examples and coefficient-count conventions, not as evidence that Excel uses that implementation.
[Read the Boost.Math discussion](https://www.boost.org/doc/libs/1_82_0/libs/math/doc/html/math_toolkit/lanczos.html).

**R2. NIST Digital Library of Mathematical Functions, §5.11(i)–(ii), particularly 5.11.1 and the positive-real remainder bound.** The numerical scales in A.3 are analytical deductions from the Bernoulli terms and this bound.
[Read DLMF §5.11](https://dlmf.nist.gov/5.11).

**R3. Microsoft Support, GAMMA.INV.** The public page specifies an iterative search and a 64-iteration limit.
[Read the GAMMA.INV documentation](https://support.microsoft.com/en-gb/excel/functions/gamma-inv-function).

**R4. Microsoft Support, CHISQ.INV.RT.** The public page specifies an iterative search and a 64-iteration limit.
[Read the CHISQ.INV.RT documentation](https://support.microsoft.com/en-us/excel/functions/chisq-inv-rt-function).

**R5. Microsoft Support, CHIINV.** The legacy worksheet-function page specifies 100 iterations.
[Read the CHIINV documentation](https://support.microsoft.com/en-us/excel/functions/chiinv-function).

**R6. Microsoft Learn, WorksheetFunction.ChiInv.** The VBA method page specifies 64 iterations.
[Read the VBA ChiInv documentation](https://learn.microsoft.com/en-us/office/vba/api/excel.worksheetfunction.chiinv).

**R7. Microsoft Support, CHISQ.INV.** The page examined does not state an iteration limit.
[Read the CHISQ.INV documentation](https://support.microsoft.com/en-us/excel/functions/chisq-inv-function).

**R8. Boost.Multiprecision, published high-precision sqrt(pi) example.** Used to distinguish the correctly rounded mathematical constant from the adjacent Gamma special-case constant supplied in the prompt. The bit and midpoint comparisons in A.4 are analytical deductions.
[Read the Boost.Multiprecision example](https://github.com/boostorg/multiprecision/blob/develop/README.md).

Public documentation was checked on 10 November 2026. Descriptive documentation is weaker evidence for numerical implementation than the supplied exact-bit observations.

