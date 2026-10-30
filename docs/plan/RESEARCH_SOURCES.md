# Sources for algorithm research and function implementation

What an agent may read, use or port when working out how Excel computes a function, and how
facts from restricted sources may reach an implementation. It applies to calc-func work, the
smart-fuzzer and every research work unit (dc-ymjf.4). Copyright protects how code is written
(text, structure, comments); it does not protect algorithms, mathematical facts, published
coefficient tables or what Excel observably does.

## Source tiers

| Tier | Sources | Use |
|---|---|---|
| 1. Free to read | Abramowitz & Stegun (US government work), DLMF, Hart et al. *Computer Approximations*, Cody's and other published papers, Intel SDM and IEEE 754, textbooks' mathematics, Excel's own outputs (captures, witness banks, COM probes) | Anyone, including clean-room implementers. Cite the source. |
| 2. Permissive code | Boost.Math (BSL-1.0), fdlibm / FreeBSD msun / openlibm, Arm optimized-routines (MIT), AMD libm and AOCL-LibM (MIT), CORE-MATH, SciPy cdflib (public domain), SLATEC and Fullerton's FNLIB (US government, public domain), Cody's SPECFUN (netlib) | Read, and port with the notice its licence asks for; add the ledger entry (`migration/licence-ledger.json`) and the notice (`deploy/notices`). Search SLATEC and SPECFUN first: they are public-domain code from the same literature Excel's 1990s functions plausibly drew on. |
| 3. Facts only | Numerical Recipes, R nmath, glibc libm, ACM TOMS code other than what calc-func already keeps | Never copied, ported or kept in our repositories. Only an analyst (below) reads them, and only facts leave. |
| 4. Not used | Microsoft's NT/CRT source (leaked), disassembly or decompilation of Excel or Microsoft runtime DLLs | Nobody reads or uses them. Calling a DLL and measuring its outputs is black-box measurement and is fine. |

The ACM TOMS 654/708 (incomplete gamma and beta) and AS 241 code already in calc-func stays, with
its source comment, and may be changed like our own.

## Two roles for tier 3

1. **Analyst** (may read tier 3): writes a specification of facts, with no code in it: the method
   family and its published origin (for example "A&S 9.4.1-9.4.6 polynomials; Miller's backward
   recurrence above order 2"), thresholds and constants as numbers, and hypotheses to test
   against Excel. Every claim names a tier-1 or tier-2 source or an Excel measurement. No
   variable names, control structure, comments or text from the restricted source.
2. **Check**: someone other than the analyst reads the specification for anything that reads like
   code and strikes it.
3. **Implementer** (never reads tier 3, nor the code being replaced or its history): works from
   the specification, tiers 1 and 2, and Excel's answers.

An agent takes one role per work unit. A unit's file names the role and the banned paths.

## Guiding the smart-fuzzer

- Seed candidate searches from tiers 1 and 2: method families, published coefficient sets (A&S,
  Hart, Cody, Boost, SLATEC), parameter grids; rank candidates by their fit to the witness banks.
- Learn the rest from Excel as a black box: where the error pattern changes (branch points),
  constants by lattice search, tie mining and double-rounding probes for the precision of
  intermediates (as "The Last Bit" did for EXP and LN).
- Every result carries a provenance line: the sources read (by tier), the model and the machine.
  A candidate without it does not land.
