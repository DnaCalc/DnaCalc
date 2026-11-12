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

## Unrounded series residual, before retained-case calibration

The gamma series publishes store(PoissonMass(a,x)*series). An inverse could
subtract p before that final store and therefore resolve a public-CDF plateau
without changing any public CDF. Isolate that hypothesis: reuse the existing
stored series factors, but form PC64(mass*sum-p) with the existing running
multiply/add primitive. Outside the Taylor arm retain the current CDF residual.
Compare the crossing and nearest-residual lattice points on retained cases,
with the ordinary residual required to reproduce invoke. This changes no
coefficient, convergence limit or public gamma calculation. A weak result
rejects the hypothesis; a strong one needs fresh frozen application queries.

The unrounded-series residual is rejected on calibration: crossing 81/256,
nearest residual 97/256 versus baseline 85 and ordinary nearest-forward 99.
It also leaves the captured inverse at ...28fc rather than Excel ...28fd.
The control reproduces invoke on every input. No gamma inverse change lands.

Further rejected-model evidence: dnacalc-evidence/research/round58/20261112-fn2-rejected-0830/,
64 files, 120,226,186 bytes; checksum copy and one-way check zero differences.
These records do not add a landed pass or change any Excel answer.
