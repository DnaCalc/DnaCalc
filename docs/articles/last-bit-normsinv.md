# The last bit: finding Excel's NORMSINV

*Work in progress, started 2026-11-17. This article is finished when every piece of Excel's normal
inverse is known. Status at the end.*

DNA Calc aims to give the same answer as Excel, down to the last bit of every double. For most
functions that is plain engineering. For some it is detective work: the function is a piece of
compiled code written in the 1990s, nobody publishes it, and the only witness is Excel itself. This
is the story of one such function, NORMSINV (and its newer name NORM.S.INV), the inverse of the
standard normal distribution: give it a probability p and it returns the z with P(Z <= z) = p.

The rules we work by: we ask Excel questions and record its answers (allowed reverse engineering of
behaviour); we read public mathematics and published algorithms; we never read or disassemble
Microsoft's code. Everything below was inferred from answers.

## Where we started

Our first NORMSINV was a textbook approximation (Acklam's) polished by one Newton step. It was good
mathematics and bad mimicry: on the recorded Excel answers it matched 1,111 of 2,210 bank rows, and
near p = 1/2 it was off by up to 581 units in the last place (ULP), because the Newton residual
cancels there. "Close" is not the target. The target is the same 64 bits.

## Step 1: map before guessing

The question service (Touchstone) lets us ask Excel millions of questions quickly. So the first
real step was not a guess but a map: 128 windows of 8,192 consecutive doubles each, about a million
answers, compared against a high-precision true value. The shape of Excel's error, not its size,
split the input range into regions:

- **p < 0.075:** a smooth sawtooth error, and an answer that depends on p itself.
- **0.075 <= p <= 0.925:** the answer depends only on the stored value q = p - 1/2, and the left and
  right halves are exact mirror images. A different program.
- **0.925 < p <= 15/16:** a third error shape.
- **p > 15/16:** the first shape again, applied to 1 - p.

The two outer regions are Wichura's AS241 algorithm, the standard published normal inverse. The
middle is something else: the mirror symmetry and the dependence on q alone say Excel computes
sqrt(2) * erfinv(2p - 1), the inverse error function, there. Notice the asymmetry: the lower tail
switches to AS241 at 0.075, the upper only at 15/16.

Right next to p = 1/2 one tiny program matched all 16,383 answers there: t = 2q, multiply by the
stored constant sqrt(pi)/2, multiply by the stored sqrt(2). Each multiplication is done in extended
precision (a 64-bit significand) and rounded to a double when stored. That detail matters
everywhere below: it is the fingerprint of the arithmetic Excel's code uses.

## Step 2: the tails fall

With the store-each-step arithmetic, AS241's tail formula matched 1,999 of 2,000 fresh answers
asked *after* the predictions were frozen, and 393,140 of 393,216 retained ones. That landed. The
76 retained answers it still misses look like a detail of the logarithm, and are open.

## Step 3: the middle resists

The middle is where the rows are, and for several rounds it would not give. Each round ruled out a
family with exact counterexamples, which is progress of a kind:

- A root search on Excel's own NORM.S.DIST, Newton steps from AS241, two Newton steps on Excel's
  ERF: the noise in Excel's answers is too large and too structured for any of them.
- AS241's own central formula and its rewrites: wrong bias.
- Padé approximants of erfinv(t)/t up to total degree 25: at best a quarter of the bits.
- "A constant minus a stored series" and similar simple cancellations: a lattice test on the
  answers showed no such grid exists.
- Straight-line models of the denominator: predictions frozen in advance, then refuted by fresh
  answers.

What the failures had in common: they guessed a whole formula and then checked bits. Bits are
an unforgiving check for a guess that is almost right, and a fit that reproduces the function to
half a ULP can still get most bits wrong.

## Step 4: measure the structure, not the bits (fn6)

The turn came from asking smaller questions. Instead of guessing the formula, the fn6 lane
measured facts about it, one at a time, at inputs so small that only the first terms matter:

- The result R is a stored double, and it is a quotient of two stored doubles: R = P(s)/Q(s), with
  s = t*t and P and Q polynomials evaluated by Horner's rule.
- Q starts with 1. P starts with sqrt(pi)/2, the near-half constant.
- The first three denominator coefficients, q1, q2, q3, read off directly from tiny inputs, with
  97 to 100 % bit matches where they apply.

Those coefficients then became a fingerprint for the *method* that produced the formula. Compute
q1 and q2 for every plausible way a 1990s programmer could have fitted a rational to erfinv
(minimax, Chebyshev-Padé, interpolation at various point sets, over various intervals) and see
which method lands on Excel's pair. Plain minimax was close but off by 0.0015 in q2: close enough
to say "degree 7 over degree 7", not close enough to be it.

fn6 also shipped a correctly rounded stand-in for the middle, which already took the worst error
from 581 to 23 ULP and NORMSINV from 1,111 to 1,433 rows.

## Step 5: the method, then the constants (fn7)

The fn7 lane finished the fingerprint search: **rational interpolation of erfinv(sqrt s)/sqrt s at
the 15 zeros of the Chebyshev polynomial T15 on [0, b]**, numerator constant free, denominator
leading 1. One interval end b reproduces q1; that b is 0.73, a number someone would type, and q2
then agrees to seven digits. The method is found.

But the exact interpolant, rounded to doubles, matched only two thirds of the answers. Here came
the most interesting step. The 15 x 15 linear system behind the interpolation is badly conditioned:
its singular values run from 4.8 down to 5e-17. Anyone solving it at limited precision gets the
exact solution plus an error that lies almost entirely along one direction, the smallest singular
vector. So fn7 scanned one number: how far along that direction Excel's constants had moved. (A
later check showed what precision: a double-precision solve would move them by about 1, an
80-bit one by 1e-3; Excel's -2e-15 matches a solve at about 100 bits, some 30 significant digits.
The constants were made with a multi-precision tool, and only the last step rounded them.) At -2.0e-15 the rounded constants matched **656,542 of 656,542** retained answers. We did
not fit sixteen constants; we found the procedure that made them, including its rounding error.

