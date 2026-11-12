# IRR publication (dc-bc1o), 2026-11-12

Codex, dna-koderbot. Sources: DNA Calc code, existing captured inputs and public
Excel outputs. No Microsoft source or disassembly.

## Predictions before replay

The two captured mismatches are npv!D39, IRR(D37:S37,.15), 16 flows, Excel
0x3fe0bb1839f46d5a versus product ...6d58; and roma!B12,
IRR(B10:Z10,.2), 25 flows, Excel 0x3fce92d394f5bba8 versus product ...bb98.
Their retained observed reads include every numeric flow without omissions.
First freeze those exact bit vectors and both expected outputs, then replay
row and column transports. Prediction: the fresh outputs equal the captures,
and transport orientation makes no difference. If not, investigate inputs or
context before changing the solver. The current solver uses a measured discount-
factor secant, stored PC64 arithmetic and NPV terms divided by worksheet POWER.
No iteration threshold or coefficient is to be tuned to these two answers.

All four replays equal the captured outputs, row and column alike. The last-bit
difference is in the solver route, not missing flow data or transport order.
Before new queries, retain the identified discount-factor secant and its existing
0.001 offset, 1e-7 stop and 200-step limit. Vary only arithmetic shape: whether
1/(1+guess) stores the addition, whether the secant expression stores every
intermediate or keeps PC64 intermediates until its assignment, and four
multiply/divide orders of the secant step. Keep the current worksheet integer
POWER graph and forward NPV fold as controls. First require the ordinary model
to reproduce both current product bits and the existing IRR unit witness.
Freeze every candidate on 128 independent flow/guess cases before asking Excel.

The arithmetic-only batch does not identify a replacement: no candidate fixes
either captured row, and the best fresh score is 86/128 versus baseline 85/128.
Running PC64 secant intermediates and an unspilled initial addition are worse.
Keep the measured secant arithmetic. Next qualify NPV's power call in isolation:
worksheet integer binary exponentiation versus sequential products, running-PC64
products, correctly rounded real powers and the platform real-power model.
The POWER kernel now has an integer-publication branch, while consumers have
separate call-site graphs. These are calibration replays of retained inputs;
a promising power shape would still need new frozen Excel holdouts and product
verification. No stop, offset or convergence coefficient changes.

The power calibration also fails to fix both captures. Best 90/131 (discount
powers) versus current 86/131; the first captured output is unchanged under
all nine power shapes, and most alternatives only halve the second error.
Do not land a closer result as an exact match.

Next isolate NPV aggregation: forward/reverse stored sums, accurate term sum,
Kahan compensation, the already measured SUM final-cancellation rule as a
negative control, and backward Horner evaluation. Compare the current powers,
real-power model and direct discount powers. Retain all secant constants and
arithmetic. This is calibration on the retained 131 cases; a selected graph
must survive a fresh frozen query, including two-flow and large-scale controls.

No aggregation model fixes both captures; best remains 90/131 with direct
discount powers, versus baseline 86/131. The shifted-period negative control
scores only 16–23/131 and breaks the known witness. Next inspect the retained
secant iterates: determine whether Excel's exact published rates appear after
the current stopping point. Compare new versus prior residual and rate-change
versus discount-change conditions with the existing 1e-7 constant, not a fitted
tolerance. Trace output is diagnostic arithmetic, not a new Excel reading.

The trace reaches roma's exact target one iteration later, but never reaches
npv!D39's target. A prior-residual stop would fix roma and damages the broader
set: 36/131 versus current 86/131. It is rejected; matching a captured rate by
adding an iteration is insufficient. Mathematical high-precision roots also do
not identify Excel's output (D39's current result is correctly rounded).

Before fresh queries, qualify the actual existing native power primitives in
one shared secant implementation: worksheet POWER (control), CRT pow, and the
raw Excel logarithm/exponential chain. A temporary private power callback lets
those countermodels share the same solver; it will be removed before landing.
This avoids substituting the host Python libm for Excel's internal primitive.
First use retained inputs as calibration, then require a fresh frozen holdout.

The shared native solver's CRT and raw-chain powers each match 88/131 versus
worksheet POWER 86/131, and neither fixes both captures. The native worksheet
control reproduces every prior model prediction. The temporary callback and
native diagnostic were removed; no power change lands.

One remaining shape ambiguity is the order of the initial secant pair. Swapping
it preserves the leading C^2 convergence error used in the prior identification,
but changes the rounded first step. Compare that swap and absolute/relative
or rate-space offset forms, always keeping the same 0.001 value, same direction
controls and unchanged stop. These are retained-case calibrations; a selected
initialisation still requires a new independent frozen query before landing.

The retained-case initialisation sweep rejects all alternatives: the current
absolute offset/order scores 86/131; swapping it scores 53/131, and every other
offset/order scores at most 79/131. Some alternatives match roma in isolation,
but none fixes D39. No IRR change lands from these calibrations.

Raw diagnostics and rejected candidates: R2 dnacalc-evidence/research/round55/
20261112-fn2-ttest-0619/ (101 files, 197,186,509 bytes for the full batch;
checksum copy and one-way check, zero differences). No IRR change landed.
