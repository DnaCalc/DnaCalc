# Question for GPT-6 Pro: how Excel's iterative financial solvers and discrete inverses really work (YIELD, IRR, CUMIPMT/CUMPRINC, BINOM.INV/CRITBINOM, the bond family)

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet that must reproduce Excel's
observable behaviour exactly (bit-identical binary64 results, the same error values). The repository
is public: https://github.com/DnaCalc/DnaCalc, MIT. This question is pinned to commit
`b2a808502f74e1590dcfc682171c8a1db1d93ee2`
(https://github.com/DnaCalc/DnaCalc/tree/b2a808502f74e1590dcfc682171c8a1db1d93ee2). Every path below is
relative to the repository root and every `file:line` refers to that commit. You may read the
repository if you can; everything you need is in the appendices, so do not depend on browsing.

The function kernels are in `crates/calc-func`. For each function we hold a bank of recorded Excel answers
(the "witness bank": inputs as doubles, Excel's answer as binary64 bits) from the oracle machine (AMD Ryzen 7
4800H, Windows x64, Excel 16.0, builds 20131, 20228, 20430 and 20527 across the banks). Our code is judged
by how many bank rows it reproduces bit for bit. The owner ranks **large differences above last-bit
differences**, so this question is about the functions where we are far off or return the wrong kind of
answer: YIELD (66 of 533 rows match, worst distance 2844 ulp), IRR (533 of 734, structural), CUMIPMT (13 of 38),
CUMPRINC (230 of 586), BINOM.INV (2290 of 2308) and CRITBINOM (1402 of 1420), the last two being off by one in the returned integer. It also
covers the bond functions that share YIELD's machinery and are only a few ulp off: DURATION (7707 of 7884), MDURATION
(1113 of 1260), PRICE (9103 of 9153).

**State of play (new baseline).** The full bank is now 3,889,129 of 3,910,795 rows. An earlier version of this question reported that
YIELD's near-par rows (price within about 1e-4 of par, settlement on a coupon date, redemption 100) are not a root of the price
function but a **closed-form rational seed** (a simple-interest annuity approximation of the yield) that Excel returns when the Newton step
at it is below 1e-10. That finding has **landed** (commit d44aaf0a, bead dc-ymjf.6): YIELD went from 16 to 66 of 533 rows, 50 gained, none lost; it
is described in B.3 as history, with the evidence. What is still open in YIELD is: (1) the evaluation order of the seed, which leaves
26 rows of the 3-period design one unit in the last place below Excel (B.4); (2) the general seed for off-coupon settlement and redemption other
than 100, which accounts for constant biases of 1e-16 to 2e-14 (B.5); (3) the derivative, iteration cap, failure result and the cash-scale
dependence. After YIELD, in order of importance: IRR, BINOM.INV/CRITBINOM, CUMIPMT/CUMPRINC, then the bond schedule functions
(PRICE, DURATION, MDURATION).

## What we ask

Please produce the following, in this order, covering the functions in the order YIELD, IRR, BINOM.INV and
CRITBINOM, CUMIPMT and CUMPRINC, then DURATION/MDURATION/PRICE.

**A. The most plausible algorithm Excel uses**: method (Newton, secant, bisection, hybrid, closed form plus polish); the starting
point or seed as a formula in the arguments; the update formula; the derivative (analytic or finite difference, with its
step); the tolerance and the convergence test (which quantity, absolute or relative, tested before or after the update, which
iterate is returned); the iteration cap and the result on non-convergence (error value or last iterate); special cases and
error rules. For the discrete inverses: the search order, the quantity compared with alpha, the comparison (`>` or `>=`)
and any tolerance. Give evidence for it **and against it** from the appendices, and say what is measured, what you infer
and what you guess.

**B. Ranked alternatives with discriminating experiments.** For each function the 2 to 5 next-best hypotheses, and for each
pair a concrete experiment: exact argument values (decimal doubles, hex where it matters) whose Excel answers would
separate them, with the outcome each hypothesis predicts. Include iteration-count-sensitive cases (arguments where the
hypotheses differ in how many iterations run) and cases that probe the stopping test directly (a price whose Newton step at the seed
is just below and just above 1e-10). We run experiments on an Excel answer service on the oracle machine (any function, any
double arguments, thousands of calls per round) and feed the answers back, so many cheap exact questions beat a few clever ones.

**C. An implementable specification for each function**: the exact computation (formula, evaluation order, constants, branch
thresholds, error mapping) in prose or pseudocode, so a Sonnet agent can implement it and we can test it against the recorded
rows. Say which bank rows you expect to move from mismatch to match and which might regress.

**D. Bold ideas**: for example using the boundary of non-convergence, the `#NUM!` frontier, the exact price at which a step crosses
the tolerance, or scale and ulp-ladder designs, to recover the iteration cap, tolerance, seed and derivative from
Excel's answers without seeing the code. Be inventive; mark each idea cheap, medium or expensive in Excel calls.

## Constraints that bind the answer

1. **Bit-exact parity with Excel is the goal.** The answer is judged by recorded Excel answers, nothing else. Excel's
   oddities are copied bit for bit; we do not want "improved" numerics.
2. **Source rules** (`docs/plan/RESEARCH_SOURCES.md`, tier table): Tier 1, free to read: A&S, DLMF, Hart et al., published
   papers, Intel SDM, IEEE 754, textbook mathematics, Excel's own outputs. Tier 2, permissive code that may be read and ported with a notice:
   Boost.Math, fdlibm/FreeBSD msun, openlibm, SLATEC, Cody's SPECFUN, SciPy cdflib and similar. Tier 3, **facts only**
   (Numerical Recipes, R nmath, glibc libm, other ACM TOMS code): you may state the method family, its published origin and
   constants as numbers, never code, structure, names or comments from them. Tier 4 (Microsoft source, disassembly or
   decompilation of Excel or Microsoft runtime DLLs) is never used. Black-box reasoning from Excel's outputs is the main tool and is
   always fine.
3. **Method: the "code shape" hypothesis.** Excel's numeric kernels are mostly simple hand-written C from the 1990s (double
   arithmetic, perhaps x87 habits, the Microsoft CRT's libm). Rank candidate computations by what someone then would plausibly have typed,
   find the shape first, then fit constants. A match percentage alone is not evidence of the right shape.
4. You cannot run anything. Ask for experiments; we run them.
5. The deliverable must be implementable: for each proposed change, the exact computation, not a description of the idea.

Microsoft's public descriptions (tier 1, recalled; please check): IRR iterates "until the result is accurate within 0.00001 percent", `#NUM!` after 20 tries; RATE `#NUM!` after 20 iterations not converging to 0.0000001; YIELD and ODDFYIELD use "the Newton method based on the formula used for PRICE" with up to 100 iterations. Our measurements do not fit the literal readings; treat them as hints.

## Reading the data

* A bank row is `args -> Excel's answer`. Numbers are IEEE binary64. "ulp" is the distance in representable doubles between
  Excel's answer and ours (0 = same bits). "Gross" means thousands of ulp or more, "last bit" 1 to a few ulp. A distance of 4e15 ulp
  just means a result near zero (1e-16) where the two answers differ in the 16th decimal of an absolute scale; relative error is given where it matters.
* Decimals are printed with 17 significant digits (round-trip exact); hex is the exact bit pattern, `0x` plus 16 digits.
* Provenance is in the row id; the YIELD rows are Excel builds 20228 and 20527; the IRR rows are 20131 (cache) and 20228/20527. No input in any bank has two
  different Excel answers, including inputs recorded on several builds.
* Some IRR bank rows repeat the same input from two sources: 734 rows are 517 distinct inputs. Counts below are rows unless marked "distinct".
* "x87" in our code means we emulate the x87 FPU: compute with a 64-bit significand, then round to 53 on each store
  (`excel_x87_add`, `excel_x87_sub`, `excel_x87_mul`, `excel_x87_div`). Our PRICE kernel reproduces Excel's bits with that staging
  and with Excel's integer-power and fractional-power staging; our YIELD solver does not use it yet (see B.6).
* Our own helper analysis (marked "analysis") was done in 40-digit arithmetic on the recorded rows, not in Excel. Anything labelled
  "measured" is a property of recorded Excel answers; anything labelled "inferred" is ours; "hypothesis" is unproven.

---


# Appendix A: where we stand (full bank, `crates/calc-func/witness-status.tsv`, commit `b2a80850`)

| function | bank rows | matched | worst distance (ulp) | class | note |
|---|---|---|---|---|---|
| YIELD | 533 | 66 | 2844 | gross | was 16 before d44aaf0a; 467 left: 33 one-unit rows near par, the rest are the unidentified general seed |
| IRR | 734 | 533 | 1024 | structural | 2 rows where Excel returns a number and we return `#NUM!` |
| CUMIPMT | 38 | 13 | 268435456 | gross | tiny-rate loans: cancellation in `count*PMT - CUMPRINC` |
| CUMPRINC | 586 | 230 | 12 | numeric | 356 misses, 206 of them 1 ulp |
| BINOM.INV | 2308 | 2290 | 1.0 vs 0.0 (integers) | gross | 18 misses, all exact CDF boundaries |
| CRITBINOM | 1420 | 1402 | same | gross | the same 18 inputs |
| DURATION | 7884 | 7707 | 3 | last bit | 177 misses, all off-coupon |
| MDURATION | 1260 | 1113 | 3 | last bit | 147 misses, same cause as DURATION |
| PRICE | 9153 | 9103 | 1 | last bit | 50 misses, off-coupon at bases 2 and 3 |
| RATE (related) | 377 | 376 | 1.3e12 (one row) | gross | RATE's solver is a secant that already matches 376 of 377 |
| NPER, NPV, XNPV, PRICEDISC, PRICEMAT, YIELDMAT (related) | 2509, 284, 3406, 2400, 3608, 1085 | all | 0 | exact | |
| XIRR, ODDFYIELD, ODDFPRICE, YIELDDISC | no rows in the status table | | | | ODDFYIELD has recorded corpus evidence, see B.8 |

How far off, in absolute and relative terms (over the mismatching rows; analysis): **YIELD** now: 33 near-par rows one unit in the last place (about 7e-17 absolute); the other designs have a constant bias, absolute 1.5e-16 to 2e-14, relative up to 4e-13 (up to 2844 ulp). **IRR**: absolute 2.8e-17 to 4.4e-16, median relative 1.4e-15, at most 2.2e-13 (the 1024-ulp root near 0.001); two rows are number against `#NUM!`. **CUMIPMT**: relative 1e-9 to 3.3e-8 on the tiny-rate loans (absolute 4e-19 to 7e-16), at most 1.7e-13 elsewhere. **CUMPRINC**: relative at most 1.3e-15. **BINOM.INV, CRITBINOM**: the integer is off by one on 18 exact-boundary rows. **DURATION, MDURATION, PRICE**: relative at most 4.4e-16.

Per-function structure follows. Appendix B (YIELD) is the one that matters most.

---

# Appendix B: YIELD (66 of 533; 16 before the near-par seed landed)

## B.1 Our code

YIELD's argument order is `YIELD(settlement, maturity, rate, pr, redemption, frequency, [basis])`; `rate` is the coupon rate, `pr` is the clean price
per 100 face, `redemption` the redemption value per 100 face. The coupon schedule is counted back from maturity, `n` coupon periods at once (never one period at a time: a coupon clamped to February 28 would otherwise turn later coupons into month ends).

The price function our YIELD inverts: `pcomp` calls `pcomp_disc(..., binexp = false)`, i.e. the C-runtime `pow` of the host, not the x87 power chain used by our PRICE (body of `pcomp_disc`, from its first statement):

```rust
// crates/calc-func/src/ported/bond_core_family.rs:340-388
 340      let coup = 100.0 * rate / c.frequency as f64;
 341      let e = dc(p.prev, p.next, c.basis, c.frequency)?;
 342      let a = dd(p.prev, c.settlement, c.basis)?;
 343      // Excel's internal discount/settlement fraction is the DERIVED complement
 344      // `dsc = E - A` (universal across bases), NOT the direct day-count span
 345      // settlement→next-coupon. For bases 0/1/4 the two coincide except on
 346      // settlement-on-31st 30/360 rows (which `E - A` also fixes); for Actual/360
 347      // (basis 2) and Actual/365 (basis 3) they differ materially (the ~cents
 348      // PRICE error, G6-03d). Excel's own COUPDAYSNC still PUBLISHES the actual
 349      // days at bases 2/3 — PRICE's internal DSC deliberately diverges from the
 350      // published COUP* function. (W109 G6-03d; b37/b38 live-Excel, build 20131.)
 351      let dsc = e - a;
 352      if yld <= -(c.frequency as f64) {
 353          return Err(derr(WorksheetErrorCode::Num));
 354      }
 355      let pw = |b: f64, ex: f64| {
 356          if binexp {
 357              excel_bond_pow(b, ex)
 358          } else {
 359              crt_pow(b, ex)
 360          }
 361      };
 362      let dirty = if p.n == 1 {
 363          let den = 1.0 + (yld / c.frequency as f64) * (dsc / e);
 364          if den <= 0.0 {
 365              return Err(derr(WorksheetErrorCode::Num));
 366          }
 367          (red + coup) / den
 368      } else {
 369          let base = 1.0 + yld / c.frequency as f64;
 370          if base <= 0.0 {
 371              return Err(derr(WorksheetErrorCode::Num));
 372          }
 373          let off = dsc / e;
 374          let mut pv = 0.0;
 375          for k in 0..p.n {
 376              pv += coup / pw(base, off + k as f64);
 377          }
 378          let redemption_discount = pw(base, off + (p.n - 1) as f64);
 379          let redemption_value = if binexp {
 380              excel_x87_div(red, redemption_discount)
 381          } else {
 382              red / redemption_discount
 383          };
 384          pv + redemption_value
 385      };
 386      let accr = coup * a / e;
 387      Ok((dirty - accr, dirty, coup, a, e))
 388  }
```

The solver `solve` is what every YIELD design not covered by the near-par seed still uses: bracket and bisect to adjacent doubles (this is wrong wherever Excel stops early):

(`solve`, `bond_core_family.rs:389-439`: lowest bracket end `-f + 1e-10`, `#NUM!` if the target exceeds the price there, upper end 1 doubled up to 80 times until the price is below target, then bisection on the double lattice until adjacent doubles, returning the end with the smaller residual.)

The landed near-par seed (`near_par_seed`, called only on a coupon date, `n > 1`, redemption exactly 100; it returns the seed when the Newton step there is under 1e-10, with the slope computed analytically from `crt_pow`), and its call site in `yield_kernel`, followed by the unchanged fall-through to `solve`. (`yield_kernel` also has an earlier `n == 1` closed form with x87 stores, which matches the bank, and a par shortcut returning `rate` for price 100 on a coupon date.)

```rust
// crates/calc-func/src/ported/bond_core_family.rs:577-607
 577  /// The near-par closed-form YIELD seed `f * i_a`,
 578  /// `i_a = (n cp - (price - R)) / ((price - R) (n + 1) / 2 + n R)`, returned when the Newton step
 579  /// `(P(y_a) - price) / P'(y_a)` at it is below 1e-10 (on a coupon date, so every discount
 580  /// exponent is a whole number of periods).
 581  fn near_par_seed(
 582      rate: f64,
 583      price: f64,
 584      red: f64,
 585      c: Ctx,
 586      p: Period,
 587  ) -> Result<Option<f64>, BondCoreEvalError> {
 588      let f = c.frequency as f64;
 589      let n = p.n as f64;
 590      let cp = 100.0 * rate / f;
 591      let d = price - red;
 592      let seed = f * ((n * cp - d) / (d * (n + 1.0) / 2.0 + n * red));
 593      if !seed.is_finite() || seed <= -f {
 594          return Ok(None);
 595      }
 596      let Ok((value, ..)) = pcomp(rate, seed, red, c, p) else {
 597          return Ok(None);
 598      };
 599      let base = 1.0 + seed / f;
 600      let mut slope = 0.0;
 601      for k in 1..=p.n {
 602          slope += k as f64 * cp / crt_pow(base, (k + 1) as f64);
 603      }
 604      slope += n * red / crt_pow(base, n + 1.0);
 605      slope = -slope / f;
 606      let step = (value - price) / slope;
 607      Ok((step.abs() < 1e-10).then_some(seed))
```

```rust
// crates/calc-func/src/ported/bond_core_family.rs:678-696
 678      // Near par on a coupon date with redemption 100, Excel returns the closed-form seed of its
 679      // Newton iteration when the first Newton step there is below 1e-10 (pro-9 solvers prompt,
 680      // B.3: the largest step among rows returning the seed is 8.8e-11, the smallest among rows
 681      // iterating on 1.17e-10; 68 bank rows). The seed approximates the annuity factor of n unit
 682      // payments at i = y/f by n / (1 + i (n + 1) / 2). Other designs (redemption not 100, off
 683      // a coupon date) carry a seed that is not identified yet and keep the solver below.
 684      if p.n > 1 && c.settlement == p.prev && red == 100.0 {
 685          if let Some(seed) = near_par_seed(rate_, price, red, c, p)? {
 686              return Ok(seed);
 687          }
 688      }
 689      // Solve over candidate yields with `pcomp` directly. `price_kernel` rejects
 690      // `yld < 0` via `rate(yld)`, but the root-finder must probe negative candidate
 691      // yields (its bracket runs down to `-frequency`); `pcomp`'s own guards
 692      // (`yld <= -frequency`, `base <= 0`) keep the domain correct (BUG-FUNC-031).
 693      solve(price, c.frequency, |cand| {
 694          Ok(pcomp(rate_, cand, red, c, p)?.0)
 695      })
 696  }
```

`price_kernel` (PRICE, which matches Excel on 9103 of 9153 rows) is the same function with `binexp = true` (Appendix F).

## B.2 The data and how it divides

The 533 rows are 384 + 120 rows from two designed discovery sets (builds 20228) and 29 rows from build 20527 (`r18-yield-par`). Earlier research rounds built
the designed sets around a hypothesised "seed" yield of 0.05: a family called **near-seed** has prices chosen so that the true yield is within about 2e-6 of 0.05 (the price at yield 0.05 and prices at dyadic and ulp-sized offsets around it), and **seed-family** has prices at other yields. Every
design is `YIELD(settlement, maturity, rate, price, redemption, frequency, basis)` with settlement either on a coupon date ("oncoupon") or between coupon dates ("offcoupon").
`n` is the number of coupon periods from the coupon date on or before settlement to maturity, A and E are the accrued and total days of the settlement period in the
convention of the basis.

| design (id prefix) | settlement | maturity | rate | red | f | basis | periods n | rows | matched | ulp min / median / max | mean (Excel - ours) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| near-seed-d-leap-b1-f2 | 2020-02-29 | 2025-01-01 | 0.04125 | 101.5 | 2 | 1 | 10 (A=59, E=182) | 48 | 0 | 1 / 21 / 52 | +2.39e-17 |
| seed-family-d-leap-b1-f2 | 2020-02-29 | 2025-01-01 | 0.04125 | 101.5 | 2 | 1 | 10 (A=59, E=182) | 18 | 0 | 1 / 11 / 47 | -3.47e-18 |
| r18-yield-par (29 rows, see B.4) | 44013..44197 | ..46753 | 0 .. 0.125 | 100 | 1,2,4 | 0,2,3 | 3..15 | 29 | 23 | 0 / 1 / 4395513236313605 (relative to ~1e-16 values) | - |
| near-seed-d-oncoupon-short-b0-f2 | 2020-07-01 | 2022-01-01 | 0.05 | 100 | 2 | 0 | 3 (A=0, E=180) | 48 | 13 | 0 / 1 / 66 | +2.46e-17 |
| near-seed-d-oncoupon-long-b0-f2 | 2020-07-01 | 2028-01-01 | 0.05 | 100 | 2 | 0 | 15 (A=0, E=180) | 48 | 30 | 0 / 0 / 86 | -1.94e-17 |
| seed-family-d-offcoupon-short-b0-f2 | 2020-08-15 | 2022-01-01 | 0.05 | 100 | 2 | 0 | 3 (A=44, E=180) | 18 | 0 | 1 / 16 / 36 | +7.02e-17 |
| near-seed-d-offcoupon-short-b0-f2 | 2020-08-15 | 2022-01-01 | 0.05 | 100 | 2 | 0 | 3 (A=44, E=180) | 48 | 0 | 1 / 18 / 71 | +1.72e-17 |
| seed-family-d-offcoupon-b2-f2 | 2020-09-20 | 2025-01-01 | 0.06 | 103 | 2 | 2 | 9 (A=81, E=180) | 30 | 0 | 26 / 70 / 156 | -5.24e-16 |
| seed-family-d-offcoupon-b3-f2 | 2020-09-20 | 2025-01-01 | 0.06 | 103 | 2 | 3 | 9 (A=81, E=182.5) | 30 | 0 | 21 / 77 / 363 | -5.26e-16 |
| near-seed-d-offcoupon-b2-f2 | 2020-09-20 | 2025-01-01 | 0.06 | 103 | 2 | 2 | 9 (A=81, E=180) | 48 | 0 | 16 / 72 / 118 | -4.83e-16 |
| near-seed-d-offcoupon-b3-f2 | 2020-09-20 | 2025-01-01 | 0.06 | 103 | 2 | 3 | 9 (A=81, E=182.5) | 48 | 0 | 17 / 74 / 124 | -5.02e-16 |
| near-seed-d-oncoupon-b4-f1 | 2021-01-01 | 2025-01-01 | 0.0325 | 97 | 1 | 4 | 4 (A=0, E=360) | 48 | 0 | 276 / 311 / 336 | -2.17e-15 |
| seed-family-d-oncoupon-b4-f1 | 2021-01-01 | 2025-01-01 | 0.0325 | 97 | 1 | 4 | 4 (A=0, E=360) | 6 | 0 | 423 / 450 / 472 | -3.11e-15 |
| seed-family-d-offcoupon-b0-f4 | 2021-02-15 | 2025-01-01 | 0.0875 | 105 | 4 | 0 | 16 (A=44, E=90) | 18 | 0 | 1137 / 1178 / 1252 | -1.64e-14 |
| near-seed-d-offcoupon-b0-f4 | 2021-02-15 | 2025-01-01 | 0.0875 | 105 | 4 | 0 | 16 (A=44, E=90) | 48 | 0 | 2628 / 2731 / 2844 | -1.89e-14 |

What the table says (ours is the new baseline): the on-coupon near-par designs now match except for one-unit rows; every other design is unchanged. Three kinds of difference:

1. **Near par, landed**: oncoupon-short (n=3) and oncoupon-long (n=15) at settlement 2020-07-01, rate 0.05, redemption 100, basis 0, f=2. Before d44aaf0a we were up to 9e6 and 1.3e7 ulp off (relative 1.3e-9 and 1.8e-9). Now 13 of 48 and 30 of 48 match; the 18 long and 9 short rows that remain apart from the 26 below are the rows where Excel iterates to the root and we are within 90 ulp (the same last-bit noise as item 3). B.3 is the evidence, B.4 the 26 open rows.
2. **Constant bias** per design, roughly independent of the price: 2e-16 to 2e-14 in the yield (tens to thousands of ulp), always Excel lower than the true root. B.5 shows this fits the same skeleton.
3. Pure last-bit noise (1 to 70 ulp, random sign): the leap-year design and offcoupon-short, and the basis 2 and 3 designs apart from their constant bias.

## B.3 History (landed in d44aaf0a): near par, Excel's answer is a closed-form rational function of the price, and it switches to the true root at a Newton step of about 1e-10

Consider the designs with settlement on a coupon date, `redemption = 100`, `rate = 0.05`, basis 0, f=2, so `P(0.05) = 100` exactly. Let `n` be the number of remaining coupon
periods, `T = n/f` (years), `tbar = (n+1)/(2*f)` (the mean time of the coupons, in years), `x = (price - 100)/100`. **Analysis**: define

```
y_a(price) = (rate*T - x) / (T + x*tbar)
```

(the exact algebra: price is approximated by `100 + 100*(rate - y)*a` with the annuity factor `a ~ T/(1 + y*tbar)`; the classic approximation `a_n ~ n/(1 + i*(n+1)/2)` of the annuity factor
of n unit payments at periodic rate `i = y/f` is what produces `T/(1 + y*tbar)`; solving that equation for `y` gives the rational function above.)
For general redemption R (still on a coupon date, `cp = 100*rate/f`, `i = y/f`) the same approximation of the annuity factor, with `1/(1+i)^n = 1 - i*a_n` used for the redemption, gives
`price ~ R + a_n*(cp - R*i)` and so `i_a = (n*cp - (price - R)) / ((price - R)*(n+1)/2 + n*R)`, `y_a = f*i_a`. At `R = 100` this is the rational function above.

The tables below use the current baseline for the "ours minus Excel" column (the old bisection was 10^2 to 10^7 ulp off on the seed rows; it is now within a few ulp). They give, for every row of the two designs with a price different from 100: the price offset, Excel's yield (decimal and bits), `y_a` computed in 40-digit arithmetic and rounded to a double, minus Excel's
yield in ulp ("closed form minus Excel"), our answer minus Excel's in ulp, and the absolute Newton step `(P(y_a) - price)/P'(y_a)` at the closed-form seed (exact analytic derivative, 40 digits).

**oncoupon-short (n = 3, T = 1.5, tbar = 1)**, settlement 44013 (2020-07-01), maturity 44562 (2022-01-01), rate 0.05, redemption 100, f=2, basis 0:

| price - 100 | Excel y | Excel hex | closed form y_a minus Excel (ulp) | ours minus Excel (ulp) | Newton step at y_a |
|---|---|---|---|---|---|
| -0.000732421875 | 0.0500051289878643 | 0x3fa99a45b34530e8 | -2.9e+8 | -8.0 | 2.01e-9 |
| -0.0003662109375 | 0.050002564487568922 | 0x3fa999efa6616713 | -1.45e+8 | -51.0 | 1.0e-9 |
| -0.000244140625 | 0.050001709656965168 | 0x3fa999d2f7710682 | -9.65e+7 | 22.0 | 6.7e-10 |
| -0.00018310546875 | 0.050001282242193315 | 0x3fa999c49ffa009a | -7.24e+7 | 7.0 | 5.02e-10 |
| -4.57763671875e-05 | 0.050000320560250403 | 0x3fa999a45b310ba2 | -1.81e+7 | -66.0 | 1.26e-10 |
| -2.288818359375e-05 | 0.05000016021730961 | 0x3fa9999ef9db3095 | -0.488 | -1.0 | 6.28e-11 |
| -1.52587890625e-05 | 0.050000106811534308 | 0x3fa9999d2f1aa5dd | -0.262 | -1.0 | 4.19e-11 |
| -1.1444091796875e-05 | 0.050000080108648697 | 0x3fa9999c49ba61a7 | -0.548 | -1.0 | 3.14e-11 |
| -7.62939453125e-06 | 0.050000053405764439 | 0x3fa9999b645a1e34 | -0.0996 | 0.0 | 2.09e-11 |
| -5.7220458984375e-06 | 0.05000004005432282 | 0x3fa9999af1a9fcc4 | 0.025 | 0.0 | 1.57e-11 |
| -3.814697265625e-06 | 0.050000026702881548 | 0x3fa9999a7ef9db86 | -0.917 | -1.0 | 1.05e-11 |
| -1.9073486328125e-06 | 0.050000013351440609 | 0x3fa9999a0c49ba78 | -0.925 | -1.0 | 5.23e-12 |
| -1.3969838619232178e-09 | 0.050000000009778896 | 0x3fa9999999af1aa1 | -0.856 | -1.0 | 3.83e-15 |
| 1.3969838619232178e-09 | 0.049999999990221117 | 0x3fa9999999841894 | -0.144 | -1.0 | 3.83e-15 |
| 1.9073486328125e-06 | 0.049999986648559744 | 0x3fa9999926e978ee | -0.141 | -1.0 | 5.23e-12 |
| 3.814697265625e-06 | 0.049999973297119825 | 0x3fa99998b4395873 | -0.349 | -1.0 | 1.05e-11 |
| 5.7220458984375e-06 | 0.049999959945680246 | 0x3fa9999841893829 | -0.623 | -1.0 | 1.57e-11 |
| 7.62939453125e-06 | 0.049999946594241 | 0x3fa99997ced9180f | 0.0364 | 0.0 | 2.09e-11 |
| 1.1444091796875e-05 | 0.049999919891363542 | 0x3fa99996e978d870 | -0.844 | -1.0 | 3.14e-11 |
| 1.52587890625e-05 | 0.049999893188487431 | 0x3fa9999604189993 | 0.00933 | 0.0 | 4.19e-11 |
| 2.288818359375e-05 | 0.049999839782739294 | 0x3fa9999439581e26 | -0.0812 | 0.0 | 6.28e-11 |
| 4.57763671875e-05 | 0.049999679439948728 | 0x3fa9998ed80297ab | 1.81e+7 | -10.0 | 1.26e-10 |
| 0.000244140625 | 0.049998290348691146 | 0x3fa999603bce9cea | 9.65e+7 | -10.0 | 6.7e-10 |
| 0.0003662109375 | 0.049997435525158063 | 0x3fa999438cedc8c8 | 1.45e+8 | -39.0 | 1.0e-9 |
| 0.000732421875 | 0.049994871063042595 | 0x3fa998ed805df454 | 2.9e+8 | 11.0 | 2.01e-9 |

**oncoupon-long (n = 15, T = 7.5, tbar = 4)**, maturity 46753 (2028-01-01), same other arguments:

| price - 100 | Excel y | Excel hex | closed form y_a minus Excel (ulp) | ours minus Excel (ulp) | Newton step at y_a |
|---|---|---|---|---|---|
| -0.00073242187494315658 | 0.050001183107531391 | 0x3fa999c14c6a934a | -1.62e+9 | 86.0 | 1.12e-8 |
| -0.00036621093744315658 | 0.050000591552478338 | 0x3fa999ad72ff41ba | -8.09e+8 | 38.0 | 5.61e-9 |
| -0.00024414062494315658 | 0.050000394368032969 | 0x3fa999a6d5321364 | -5.39e+8 | -4.0 | 3.74e-9 |
| -0.00018310546869315658 | 0.050000295775917671 | 0x3fa999a3864bb8ad | -4.05e+8 | -12.0 | 2.81e-9 |
| -4.5776367130656581e-05 | 0.05000007394391881 | 0x3fa9999c14c5ff40 | -1.01e+8 | 31.0 | 7.02e-10 |
| -2.2888183536906581e-05 | 0.050000036971954633 | 0x3fa9999ad72fc9bd | -5.06e+7 | -28.0 | 3.51e-10 |
| -1.5258789005656581e-05 | 0.050000024647968405 | 0x3fa9999a6d530e44 | -3.37e+7 | 27.0 | 2.34e-10 |
| -1.1444091740031581e-05 | 0.050000018485975892 | 0x3fa9999a3864b0de | -2.53e+7 | 2.0 | 1.75e-10 |
| -7.6293944744065811e-06 | 0.050000012323983677 | 0x3fa9999a037653a3 | -1.69e+7 | -3.0 | 1.17e-10 |
| -5.7220458415940811e-06 | 0.050000009155273625 | 0x3fa99999e83e4276 | 0.489 | 0.0 | 8.77e-11 |
| -3.8146972087815811e-06 | 0.050000006103515661 | 0x3fa99999ce075f75 | 0.0093 | 0.0 | 5.85e-11 |
| -2.8610228923753311e-06 | 0.050000004577636696 | 0x3fa99999c0ebedf7 | 0.625 | 0.0 | 4.39e-11 |
| -1.396927018504357e-09 | 0.050000000002235083 | 0x3fa99999999e83d7 | 0.44 | 0.0 | 2.14e-14 |
| 1.3970407053420786e-09 | 0.049999999997764735 | 0x3fa999999994af42 | 0.346 | 0.0 | 2.14e-14 |
| 2.8610230060621689e-06 | 0.049999995422363261 | 0x3fa9999972474536 | 0.294 | 0.0 | 4.39e-11 |
| 3.8146973224684189e-06 | 0.049999993896484407 | 0x3fa99999652bd3c8 | 0.568 | 0.0 | 5.85e-11 |
| 5.7220459552809189e-06 | 0.049999990844726748 | 0x3fa999994af4f0f3 | 0.827 | 0.0 | 8.77e-11 |
| 7.6293945880934189e-06 | 0.049999987676017162 | 0x3fa999992fbce009 | 1.69e+7 | 23.0 | 1.17e-10 |
| 1.1444091853718419e-05 | 0.049999981514026189 | 0x3fa99998face8381 | 2.53e+7 | 32.0 | 1.75e-10 |
| 1.5258789119343419e-05 | 0.049999975352036041 | 0x3fa99998c5e02770 | 3.37e+7 | -17.0 | 2.34e-10 |
| 2.2888183650593419e-05 | 0.049999963028055545 | 0x3fa999985c036f31 | 5.06e+7 | -16.0 | 3.51e-10 |
| 4.5776367244343419e-05 | 0.049999926056121267 | 0x3fa999971e6d4a83 | 1.01e+8 | -35.0 | 7.02e-10 |
| 0.00024414062505684342 | 0.049999605633110976 | 0x3fa9998c5e03a3cb | 5.39e+8 | -42.0 | 3.74e-9 |
| 0.00036621093755684342 | 0.049999408450095068 | 0x3fa99985c0399a2c | 8.09e+8 | -8.0 | 5.61e-9 |
| 0.00073242187505684342 | 0.04999881690276433 | 0x3fa99971e6df43e3 | 1.62e+9 | 61.0 | 1.12e-8 |

Read this as follows (all analysis on recorded Excel answers):

* For the 38 rows of the short design and 30 of the long design where the Newton step at the seed is small, Excel's answer equals `y_a` to within one ulp (the "closed form minus Excel" column has absolute value at most 1.34 ulp for all of them; these are rounding differences between my 40-digit value and a double, which says nothing yet about Excel's evaluation order). The old bisection was 10^2 to 10^7 ulp away (the tables omit the rows with |price - 100| under 1e-9).
* In the 9 short and 17 long rows with larger step, Excel's answer is the true root to within about 90 ulp (the "ours minus Excel" column is then small).
* The seed `y_a` has error against the true root that is **linear in the price offset** with relative slope error 3.9e-4 (n=3) and 9.5e-3 (n=15), which is why the stopped Excel answers have the slope `-0.007000000` and `-0.001600000` against the price, instead of the true `-0.0070027` and `-0.0016153`: `0.007 = 1.05/150 = (1 + rate*tbar)/(100*T)`, `0.0016 = 1.2/750`.
* **The switch is a step threshold.** The largest `|Newton step at y_a|` among the rows where Excel returned `y_a` is 6.28e-11 (short) and 8.77e-11 (long); the smallest among the rows where Excel iterated on is 1.256e-10 (short) and 1.170e-10 (long). Both designs are consistent with a test `|step| < 1e-10` tested **before** the update (return the current iterate), which agrees with an independent, earlier measurement on ODDFYIELD (B.8).
* That the stop is on the **step size**, not on `|price residual|`, is shown by the two designs having different residuals at the switch: residual/slope is what is common.