Then the proof, the way every claim here has to earn its place: predictions written to a file and
hashed before asking Excel, then 44,576 fresh answers in dense windows and random inputs, all
matched, plus 164 inputs chosen to decide the last bit of one constant between two candidates (164
to 0). A probe at the boundary showed the switch is "s <= 0.5" on the stored s.

The bank moved: NORMSINV 1,433 -> 2,022 of 2,210, worst error 23 -> 10 ULP; NORM.S.INV, NORM.INV,
NORMINV, LOGINV, LOGNORM.INV, CONFIDENCE and CONFIDENCE.NORM rose with it, and the full bank gained
2,432 rows.

## Step 6: peel from the outside (fable-r2)

Piece B showed the same fingerprint half working: the oscillation of Excel's error had the 13-node
signature on the same upper end 0.73, but the result was not t times a stored ratio, and every
interpolant tried reached at most 23 % of the bits. Right after piece A fell, a short fresh-eyes
review (Claude Fable 5.2, 35 minutes) took a different first step: find the *last* operation
before guessing the formula. For 22 candidate intermediates and six last operations it counted
the answers that no stored double could produce, and compared each count with the same count on
answers shifted by three ULP (a null: how many a wrong hypothesis also "explains"). One candidate
stood out with 4 unreachable answers against 24,524 for the null:

    a = sqrt(-ln(1 - |t|)),   y = a * R,   R = P(s)/Q(s)

Once the last operation was known, the ratio was nearly observable, and the target function became
obvious: R interpolates erfinv(sqrt s) / sqrt(-ln(1 - sqrt s)) at fn7's 13 nodes, degree 7 over
degree 5. Ranking about 500 families by how well their exact error matched Excel's bias curve, before
looking at any bits, had already put this one first. fn7's families had failed because the target
function was wrong, not the constants. Piece B's constants are the exact interpolant rounded, with
no perturbation at all. On fresh answers whose predictions were frozen first it matched 33,303 of
33,313; the other ten need the logarithm moved by one ULP.

That residue is itself a clue. The logarithm inside the kernel is not the worksheet LN, nor the C
library's log, nor a correctly rounded one, and its miss rate equals that of the AS241 tails (76 of
393,216). Both pieces very likely share one internal log, and each miss is a direct reading of it.

## Where we stand (2026-11-17, midday)

| Region of t = 2p - 1 | Program | Status |
|---|---|---|
| \|t\| <= 1/sqrt 2 (piece A) | degree-7 rational in s = t^2, Chebyshev interpolation on [0, 0.73], perturbed solve | exact, in DNA Calc |
| 1/sqrt 2 < \|t\| <= 0.85 (piece B) | sqrt(-ln(1-\|t\|)) times a (7,5) rational at 13 Chebyshev zeros on s in [0.5, 0.73] | identified, not yet in DNA Calc |
| 0.85 < t <= 0.875 (piece C, upper only) | last operation the same as B; the ratio has two sharp noise steps that suggest a centred, scaled variable on a much wider interval | open |
| tails (AS241) | stored-step AS241 | exact except 76 of 393,216, shared internal log |

Piece C exists only on the upper side, and it is probably not a small piece in its own right but a
window onto erfinv's outer piece, built for a range far wider than the sliver NORMSINV uses before
handing p > 15/16 to the AS241 tail.

*To be continued.*

## What this taught us so far

- Map the shape of the error before guessing a formula; region edges and symmetries are free facts.
- Read structure at the edges where only one term matters, one coefficient at a time.
- Treat measured constants as a fingerprint of the method that produced them, and search methods,
  not coefficients.
- When constants are produced by an ill-conditioned solve, model the solve's error direction; one
  parameter can stand in for sixteen.
- Peel from the outside: find the last stored operation with a constants-free preimage test, and
  always run the same test on deliberately wrong answers to know what "no signal" looks like.
- Rank candidate families by their error curve before checking bits; a family that scores below a
  correctly rounded stand-in has the wrong target function.
- Freeze predictions before asking. A claim is what survives fresh answers.
