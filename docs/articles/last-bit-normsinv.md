# The last bit: finding Excel's NORMSINV

*Started 2026-11-17 and completed 2026-11-20.*

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
- **0.075 <= p < 0.925:** the answer depends only on the stored value q = p - 1/2, and the left and
  right halves are exact mirror images. A different program.
- **0.925 <= p <= 15/16:** a third error shape.
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
76 retained misses looked like a detail of the logarithm. We came back to those in the last round.

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

## What we took into the last search

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


## Step 7: the two steps in the last piece (Gemini Carbon)

First fn9 settled the shared logarithm. The x87 computes the logarithm with FYL2X, stores it to a
double, then takes the square root with FSQRT and stores that too. With those two stores, all
393,216 retained lower-tail answers matched. Piece B went into the same kernel. That left a narrow
window on the upper side, from p = 0.925 through 15/16 inclusive.

Gemini Carbon started with the two places in that window where the scatter suddenly halves.
If Excel switched formulas there, what would happen to the repeating patterns in the low bits?
Carbon followed two such patterns through consecutive doubles. At the first step one pattern
doubled in frequency while the other went straight through. At the second step they traded roles.
One rational could do that. Each time a stored polynomial falls below a power of two, the spacing
of the doubles it can land on halves.

The frequencies tell us how fast those polynomials move. Counting cycles across neighbouring
windows and requiring their ratio to follow erfinv(t) / sqrt(-ln(1-t)) pinned down their slopes.
The denominator passed through 2 near t = 0.854685 and the numerator through 1 near t = 0.869236.
Following the denominator to the point where it would equal 1 gave 0.878906250, just outside the
window. In binary that number has a short name, 225/256. The centre was measured before a table
of coefficients was tried.

Expand the ratio at that centre as a degree-nine numerator over a degree-nine denominator, a Padé
approximant. Round its coefficients to doubles and use Horner's rule in t - 225/256, storing every
product and sum. Multiply the stored quotient by the same log-root as piece B, store, then multiply
by the stored square root of two. That program even explains the slight upward drift that had
sent us looking for a poorer approximation. Rounding the numerator's constant term contributes
almost half a unit in its last place. Dividing that fixed error by a falling denominator makes it
look like a changing approximation error.

The precision used to make the table left one more test. Solving at 80 and at 96 bits of working
precision gave adjacent doubles for the eighth-degree denominator coefficient. Carbon found an input,
p = `0x3fed999bc5f0efd8` as binary64 bits, where that one bit changes the final answer. Excel picked
the 96-bit table. Some last bits of the highest-degree terms have no direct discriminator yet.
The Padé construction specifies those terms too.

Carbon froze the predictions and their SHA-256 before asking Excel. The 32,452 new probabilities
included three runs of consecutive doubles, inputs spread across the window, and inputs chosen
to distinguish coefficient bits or x87 stores. Excel build 20527 returned **32,452 of 32,452**
predicted words under NORM.S.INV and **32,452 of 32,452** under NORMSINV. The input immediately
below 0.925 belongs to B. At exactly 15/16 Excel still uses C, and one double above it uses AS241.
On the lower side B goes straight to AS241. That accounts for the last piece.

In DNA Calc the old kernel matched 16,910 of the 32,452 frozen inputs under each name. The new
one matched all of them, then all 1,230,456 distinct inputs from the earlier piece C sweeps.
The full bank moved from 2,197 to 2,210 of 2,210 for NORMSINV and from 2,195 to 2,206 of 2,206
for NORM.S.INV. Across those two names, NORMINV, NORM.INV, LOGINV, LOGNORM.INV, CONFIDENCE and
CONFIDENCE.NORM, that makes 21,849 to **21,898 of 21,898**, including the rows where Excel returns an error.
