# F inverse coordinates (dc-bc1o), 2026-11-12

Codex, dna-koderbot. Existing DNA Calc code, defining beta/F identities and
public Excel outputs only. No Microsoft source or disassembly.

## Predictions before queries

The current F inverse bisects its published F surface directly. Excel's T
inverse instead publishes a beta-coordinate transform. Test the analogous F
shape independently: lower inverse uses z=BETA.INV(p,m/2,n/2), then
(n/m)*z/(1-z); right inverse uses z=BETA.INV(p,n/2,m/2), then
(n/m)*(1/z-1). Compare the reflected 1-p orientation, factor/division orders,
and ordinary versus stored-PC64 stages. Freeze coordinates before BETA.INV
queries and every composed prediction before F.INV/F.INV.RT/FINV queries.
These public compositions identify the caller graph separately from remaining
production beta-inverse errors. Include the captured-style p=2/15, m=6.6,n=8
control and independent integer/fractional-degree inputs.

Separate endpoint hypotheses: F.INV at p=0 returns zero and p=1 returns #NUM!;
F.INV.RT/FINV at p=1 return zero and p=0 return #NUM!, after valid degrees.
They are predictions, not assumed rules; invalid-degree controls stay explicit.
A weak fit rejects the transform, and no coefficients are fitted to answers.

The fresh public compositions are rejected: best lower inverse 34/128, right
inverse 43/128, with FINV and F.INV.RT identical. None fixes the lower inverse
control; several right-tail forms match its control, but that is not enough.
No F inverse formula or endpoint guard changes from these weak compositions.
