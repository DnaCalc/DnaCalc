# Problem functions: misses beyond 4 ulp

The functions that are not yet at "only a few last bits left": every function with at least one full-bank row
or oracle-corpus cell that misses Excel by more than 4 ulp, or with a non-numeric mismatch, a refusal, an
excluded or an unread row. Functions whose every miss is within 4 ulp are not listed; they are the last-bit
long tail (`docs/generated/problem-functions.md` has that triage at 16 ulp, with corpus calls unwrapped from
INDEX).

Measured at 7a3e51cd on dna-koderbot (Linux x86_64):

- full bank: `cargo xtask witness judge --bank full --trace <jsonl>`: 3,910,795 rows, 3,889,493 matched,
  21,302 failed (2,064 of them excluded, 4 unread);
- corpus: `cargo xtask corpus --oracle`: 78,264 formula cells, 76,794 pass, 1,470 fail (606 mismatched,
  668 refused, 1 failed or missing).

**57 functions, 5,949 rows and cells**: 2,839 bank rows beyond 4 ulp, 2,148 bank rows non-numeric (80), excluded (2,064) or
unread (4), and 962 corpus cells (191 beyond 16 ulp, 103 non-numeric, 668 refused). Bank and corpus are separate
observations and may overlap. Five more functions have corpus cells only in the "2 to 16 ulp" bucket, which
the corpus summary does not split at 4: BETA.INV, BINOM.DIST.RANGE, T.DIST.2T, T.TEST, TTEST (Fable lane).

How the columns are counted (`docs/plan/problem-functions/problems.py` over the judge trace,
`problem_doc.py` for the table):