So the skeleton, now implemented for this design: `y := y_a(price)`; loop { `step := (P(y) - price)/P'(y)`; if `|step| < 1e-10` return `y`; `y := y - step` }. The derivative is not measured (rows that stop do not depend on it; rows that continue converge to the root to a few ulp), and the iteration cap and failure result are not measured at all.

For `n == 1` (a single remaining coupon) our code already has a closed form that matches the bank (dc-j9k.31). For `n > 1` at par on a coupon date `y_a = rate` up to rounding, which is why the par shortcut matched most `price == 100` rows.

## B.4 Open: the seed's evaluation order (26 + 7 rows one unit off) and the par rows

The 29 `r18-yield-par` rows (Excel 20527) probe prices exactly 100 and 100 plus one ulp (1.42e-14) with `rate` 0, 0.05, 0.0575, 0.125 and f = 1, 2, 4, basis 0 (plus bases 2 and 3 at four rows). Price exactly 100 gives `rate` itself on 14 of the 17 `price == 100` rows (our par shortcut returns `rate`); on the other three Excel is one ulp away from `rate`, and they are informative: settlement 44013, maturity 44562, rate 0.0575, price 100, f=2 returns exactly the rate at basis 0 (row 012), one ulp **below** at basis 2 (row 024) and one ulp **above** at basis 3 (row 026), although on a coupon date the three bases have the same n, so a basis-dependent quantity (a day-count ratio in the seed, the first Newton step, or the price evaluation at the seed) is already in the first iterate. None of the simple floating-point variants `(rate*T)/T` with T = days/360, days/365, days/365.25 reproduces that pattern. At price 100 plus one ulp, rate 0 (zero-coupon, `T = n/f` = 1.5, 1.25, 3), Excel's answers are **exactly** `-(price-100)/(100*T)`: -9.4739e-17, -1.13687e-16, -4.73695e-17. That is one plain Newton step from the seed `y = rate = 0` with derivative `-100*T` (the exact slope there), or equivalently `y_a`. With the landed seed we now return those three values exactly (as the old bisection did not). The table (decimal and bits, our bits now, ulp):

