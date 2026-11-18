# Last-bit guide: how we find Excel's numeric kernels

A working guide for agents doing bit-exact research against recorded Excel answers. It collects the
methods that have worked, the tools behind them and the traps. Started 2026-11-17 from the NORMSINV
work (docs/articles/last-bit-normsinv.md, dc-j9k.47) and the fresh-ideas review fable-1
(DnaCalc-Research/design/fresh-ideas-fable-1.md, 2026-11-09). Add to it when a method wins or a
trap costs a round; keep each entry short and point at the evidence.

## Premise

Excel's kernels are small hand-written programs from the 1990s and 2000s. Find the shape of the
program first (what someone would have typed), then the constants. A match percentage is a test of a
finished hypothesis, never a training signal.

## Methods that have worked

1. **Map before guessing.** Dense windows of consecutive doubles, compared with a high-precision
   reference, give regions, edges and symmetries for free: where the error shape changes, whether
   the answer depends on the input or only on a derived stored value, whether mirror identities hold
   bit for bit. (NORMSINV: four regions and the asymmetric tails from one sweep.)
2. **Read structure where only one term matters.** At tiny or special inputs a program collapses to
   one or two terms; read those terms exactly, then the next ones at slightly larger inputs.
   (NORMSINV: Q monic, P(0) = RN(sqrt(pi)/2), q1..q3 from t < 1e-2.)
3. **Constants are a fingerprint of the method.** Compute the first measured constants for every
   plausible construction method (minimax, Chebyshev-Padé, interpolation at zeros/extrema/equispaced
   points, interval ends someone would type) and keep the method that reproduces them. Search methods,
   not coefficients. (NORMSINV piece A: interpolation at the 15 zeros of T15 on [0, 0.73].)
4. **Model the producer's own rounding.** Constants made by an ill-conditioned solve carry an error
   along the smallest singular vectors; scan that one amplitude instead of fitting every constant.
   Compute the amplitude the producer's working precision implies, and scan by distinct rounded
   coefficient sets rather than a log grid. (Piece A: alpha = -2.0e-15, a solve at about 100 bits,
   turns 436,842 matches into 656,542 of 656,542; piece B's better-conditioned system needs none.)
10. **Peel from the outside.** Before any formula, find the last stored operation: for candidate
   intermediates and last operations, count answers no stored double can produce, and run the same
   count on answers shifted a few ULP as the null. The winner makes the next intermediate nearly
   observable. (Piece B: sqrt(-ln(1-|t|)) times a stored ratio, 4 unreachable against a null of
   24,524; fable-r2.)
11. **Rank families at function level before bits.** Compare each candidate's exact error curve
   (high precision) with Excel's binned bias curve, one free constant; only the top few go to bit
   tests. (Piece B: about 500 families in seconds, the right one first.)
12. **Natural experiments.** Find inputs where one intermediate's rounding is frozen (it steps by a
   whole number of ULP) or one term dominates, and predict the shift. (Piece C's last operation,
   from one window below p = 15/16.)
5. **Store points follow source variables.** Extended-precision arithmetic with each assignment
   stored to binary64 is the usual fingerprint; enumerate store policies over plausible source lines,
   not arbitrary masks. A typed constant may be the rounding of its decimal text in extended
   precision, not binary64 (fable-1, premise 2).