- **Bank > 4 ulp**: numeric rows whose expected and calculated doubles are more than 4 representable values
  apart. **Bank other**: non-numeric mismatches (text, error, type), rows the judge excluded (WEEKNUM's
  order-dependent rows, MINVERSE's unknown error outcomes, TEXTSPLIT spill layouts) and unread rows
  (POISSON.DIST: 4). Excluded and unread rows count as failures, as the judge counts them.
- **Worst ulp**: the largest distance among the bank rows beyond 4 ulp.
- **Corpus**: cells by the root-cause shape of `cargo xtask corpus` (`summary.json`): beyond 16 ulp
  ("differs within 15 significant digits" or more) / 2 to 16 ulp / non-numeric plus refused. Its function
  is the last executed call, so INDEX can stand for the expression it wraps (`dc-ivym` sorts those out).
- **Owner**: the open bead that owns the function's misses. BETA*, F* and T* functions belong to the Fable
  lane (beta/F/t family).

Ordered by bank rows beyond 4 ulp plus bank other plus corpus beyond 16 ulp, non-numeric and refused.

| Function | Bank rows | Bank failed | Bank > 4 ulp | Bank other | Worst ulp | Corpus > 16 / 2-16 / other | Owner |
|---|---:|---:|---:|---:|---:|---|---|
| WEEKNUM | 13332 | 1950 | 0 | 1950 | - | - | dc-zmk.2 |
| FDIST | 4399 | 3738 | 961 | 0 | 23 | 0 / 0 / 0 | dc-j9k.44 (Fable lane) |
| BETA.DIST | 2822 | 1779 | 626 | 0 | 170 | 1 / 0 / 0 | dc-j9k.45 (Fable lane) |
| YIELD | 533 | 436 | 414 | 0 | 2844 | 31 / 3 / 0 | dc-j9k.46 |
| FORECAST.ETS.SEASONALITY | - | - | 0 | 0 | - | 0 / 0 / 368 | dc-6332.5, dc-6332.9 |
| IRR | 734 | 185 | 181 | 2 | 1024 | 3 / 0 / 3 | dc-j9k.43 |
| FORECAST.ETS | - | - | 0 | 0 | - | 0 / 1 / 150 | dc-6332.5, dc-6332.9 |
| FORECAST.ETS.CONFINT | - | - | 0 | 0 | - | 0 / 2 / 150 | dc-6332.5, dc-6332.9 |
| MINVERSE | 2243 | 112 | 0 | 112 | - | - | dc-j9k.4 |
| ROUNDDOWN | 59995 | 102 | 100 | 0 | 10 | - | dc-j9k.39 |
| ROUNDUP | 52841 | 102 | 100 | 0 | 10 | - | dc-j9k.39 |
| TRUNC | 51317 | 100 | 100 | 0 | 10 | - | dc-j9k.39 |
| BETADIST | 294 | 151 | 47 | 0 | 170 | 10 / 1 / 0 | dc-j9k.45 (Fable lane) |
| DATE | 22250 | 0 | 0 | 0 | - | 0 / 0 / 47 | dc-ivym |
| BINOM.DIST | 70349 | 29 | 16 | 0 | 651 | 29 / 14 / 0 | dc-j9k.45 (Fable lane) |
| CUMPRINC | 586 | 356 | 4 | 0 | 12 | 39 / 61 / 0 | dc-j9k.49 |
| GAMMALN.PRECISE | 14278 | 3633 | 43 | 0 | 7 | - | dc-6332.7, dc-6332.3 |
| GAMMALN | 4538 | 2401 | 41 | 0 | 7 | - | dc-6332.7, dc-6332.3 |
| COMPLEX | 10694 | 40 | 0 | 40 | - | - | dc-b1l.25 |
| INDEX | - | - | 0 | 0 | - | 14 / 0 / 23 | dc-ivym |
| ODDFYIELD | - | - | 0 | 0 | - | 30 / 0 / 0 | dc-j9k.40 |
| CUMIPMT | 38 | 25 | 19 | 0 | 268435456 | 8 / 28 / 0 | dc-j9k.49 |
| ROUND | 36945 | 24 | 24 | 0 | 14 | 2 / 0 / 0 | dc-j9k.18 |
| TDIST | 92 | 64 | 26 | 0 | 24 | 0 / 2 / 0 | dc-6332.4, dc-6332.8 (Fable lane) |
| NEGBINOM.DIST | 510 | 60 | 21 | 0 | 183 | 1 / 0 / 0 | dc-j9k.45 (Fable lane) |
| GAMMA.INV | 204 | 133 | 19 | 0 | 55 | 0 / 0 / 0 | dc-6332.18 |
| POISSON.DIST | 49714 | 552 | 3 | 4 | 7 | 10 / 17 / 0 | dc-j9k.42, dc-6332.3 |
| HYPGEOM.DIST | 3661 | 89 | 12 | 0 | 22 | 4 / 0 / 0 | dc-j9k.48 |
| CHIINV | 100 | 64 | 14 | 0 | 91 | - | dc-6332.18 |
| GAMMA.DIST | 77975 | 161 | 13 | 0 | 8 | 0 / 0 / 0 | dc-6332.3, dc-6332.7, dc-j9k.5 |
| F.TEST | 78 | 52 | 11 | 0 | 11 | - | dc-6332.4, dc-6332.8 (Fable lane) |
| FINV | 32 | 29 | 9 | 0 | 41 | 0 / 0 / 0 | dc-6332.4, dc-6332.8 (Fable lane) |
| IMSINH | 1209 | 9 | 0 | 9 | - | - | dc-b1l.25 |
| Infix & | - | - | 0 | 0 | - | 0 / 0 / 9 | dc-b1l.25 |
| IMCOT | 1209 | 0 | 0 | 0 | - | 0 / 0 / 8 | dc-vdma |
| IMCSCH | 1209 | 8 | 0 | 8 | - | - | dc-b1l.25 |
| IMCOSH | 1209 | 7 | 0 | 7 | - | - | dc-b1l.25 |
| BINOM.INV | 2308 | 6 | 6 | 0 | 4503599627370496 | - | dc-j9k.41 |
| CRITBINOM | 1420 | 6 | 6 | 0 | 4503599627370496 | - | dc-j9k.41 |
| FORECAST.ETS.STAT | - | - | 0 | 0 | - | 6 / 4 / 0 | dc-6332.5, dc-6332.9 |
| TINV | 32 | 10 | 6 | 0 | 17 | - | dc-6332.4, dc-6332.8 (Fable lane) |
| CONCAT | 20 | 5 | 0 | 5 | - | - | dc-b1l.25 |
| CONCATENATE | 10 | 5 | 0 | 5 | - | - | dc-b1l.25 |
| HSTACK | - | - | 0 | 0 | - | 0 / 0 / 5 | dc-ivym |
| SORT | - | - | 0 | 0 | - | 0 / 0 / 5 | dc-zsn.2.4 |
| BETAINV | 62 | 44 | 4 | 0 | 12 | 0 / 2 / 0 | dc-6332.4, dc-6332.8 (Fable lane) |
| F.DIST | 369 | 189 | 4 | 0 | 8 | - | dc-j9k.44 (Fable lane) |
| IMSECH | 1209 | 4 | 0 | 4 | - | - | dc-b1l.25 |
| T.DIST.RT | 6 | 5 | 3 | 0 | 24 | 1 / 0 / 0 | dc-6332.4, dc-6332.8 (Fable lane) |
| CHISQ.INV | 40 | 26 | 2 | 0 | 20 | 1 / 0 / 0 | dc-6332.18 |
| TEXTSPLIT | 11 | 2 | 0 | 2 | - | 0 / 0 / 1 | dc-ejvk.3 |
| IMTAN | 1209 | 0 | 0 | 0 | - | 0 / 0 / 2 | dc-vdma |
| BESSELY | 46 | 0 | 0 | 0 | - | 1 / 0 / 0 | dc-j9k.16 |
| CHIDIST | 388 | 8 | 1 | 0 | 6 | - | dc-6332.3, dc-6332.7, dc-j9k.5 |
| FV | 2561 | 3 | 1 | 0 | 100663296 | - | dc-j9k.51 |
| GAMMA | 12603 | 4410 | 1 | 0 | 5 | - | dc-6332.7, dc-6332.3 |
| RATE | 377 | 1 | 1 | 0 | 1333068467558 | - | dc-j9k.51 |

