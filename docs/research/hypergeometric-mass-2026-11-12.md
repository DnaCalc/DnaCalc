# Hypergeometric mass orientation (dc-bc1o), 2026-11-12

Codex, dna-koderbot. DNA Calc code, captured inputs and public Excel answers.
No Microsoft source, disassembly or restricted source.

## Predictions before queries

HYPGEOM.DIST(2,6,8,44,FALSE) is two ulp high; its cumulative is one ulp low.
The mass currently uses three binomial masses with p=sample_size/population:
B(s; successes,p)*B(sample_size-s; failures,p)/B(sample_size;population,p).
The mass identity also permits the transposed orientation with p=successes/N,
or p=1/2. Mathematically identical forms can publish different low bits.

Freeze fresh near-mode integer cases and all three public binomial arguments
before primitive queries. Then freeze composed results for each orientation,
(a/c)*b, (a*b)/c and (b/c)*a, plain versus PC64, before HYPGEOM.DIST targets.
No coefficient or threshold fitting. The public binomial uses 1-p; the production
kernel currently computes its complement independently, an explicit limit of this
composition test. A poor fit rejects the composition rather than tuning it.

The public mass compositions fail: best 23/192, and the current product itself
matches only 20/192 on these fresh near-mode cases, despite 3572/3661 full-bank
rows matching. The captured answer is two ulp below the correctly rounded exact
combinatorial ratio, so replacing the kernel with exact arithmetic is not an
Excel-bit solution. No change is made from these failed compositions.

A sequence probability with successes first, then failures, and the number of
sequences C(n,s) multiplied last reproduces the captured PDF (multiplying C first
does not). This alone is calibration, not identification. Before fresh queries,
compare ratio-product versus multiply-then-divide, success/failure/reverse/grouped
orders and coefficient first/last, plain versus PC64. Use sample sizes <=20 so
C(n,s) is exactly representable and has no unrelated coefficient algorithm.
Freeze every model result before the new HYPGEOM.DIST answers. Reject a poor
fit; no rounded coefficient or epsilon is to be fitted.

The fresh sequence-product batch rejects those shapes: best 33/192, while the
success-first/coefficient-last ratio product that matched the captured PDF
matches only 24/192. Its captured cumulative is still one ulp wrong as well.
No hypergeometric arithmetic change lands. The failed shapes and all predictions
are retained rather than rescued with constants. The wide near-mode gap stays
within dc-bc1o's existing statistical root-cause batch.
