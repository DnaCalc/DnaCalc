# Student t tail shape (dc-bc1o), 2026-11-12

Codex on dna-koderbot; DNA Calc code, existing captured inputs and public Excel
answers only. No Microsoft source or disassembly, no restricted source code.

## Predictions before queries

T.DIST.2T(4/3,6) is one ulp high in the captured workbook. The current T tails
form beta coordinates directly as df/(df+x*x) and x*x/(df+x*x). The existing
F tails instead form the smaller/larger ratio and then the paired coordinates.
Mathematically, the two-sided t tail equals the right F tail at x*x, degrees
(1,df). Hypothesis: Excel shares the F tail graph, including that ratio order,
with T.DIST.2T, then halves it for T.DIST.RT and complements for T.DIST CDF.

Freeze independent ordinary/PC64 square inputs before F.DIST.RT primitive
queries, and then freeze all composed target predictions before asking the three
T functions. Include the captured input, degrees 1/2/3/6 and larger, both CDF
signs, plus fresh cases selected where square publication differs. A matching
public identity alone will not count as a product fix; the actual product's F
kernel still needs to match the measured primitive on the selected inputs.

The shared rounded F-tail identity matches 185/224 two-tail and right-tail
answers, and 186/224 signed CDF answers. It fixes the captured two-tail/right-tail
values as a public composition, but fails broadly at degrees 1 and 2 and in
several general-degree cases. It is not an identified universal replacement.
No T production change is made in this batch.

At the captured input, direct and ratio coordinates have identical bits, but
F supplies the smaller coordinate first to BRATIO and uses its complement,
whereas T currently supplies the larger coordinate first. Before new queries,
compare direct, ratio and reciprocal-ratio coordinates with fixed versus
smaller-first orientation. Freeze fresh inputs and coordinates before the beta
queries, then composed predictions before T targets. Rounded public complements
may lose information relative to BRATIO's paired outputs; treat that as an
evidence limit, not a reason to fit coefficients.

Fresh 256-case public-beta orientation probe: direct smaller-first composition
190/256; fixed orientation 126/256. At degrees 1/2, fixed orientation is 45/45,
whereas smaller-first is 22/45. The captured row remains ...7f0 under rounded
public beta compositions, versus Excel ...7f1: those do not expose BRATIO's
unrounded paired complement. Existing T product replay is 361/480 across the
two retained batches, so primitive-pair arithmetic needs direct qualification.

Next prediction plan, before any new Excel answers: use the existing BRATIO
primitive in a temporary native diagnostic to freeze candidate paired-coordinate
results, including both of its outputs. Compare fixed/reversed/smaller-first
orientation, direct versus ratio versus normalise-then-square coordinates,
paired divisions versus forming one coordinate as a complement, and ordinary
versus stored-PC64 arithmetic. No duplicate evaluator or coefficient table is
introduced. The diagnostic is research-only and will be removed before a change
lands; application verification will still go through calc_func::invoke.