| id | settlement | maturity | rate | price | f | basis | Excel y | Excel hex | ours hex | ulp |
|---|---|---|---|---|---|---|---|---|---|---|
| 009 | 44013 | 44562 | 0 | 100.00000000000001 | 2 | 0 | -9.4739031434680031e-17 | 0xbc9b4e81b4e81b4f | 0xbc9b4e81b4e81b4f | same |
| 011 | 44013 | 44562 | 0.050000000000000003 | 100.00000000000001 | 2 | 0 | 0.049999999999999913 | 0x3fa999999999998d | 0x3fa999999999998c | 1 |
| 012 | 44013 | 44562 | 0.057500000000000002 | 100 | 2 | 0 | 0.057500000000000002 | 0x3fad70a3d70a3d71 | 0x3fad70a3d70a3d71 | same |
| 024 | 44013 | 44562 | 0.057500000000000002 | 100 | 2 | 2 | 0.057499999999999996 | 0x3fad70a3d70a3d70 | 0x3fad70a3d70a3d71 | 1 |
| 026 | 44013 | 44562 | 0.057500000000000002 | 100 | 2 | 3 | 0.057500000000000009 | 0x3fad70a3d70a3d72 | 0x3fad70a3d70a3d71 | 1 |
| 013 | 44013 | 44562 | 0.057500000000000002 | 100.00000000000001 | 2 | 0 | 0.057499999999999905 | 0x3fad70a3d70a3d63 | 0x3fad70a3d70a3d63 | same |
| 015 | 44013 | 44562 | 0.125 | 100.00000000000001 | 2 | 0 | 0.1249999999999999 | 0x3fbffffffffffff9 | 0x3fbffffffffffff9 | same |
| 017 | 44105 | 44562 | 0 | 100.00000000000001 | 4 | 0 | -1.1368683772161601e-16 | 0xbca0624dd2f1a9fb | 0xbca0624dd2f1a9fb | same |
| 019 | 44105 | 44562 | 0.050000000000000003 | 100.00000000000001 | 4 | 0 | 0.049999999999999878 | 0x3fa9999999999988 | 0x3fa9999999999988 | same |
| 020 | 44105 | 44562 | 0.057500000000000002 | 100 | 4 | 0 | 0.057500000000000009 | 0x3fad70a3d70a3d72 | 0x3fad70a3d70a3d71 | 1 |
| 021 | 44105 | 44562 | 0.057500000000000002 | 100.00000000000001 | 4 | 0 | 0.057499999999999885 | 0x3fad70a3d70a3d60 | 0x3fad70a3d70a3d5f | 1 |
| 023 | 44105 | 44562 | 0.125 | 100.00000000000001 | 4 | 0 | 0.12499999999999988 | 0x3fbffffffffffff7 | 0x3fbffffffffffff7 | same |
| 001 | 44197 | 45292 | 0 | 100.00000000000001 | 1 | 0 | -4.7369515717340015e-17 | 0xbc8b4e81b4e81b4f | 0xbc8b4e81b4e81b4f | same |
| 003 | 44197 | 45292 | 0.050000000000000003 | 100.00000000000001 | 1 | 0 | 0.049999999999999961 | 0x3fa9999999999994 | 0x3fa9999999999993 | 1 |