6. **Qualitative signatures kill faster than counts.** Which root a solver lands on, where an error
   vanishes, whether a threshold sits on a typed literal (GAMMALN's 0.7 join), lattice/phase
   coherence of the answers on a store grid (piece A's denominator read-out).
7. **Other Excel functions are free oracles for intermediates.** Mirrors, complements, pdf/cdf
   ratios, LN/EXP round trips: ask the relative before the absolute (fable-1, premise 4).
8. **Bisect on predicates.** `bisect_edge` and `consecutive_run` find a dispatch edge in tens of
   questions (piece A's `s <= 0.5` on the stored s).
9. **Freeze, then ask.** Write predictions to a file, hash it, then ask fresh questions designed to
   split the surviving candidates (164 inputs decided p7's last bit, 164 to 0).

## Traps we fell into

- Guessing a whole formula and checking bits: an almost-right shape scores badly and teaches little.
- Fitting many ill-conditioned constants to answers: reproduces the function to half an ULP and few
  bits (fn6's 12-coefficient least squares: 2,105 of 5,000).
- Estimator bias read as signal (fn7's denominator read-outs 2-4 % low until calibrated on emulated
  answers; a lattice peak that was the output grid).
- Comparing against stale committed tables when counting gains and losses.
- A family that scores below the correctly rounded stand-in means the target function is wrong, not
  the constants (fn7 scanned piece B's constants for hours with the wrong target).
- Reachability counts without a null: "12.8 % unreachable" meant little until shifted answers
  showed what a wrong hypothesis scores (10-20 %); some "all reachable" results are vacuous.

## Toolbox

| Tool | Where | Use |
|---|---|---|
| Touchstone Excel answer service | tools/excel-service | batched questions, `bisect_edge`, `consecutive_run` |
| x87 store emulation | docs/research/normal-inverse-central-2026-11-17/fx87.py, fn7-emu.c | exact extended-precision Horner with stores |
| Phase-coherence denominator read-out | fn7-dread2.py | read Q(s) and (P-Q)' from dense windows |
| Null-direction scan | fn7-nullscan.py, fn7-nullfine.py | perturb an ill-conditioned solve along its smallest singular vector |
| Method fingerprint scan | fn7-fam4.py | first constants of candidate construction methods |
| Regime replay, preimage intersection, relative-oracle checklist | fresh-ideas-fable-1.md, cross-cutting tools | proposed, partly built |
| `lastop` last-operation preimage scan with a shifted null | fable-r2 w/reach2.c, reach3.c | find the outermost stored operation |
| `family-rank` exact-error ranking against the bias curve | fable-r2 w/biasfit3.py | rank target/variable/split/nodes families |
| `producer` + `finescan` | fable-r2 w/precA.py, finescan.py | emulate the producer at a stated precision; distinct rounded sets |
| `noiseprof` window error sd | fable-r2 w/noiseprof.py | step fingerprints of shifted variables (a filter, not a proof) |
| Frozen-rounding windows | fable-r2 w/q2, w/q3 | one-number dependency tests |
| To build next | fable-r2 review section 3 | additive-lattice (gcd) reader, equality-at-boundary scorer for `bisect_edge` switch points, preimage-chain tester for iterative solvers |

The fable-r2 scripts live in ~/codex-runs/fable-r2/w on dna-firehorse; copy the ones a lane uses
into the function's research folder when it lands.

## Per stuck case: what the transferred method tries first (fable-r2, 2026-11-17)

- **ETS period rule:** collect exact switch points with `bisect_edge`, score candidate statistics by
  equality at the switch points (no fitted threshold); first test invariance to dropping the first or
  last k points (window phase matters).
- **GAMMALN [0.7, 3):** read the answer grid near the roots 1 and 2 (a last subtraction leaves answers
  on multiples of ulp(U)); its binade jumps size the cancelled terms; then peel y + V.
- **Incomplete beta arms:** last-operation scan for the final multiply over prefactor shapes with a
  null; then one-term regimes (b = 1, b = 2); the stop tolerance where k vs k+1 terms flips a bit.
- **YIELD:** apply each candidate update map to Excel's own answer (the true map leaves it fixed within
  the stop rule; its preimage chain reaches a typed seed); the which-root signature first.
- **CHIINV at large df:** preimage test for a stored cube x = st(df * st(st(c*c)*c)) and its orders,
  with a shifted null, before extending any fitted formula.

## Reviews

When a hard function is stuck, or right after a breakthrough, a short bounded fresh-eyes review
(Claude Fable 5.2, read-only, a few discriminating questions allowed) looks for the missing piece and
for the rule that generalises to the other stuck cases. Its results land here and in the bead.
