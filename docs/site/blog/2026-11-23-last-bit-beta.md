# Deleting six patches across 29 statistical functions
Tags: Excel parity, last bit, statistics, incomplete beta
Status: draft

Twenty-nine Excel worksheet functions—`BETA.*`, `BINOM.*`, `NEGBINOM.*`, `F.*`, `T.*`, and `CONFIDENCE.T`—all call one underlying mathematical kernel: the regularized incomplete beta function `I_x(a, b)` and its inverse. When an implementation does not yet match that kernel's real overflow checks and inner loops, patches accumulate around the edges.

By the morning of November 22, our incomplete-beta path carried five hard-coded `#NUM!` bounding boxes, a tiny-second-shape override, and inherited TOMS 708 inner loops—and still missed Excel on `20,467` of `192,972` bank rows.

## The insight: real stopping rules beat hard-coded fences

Across four landings in one day (`a423c5c0d`, `8ac7c9e8d`, `b0c75eb2e`, and `9b35d0fce`), replacing guesswork with Excel's actual solver guards and loop formulations cut those misses nearly in half while deleting more boundary patches than we added:

- **One-ULP cliffs from subnormal flushes and exhaustion (`+1,220 / -0`):** Call `BETA.INV(0.49, 0.001, b)`. At `b = 1.0`, `b0 <= 1` sends the subnormal initial seed `x0` straight into the power series `bpser`, returning `0.0`. Nudge `b` by a single bit to `1 + 2^-52`, and `bratio` enters `b0 > 1, x0 < 0.1`, flushes `x0 * b0` to `0.0`, and flips immediately to `#NUM!`. Yet at `b = 2.5` (`p = 0.475478035806142`, `a = 0.00105`), `x0 * 2.5` crosses back above `2^-1022` and Excel returns `0.0` again (`54 / 54` on probes and holdouts). Together with `0.5` overflow seeding, subnormal derivative zeroing, and a `700`-trial `best_err <= 1e-12 * p` threshold (`48 / 48` on unseen holdouts), those rules let us delete all five `#NUM!` bounding boxes and the tiny-shape patch (`166` insertions, `270` deletions).
- **80-bit extended precision inside `bgrat` (`+4,321` net):** For large/small shape pairs, rounding `(a + b - 0.5) * ln(1 + b/a)` to 53 bits before subtracting `b` throws away `11` to `27` bits of significand. Keeping the asymptotic recurrence in `Ext80` (`excel_beta_bgrat`) and storing `u * summ` to `binary64` before adding `w` beat the unspilled rival `118 / 120` vs `109 / 120` (`24 / 24` vs `18 / 24` on unseen `H5`) and cleared `139 / 139` `bgrat` rows in the Windows `ww2` suite.
- **Modified Lentz instead of Wallis `bfrac` (`+4,495` net):** For interior large shapes, Excel does not run TOMS 708's Wallis recurrence (`83 / 586` on probes). It evaluates DiDonato & Morris Eq. (2.8) inside the same `PC = 64` Modified-Lentz template used for gamma (`TINY = 1.0e-30`, `STOP_TOL = 10.0 * f64::EPSILON`, `alpha = st(st(nox * x) * x)` left-to-right, and an `FST` `n * (b - n) * x` term). Checking `!nox.is_finite()` before multiplying by `x` twice reproduces `24 / 25` `#NUM!` edges without a separate range guard.

## What moved—and what stays open

- **29-caller full witness bank (`192,972` rows):** Exact matches rose from **`172,505` to `182,814`** (**`+10,309` net**, `94.74%` exact), cutting misses from `20,467` to `10,158` with zero functions regressed at any stage (reaching `8,592 / 15,845` on `BETA.DIST`, `7,220 / 7,636` on `BETA.INV`, `71,044 / 71,362` on `BINOM.DIST`, and `5,470 / 5,487` on `T.DIST.2T`).
- **Frozen suites (`cb18`):** `Probe 1` rose from `25 / 200` baseline (`75 / 200` prior HEAD) to `178 / 200` exact; the unseen SHA-256 holdout rose from `23 / 240` (`89 / 240` prior HEAD) to `187 / 240` exact (`H4` `T.DIST*` `48 / 48`, `H5` `bgrat` `24 / 24`), confirmed natively, under `soft-x87`, and on Win64 (`ww7`).
- **Still open (`10,158` misses):** Now that `bfrac` and `bgrat` match Excel, remaining `1..5`-ULP and `11..58+`-ULP offsets trace upstream to `beta_density_at`, `excel_ln_beta` (`a, b >= 8`), and `bup` store ordering under `dc-j9k.55` (along with Windows sweep and `perf` parity checks), plus caller-specific sibling beads (`dc-j9k.5`, `.44`, `.70`, `.73`–`.76`, and `.86`).

[Read the full incomplete-beta article](/articles/last-bit-beta/) (`docs/articles/last-bit-beta.md`).