Of the 29 rows, 23 now match; the six that do not are rows 003, 011, 021 (price 100 + 1 ulp; ours one unit below Excel) and 020, 024, 026 (price exactly 100, rates 0.0575; discussed above).

**The 26 rows.** In the 3-period design (settlement 44013, maturity 44562, rate 0.05, redemption 100, f 2, basis 0) every row where Excel returns the seed and we do not (26 of 39) is **exactly one unit in the last place, ours below Excel, never above**. (One row of the 15-period design, price 100 + 4.26e-14, is also one unit off, ours below: Excel `0x3fa9999999999990`, ours `0x3fa999999999998f`; the other 30 seed rows of that design match, and so do 13 of the 39 short ones.) The table:

| price - 100 | Excel hex | ours hex | ours minus Excel (units of the last place) |
|---|---|---|---|
| -2.288818359375e-05 | 0x3fa9999ef9db3095 | 0x3fa9999ef9db3094 | -1 |
| -1.52587890625e-05 | 0x3fa9999d2f1aa5dd | 0x3fa9999d2f1aa5dc | -1 |
| -1.1444091796875e-05 | 0x3fa9999c49ba61a7 | 0x3fa9999c49ba61a6 | -1 |
| -3.814697265625e-06 | 0x3fa9999a7ef9db86 | 0x3fa9999a7ef9db85 | -1 |
| -1.9073486328125e-06 | 0x3fa9999a0c49ba78 | 0x3fa9999a0c49ba77 | -1 |
| -1.3969838619232178e-09 | 0x3fa9999999af1aa1 | 0x3fa9999999af1aa0 | -1 |
| -6.9849193096160889e-10 | 0x3fa9999999a45a1e | 0x3fa9999999a45a1d | -1 |
| -4.6566128730773926e-10 | 0x3fa9999999a0c49d | 0x3fa9999999a0c49c | -1 |
| -3.4924596548080444e-10 | 0x3fa99999999ef9dc | 0x3fa99999999ef9db | -1 |
| -1.1641532182693481e-10 | 0x3fa99999999b645b | 0x3fa99999999b645a | -1 |
| -1.4551915228366852e-11 | 0x3fa999999999d2f3 | 0x3fa999999999d2f2 | -1 |
| -9.0949470177292824e-13 | 0x3fa9999999999d30 | 0x3fa9999999999d2f | -1 |
| -2.2737367544323206e-13 | 0x3fa9999999999a80 | 0x3fa9999999999a7f | -1 |
| -5.6843418860808015e-14 | 0x3fa99999999999d4 | 0x3fa99999999999d3 | -1 |
| -1.4210854715202004e-14 | 0x3fa99999999999a8 | 0x3fa99999999999a7 | -1 |
| 1.4210854715202004e-14 | 0x3fa999999999998d | 0x3fa999999999998c | -1 |
| 5.6843418860808015e-14 | 0x3fa9999999999961 | 0x3fa9999999999960 | -1 |
| 2.2737367544323206e-13 | 0x3fa99999999998b5 | 0x3fa99999999998b4 | -1 |
| 9.0949470177292824e-13 | 0x3fa9999999999605 | 0x3fa9999999999604 | -1 |
| 1.1641532182693481e-10 | 0x3fa999999997ceda | 0x3fa999999997ced9 | -1 |
| 3.4924596548080444e-10 | 0x3fa9999999943959 | 0x3fa9999999943958 | -1 |
| 1.3969838619232178e-09 | 0x3fa9999999841894 | 0x3fa9999999841893 | -1 |
| 1.9073486328125e-06 | 0x3fa9999926e978ee | 0x3fa9999926e978ed | -1 |
| 3.814697265625e-06 | 0x3fa99998b4395873 | 0x3fa99998b4395872 | -1 |
| 5.7220458984375e-06 | 0x3fa9999841893829 | 0x3fa9999841893828 | -1 |
| 1.1444091796875e-05 | 0x3fa99996e978d870 | 0x3fa99996e978d86f | -1 |

Our evaluation (code above): `cp = 100*rate/f`, `d = price - R`, `seed = f * ((n*cp - d) / (d*(n+1)/2 + n*R))`, all plain binary64. **Analysis** on the 39 + 31 recorded seed rows (the rows with |price - 100| under 3e-5 and 7e-6): this form gives 13 of 39 for n = 3 and 30 of 31 for n = 15. The variants `rate*100/f`, `100*(rate/f)`, `(n+1)/2*d` for `d*(n+1)/2`, `n*R + ...` swapped, `f*(...)` moved outside or inside, and the same with every operation stored through the x87 emulation (RN53(RN64(op))), all give exactly the same 13 and 30. The forms `(rate*T - x)/(T + x*tbar)` with `T = n/f`, `tbar = (n+1)/(2f)`, `x = (price-100)/100`, and the same on `x = d/R`, give 21 of 39 and 22 of 31 (plain and x87), i.e. better for n = 3 and worse for n = 15. No form tried matches both designs, so the evaluation order or the constants (for example a `T = 1.5` that arrives as `n/f` or as a day ratio, or the factor `(n+1)/2` applied to a different operand) differ from all of these.

## B.5 Inferred: the other designs fit the same skeleton, with a seed that is not yet identified

For the other designs Excel is close to the true root, with a **constant** bias per design (B.2 item 2). With the skeleton above, a bias that does not depend on the price is what Newton's quadratic convergence gives when the iterate returned is the
last one before a step under 1e-10: from a seed with error `e0`, `e1 = C*e0^2`, `e2 = C*e1^2` where `C = P''/(2P')` at the root, and the bias is the error of the returned iterate. Since the seed formula is smooth in the price and the near-seed prices span only 2e-6 in yield, the seed error, and so the bias, is nearly constant in a design. **Analysis**, one design per line (root from our 40-digit price function; "Excel minus true root" is the bias, "seed error needed" is what `e0` must be for two updates to leave that bias; the last column is the error of my annuity formula applied to the clean price with n = remaining coupons minus one plus the fractional period (A/E-based), the best of four variants I tried; in the four off-coupon designs the other three variants (clean price with integer n, dirty price with integer n, dirty price with fractional n) are 8 to 70 times worse, and for oncoupon-b4-f1 they coincide with it because A = 0):

