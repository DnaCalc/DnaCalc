# Chi-square inverse stages (dc-bc1o), 2026-11-12

Codex on dna-koderbot; public Excel observations and existing repository code.
No Microsoft source or disassembly.

Before new queries, freeze current GAMMA.INV at unit scale for 256 new
probability/shape pairs, its adjacent lower/upper floats, and selection of the
nearest public forward-CDF lattice point. Include the captured p=2/15,a=2.5
case separately. Ask GAMMA.INV and GAMMA.DIST at adjacent points, then
qualify CHISQ.INV against the measured unit-root times two. A mismatching
forward surface must be distinguished from root endpoint publication; do not
move the result one ulp solely because that fixes a captured example.

The native model reproduces invoke on all 257 controls. The 256 fresh inverses
match current 85, lower 61, upper 34, nearest-forward 99. The public forward
CDF matches only 481/771 adjacent points. At the captured case, all three
forward points match Excel and current CDF(root) equals p exactly, but Excel
chooses the next root above. Nearest-forward leaves this captured case wrong.

For retained-case accuracy calibration, compute mathematical roots only for
the 128 integer/half-integer shape controls, using the defining convergent
incomplete-gamma series, factorial/half-factorial normalization and Decimal
Newton at both 80 and 100 digits (all rounded roots agree). They match 66/128
versus current 43/128, including the captured point, but are far from a full
Excel program. No solver change follows from mathematical accuracy alone.
The first forward request mistakenly sent unwrapped float values; the service
refused it. The corrected request reuses the original frozen predictions.