| design | n | rate | red | f | basis | Excel minus true root | C = P''/(2P') | seed error needed if 2 updates | seed error of my annuity formula (A) |
|---|---|---|---|---|---|---|---|---|---|
| d-offcoupon-b0-f4 | 16 | 0.0875 | 105 | 4 | 0 | -1.85e-14 | -1.94 | +-0.000224 | 9.66e-5 |
| d-oncoupon-b4-f1 | 4 | 0.0325 | 97 | 1 | 4 | -2.19e-15 | -2.34 | +-0.000114 | -4.63e-5 |
| d-offcoupon-b2-f2 | 9 | 0.06 | 103 | 2 | 2 | -4.26e-16 | -2.25 | +-7.83e-5 | 4.97e-5 |
| d-offcoupon-b3-f2 | 9 | 0.06 | 103 | 2 | 3 | -4.34e-16 | -2.25 | +-7.86e-5 | 4.96e-5 |
| d-leap-b1-f2 | 10 | 0.04125 | 101.5 | 2 | 1 | -1.54e-16 | -2.53 | +-5.56e-5 | -2.69e-5 |

Reading: the needed seed errors are 1.6 to 2.5 times those of my generalisation, always the same order. That is a sign that the skeleton is right (a wrong-shaped seed would miss by orders of magnitude, and the other variants do) and that Excel's seed formula differs from my annuity approximation by a second-order term, an accrued-interest convention, or the treatment of the fractional first period. This is where we ask for your help. For oncoupon-b4-f1 (A = 0, so no fractional period and clean = dirty) the difference between 4.63e-5 and the needed 1.14e-4 is the cleanest evidence: with A = 0 only the formula for a redemption of 97 and a coupon of 0.0325 at n = 4, f = 1, basis 4 is in question.
(Caveat: "two updates" assumes an analytic derivative; a finite-difference derivative makes the convergence factor differ and changes the needed seed error. The designs of B.2 do not pin the derivative.)

Other facts about these designs that bear on a seed formula: the two off-coupon basis-2 and basis-3 designs have almost identical biases (-4.8e-16 and -5.0e-16) although their `E` differs (180 and 182.5 days) and their accrued fractions differ; the leap-year design (basis 1, settlement 2020-02-29, E = 182) has the smallest bias.

## B.6 Hypothesis: our current objective is also wrong in the last bits, independent of the seed

Even if the seed and stopping rule are found, Excel's objective is Excel's own price evaluation. Our PRICE kernel reproduces Excel's PRICE bits on 9103 of 9153 bank rows with the x87 staging and a specific power chain (Appendix F), but `pcomp` as used by the YIELD solver still uses the host's `pow` (`binexp = false`). A tracker note says swapping the price under the solver "would regress the current witnesses"; with the correct solver that statement should be re-examined.

## B.7 What earlier research rounds found, and what they could not see

* R34: "1600 fixed Newton programs refute; 252 exact-dyadic pairs return identically but only 39 roots are exact per surface. YIELD money scaling changes roots and NUM outcomes; 1143 fixed negative-root cases yield 698 numbers and 445 NUM. This refutes a homogeneous relative-stop premise, without identifying seed, derivative or tolerance."
* R35: "RATE-stop, transformed secant, bracket and fixed-count candidates refute. Adjacent-price jumps constrain termination near 1e-10, but no routine is identified" (the 1e-10 we now see in B.3). R41: "48 adjacent price edges did not identify an iteration count or stop rule."
* R44/R46 (`yield-stop.txt`): scale designs (price and redemption multiplied by 2^k, zero coupons, settlement on a coupon date; 45 designs; 5,005 calls). Every design changes under scaling. For n=2, f=1, price 25, redemption 100 the true root is exactly 1; Excel returns exactly 1 at cash exponent -500, `0x3feffffffffc4e9a` (2.69e-11 below 1) at exponent 0 and `0x3feffffffff7cc5c` (5.97e-11 below) at +500, with the transition around ordinary face amounts; at small cash, price 25 returns exactly 1.0 and price 80 (n=2, f=1) returns the **negative** root -2.118. 100 fixed or cash-dependent seed programs and 150 expanding-bracket programs, all under analytic Newton, fail a 16-ulp necessary-membership check.
* R49: "all real inverse branches to 24 steps recover no common seed under analytic Newton"; the start "depends on the price". The closed-form seed of B.3 is such a start; the search did not include it.
* B.3 explains the near-par rows. It does **not** by itself explain the scale experiments: the rational seed is homogeneous in (price, redemption) for zero coupons and the step test is a ratio, so both are scale-free. Something in Excel's iteration is not (an absolute tolerance on the price residual, a finite-difference derivative with an absolute step, an absolute constant in a safeguard of the seed). Please say what, using these numbers. Under analytic Newton in `z = 1 + y/f` with `r = price/redemption` for a zero-coupon n-period bond, `z_next = ((n+1)*z - r*z^(n+1))/n` (R49); for n = 2 the inverse of this map is cubic, so the negative root can be reached.

## B.8 ODDFYIELD has the same 1e-10 (not in the bank; research note `odd-first-yield.txt`)

On 336 random odd-first-period bonds Excel's ODDFYIELD stops short of the true root on 328; **residual divided by the price slope is below 1e-10 on every one (largest 9.85e-11), spread log-uniformly down to 1e-15**; 321 of 336 lie below the root (Newton from the left on a convex price). For a zero-coupon design (T = 24.6111; 100 prices from 60 to 96.6) the answer is the root to 1e-16 up to price 61.11, short by 8.9e-11 from 61.48 shrinking to the root's noise by 83, and a second band starts at 92.19 (8.5e-11): "each band is one Newton step that would be just under 1e-10, not taken". No constant start (0 to 0.2 in 5e-4 steps, analytic or difference slopes, stops 1e-9 to 1e-12, test before or after) reproduced the 100 answers. R49: ODDFYIELD and YIELD agree bit for bit on 6,006 zero-coupon scale-scan questions, so presumably one iteration. The corpus bond ODDFYIELD(39763, 44256, 39736, 39873, 0.0575, 84.5, 100, 2, 0) = `0x3fb3c65d22ec15b6` (ours `0x3fb3c65d22eca7af`); Newton run backwards gives the reconstructed start 0.0401383104 (not the rate 0.0575), from which forward Newton with a slope step of 1e-7 lands within 8 ulp of Excel's answer.

## B.9 Questions for YIELD

1. **The seed's evaluation order.** Give an expression (association, operand grouping, where `f`, `T`, `tbar`, `n`, `R` enter, which operations are stored) that reproduces Excel's bits on all 39 + 31 seed rows, including the 26 in the table (Excel always one unit above ours in the 3-period design) and the 6 `r18` rows. Use the data in B.3, B.4 and the tables; state the predicted outcome on a few new inputs we can ask.
2. **The general seed.** Off-coupon settlement and redemption not 100 (B.5): a seed formula that reproduces the needed seed error in each design of the bias table (with a stated derivative). Think about what a 1990s programmer would type: "approximate yield" start formulas with accrued interest, odd periods and redemption not at par.
3. The derivative (analytic duration formula or difference; step; units of the 1e-10 test), the iteration cap (100?), the result when it does not converge (last iterate or `#NUM!`), and what produces `#NUM!` for a price with no yield (we return `#NUM!` above the price at -frequency; Excel's rule is unknown).
4. Reconcile with the scale experiments (B.7).
5. Which price function does Excel iterate on? The bias in B.5 is 1e-14 in yield; last-bit parity also needs the objective (B.6).

---

# Appendix C: IRR (533 of 734 rows) and its sibling RATE

## C.1 Our code

`IRR(values, [guess])`: `irr_kernel` validates the flows, then runs a secant method in the discount factor `v = 1/(1+rate)`, with every operation stored through the x87 emulation, and POWER's integer-power staging for the objective. Its stop is RATE's. The doc comment records a measurement made from the bank (not a port): over guesses at dyadic distance `d` from the root, the answer's error is `K*d` with `K/C^2 = 1.000e-6` over 15 sets of flows, `C = P''/(2P')` of the polynomial in `v` (in `1 + rate` the same ratio spreads by ten times); two flows are linear in `v`, which is why their error does not depend on `d`.

```rust
// crates/calc-func/src/ported/cashflow_rate_family.rs:475-513
 475  fn irr_discount_secant(cashflows: &[f64], guess: f64) -> Option<f64> {
 476      use crate::ported::excel_numeric::{
 477          excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul, excel_x87_sub as sub,
 478      };
 479      use crate::ported::power_fn::power_kernel;
 480      let npv = |v: f64| {
 481          let base = div(1.0, v);
 482          cashflows
 483              .iter()
 484              .enumerate()
 485              .try_fold(0.0, |sum, (k, &flow)| {
 486                  power_kernel(base, k as f64)
 487                      .ok()
 488                      .map(|power| add(sum, div(flow, power)))
 489              })
 490      };
 491      let mut before = div(1.0, add(1.0, guess));
 492      let mut value_before = npv(before)?;
 493      if value_before == 0.0 {
 494          return Some(guess);
 495      }
 496      let mut current = add(before, 0.001f64.copysign(value_before));
 497      let mut value = npv(current)?;
 498      for _ in 0..200 {
 499          let change = sub(value, value_before);
 500          if change == 0.0 {
 501              return None;
 502          }
 503          let next = sub(current, div(mul(value, sub(current, before)), change));
 504          let step = sub(next, current).abs();
 505          (before, value_before) = (current, value);
 506          current = next;
 507          value = npv(current)?;
 508          if step < 1e-7 && value.abs() < 1e-7 {
 509              return Some(sub(div(1.0, current), 1.0));
 510          }
 511      }
 512      None
 513  }
```

RATE (`financial_time_value_family.rs:457-517`), which matches the bank on 376 of 377 rows, has the same structure on the rate itself: second point = guess + 0.001 against the sign of the balance, secant update, stop `|step| < 1e-7 && |balance| < 1e-7`, 200 steps, then `#NUM!`; a zero balance at the guess returns the guess, a zero secant denominator returns `#NUM!`. IRR no longer uses the older Newton-plus-plateau machinery in the same file (the 2026-10-30 tracker round below describes it; XIRR still uses it).

## C.2 The data

734 rows, 517 distinct inputs (217 inputs appear twice: the 20131 cache rows repeat inputs of the 20228/20527 question sets). Sets:

* `irr-discovery-d00 .. d19` (300 rows): 20 designs of 2 to 8 flows (magnitudes from 1e-150 to 1e150 are included), each asked with 15 guesses: 0, 0.1, 0.5, 2, -0.25, -0.75, and the root with single-bit mantissa offsets ("m46 .. m28" below the root by about 1e-3, 1.5e-5, 2.3e-7, 3.7e-9; "p28 .. p46" above by the same; "root" the root itself). The matrix below shows `ok` (same bits) or `uN` (we differ by N ulp of the rate; N = 8, 16, 32 means 1, 2, 4 ulp of v near 0.9, since the rate is `1/v - 1` and a rate near 0.1 has ulp 1.4e-17 against v's 1.1e-16).
* family A (flows -1000, 300, 420, 680, root 0.1634...) and family B (flows -1000, 1210, root exactly 0.21) with about 100 guesses each (spanning -0.5 to 5, and "ladders" of guesses within a few ulp of the root), 20 more small sets.

Distinct-input mismatches: 143 of 517 (design set 85 of 300, family B 32 of 117, family A 26 of 100). The distance histogram over distinct mismatching inputs (numeric answers): 1 ulp: 2, 2: 1, 3: 3, 4: 4, **8: 76**, 9: 1, 10: 1, **16: 25**, **32: 16**, 70: 1, 128: 2, **1024: 9**. In other words almost every miss is 1, 2 or 4 ulp of `v` (the final step `1/v - 1` amplifies the unit in `v` to 8 ulp in the rate), and the 1024 is a root near 0.001 (flows -1000, 1000, 1).

Two rows are the "wrong kind of answer": Excel returns a number, we return `#NUM!` (200 secant steps without the step and the NPV both under 1e-7):

* design d17, flows `[-1000, 1, 1, 1, 1, 1200]`, guess 2.0 (`0x4000000000000000`): Excel `0x3fa3670ea5237340` = 0.037895639093881157; the root is 0.0378956390932830, so Excel stopped **6e-13 away from the root**.
* design d19, flows `[-1000, 0.001, 0.01, 0.1, 1, 10, 100, 1000]`, guess 2.0: Excel `0x3f8f91595cbb3a00` = 0.015413950102268537; root 0.0154139501021752 (error 9e-14).

(The 30 rows where both Excel and we return `#NUM!` are designs d11 and d14, flows of magnitude 1e149 and 1e122 to 1e123, at all 15 guesses; our code's note says "the NPV test is why flows of 1e153 are `#NUM!`: their NPV has no value that small to land on".)

Matrix (rows = designs; entries per guess label, labels as described above):

| design | flows (decimal) | far_zero | far_p010 | far_p050 | far_p200 | far_n025 | far_n075 | m46 | m40 | m34 | m28 | root | p28 | p34 | p40 | p46 | matched |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| d12 | -1e-150, 2.5e-151, 3.25e-151, 6.75e-151 | u16 | ok | u16 | ok | ok | ok | u32 | u16 | u16 | u16 | ok | ok | ok | u16 | u16 | 7/15 |
| d04 | -100, 10, 20, 30, 60 | ok | u32 | ok | ok | ok | ok | u32 | ok | ok | ok | ok | u32 | u32 | ok | ok | 11/15 |
| d02 | -777.25, 250.5, 350.75, 450.125 | ok | ok | u8 | ok | ok | ok | ok | ok | ok | ok | u8 | u8 | u8 | ok | u8 | 10/15 |
| d00 | -997, 1103 | ok | ok | u16 | u16 | ok | u16 | ok | ok | u16 | ok | ok | ok | ok | u16 | ok | 10/15 |
| d07 | -1000, 0, 0, 0, 2000 | u8 | ok | u8 | ok | u8 | u8 | u8 | ok | ok | ok | u8 | ok | ok | ok | ok | 9/15 |
| d10 | -1000, 0, 0, 900 | ok | ok | ok | ok | ok | ok | ok | ok | u32 | u32 | ok | ok | u32 | ok | u32 | 11/15 |
| d19 | -1000, 0.001, 0.01, 0.1, 1, 10, 100, 1000 | ok | ok | ok | E:0x3f8f91595cbb3a00/error:Num | ok | ok | ok | ok | u128 | ok | u70 | ok | u128 | ok | ok | 11/15 |
| d17 | -1000, 1, 1, 1, 1, 1200 | ok | ok | u32 | E:0x3fa3670ea5237340/error:Num | ok | ok | ok | u32 | u32 | ok | ok | ok | u32 | ok | ok | 10/15 |
| d09 | -1000, 100, 200, 300 | ok | ok | u4 | ok | ok | ok | u4 | ok | ok | ok | u8 | ok | ok | ok | ok | 12/15 |
| d01 | -1000, 100, 1150 | ok | ok | ok | u16 | u16 | ok | ok | ok | u16 | u16 | ok | ok | ok | ok | u16 | 10/15 |
| d08 | -1000, 700, 0, 0, 0, 700 | ok | ok | ok | ok | ok | ok | ok | ok | u8 | u8 | ok | u8 | u8 | ok | ok | 11/15 |
| d16 | -1000, 1000, 1.1e-05 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | 15/15 |
| d18 | -1000, 1000, 1 | u1024 | u1024 | ok | u1024 | u1024 | ok | ok | ok | u1024 | u1024 | u1024 | u1024 | ok | ok | u1024 | 6/15 |
| d13 | -1024, 123.25, 456.5, 789.75 | u8 | ok | u8 | ok | u8 | u8 | ok | ok | ok | ok | u8 | ok | u8 | u8 | ok | 8/15 |
| d03 | -1234.5, 50.25, 300.75, 1200.12 | u16 | u16 | u16 | ok | ok | ok | ok | u16 | ok | ok | u3 | ok | ok | ok | ok | 10/15 |
| d06 | -2048, 512, 0.5, 384, 640, 256, 128, 1024 | ok | ok | ok | ok | ok | ok | u16 | ok | u16 | ok | ok | ok | ok | ok | u16 | 12/15 |
| d15 | -3216.99, 387.201, 1434.14, 2481.07 | ok | ok | u8 | u8 | ok | ok | ok | u8 | ok | ok | ok | u8 | ok | ok | ok | 11/15 |
| d05 | -5000, 100, 600, 900, 1200, 3500 | ok | ok | ok | ok | ok | ok | u32 | ok | u32 | u32 | u9 | ok | ok | ok | ok | 11/15 |
| d14 | -2.64422e+123, 3.18262e+122, 1.1788e+123, 2.03933e+123 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | 15/15 |
| d11 | -1e+150, 2e+149, 3e+149, 7e+149 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok | 15/15 |

For design d00 (flows -997, 1103; root 0.10631895687061195 = `0x3fbb37b81ab440bd`) Excel returns `0x3fbb37b81ab440c0` for most guesses, `...b0` for guesses 0, 0.1 and 0.5 and `...a0` for -0.75.

Family B (117 distinct guesses; the table shows the 32 mismatching guesses; the other 85 match): flows `[-1000, 1210]` (the NPV is linear in `v`: the secant converges in one step and every difference is rounding in the update and in the conversion). Excel's answers take only three values over the 117 guesses: `0x3fcae147ae147ae0` (90 rows), `0x3fcae147ae147ae8` (20) and `0x3fcae147ae147ad8` (7); they are 0.20999999999999996, 0.2100000000000002 and 0.20999999999999974, spaced by 8 ulp = 1 ulp of v:

| guess | Excel answer hex | ours |
|---|---|---|
| -0.29999999999999999 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| -0.10000000000000001 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.02 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.050000000000000003 | 0x3fcae147ae147ad8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.052499999999999991 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.089999999999999997 | 0x3fcae147ae147ae0 | 0x3fcae147ae147ae8 (8 ulp) |
| 0.10499999999999998 | 0x3fcae147ae147ad8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.105 | 0x3fcae147ae147ad8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.11 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.14999999999999999 | 0x3fcae147ae147ae0 | 0x3fcae147ae147ae8 (8 ulp) |
| 0.20000000000000001 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.20804687499999996 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.20951171874999996 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.20999952316284176 | 0x3fcae147ae147ae0 | 0x3fcae147ae147ae8 (8 ulp) |
| 0.20999999999999985 | 0x3fcae147ae147ae0 | 0x3fcae147ae147adc (4 ulp) |
| 0.20999999999999988 | 0x3fcae147ae147ae0 | 0x3fcae147ae147add (3 ulp) |
| 0.20999999999999991 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ade (10 ulp) |
| 0.20999999999999994 | 0x3fcae147ae147ae0 | 0x3fcae147ae147adf (1 ulp) |
| 0.20999999999999999 | 0x3fcae147ae147ae0 | 0x3fcae147ae147ae1 (1 ulp) |
| 0.21000000000000002 | 0x3fcae147ae147ae0 | 0x3fcae147ae147ae2 (2 ulp) |
| 0.21000000000000005 | 0x3fcae147ae147ae0 | 0x3fcae147ae147ae3 (3 ulp) |
| 0.21000000000000008 | 0x3fcae147ae147ae0 | 0x3fcae147ae147ae4 (4 ulp) |
| 0.2100000000000003 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.21390624999999996 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.21781249999999996 | 0x3fcae147ae147ad8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.22562499999999996 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.25 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 0.29499999999999993 | 0x3fcae147ae147ae0 | 0x3fcae147ae147ae8 (8 ulp) |
| 0.29999999999999999 | 0x3fcae147ae147ae0 | 0x3fcae147ae147ae8 (8 ulp) |
| 0.41999999999999993 | 0x3fcae147ae147ad8 | 0x3fcae147ae147ae0 (8 ulp) |
| 1 | 0x3fcae147ae147ae8 | 0x3fcae147ae147ae0 (8 ulp) |
| 5 | 0x3fcae147ae147ad8 | 0x3fcae147ae147ae0 (8 ulp) |

Family A (flows -1000, 300, 420, 680): 26 of 100 distinct guesses mismatch, 25 by 8 ulp (1 ulp of v) and 1 by 16; e.g. guess 0.02: Excel `0x3fc4ea7987789198`, ours `0x3fc4ea79877891a0`; guess 0.04085140017224731: Excel `0x3fc4ea798778a018`, ours `0x3fc4ea798778a020`; guess -0.1: Excel `0x3fc4ea798778a1d8`, ours `0x3fc4ea798778a1e0`. Ours is the higher answer in 16 of the 26 and the lower in 10 (analysis).

Notice the ladder rows of family B (guesses at -2097152 ... +2097152 ulp around the double nearest the exact root, 0.21 = `0x3fcae147ae147ae1`): for guesses within 4 ulp of that double (the 8 ladder rows `m4, m3, m2, m1, p1, p2, p3, p4`) our code returns **the guess verbatim**, because the NPV at `1/(1+guess)` is exactly 0 and the code returns `guess` at that point (the `value_before == 0.0` line of the excerpt). Excel returns `0x3fcae147ae147ae0` on all of those rows except `m2` (`...ae8`), and on 30 of the 32 ladder rows overall (the other one is `p12`, also `...ae8`): a fixed point of its iteration and conversion, independent of the guess over a range of +-2 million ulp. So Excel does not exit early on a zero NPV; it converts `v` back, `st(st(1/v) - 1)`, and the answer is a fixed point of the whole thing.

## C.3 What earlier rounds concluded (tracker dc-j9k.43 and dc-j9k.1; `fable52-last-bit.md` section 2)

* Round of 2026-10-30: Newton or secant on the rate or on `q = 1 + rate`, Newton polish, plateau search; 12,288 simple Newton combinations of primitive stores refuted (residual, step or relative-step stops, 20 or 100 iterations, scaling); best simple Newton 308 matches (36 gains, 39 losses). Large flows change Excel's `#NUM!` outcomes. A two-flow `q = 1 + rate` secant with a bitwise fixpoint and a 20-iteration bound gained 11 matches without loss (this predates the `v` finding below; the two disagree on the variable, and `v` is better).
* **Fable 5.2 (2026-11-09; 160 random problems of 3 to 9 flows, 200 two-flow problems, predictions frozen before asking): the solver variable is `v = 1/(1+rate)`**: the same solver on the rate matches 1 of 153 numeric answers, every `v` variant 82 to 98; all seven error cells agree. **The final conversion is `st(st(1/v) - 1)`**, beating `(1 - v)/v` (30 of 153) and the register-kept quotient (30 to 38). **The objective arithmetic is not found**: NPV's loop with base `1/v`, binary powering both ways, `t += flow*q; q *= v`, Horner in `v`, `p /= v` and all 16 stored/register masks of each each have at least 55 contradictions on the 153 answers (best 98 of 153). "Every candidate lands on Excel's iterate to within a few units of the rate; the last secant step is driven by the rounding residue of the objective, so any wrong operation costs the same third of the cases."
* **Two flows already fail**: `P(v) = f0 + f1*v` has no power, yet `f0 + f1*v` gives 130 of 200, `f0 + f1/(1/v)` 149 of 200 and 63 programs over three objectives, seven update expressions and three stop rules top out at 152 of 200; "in 32 of 200 cases no iterate of the best program equals Excel's answer at all". Untested hypotheses: H1 the second point is `1/(1 + guess +- 0.001)`, not `v0 +- 0.001`; H2 the objective is evaluated on the rate, `b = st(1 + st(st(1/v) - 1))`; H3 the stop compares other quantities.
* NPV is exact (284 of 284): `b = st(1 + rate); p = 1; t = 0; for each value v: t = st(t + st(v/p)); p = st(p*b); NPV = st(t/b)`: every operation stored, powers by sequential stored products, the first flow at time 0, one division by `1 + rate` at the end (Fable 5.2 section 1).

## C.4 Questions for IRR

1. The objective: what does Excel evaluate for flows `f0, f1, ...` at `v`? The facts above say it is not NPV(1/v - 1), not Horner in v, not powering. Consider that `v` may only be an internal variable of an NPV-like helper called with `rate = 1/v - 1`, stored, or that the secant is on a different transformed function.
2. The second point and the update expression for two flows (linear in `v`, so the answer is exact up to rounding and only the first step matters): why does Excel's answer depend on the guess at all (family B: three distinct answers `...ae0`, `...ae8`, `...ad8`) and which update expression reproduces which guess's answer? Design experiments for this using flows `-1, 2^j` (exact products).
3. The stop: `step < 1e-7 && |NPV| < 1e-7` fits RATE; for the two guess-2.0 rows Excel stops with an error of 6e-13 and 9e-14 in the rate where our reading of the same stop runs out of iterations. What stop and cap would return those values (and `#NUM!` elsewhere)? Our 200-step cap versus Microsoft's "20 tries" is unresolved; we have not measured the cap directly.
4. The zero-NPV exit: Excel does not return the guess; what is evaluated instead?

---

# Appendix D: BINOM.INV and CRITBINOM (2290 of 2308 and 1402 of 1420)

`BINOM.INV(trials, probability_s, alpha)` and the legacy `CRITBINOM` return the same integer on every recorded input.

## D.1 Our code

The search is a forward sum of masses from k = 0, returning the first `k` with `sum > alpha` (strictly greater). The mass recurrence starts at `q^n` when `p <= q`, else the mass is taken from `binom_mass`. The public cumulative `BINOM.DIST(k, n, p, TRUE)` is separate and is the regularised incomplete beta (`binom_cumulative`), which our code found to equal Excel's `BETA.DIST(q, n-k, k+1, TRUE)` bits on all of 128 live rows at n = 30 and 100 ("not a sum of the masses: that sum misses nine rows in ten by up to 188 units"):

```rust
// crates/calc-func/src/ported/discrete_dist.rs:290-318
 290  pub fn binom_inv_kernel(trials: f64, p: f64, alpha: f64) -> R {
 291      let n = trials.trunc();
 292      if n < 0.0 || !(p > 0.0 && p < 1.0) || !(alpha > 0.0 && alpha < 1.0) {
 293          return Err(NUM);
 294      }
 295      let q = 1.0 - p;
 296      let mut sum = 0.0;
 297      let mut k = 0.0;
 298      // The forward recurrence starts at the larger-probability endpoint; use the existing
 299      // mass calculation when that endpoint is the upper tail or its starting mass underflows.
 300      let mut mass = if p <= q {
 301          crate::ported::excel_numeric::crt_pow(q, n)
 302      } else {
 303          0.0
 304      };
 305      while k < n {
 306          sum += if mass == 0.0 {
 307              binom_mass(k, n, p, q)
 308          } else {
 309              mass
 310          };
 311          if sum > alpha {
 312              return Ok(k);
 313          }
 314          k += 1.0;
 315          mass *= (n - k + 1.0) / k * p / q;
 316      }
 317      Ok(n)
 318  }
```

## D.2 The data

2308 BINOM.INV rows: 2152 from the main question set (all match: 1519 numeric, 633 error-valued, i.e. `#NUM!` outside the domain `0 < p < 1`, `0 < alpha < 1`, `trials >= 0`, which we reproduce on all of them), and **156 rows of an exact-boundary replay** (`dc-1943-binom-replay`): `n` = 1 to 6 with `p` in {0.1, 0.25, 0.5, 0.75} and `alpha` equal to a cumulative tail value or a neighbour of it (`alpha_neighbours`, 21 rows: the double just below, at and just above an exact tail; `ulp_scan_plus_minus_4`, 135 rows: alpha at -4 ... +4 ulp around a tail). **All 18 misses are in the replay set**; the other 138 replay rows match. CRITBINOM has the same 156 replay rows and 1264 others: the same 18 misses, nothing else.

In the table, "exact CDF(k_Excel) vs alpha" is computed in exact rational arithmetic from the doubles `p` and `alpha` (analysis); `>` means the exact cumulative probability up to and including Excel's answer `k` exceeds `alpha`, `=` equals, `<` below. A correct reading of "smallest k whose CDF is at least alpha" in exact arithmetic would give a column pair `(>= , <)` on every row. The table lists the 18 misses (marked MISS) and the other replay rows whose alpha is exactly an exact tail (the tie rows); the remaining replay rows (alpha a few ulp away from a tail, not at a tie) all match.

| n | p | alpha | exact CDF(k_Excel) vs alpha | exact CDF(k_Excel - 1) vs alpha | Excel k | ours k |
|---|---|---|---|---|---|---|
| 1 | 0.5 | 0.5 | > | = | 1 | 1 |
| 2 | 0.25 | 0.5625 | > | = | 1 | 1 |
| 2 | 0.25 | 0.9375 | = | < | 1 | 2 **MISS** |
| 2 | 0.5 | 0.25 | > | = | 1 | 1 |
| 2 | 0.5 | 0.75 | = | < | 1 | 2 **MISS** |
| 2 | 0.75 | 0.0625 | > | = | 1 | 1 |
| 2 | 0.75 | 0.4375 | = | < | 1 | 2 **MISS** |
| 3 | 0.25 | 0.421875 | = | n/a (k=0) | 0 | 1 **MISS** |
| 3 | 0.25 | 0.84375 | = | < | 1 | 2 **MISS** |
| 3 | 0.25 | 0.984375 | = | < | 2 | 3 **MISS** |
| 3 | 0.5 | 0.125 | = | n/a (k=0) | 0 | 1 **MISS** |
| 3 | 0.5 | 0.5 | = | < | 1 | 2 **MISS** |
| 3 | 0.5 | 0.875 | = | < | 2 | 3 **MISS** |
| 4 | 0.5 | 0.0625 | > | = | 1 | 1 |
| 4 | 0.5 | 0.3125 | = | < | 1 | 2 **MISS** |
| 4 | 0.5 | 0.31250000000000006 | < | < | 1 | 2 **MISS** |
| 4 | 0.5 | 0.6875 | = | < | 2 | 3 **MISS** |
| 4 | 0.5 | 0.9375 | = | < | 3 | 4 **MISS** |
| 5 | 0.5 | 0.03125 | > | = | 1 | 1 |
| 5 | 0.5 | 0.1875 | = | < | 1 | 2 **MISS** |
| 5 | 0.5 | 0.18750000000000003 | < | < | 1 | 2 **MISS** |
| 5 | 0.5 | 0.49999999999999994 | > | > | 3 | 2 **MISS** |
| 5 | 0.5 | 0.5 | > | = | 3 | 3 |
| 6 | 0.5 | 0.34375 | = | < | 2 | 3 **MISS** |
| 6 | 0.5 | 0.34375000000000006 | < | < | 2 | 3 **MISS** |

Observations (analysis):

* Excel agrees with the exact-rational "smallest k with CDF >= alpha" on 145 of the 156 replay rows and with "CDF > alpha" on 138; so neither exact reading explains the data. Ours is "exact CDF > alpha" on all 156 (on these small cases our sum of masses behaves like the exact sum).
* Of the 21 rows where alpha equals the exact CDF at some k, Excel returns that k on 14 (the `>=` reading) and the next on 7. For example (n=1, p=0.5, alpha=0.5): exact CDF(0) = 0.5 = alpha, and Excel returns 1 (strict); for (n=3, p=0.5, alpha=0.125): exact CDF(0) = 0.125 = alpha and Excel returns 0 (non-strict). So the comparison outcome at a tie depends on what Excel's computed tail is, i.e. on its rounding.
* Three rows have alpha **one ulp above** the exact tail (alpha = 0.31250000000000006 at n=4; 0.18750000000000003 at n=5; 0.34375000000000006 at n=6, all p=0.5) and Excel returns the k whose exact tail is *below* alpha, i.e. Excel's computed tail is at least that alpha: the computed tail was rounded up by an ulp.
* One row goes the other way: (n=5, p=0.5, alpha=0.49999999999999994): the exact CDF(2) = 0.5 exceeds alpha, but Excel returns 3, because Excel's public `BINOM.DIST(2, 5, 0.5, TRUE)` is `0x3fdffffffffffffe` = 0.49999999999999989, one ulp *below* this alpha (ours: 0.5).

## D.3 What is known (tracker dc-j9k.41, `DnaCalc-Research/design/binomial-inverse.txt`)

* Measured by designing 252 boundaries (p in {1/8 ... 7/8}, n = 1..8, every k < n, alpha at the exact lower tail, which is exactly representable, and at three binary64 neighbours on either side; 1,764 calls): with **Excel's own public `BINOM.DIST(k, n, p, TRUE)` as the tail**, all 200 boundaries with k >= 1 meet alpha exactly when `tail >= alpha`, and all 52 at k = 0 only when the tail is strictly greater than alpha. No sum of public masses (in double or x87, `>=` or `>`) explains more than 104 of 252.
* So: **BINOM.INV is the smallest k such that (k = 0: `BINOM.DIST(0, n, p, TRUE) > alpha`; k >= 1: `BINOM.DIST(k, n, p, TRUE) >= alpha`)**, where the CDF is Excel's own public cumulative.
* Applied to calc-func (patch retained in the research repository: a mass recurrence only locates the start of the search, then the CDF with the rule above), the bank goes BINOM.INV 2,290 -> 2,302 of 2,308 and CRITBINOM 1,402 -> 1,414 of 1,420, **but loses** BINOM.INV(5, 0.5, 0.5) and its twin, because calc-func's `BINOM.DIST(2, 5, 0.5, TRUE)` is 0.5 where Excel gives `0x3fdffffffffffffe`; the six remaining misses are the same kind (live BINOM.INV agrees with live BINOM.DIST on all six). So the rule waits on the binomial CDF bits (the incomplete beta is another research topic; the beta bits at `BINOM.DIST(2,5,0.5,TRUE)`, `(1,5,0.5,TRUE)` and `(2,6,0.5,TRUE)` are the three that must match).
* The k = 0 rule is odd: why strict at 0 and non-strict above? Possibly because at k = 0 the tail is the mass `q^n` computed by a different path (a power) than for k >= 1 (the regularised beta).

## D.4 Questions for BINOM.INV / CRITBINOM

1. Is the rule "BINOM.DIST(k,...) >= alpha, with strict `>` at k = 0" the whole story, or is there a different search (for example a mass recurrence summed in a particular order, a starting point near the mean with stepping, a bisection on k, or a normal-approximation start with a local walk) that coincides with it on the 252 designed boundaries but differs elsewhere? State what in the 156 replay rows and the 2152 others supports your answer.
2. How should the search treat the tail at large `n` (where Excel's `BINOM.DIST` may use a different branch)? Give an experiment whose outcome would reveal a starting point or step size (for example `n` around 1e6 and alpha at consecutive tails).
3. Is there a measurable difference between BINOM.INV and CRITBINOM anywhere (inputs outside the replay)? Both match each other on all 1420 shared rows.

---

# Appendix E: CUMIPMT (13 of 38) and CUMPRINC (230 of 586)

`CUMPRINC(rate, nper, pv, start, end, type)` is the sum of principal payments of periods `start` to `end` of a loan; `CUMIPMT` the same for interest. These are not iterative solvers but they are among the largest differences (CUMIPMT) and they depend on PMT/PPMT bits, which are a separate research topic. We ask only about the *shape* (summation order, which quantity is stored), not the logarithm or exponential substrate.

## E.1 Our code

Arguments (identified, landed): `nper` kept as given (not truncated), start and end compared as given (`start >= 1`, `start <= end`, `end <= nper`, else `#NUM!`), then the whole periods `ceil(start) .. floor(end)` summed; an interval holding no whole period is 0. `type` must be exactly 0 or 1. `rate <= 0` or `pv <= 0` is `#NUM!`.

CUMIPMT is `count * PMT - CUMPRINC` (not the sum of IPMT: over 400 readings that matches 131 of 195 bit for bit against 53), with both the product and the subtraction stored through the x87 emulation; for type 1 and start 1 the first period (no interest) is skipped. CUMPRINC for type 0 is the **sum of public PPMT values in plain binary64**, for type 1 a mixed `payment - interest` / discounted-payment form:

(`principal_sum`, `cumulative_finance_family.rs:72-117`: for type 0, `total += ppmt(rate, period, nper, pv, 0, end-of-period)` over the whole periods in order, in plain binary64; for type 1, `payment = pmt(...)` once, period 1 is `payment - ipmt(...)`, later periods use `discounted_payment_principal` = `exp(-(remaining*log1p(rate))) * (payment - carried)`. `interest_finish` = `x87_sub(x87_mul(count, payment), principal)`.)

PPMT for end-of-period payments (`financial_time_value_family.rs:767-838`, used by the type 0 sum): the principal is `RN(RN(RN(q - fv) * u) * rate)` with `u = exp(-(m*L))`, `m = nper - period + 1`, `L` the PMT logarithm (an atanh-series `log1p`), `q` the stored PMT quotient; PPMT matches 138 of 138 recorded rows with fv = 0. The sum over periods is a plain binary64 `total += principal` in period order.

## E.2 The data

CUMPRINC: 586 rows from 8 loans: `(rate, nper)` = (7.775e-9, 360), (8.05e-9, 1), (0.0013616, 127), (0.0041667, 360), (0.0043595, 67), (0.0124423, 257), (0.0215703, 31), (0.0649385, 17); each with several pv values, both types, and intervals chosen as full, prefix, suffix, interior and singleton (first, last) periods. Matches by loan:

| rate | nper | rows | matched |
|---|---|---|---|
| 7.7750176297663102e-09 | 360 | 12 | 4 |
| 8.0499406029856276e-09 | 1 | 2 | 1 |
| 0.0013616417420825142 | 127 | 124 | 27 |
| 0.0041666666666666666 | 360 | 16 | 7 |
| 0.0043595306416848566 | 67 | 108 | 53 |
| 0.012442315438279733 | 257 | 108 | 40 |
| 0.021570322256081968 | 31 | 108 | 47 |
| 0.064938526109715497 | 17 | 108 | 51 |

356 misses: 206 at 1 ulp, 112 at 2, 27 at 3, 7 at 4, one each at 6, 7, 10, 12. By type 0: 93 of 293 match; type 1: 137 of 293. A random sample (14 misses, 6 matches):

| id | rate | nper | pv | start | end | type | Excel | ours | ulp |
|---|---|---|---|---|---|---|---|---|---|
| cumx-c02-t1-singleton_early-v05 | 0.0043595306416848566 | 67 | 215433.66526141838 | 16 | 16 | 1 | -2949.905914533058 | -2949.9059145330575 | 1 |
| cumx-c03-t0-singleton_first-v03 | 0.0013616417420825142 | 127 | 441.26729011796533 | 1 | 1 | 0 | -3.1851350688471824 | -3.1851350688471816 | 2 |
| cumx-c04-t0-suffix_middle-v01 | 0.012442315438279733 | 257 | 6086.3223294145773 | 155 | 257 | 0 | -4573.9206972493594 | -4573.9206972493612 | 2 |
| cumx-c00-t0-prefix_middle-v02 | 0.064938526109715497 | 17 | 1567.7237751401974 | 1 | 10 | 0 | -717.49117605792594 | -717.49117605792583 | 1 |
| cumx-c03-t0-interior-v04 | 0.0013616417420825142 | 127 | 220.63364505898258 | 32 | 76 | 0 | -77.036864840658126 | -77.03686484065814 | 1 |
| cumx-c03-t1-prefix_middle-v00 | 0.0013616417420825142 | 127 | 441.26729011796516 | 1 | 76 | 1 | -255.10990737800464 | -255.10990737800461 | 1 |
| cumx-c01-t1-singleton_early-v02 | 0.021570322256081968 | 31 | 11979.191342458047 | 7 | 7 | 1 | -306.54911290353976 | -306.54911290353971 | 1 |
| cumx-c03-t0-singleton_first-v01 | 0.0013616417420825142 | 127 | 441.26729011796522 | 1 | 1 | 0 | -3.1851350688471816 | -3.1851350688471802 | 3 |
| cumx-c04-t1-full-v00 | 0.012442315438279733 | 257 | 6086.3223294145764 | 1 | 257 | 1 | -6086.3223294145755 | -6086.3223294145773 | 2 |
| cumx-c01-t1-prefix_middle-v01 | 0.021570322256081968 | 31 | 11979.191342458045 | 1 | 18 | 1 | -6108.9507724744626 | -6108.9507724744626 | same |
| cumx-c03-t1-suffix_middle-v04 | 0.0013616417420825142 | 127 | 220.63364505898258 | 77 | 127 | 1 | -93.078691369980291 | -93.078691369980291 | same |
| cumx-c04-t1-singleton_middle-v05 | 0.012442315438279733 | 257 | 12172.644658829153 | 154 | 154 | 1 | -43.141111155415352 | -43.141111155415352 | same |

CUMIPMT, all 38 rows (Excel build 20527, 4 loans). The "gross" distance comes from the two loans with a tiny rate (7.8e-9 and 8.05e-9), where `count*PMT - CUMPRINC` cancels almost completely. **Analysis in 60-digit arithmetic: in those rows Excel is the less accurate of the two**: for example the row `rate 8.0499406029856276e-09, nper 1, pv 0.05191834857052053, start 1, end 1, type 0` has true value -4.1793962219779404e-10, Excel gives -4.1793960653402351e-10 (relative error 3.8e-8) and we give -4.1793962041181132e-10 (4.3e-9). The absolute error of Excel's answer there is 1.6e-17, about 2 ulp of the payment (0.0519...), so the structure `count*PMT - CUMPRINC` with PMT and CUMPRINC each rounded is right and what is wanted is the exact rounding of those two operands. With nper = 1 the payment is `pv*(1+rate)` (negative) and the principal is `-pv`; CUMIPMT is then PMT - principal, in which `1 + rate` rounds with a relative error of up to 1.4e-8.

| rate | nper | pv | start | end | type | Excel | ours | ulp |
|---|---|---|---|---|---|---|---|---|
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 1 | 1 | 0 | -7.7289191852125683e-09 | -7.7289191847788874e-09 | 262144 |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 1 | 1 | 1 | 0 | 0 | same |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 1 | 180 | 0 | -1.0453366430795974e-06 | -1.0453366424134636e-06 | 3145728 |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 1 | 180 | 1 | -1.0376077157481234e-06 | -1.037607715248523e-06 | 2359296 |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 1 | 360 | 0 | -1.3950705619603099e-06 | -1.3950705612941761e-06 | 3145728 |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 1 | 360 | 1 | -1.3873416320198118e-06 | -1.3873416313536779e-06 | 3145728 |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 2 | 2 | 0 | -7.7074499950716135e-09 | -7.7074499950716135e-09 | same |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 2 | 2 | 1 | -7.7074499352236536e-09 | -7.7074499347899728e-09 | 262144 |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 181 | 360 | 0 | -3.4973391904724593e-07 | -3.4973391915826824e-07 | 2097152 |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 181 | 360 | 1 | -3.4973391632719952e-07 | -3.4973391632719952e-07 | same |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 360 | 360 | 0 | -2.146924998891464e-11 | -2.146924998891464e-11 | same |
| 7.7750176297663102e-09 | 360 | 0.99407095300605552 | 360 | 360 | 1 | -2.146924998891464e-11 | -2.1469249555233771e-11 | 134217728 |
| 8.0499406029856276e-09 | 1 | 0.05191834857052053 | 1 | 1 | 0 | -4.1793960653402351e-10 | -4.1793962041181132e-10 | 268435456 |
| 8.0499406029856276e-09 | 1 | 0.05191834857052053 | 1 | 1 | 1 | 0 | 0 | same |
| 0.0013616417420825142 | 127 | 220.63364505898258 | 1 | 1 | 0 | -0.30042398082012767 | -0.30042398082012833 | 12 |
| 0.0013616417420825142 | 127 | 220.63364505898258 | 1 | 63 | 0 | -14.571928015448933 | -14.571928015448947 | 8 |
| 0.0013616417420825142 | 127 | 220.63364505898258 | 1 | 127 | 0 | -19.776277376969659 | -19.776277376969546 | 32 |
| 0.0013616417420825142 | 127 | 220.63364505898258 | 2 | 2 | 0 | -0.29825547438817135 | -0.29825547438817179 | 8 |
| 0.0013616417420825142 | 127 | 220.63364505898258 | 64 | 127 | 0 | -5.2043493615206842 | -5.2043493615206842 | same |
| 0.0013616417420825142 | 127 | 220.63364505898258 | 127 | 127 | 0 | -0.0025740713016322303 | -0.0025740713016326744 | 1024 |
| 0.0041666666666666666 | 360 | 200000 | 1 | 1 | 0 | -833.33333333333348 | -833.33333333333337 | 1 |
| 0.0041666666666666666 | 360 | 200000 | 1 | 180 | 0 | -129023.60154008213 | -129023.60154008212 | 1 |
| 0.0041666666666666666 | 360 | 200000 | 1 | 360 | 0 | -186511.56856874016 | -186511.5685687401 | 2 |
| 0.0041666666666666666 | 360 | 200000 | 2 | 2 | 0 | -832.33204203045455 | -832.33204203045443 | 1 |
| 0.0041666666666666666 | 360 | 200000 | 181 | 360 | 0 | -57487.967028657993 | -57487.967028657993 | same |
| 0.0041666666666666666 | 360 | 200000 | 360 | 360 | 0 | -4.4549512283167587 | -4.4549512283165313 | 256 |

## E.3 What is known (tracker dc-j9k.49, `design/cumulative-payments.txt`)

* "Summation shape (identified where `1 + rate` is a power of two). For type 0 the range is a **backward recurrence in the x87 unit**: `f = u_e * r` with `u_e = EXP(-(m_e * L))`, `m_e = nper - end + 1`, `L` the PMT logarithm; `sum += f; f /= (1 + r)` (the sum and `1 + r` kept extended); the stored sum is multiplied by `q = pv / em_n` (x87 multiply). With `1 + r = 2` or `4` this holds on 600 of 600 ranges and 115 of 115 singletons. Range sums are not sums of Excel's own singletons (199 of 480 left to right on the r = 1 loans). Fresh exponentials per period refute (54 to 69 of 150)."
* "For other rates the same program is within 3 ulp on ranges (current code up to 16) but singletons fail: the last period of a 1000-period loan at r = 0.75 (`em_n = -1`, so `q = -pv`) is not one rounding of pv times any constant (2,000 consecutive pv; best cover 82%), not a function of the public PMT, nor of `RN(pv * A)` for any A in the merging ranges; it is exactly homogeneous in pv (P(2pv) = 2P(pv), P(1024pv) = 1024P(pv)). So a second pv-dependent rounding exists that vanishes when `1 + r` is a power of two. `L` offsets of +-8 ulp, PC53, CRT/portable exponentials, pow forms and forward-growth quotients all refute."
* "Type 1 is `pmt1 * v^m` for `k >= 2`; the current discounted fallback is up to 62 ulp off; a loop with the due coefficient is within 2 ulp on the bank but loses 28 matched rows, so it is not landed. CUMIPMT's larger bank differences (15) are cancellation in `count * PMT - CUMPRINC` at rates near 1e-8: they follow CUMPRINC's last bit."
* Fresh set of 407 random fractional calls: CUMPRINC 114 exact, 286 within 16 ulp, 5 larger (all type 1); CUMIPMT 289/102/14.
* Earlier rounds (work-reports 11 to 40): nine component-sum models, 18 aggregate finishes, 300 common-factor, 225 stored-component, 120 forward and 120 backward free-initial recurrences, 72 affine-balance programs and 1,080 remaining-exponent profiles refuted; a binary64 sum of two sub-range results reproduces only 10 of 25 CUMPRINC and 11 of 25 CUMIPMT long controls (a range is not the sum of its parts). The code above is the version before the backward-recurrence finding was landed.

## E.4 Questions for CUMIPMT / CUMPRINC

1. For type 0, give the exact algorithm of the backward x87 recurrence (what is `em_n`, `u_e`, the order and the stores), generalised to `1 + r` not a power of two. The 2nd pv-dependent rounding that vanishes when `1 + r` is a power of two suggests something like a product of the form `pv * (1/(1+r)^k)` or a division by a stored `(1+r)^n - 1`; state which and what the cheapest experiment is (e.g. pv = 1, 2, 3, ... at fixed rate and nper, singleton last period).
2. For type 1, the same.
3. For the tiny-rate loans (where `1 + rate` is not a power of two and cancels), which operand rounding makes Excel's CUMIPMT at nper = 1 equal -4.1793960653402351e-10 for rate 8.0499406029856276e-09 and pv 0.05191834857052053? This is a one-flow case: PMT(r, 1, pv) minus the principal -pv, so it should reveal the PMT form at nper = 1 in one experiment.

---

# Appendix F: PRICE (9103 of 9153), DURATION (7707 of 7884), MDURATION (1113 of 1260)

These share YIELD's price machinery (the schedule of B.1, `pcomp_disc`, `excel_bond_pow`). They are last-bit, listed here because the same objective must be right for YIELD's iterates, and because they are closer to being solved than YIELD.

## F.1 Our code

PRICE is `pcomp_disc(rate, yld, red, c, p, binexp = true)` (excerpt in B.1). Its discount factor `base^(off + k)` uses `excel_bond_pow`: for a non-negative integer exponent below 1024 (on-coupon settlement, `off = 1`) it is **binary exponentiation with products stored from PC64** (the "integer power" of the C runtime, identified on 961 of 10,752 POWER controls and the paired PRICE compositions); for a fractional exponent it is the x87 chain `exp(RN53(RN64(exp * ln base)))` (`excel_pow_chain`, not the platform `powf`: the two disagree by 1 ulp on actual/360 and actual/365 ladders at high yields; the chain reproduces Excel where `powf` does not: 3474 -> 3656 and 3446 -> 3658 rows on bases 2 and 3). The internal settlement fraction is the derived complement `dsc = E - A`, not the day-count span settlement to next coupon (this fixes the "cents" error of bases 2 and 3). PRICE = (dirty - accrued) with `dirty = sum coup/pw(base, off+k) + red/pw(base, off+n-1)` for n > 1 (redemption divided with `excel_x87_div`), and for n = 1 `(red + coup) / (1 + (yld/f)*(dsc/e))`.

DURATION and MDURATION (`crates/calc-func/src/ported/bond_core_family.rs:697-771`): the discount factors are the same `excel_bond_pow(base, off + k)`, the coupon PV `coup/disc` with `excel_x87_div`, weighted by `off + k` and summed with `excel_x87_add`, the redemption weighted *before* division (`diff*100/disc`), the final ratio `num/den` divided by `f`; MDURATION = `duration / (1 + yld/f)` with one stored divide (measured 300/300 against Excel's own DURATION as input).

## F.2 The data

* PRICE: 9153 rows (8417 from the 20131/20228 oracle set; 528 + 136 + 72 residual-graph/companion discovery rows). 50 distinct misses, all 1 ulp, all **off-coupon at bases 2 and 3** (settlement mid-period with actual-day accrual) and, for 13 of the 14 catalogue (`oc`) rows, high yields (0.12 and 0.2; the 14th has 0.001), plus a design `pdg*` (yield 0.2005 and 0.0839, redemption 1 and 2 and 9.5e-7), none on bases 0, 1, 4 (all 561 rows matched), none on on-coupon settlements (351 matched), none with n <= 2. By frequency: f=1 0 of 338 miss, f=2 16 of 6771, f=4 34 of 2044.
* DURATION: 7884 rows, 177 misses (1 ulp: 120, 2: 47, 3: 10), **all off-coupon** (on-coupon: 420 of 420 match). Misses sit in 16 of 284 distinct off-coupon schedules (settlement, maturity, frequency), e.g. (46418, 47848, f=1: n=4, A=31, E=365), (45077, 46387, f=2: n=8, A=151, E=181), (45777, 46081, f=4: n=4, A=61, E=89), (44957, 46387, f=2: n=8, A=31, E=181), with 14 to 18 of the 30 inputs (the yields and bases) of those schedules missing; the other 268 schedules have no miss. By n among off-coupon rows: n=1: 8 of 30 miss, n=2: 6 of 54, n=3: 0 of 3804, n=4: 90 of 234, n=5-7: 0 of 252, n 8-12: 67 of 2040, n>12: 6 of 1050.
* MDURATION: 1260 rows, 147 misses with the same cause (on-coupon: 300 of 300).

DURATION off-coupon misses (8 random misses; `A`, `E`, `n` as above; the Excel result and ours differ by 1 to 3 ulp):

| settlement | maturity | coupon | yield | f | basis | n | A | E | Excel | ours | ulp |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 45777 | 46081 | 0.032500000000000001 | 0.029999999999999999 | 4 | 0 | 4 | 61 | 89 | 0.82132594499124056 | 0.82132594499124068 | 1 |
| 46630 | 47238 | 0.072499999999999995 | 0.044999999999999998 | 2 | 3 | 4 | 123 | 183 | 1.5633926910197657 | 1.5633926910197655 | 1 |
| 46507 | 46812 | 0.065000000000000002 | 0.070000000000000007 | 4 | 3 | 4 | 61 | 90 | 0.80908048201874661 | 0.80908048201874683 | 2 |
| 45077 | 46387 | 0.055 | 0.029999999999999999 | 2 | 4 | 8 | 151 | 181 | 3.2465441886035862 | 3.2465441886035866 | 1 |
| 46361 | 47016 | 0.085000000000000006 | 0.044999999999999998 | 4 | 4 | 8 | 76 | 91 | 1.6579729326790398 | 1.6579729326790402 | 2 |
| 46630 | 47238 | 0.072499999999999995 | 0.070000000000000007 | 2 | 3 | 4 | 123 | 183 | 1.5607594771362665 | 1.5607594771362663 | 1 |
| 46418 | 47848 | 0.029999999999999999 | 0.28000000000000003 | 1 | 4 | 4 | 31 | 365 | 3.6409725546708129 | 3.640972554670812 | 2 |
| 46845 | 47168 | 0 | 0.083888885163598576 | 4 | 3 | 4 | 43 | 90 | 0.88219178082191785 | 0.88219178082191796 | 1 |

PRICE misses (16 of the 50 distinct):

| settlement | maturity | rate | yield | redemption | f | basis | n | A | E | Excel | ours | ulp |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 44099 | 45658 | 0.080000000000000002 | 0.001 | 100 | 4 | 2 | 18 | 86 | 92 | 133.58695924615267 | 133.5869592461527 | 1 |
| 44104 | 45658 | 0.059999999999999998 | 0.12 | 103 | 2 | 2 | 9 | 91 | 184 | 82.286636737111579 | 82.286636737111564 | 1 |
| 44122 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 2 | 9 | 109 | 184 | 62.764264575433799 | 62.764264575433792 | 1 |
| 44137 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 2 | 9 | 124 | 184 | 63.029241990187671 | 63.029241990187664 | 1 |
| 44137 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 3 | 9 | 124 | 184 | 62.99902983902021 | 62.999029839020203 | 1 |
| 44143 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 3 | 9 | 130 | 184 | 63.104513025195487 | 63.10451302519548 | 1 |
| 44152 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 2 | 9 | 139 | 184 | 63.298325907164539 | 63.298325907164532 | 1 |
| 44155 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 3 | 9 | 142 | 184 | 63.317403175646035 | 63.317403175646028 | 1 |
| 44161 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 3 | 9 | 148 | 184 | 63.424814167081067 | 63.42481416708106 | 1 |
| 44167 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 2 | 9 | 154 | 184 | 63.57154907218974 | 63.571549072189732 | 1 |
| 44174 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 2 | 9 | 161 | 184 | 63.700479036266827 | 63.700479036266834 | 1 |
| 44179 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 3 | 9 | 166 | 184 | 63.75093510210008 | 63.750935102100087 | 1 |
| 44182 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 2 | 9 | 169 | 184 | 63.848944492208517 | 63.84894449220851 | 1 |
| 44188 | 45658 | 0.059999999999999998 | 0.20000000000000001 | 103 | 2 | 3 | 9 | 175 | 184 | 63.916195148894168 | 63.916195148894161 | 1 |
| 46463 | 47859 | 0.05997282495330903 | 0.20055555505885014 | 1 | 2 | 2 | 8 | 65 | 181 | 15.941558200245238 | 15.941558200245236 | 1 |
| 46463 | 47859 | 0.05997282495330903 | 0.20055555505885017 | 1 | 2 | 2 | 8 | 65 | 181 | 15.941558200245238 | 15.941558200245236 | 1 |

## F.3 What is known

* R36/R37: "DURATION now discounts coupons before weighting; redemption is weighted before division. Quotients, both forward sums and the final ratio store from PC64. After initial controls ... a separate 288 fresh cases per function match through the actual route; independent rational replay agrees on 576 outputs." "Fractional private powers remain unresolved": "432 fresh fractional schedules refute 22 conditional power profiles including ten compiled provider/store profiles" (R38). "864 settlement calls; 48 subtraction, 64 geometric and 256 negative-POWER labels refute" (R35).
* Fable 5.2 (`fable52-last-bit.md` section 3 and 7): MDURATION = `st(DURATION / st(1 + st(yld/freq)))` on 300/300 random bonds with Excel's own DURATION as the leaf; "re-read DURATION with 'every operation stored, powers by sequential stored products' (as in NPV) before any POWER-based form".
* The misses being concentrated in a few schedules (the fractional exponent `off = (E - A)/E` is the same for all yields) says the residual error is in the power for those specific exponents, not in the sums.

## F.4 Questions

1. For the fractional exponent `off + k` with `off = (E - A)/E` (a ratio of two small integers), what power routine does Excel use? Candidates we have not excluded: `exp(y*ln x)` with an x87 `ln` and `exp` on 64-bit significands and a particular rounding of `y*ln x`; a split of the exponent into integer and fractional parts (`x^k * x^off`); repeated multiplication for the integer part and `exp/ln` for the fraction. What discriminating inputs (exponent values where candidates differ, at yields where `ln(1 + y/f)` is exact or has a short significand) would you ask for?
2. For YIELD (Appendix B) the same objective must be reproduced; does anything in the DURATION/PRICE misses suggest a different power routine than the one our PRICE uses?

---

# Appendix G: other facts that may help

* **Excel answer service.** We can ask Excel any function with any double arguments; one question is one cell formula. We typically run 1,000 to 5,000 questions per round. Answers come back as binary64 bits. Any function can be asked, including the bond functions and their date helpers. A function that returns a result Excel could not compute returns an error value, which is also recorded.
* **What we have not measured**, in case you want to ask for it: the iteration cap of any solver, the exact `#NUM!` frontier of YIELD in price, and the first-iteration behaviour of YIELD for prices far from par (the bias designs are the only evidence there).

