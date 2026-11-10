# Question for GPT-6 Pro: the last bits of Excel's gamma-function family

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet that must reproduce Excel's
numbers bit for bit. The repository is public: https://github.com/DnaCalc/DnaCalc (MIT). This
question is pinned to commit `b2a808502f74e1590dcfc682171c8a1db1d93ee2`
(https://github.com/DnaCalc/DnaCalc/tree/b2a808502f74e1590dcfc682171c8a1db1d93ee2). Every
`file:line` below refers to that commit and is relative to the repository root. You may read the
repository if you can; everything you need is in the appendices, so do not depend on browsing.

The oracle is a recorded bank of Excel's own answers (machine: AMD Ryzen 7 4800H, Windows x64,
Excel builds 20131, 20228, 20430 and 20527; the builds agree wherever the same input was asked
on more than one). Each answer is a binary64 bit pattern. We compare our function with those
answers and count a row as matched only if all 64 bits agree. Current counts, matched of rows
("full bank"):

| Function | matched / rows | Notes |
|---|---:|---|
| GAMMA | 8,193 / 12,603 | misses are 1 to 5 ulp: 4,380 below x = 10, 9 in [12, 48), 21 below x = -10 |
| GAMMALN | 2,137 / 4,538 | 26 rows are #NUM! on both sides; worst 7 ulp |
| GAMMALN.PRECISE | 10,645 / 14,278 | identical bits to GAMMALN wherever both were read; worst 7 ulp |
| GAMMA.DIST | 77,808 / 77,975 | 58 cumulative and 109 density misses |
| GAMMA.INV | 66 / 204 | worst 55 ulp |
| CHIDIST | 372 / 388 | 16 misses, worst 6 ulp (24 before a constant was fixed, see H) |
| CHIINV | 37 / 100 | worst 91 ulp |
| CHISQ.INV | 14 / 40 | worst 20 ulp |
| POISSON.DIST | 49,159 / 49,714 | 4 rows unread; of 551 misses, 542 are cumulative rows at mean 700 to 1,010 with results near 2.2e-308 |

These functions share kernels (appendix A). The bits that remain wrong are the "last bits":
1 to 7 ulp for most rows. We have spent many research rounds on them. A large part of the family
is already exact, and the methods that got it there are listed in appendix G, because the
remaining misses may share a cause with them.

## What we ask

You are a single-shot model with no tools. You cannot run code, Excel or our repository. Your
answer will be implemented by other agents and tested against recorded Excel answers, so every
claim must be concrete enough to implement and to refute with data. Please produce, in this order:

**A. The most plausible computation shape for each function.** For GAMMALN (and
GAMMALN.PRECISE), GAMMA, the incomplete-gamma family (GAMMA.DIST, CHIDIST, POISSON.DIST) and the
inverses (GAMMA.INV, CHIINV, CHISQ.INV): the method family Excel uses, as specifically as the
evidence allows. Where it is relevant, say which of these you think it is and why: Lanczos (which
g and n), Stirling with which correction series and which switch point, a reflection formula
(which form), log-gamma as the logarithm of gamma versus a separate series, rational
approximations in the style of Hart et al. or Cody (which table, which degrees, which interval
seams), recurrences (up or down, product then log, or log then sum), Loader-style saddle-point
forms, series and continued fractions for the incomplete gamma, and for the inverses the
iteration (bracketing, Newton, secant, bisection, stopping rule). For each, give the evidence in
our data for and against (appendices C to F), and say what you would expect the data to look
like if you were wrong.

**B. Ranked alternatives with discriminating experiments.** For each function, a ranked list of
alternatives to your first choice, and for every pair that your analysis of our data cannot
separate, an experiment we can run on our Excel answer service (it answers tens of thousands of
formula cells per minute, so designed windows of 1,024 consecutive doubles, grids and bisections on
predicates between two public functions are all cheap). Give specific inputs as hex doubles (or
as a rule that generates them, for example "the 1,024 doubles starting at 0x3ff8000000000000"),
and the predicted Excel bits, or at least the predicted direction and size of the ulp deviation
from a stated reference, under each hypothesis. Order the experiments by how much of the family
each one would settle.

**C. An implementable specification of the most likely shape.** Evaluation order, every
constant as a number (say which are published values and which are your inference), branch
thresholds, which intermediate is stored to binary64 and which stays in the 64-bit register
(notation below), and how the pieces compose for the negative and tiny arguments. Write it as
prose or pseudocode, never as code copied from a restricted source. State which rows of
appendix E you expect the specification to turn from mismatched to matched, and which not.

**D. Bold ideas.** Things we have not tried, for example reading the shape from the pattern of
which rows tie, from the sign map of the errors, from error curves like those in appendix D,
from the one-sided structure of the GAMMA bias, from cross-function identities in Excel (one
public function as a free oracle for an intermediate of another), or from what a 1990s programmer
would have typed to avoid a special case.

Please say plainly where you are guessing, and give a confidence for each of your main claims.

Priorities, by what they would unlock: (1) the GAMMALN core on [0.7, 3), which also fixes
GAMMALN below 0.7 and on (3, 8) and every caller that uses GAMMALN as a leaf; (2) the GAMMA seed
`Gamma(1+y)` on [1, 2), which fixes GAMMA below 10 and for negative non-integers; (3) the
incomplete-gamma arms still off (a < 1, fractional shapes, integer-shape finite sums) and the inverse search (GAMMA.INV, CHIINV, CHISQ.INV);
(4) POISSON.DIST at large means; (5) the 167 GAMMA.DIST misses (they mostly follow from 1 and 3).

## Constraints that bind the answer

1. **Bit-exact parity with Excel is the goal.** The answer is judged by recorded Excel answers
   only. An "improved" algorithm that is closer to the true value is worth nothing here; Excel's
   oddities (an answer one ulp off the true value, a constant with a rounding error) are to be
   copied bit for bit.
2. **Source rules.** From our `docs/plan/RESEARCH_SOURCES.md`:

   | Tier | Sources | Use |
   |---|---|---|
   | 1. Free to read | Abramowitz and Stegun, DLMF, Hart et al. *Computer Approximations*, Cody's and other published papers, Intel SDM, IEEE 754, textbook mathematics, Excel's own outputs | Anyone |
   | 2. Permissive code | Boost.Math, fdlibm and relatives, Arm optimized-routines, AMD libm, CORE-MATH, SciPy cdflib, SLATEC and FNLIB, Cody's SPECFUN (netlib) | Read, port with notice |
   | 3. Facts only | Numerical Recipes, R nmath, glibc libm, ACM TOMS code other than what calc-func already keeps | Never copy. Give facts only: the method family, its published origin, constants as numbers. No code, structure or names from them |
   | 4. Not used | Microsoft source, disassembly or decompilation of Excel or Microsoft runtime DLLs | Never |

   You may use tiers 1 and 2 freely. For tier 3 give facts only (for example "reduction to [2,3)
   by a running product and a logarithm is a known textbook method", with its origin and the
   constants as numbers); never code, structure or names. Tier 4 is never used. Black-box reasoning
   from Excel's outputs is welcome and is the main method.
3. **Method: the "code shape" hypothesis.** Excel's numeric kernels are mostly simple hand-written
   1990s C: double arithmetic, perhaps x87-era habits, the Microsoft CRT's libm. Rank candidate
   computations by what someone then would plausibly have typed (a table of constants, a
   running product, a Horner loop, a special case for integers), find the shape first, then fit
   constants. A match percentage alone is not evidence of the right shape, and a fitted constant
   that merely lowers the error is not an answer.
4. **You cannot run anything**, so every number in your answer is a prediction. Ask for
   experiments rather than assuming outcomes; we run them and feed the results back.
5. **The deliverable must be implementable**: for each proposed change, the exact computation
   (formula, evaluation order, constants, thresholds).

## Notation for the arithmetic Excel appears to use

Our measurements show that Excel's 64-bit build still runs many 1990s numeric bodies on the x87 with
the control word 0x133F (64-bit significand, round to nearest). We write:

- `st(e)`: the expression `e` evaluated in the x87 register (64-bit significand) and then stored to
  binary64, one double rounding. `st(a*b) = RN53(RN64(a*b))`. A chain such as `st(st(a*b)+c)` stores
  after every operation; a chain such as `st(a*b+c)` keeps the intermediate in the register.
- `LN(x)` and `EXP(x)`: Excel's own worksheet LN and EXP. LN is `FYL2X` with ln 2 loaded; EXP is
  `2^(x log2 e)` by `F2XM1`, `FSCALE` (a negative argument as the reciprocal of the positive one),
  each in the register and stored once. Both are reproduced bit for bit by our code on very large
  banks (LN: 79,439 fresh calls in the latest round), so they are leaves we trust and are not the
  cause of the misses below.
- "ulp" is a unit in the last place of the result's binade. In every table `d` is
  `ours minus Excel` in ulps (positive: our answer is larger). Errors against the true value use a
  50-digit mpmath reference ("true"), signed, in ulps of the true value unless stated.
- Hex doubles are the IEEE 754 bit patterns.

Appendices: A routing of the functions; B our kernels (excerpts); C where the ulp errors are, per
function and region; D Excel's error against the true value (the central black-box evidence);
E sample rows and join neighbours; F the other functions' failing rows; G what is already exact and
how; H what earlier research rounds concluded, including claims that were refuted; I the constants
of our stand-in tables.


---

# Appendix A: which function routes through which kernel

All paths are under `crates/calc-func/src/`. `st(...)` and `LN`, `EXP` as in the notation above.

| Function | Route in our code | Kernel (file:line) |
|---|---|---|
| GAMMA(x) | `gamma_kernel` special cases (pole, integer, half-integer), else `gamma_excel`: x >= 10 Stirling product; x <= -10 reflection; 0 < x <= 1e-16 reciprocal; otherwise (x-1)(x-2)... product times the seed `Gamma(1+y)` (a Cody/Stoltz rational stand-in), below 1 the seed divided down | `ported/special_dist_family.rs:118-158`, `ported/excel_numeric/gammaln.rs:307-466` |
| GAMMALN, GAMMALN.PRECISE | one function: `gammaln_kernel` (domain guard) then `gammaln_excel`: x < 0.7 `core(x+1) - LN(x)`; [0.7,1.5) B1 (Cody-Hillstrom 1967 table, plain double); [1.5,2) B2 (SPECFUN dlgama table, x87 register); [2,3] B3 (rational read from Excel's answers); (3,8) recurrence from B3 by running product and LN; x >= 8 Stirling with ten stored Bernoulli terms | `ported/special_dist_family.rs:160-175`, `ported/excel_numeric/gammaln.rs:1-301` |
| GAMMA.DIST(x,a,b,TRUE) | a = 0.5: ERF.PRECISE(sqrt(x/b)); a = 1: EXPON.DIST(x/b); else `regularized_gamma_p(a, x/b)` = `gratio(a, x/b).0` | `ported/beta_gamma_stats_family.rs:94-130`, `ported/special_math_common.rs:456-706, 878-886` |
| GAMMA.DIST(x,a,b,FALSE) | `poisson_mass(a, x/b) * st(a/x)`, with `y = st(x/b)` | `ported/beta_gamma_stats_family.rs:131-145`, `ported/discrete_dist.rs:136-162` |
| CHISQ.DIST(x,k,TRUE) | `GAMMA.DIST(x, k/2, 2, TRUE)` | `ported/chi_f_t_family.rs:54-80` |
| CHIDIST, CHISQ.DIST.RT (x, k) | k = 1: ERFC.PRECISE(sqrt(x/2)); k = 2: EXP(-(x/2)); else `regularized_gamma_q(k/2, x/2)` = `gratio(k/2, x/2).1` | `ported/chi_f_t_family.rs:81-98` |
| POISSON.DIST(k,m,TRUE) | k = 0: EXP(-m); else `Q(k+1, m)` = `gratio(k+1, m).1`: it is the same function as CHIDIST(2m, 2k+2) bit for bit (measured, fable-2 p1: 462 rows; R36: 120 paired controls; R37: 252 more) | `ported/discrete_dist.rs:320-348` |
| POISSON.DIST(k,m,FALSE) | `poisson_mass(k, m)`: the same prefactor GAMMA.DIST's density and `gratio` use | `ported/discrete_dist.rs:136-162` |
| GAMMA.INV(p,a,b) | `st(b * bisect(P(a, .) = p))`: bisection over adjacent doubles on our own `gratio(a, v).0` until the bracket is one ulp wide, publishing the upper end | `ported/beta_gamma_stats_family.rs:156-188`, `ported/special_math_common.rs:2511-2542` |
| CHISQ.INV(p,k) | `GAMMA.INV(p, k/2, 2)` | `ported/chi_f_t_family.rs:114-130` |
| CHIINV, CHISQ.INV.RT (p,k) | bisection on the negated `gratio(k/2, x/2).1` | `ported/chi_f_t_family.rs:100-142` |

What the table means for the misses:

- `gratio` (the incomplete gamma ratio, ported from the public-domain TOMS 654 / NSWC code kept in
  our tree, with identification-driven changes; excerpt in appendix B) has these arms, selected
  by `a`, `x`: `a = 1`: exponential (`-expm1(-x)`, `exp(-x)`); `a < 1`: Taylor series for
  `x < 1.1`, continued fraction otherwise; `1 <= a < 20` with `a > x` or `x >= 31`: prefactor
  `r = poisson_mass(a, x)`, then for `x <= max(a, 2.3026)` the forward series
  `1 + c1 + c2 + ...` stopped at `c/ans <= 2^-53` with result `st(r * ans)`, for `x < 31` a
  continued fraction (Cephes-style, in the registers, stored once) times `st(r * a)`, and for
  `x >= 31` an asymptotic tail; `1 <= a < 20` with `a <= x < 31`: if `2a` is an integer a closed
  finite sum (integer `a`: `exp(-x)` times a partial sum of `x^j/j!`; half-integer `a`: starting
  from `Q(1/2, x)` and `exp(-x)/(sqrt(pi) sqrt(x))`), else the continued fraction; `a >= 20`:
  Temme-type expansions near `x = a`, else the arms above. The prefactor is always
  `poisson_mass(a, x)`.
- `poisson_mass(a, x)` has two routes (excerpt in appendix B): for `x <= 1` or `lambda < 1.1`
  it is `EXP(st(a LN(lambda)) - lambda - GAMMALN(a+1))` with the two subtractions compensated;
  otherwise Loader's saddle point `EXP(-s(a) - dev(a, lambda)) / sqrt(2 pi a)`.
  **So GAMMA.DIST's density, POISSON.DIST's mass at small means and part of every cumulative below
  `lambda = 1.1` take their low bits from our GAMMALN.** A wrong GAMMALN on [1, 3) (shape
  `a + 1` in [1, 3)) therefore shows up as GAMMA.DIST density misses at shapes `a < 2`.
- CHIDIST and POISSON.DIST (cumulative) are one function of two spellings; GAMMA.DIST (TRUE) is the
  complementary arm `P`, a separate caller: `1 - CHIDIST` agrees with it only in part.
- The inverses invert our `gratio`, so they inherit its inaccuracies and add a stopping rule that
  differs from Excel's (appendix F.3).


---

# Appendix B: our kernels (excerpts, commit b2a80850)
Constants below that are typed as `f64::from_bits(...)` are binary64 patterns; the decimal values of the
tables are in appendix I. Everything that says `excel_x87_mul`, `excel_x87_add`, `excel_x87_sub`,
`excel_x87_div` is `st(a op b)`; `excel_log`, `excel_exp` are Excel LN and EXP as described above;
`Native::mul/add/div/store` with `CW` is the explicit register arithmetic of the notation.

## B.1 GAMMALN and GAMMALN.PRECISE (one function)
```rust
// crates/calc-func/src/ported/excel_numeric/gammaln.rs:1-24
//! Excel `GAMMALN` / `GAMMALN.PRECISE` positive-argument kernel and worksheet `GAMMA` (dc-j9k.20).
//!
//! Bands of `GAMMALN`, dispatching on the positive argument `x`:
//!   * `x < 0.7`         `core(x+1) - ln(x)`, one double subtract.
//!   * `0.7 <= x < 1.5`  B1: stands in the Cody-Hillstrom 1967 n=7 table in plain double.
//!   * `1.5 <= x < 2`    B2: stands in SPECFUN's table in x87 continuous arithmetic.
//!   * `2 <= x <= 3`     B3: Excel's own form, read from its answers: see [`b3_with`].
//!   * `3 < x < 8`       the recurrence from the `[2,3)` band, identified: see [`recurrence`].
//!   * `x >= 8`          Stirling: ten Bernoulli terms, worksheet LN and stored stages.
//!
//! Excel's bands below 3 are rationals in `x` itself: noise reaches a full unit of the result at
//! `x = 1 + 2^-52` and `2 + 2^-51`, where a polynomial in `x - 1` or `x - 2` could carry none, and the
//! steps of the mean error mark Horner products stored before each coefficient is added. The tables
//! of `[1.5,2)` and `[0.7,1.5)` are not identified, and `[1.5,2)` and `[2,3]` are separate bands.
//!
//! The x87 operations run on `super::x87::raw` on `x86_64`, and on `super::soft87` elsewhere.
// ---- band anchors (correctly-rounded transcendentals) --------------------
const D2: f64 = f64::from_bits(0x3FDB_0EE6_0720_93CE); // 1 - EulerGamma
const D4: f64 = f64::from_bits(0x3FFC_AB0B_FA2A_2002); // ln 6
const LS2PI: f64 = f64::from_bits(0x3FED_67F1_C864_BEB5); // ln(2*pi)/2

// ---- dispatch threshold ---------------------------------------------------
const THR07: f64 = f64::from_bits(0x3FE6_6666_6666_6666); // 0.7


// ... [lines omitted: B1 and the B2 coefficient tables are in appendix I and the published SPECFUN dlgama P2/Q2; stirl8 is G.1]

// crates/calc-func/src/ported/excel_numeric/gammaln.rs:161-218
/// B3 band `[2, 3]`.
fn b3(x: f64) -> f64 {
    // The top of the band, as Excel gives it: one ulp above stored ln 2. It is read, because the
    // table below is two ulps above it there, and GAMMA.DIST's shape 2 takes it in every prefactor.
    if x == 3.0 {
        return f64::from_bits(0x3FE6_2E42_FEFA_39F0);
    }
    b3_with::<super::Native>(x)
}

/// `y = (x-2) * R`, `R = P(x)/Q(x)`, both polynomials of degree 8 in `x` itself, as Excel 16.0.20527
/// evaluates them on the x87 at precision 64. Every Horner product and sum is stored to binary64,
/// except each polynomial's last sum, which stays in the register for the division; `R` is stored,
/// and so is the answer.
///
/// Read from Excel's answers on windows of consecutive doubles. Each stored stage leaves a line in
/// the error, and the phases of 15 stage lines over 2,000 windows give the stage values; the stages
/// link as `S(k-1) = x S(k) + c(k-1)`. Independent checks: with only the two last coefficients left
/// to the identity, `P/Q` meets `lnGamma(x)/(x-2)` to 8e-17 on `[2,3]`, and a simulation of this form
/// leaves the same noise as Excel at every `x` on `[2,3]`.
///
/// How far the bits go: 98% of rows within 0.002 of 2.5 agree bit for bit, 70% on `[2.4666, 2.6031]`,
/// 32% elsewhere on `[2,3]`. Past that stretch the low bits of the coefficients are not read yet:
/// each stage that crosses a power of two exposes bits the last binade hid. `Q(0)` comes from the
/// identity and `P(0)` from the sharp minimum of disagreeing rows at 2.5, not from a line.
fn b3_with<F: super::Fpu>(x: f64) -> f64 {
    use super::soft87::{CW_PC64_RN as CW, ext_from_f64 as ext};

    let chain = |c: &[u64; 9]| {
        let mut value = f64::from_bits(c[8]);
        for &bits in c[1..8].iter().rev() {
            let product = F::store(&F::mul(&ext(value), &ext(x), CW), CW);
            value = F::store(&F::add(&ext(product), &ext(f64::from_bits(bits)), CW), CW);
        }
        let product = F::store(&F::mul(&ext(value), &ext(x), CW), CW);
        F::add(&ext(product), &ext(f64::from_bits(c[0])), CW)
    };
    let (num, den) = (chain(&B3_P), chain(&B3_Q));
    let r = F::store(&F::div(&num, &den, CW), CW);
    F::store(&F::mul(&ext(r), &ext(x - 2.0), CW), CW)
}

fn b2_with<F: super::Fpu>(x: f64) -> f64 {
    use super::soft87::{CW_PC64_RN as CW, ext_from_f64};

    let t = ext_from_f64(x - 2.0);
    let mut xnum = ext_from_f64(0.0);
    let mut xden = ext_from_f64(1.0);
    for i in 0..8 {
        xnum = F::add(&F::mul(&xnum, &t, CW), &ext_from_f64(P2[i]), CW);
        xden = F::add(&F::mul(&xden, &t, CW), &ext_from_f64(Q2[i]), CW);
    }
    let r = F::div(&xnum, &xden, CW);
    let t1 = F::mul(&t, &r, CW); // t * r
    let t2 = F::add(&ext_from_f64(D2), &t1, CW); // D2 + t*r
    let t3 = F::mul(&t, &t2, CW); // t * (D2 + t*r)
    F::store(&t3, CW)
}

// ... [lines omitted: the unused B4 band]

// crates/calc-func/src/ported/excel_numeric/gammaln.rs:253-301
fn recurrence(x: f64) -> f64 {
    recurrence_from(x, b3(2.0 + (x - x.floor())))
}

fn recurrence_from(x: f64, band: f64) -> f64 {
    let n = x.floor();
    let y = x - n;
    let mut z = y + 2.0;
    let mut k = 3.0;
    while k < n {
        z = super::excel_x87_mul(z, y + k);
        k += 1.0;
    }
    super::excel_x87_add(super::excel_log(z), band)
}

/// Direct core for `x` in `[0.7, 8.0)`: dispatch by band.
fn core(x: f64) -> f64 {
    if x < 1.5 {
        b1(x)
    } else if x < 2.0 {
        b2(x)
    } else if x <= 3.0 {
        b3(x)
    } else {
        recurrence(x)
    }
}

/// Excel `GAMMALN` / `GAMMALN.PRECISE` for a positive, finite argument.
///
/// Callers guard the domain: `x` must be finite and `> 0` (the surface returns
/// `#NUM!` otherwise). `x = 1.0` and `x = 2.0` return exactly `0.0`.
pub(crate) fn gammaln_excel(x: f64) -> f64 {
    if x < THR07 {
        let xp = x + 1.0;
        let inner = if xp < 8.0 { core(xp) } else { stirl8(xp) };
        // R33 paired public calls below .7: the public upper-band result minus
        // worksheet LN matches all 1217 adjacent/exponent controls. Another
        // 257 primitive LN disagreements reject the runtime-log composition.
        // The upper table remains approximate; subtraction precision was
        // indistinguishable in those controls and is unchanged here.
        inner - super::excel_log(x)
    } else if x < 8.0 {
        core(x)
    } else {
        stirl8(x)
    }
}
```

## B.2 GAMMA
```rust
// crates/calc-func/src/ported/excel_numeric/gammaln.rs:307-315
/// `2.5066...`: binary64 `sqrt(2 pi)`. Its rounding error, +0.66 of 2^-53, is the bias Excel's
/// answers show on every band of GAMMA.
const SQRT_2PI: f64 = f64::from_bits(0x4004_0D93_1FF6_2706);

// SPECFUN GAMMA: `1 + N(y)/D(y)` for `Gamma(1+y)`, `0 <= y < 1`. W. J. Cody and L. Stoltz, latest
// modification October 12, 1989, https://netlib.org/specfun/gamma; every digit below compared
// with that page's double-precision DATA P and Q on 2026-11-04. Excel's own seed on
// this interval is not identified: it is a chain of stored multiplications that starts from
// `SQRT_2PI`, and this rational stands in for its value only.

// ... [lines omitted: SEED_P and SEED_Q: the 8+8 published SPECFUN gamma coefficients (Cody and Stoltz) for Gamma(1+y), 0 <= y < 1]

// crates/calc-func/src/ported/excel_numeric/gammaln.rs:336-466

/// `Gamma(1+y)` for `0 <= y < 1`: the value every band below 10 multiplies or divides.
fn gamma_seed(y: f64) -> f64 {
    let (mut num, mut den) = (0.0_f64, 1.0_f64);
    for i in 0..8 {
        num = (num + SEED_P[i]) * y;
        den = den * y + SEED_Q[i];
    }
    num / den + 1.0
}

/// `x^n` from the low bit up. Each square and each partial product is stored to binary64.
fn integer_power(x: f64, mut n: u32) -> f64 {
    let (mut product, mut square) = (None, x);
    while n != 0 {
        if n & 1 != 0 {
            product = Some(match product {
                Some(p) => super::excel_x87_mul(p, square),
                None => square,
            });
        }
        n >>= 1;
        if n != 0 {
            square = super::excel_x87_mul(square, square);
        }
    }
    product.unwrap_or(1.0)
}

/// `GAMMA(x)` for `x >= 10`: `sqrt(2 pi) * x^x * exp(-x) * sqrt(1/x) * exp(s(x))`, with `s` the Stirling
/// error of the masses, the same ten terms stored once.
///
/// With `n = floor(x/2)` and `f = x/2 - n`, `v = x^(x/2)` is `x^n`, by binary powering with stored
/// squares, times `exp(f * ln x)`, where `ln x`, their product and the exponential are stored, and
/// `v` is stored. `1/x` and its square root are stored; so are their product with the stored
/// `exp(-x)`, then that times `v`, and again times `v`, then the constant, then `EXP(s(x))`.
///
/// Read from Excel 16.0.20527 answers on windows of consecutive doubles from 10 to 170. Lines at
/// `2m`, `8(m-1)`, `32(m-1)` (`m` the significand of `x`) with weights `x/2`, `x/4` and `x/8` are the
/// stored `x^2`, `x^4`, `x^8`; a line at `1/x` in units of `ulp(ln x)`, weight `2f`, is the stored
/// logarithm; the residual bias of +0.66 of 2^-53 is the constant. With those in, the lines left
/// in the misses at 50.3 stood at the frequencies of `1/x` and `sqrt(1/x)`: 17,988 of 18,000 adjacent
/// doubles there, 5,994 of 6,000 from 36 to 170. From 10 to 36 `s(x)` matters and `w` is its stored
/// exponential: 5,970 of 6,000, where a truncated series left a slope of a unit near 11.
fn gamma_stirling(x: f64) -> f64 {
    use super::soft87::ext_from_f64 as ext;
    use super::{CW, Fpu, Native};

    let half = 0.5 * x;
    let n = half.floor();
    let root = super::excel_exp(super::excel_x87_mul(half - n, super::excel_log(x)));
    let v = super::excel_x87_mul(integer_power(x, n as u32), root);
    let root_of_reciprocal =
        Native::store(&Native::sqrt(&ext(super::excel_x87_div(1.0, x)), CW), CW);
    let scaled = super::excel_x87_mul(
        v,
        super::excel_x87_mul(super::excel_exp(-x), root_of_reciprocal),
    );
    let body = super::excel_x87_mul(super::excel_x87_mul(scaled, v), SQRT_2PI);
    super::excel_x87_mul(body, super::excel_exp(super::excel_stirling_series(x)))
}

/// `GAMMA(-m)` for `m >= 10`: `pi / (GAMMA(m) z)` with `z = m sin(pi t)`, where `t` is the fraction of
/// `m` folded to at most 1/2. `pi t`, the sine and `z` are stored; the double `pi` is divided by
/// `GAMMA(m) z` in the registers. On 6,000 Excel 16.0.20527 answers from -170 to -10, 75% agree in
/// every bit, where the seed chain this replaces was tens of units away; the misses are one unit,
/// and the spectrum of Excel minus this form at -50.3 has no line standing out.
fn gamma_reflected(m: f64) -> f64 {
    use super::soft87::ext_from_f64 as ext;
    use super::{CW, Fpu, Native};

    let whole = m.floor();
    let fraction = m - whole;
    let folded = if fraction > 0.5 {
        1.0 - fraction
    } else {
        fraction
    };
    let sine = super::crt_sin(super::excel_x87_mul(core::f64::consts::PI, folded));
    let z = super::excel_x87_mul(sine, m);
    let g = gamma_stirling(m);
    let product = Native::mul(&ext(g), &ext(z), CW);
    let value = Native::store(&Native::div(&ext(core::f64::consts::PI), &product, CW), CW);
    if whole % 2.0 == 0.0 { -value } else { value }
}

/// `(x-1)(x-2)...(1+y) * seed`, the product from the top, every step stored.
fn product_times_seed(x: f64, seed: f64) -> f64 {
    let floor = x.floor();
    let mut product = 1.0;
    let mut k = 1.0;
    while k < floor {
        product = super::excel_x87_mul(product, x - k);
        k += 1.0;
    }
    super::excel_x87_mul(product, seed)
}

/// Worksheet `GAMMA` of a finite, non-integer or large argument. Callers guard the poles.
///
/// Below 10, with `y` the fraction of `x`, `GAMMA(x)` is the product `(x-1)(x-2)...(1+y)`, formed
/// from the top with every step stored, times the seed `Gamma(1+y)`, stored: 2,000 of 2,000 on
/// the Excel 16.0.20527 answers at each of `x = 3+y ... 9+y`, given `GAMMA(1+y)`. Below 1 the seed
/// is divided by `y`, then by each lower factor, in the registers.
pub(crate) fn gamma_excel(x: f64) -> f64 {
    use super::soft87::ext_from_f64 as ext;
    use super::{CW, Fpu, Native};

    if x >= 10.0 {
        return gamma_stirling(x);
    }
    if x <= -10.0 {
        return gamma_reflected(-x);
    }
    // Recorded on the decade grid from 1e-307 and on the 27 doubles below 1e-16; 2e-16 misses.
    if x > 0.0 && x <= 1e-16 {
        return super::excel_x87_recip(x);
    }
    let floor = x.floor();
    let seed = gamma_seed(x - floor);
    if x >= 1.0 {
        return product_times_seed(x, seed);
    }
    let mut value = ext(seed);
    let mut k = -floor;
    while k >= 0.0 {
        value = Native::div(&value, &ext(x + k), CW);
        k -= 1.0;
    }
    Native::store(&value, CW)
}
```

## B.3 The inverse search
```rust
// crates/calc-func/src/ported/beta_gamma_stats_family.rs:177-188
    let mut hi = (alpha + 10.0 * alpha.sqrt() + 10.0).max(1.0);
    // bisect_inverse requires f(hi) >= target on entry.
    for _ in 0..200 {
        if regularized_gamma_p(alpha, hi) >= probability {
            break;
        }
        hi *= 2.0;
    }
    let unit = bisect_inverse(probability, 0.0, hi, |v| regularized_gamma_p(alpha, v));
    let x = crate::ported::excel_numeric::excel_x87_mul(beta, unit);
    Ok(x)
}
```
```rust
// crates/calc-func/src/ported/special_math_common.rs:2511-2542
pub fn bisect_inverse<F>(target: f64, lo: f64, hi: f64, f: F) -> f64
where
    F: Fn(f64) -> f64,
{
    // Excel publishes fully-converged roots of its own forward (W109 agent-C
    // verdict; the early-stop 4*EPS relative cutoff produced up to +1.9M-ULP
    // errors at small roots on the b14 corpora). Bisect on the float lattice
    // until lo and hi are adjacent doubles, maintaining f(lo) < target <= f(hi),
    // and publish hi.
    let key = |x: f64| -> i64 {
        let i = x.to_bits() as i64;
        if i < 0 { i64::MIN.wrapping_sub(i) } else { i }
    };
    let unkey = |k: i64| -> f64 {
        if k < 0 {
            f64::from_bits(i64::MIN.wrapping_sub(k) as u64)
        } else {
            f64::from_bits(k as u64)
        }
    };
    let mut lok = key(lo) as i128;
    let mut hik = key(hi) as i128;
    while hik - lok > 1 {
        let midk = lok + (hik - lok) / 2;
        if f(unkey(midk as i64)) >= target {
            hik = midk;
        } else {
            lok = midk;
        }
    }
    unkey(hik as i64)
}
```

## B.4 The prefactor and POISSON.DIST
```rust
// crates/calc-func/src/ported/discrete_dist.rs:136-164
/// The Poisson mass at `x` for `lambda > 0`, with `x` not necessarily whole: it is also GAMMA.DIST's
/// prefactor `y^a exp(-y) / Gamma(a + 1)`, in the cumulative's arms and in the density, which is the
/// mass times the stored `a/y`. At `a = 2` the error of GAMMA.DIST jumps at `y = 1.1` (5,000 live
/// answers around it), and with this route the bank's failures fall from 29,872 to 594.
///
/// Two routes, split at `lambda = 1.1` (bisected on live answers at `x = 2, 3, 7`) with `x <= 1`
/// always on the first:
///
/// - `exp(x ln lambda - lambda - GAMMALN(x + 1))`.
///   `ln lambda` is stored (the answers sit on its grid) and so is the product with `x`; the stored
///   GAMMALN is why the bank's constants carry its low bits (+1 unit at `x = 2, 8`). The two
///   subtractions are compensated (`excel_exp_compensated`).
/// - Loader's saddle point `exp(-stirling_error(x) - deviance(x, lambda)) / sqrt(2 pi x)`, with the
///   exponent and the exponential each stored: the answers sit on the exponent's binary64 grid
///   once the normalizer is removed, from `lambda` near 4 to 1000 and `x` up to 100.
pub(crate) fn poisson_mass(x: f64, lambda: f64) -> f64 {
    if x == 0.0 {
        return excel_exp(-lambda);
    }
    if x <= 1.0 || lambda < 1.1 {
        let t = excel_x87_mul(x, excel_log(lambda));
        return excel_exp_compensated(t, &[lambda, gammaln_excel(x + 1.0)]);
    }
    let exponent = excel_x87_sub(-stirling_error(x), deviance(x, lambda));
    excel_x87_div(
        excel_exp(exponent),
        (2.0 * core::f64::consts::PI * x).sqrt(),
    )
}
```

The wrapper `poisson_dist_kernel` (`discrete_dist.rs:320-348`): `k = 0` gives `EXP(-mean)`, the mass is `poisson_mass(k, mean)`, the cumulative is `Q(k+1, mean)` by `gratio`; a result below the smallest normal double is flushed to 0 for `k >= 1`.

## B.5 The incomplete gamma ratio `gratio` (TOMS 654 port)
```rust
// crates/calc-func/src/ported/special_math_common.rs:80-86
const GRATIO_ALOG10: f64 = 2.30258509299405;
const GRATIO_RT2PIN: f64 = 0.398942280401433;
const GRATIO_RTPI: f64 = 1.77245385090552;
// sqrt(pi) to binary64 for the half-integer finite sum (dc-ymjf.6).
const GRATIO_RTPI_FULL: f64 = 1.772_453_850_905_516;
const GRATIO_THIRD: f64 = 0.333333333333333;
const GRATIO_E: f64 = 2.220446049250313e-16; // spmpar(1) = 2^-52

// ... [lines omitted: the a < 1 branches: a Taylor series for x < 1.1, else a continued fraction with prefactor a*exp(a ln x - x)(1 + gam1(a))]

// crates/calc-func/src/ported/special_math_common.rs:533-593
    // a >= 1
    if a < GRATIO_BIG[iop - 1] {
        if a > x || x >= x0 {
            // statement 20.
            let m = poisson_mass(a, x);
            return gratio_after40(
                a,
                x,
                m,
                crate::ported::excel_numeric::excel_x87_mul(m, a),
                e,
                acc,
                x0,
                &mut wk,
            );
        }
        let twoa = a + a;
        let m = twoa as i64;
        if twoa == m as f64 {
            let i = m / 2;
            let mut summ;
            let mut t;
            let mut n;
            let mut c;
            if a == i as f64 {
                // statement 210: integer a, finite sum
                summ = crate::ported::excel_numeric::excel_exp(-x);
                t = summ;
                n = 1i64;
                c = 0.0;
            } else {
                // statement 220: half-integer a, finite sum.
                // DEVIATION 2: erfc1(0, sqrt(x)) == Q(0.5, x) == gratio(0.5, x).1
                let rtx = x.sqrt();
                summ = gratio(0.5, x).1;
                t = crate::ported::excel_numeric::excel_exp(-x) / (GRATIO_RTPI_FULL * rtx);
                n = 0i64;
                c = -0.5;
            }
            while n != i {
                n += 1;
                c += 1.0;
                t = (x * t) / c;
                summ += t;
            }
            let qans = summ;
            return (0.5 + (0.5 - qans), qans);
        }
        let m = poisson_mass(a, x);
        return gratio_after40(
            a,
            x,
            m,
            crate::ported::excel_numeric::excel_x87_mul(m, a),
            e,
            acc,
            x0,
            &mut wk,
        );
    }


// ... [lines omitted: statement 30, a >= 20: the Temme expansions (about 120 lines)]

// crates/calc-func/src/ported/special_math_common.rs:622-663
fn gratio_after40(
    a: f64,
    x: f64,
    r_series: f64,
    r: f64,
    _e: f64,
    acc: f64,
    x0: f64,
    wk: &mut [f64; 21],
) -> (f64, f64) {
    if r == 0.0 {
        return if x <= a { (0.0, 1.0) } else { (1.0, 0.0) };
    }
    if x <= a.max(GRATIO_ALOG10) {
        // Taylor series. Identification (W109 agent-B, a=2 slice): Excel sums
        // FORWARD (1 + c1 + c2 + ...) with 1/a as an OUTER factor — not the
        // NSWC wk[] backward-tail staging (28/45 vs 16/45 bit-exact).
        // Refined (session 4): the exact form is Cephes-igam style — ans
        // accumulates FROM 1.0, stop at c/ans <= MACHEP (2^-53), publication
        // (r/a)*ans. 25/45 at a=2 with CR exp (vs 16/45 for 1+summ staging);
        // the residual is Excel's statically-linked legacy-CRT exp (one-sided
        // low, fdlibm-proxy 28/45).
        let _ = acc;
        use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div, excel_x87_mul};
        let mut rr = a;
        let mut c = 1.0f64;
        let mut ans = 1.0f64;
        loop {
            rr += 1.0;
            c = excel_x87_mul(c, excel_x87_div(x, rr));
            ans = excel_x87_add(ans, c);
            if excel_x87_div(c, ans) <= 1.110_223_024_625_156_5e-16 {
                break;
            }
        }
        // The series product is stored from x87 PC64 before binary64 publication.
        let p = crate::ported::excel_numeric::excel_x87_mul(r_series, ans);
        return (p, 0.5 + (0.5 - p));
    }
    if x < x0 {
        return gratio_cf(a, x, r, _e, acc);
    }

// ... [lines omitted: statement 100, the asymptotic tail for x >= 31: terms t = (a-1)/x, t *= (a-2)/x, ... summed backward in a 20-slot array, then the answer is st(st(r_series * st(a/x)) * (1 + sum))]

// crates/calc-func/src/ported/special_math_common.rs:694-706
    use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul};
    let qans = excel_x87_mul(excel_x87_mul(r_series, excel_x87_div(a, x)), 1.0 + summ);
    (0.5 + (0.5 - qans), qans)
}

/// The upper tail's continued fraction, in place of Morris's statement 250. In the registers and
/// stored once, 4,366 of 6,000 adjacent answers at (5.3, 2.3), where every stage stored takes 3,906
/// and Morris's fewer. The last stages are not read: the misses are one unit.
fn gratio_cf(a: f64, x: f64, r: f64, _e: f64, _acc: f64) -> (f64, f64) {
    use crate::ported::excel_numeric::{excel_gamma_fraction, excel_x87_mul};
    let qans = excel_x87_mul(r, excel_gamma_fraction(a, x));
    (0.5 + (0.5 - qans), qans)
}
```


---

# Appendix C: where the ulp errors are

**How this was sampled.** There is no sampling here: the tables are complete counts over every row of
our full bank for the function (GAMMA 12,603 rows; GAMMALN.PRECISE 14,278; GAMMALN's 4,538 rows are
given as match rates in the text below, since its bits equal PRECISE's wherever both were read). The rows
are a mixture: windows of 512 to 1,024 consecutive doubles at designed points (integers, half
integers, 0.7, 1.5, 2, 3, 8 and exponent edges, which expose individual rounding stages),
random and grid points, and old general banks. Mismatch rates therefore depend on the sampling,
and regions with many consecutive-double windows (1.5 to 1.7 for GAMMALN.PRECISE) weigh more.
Regions follow the joins in our kernels: 0.7, 1.5, 2, 3, 8, 10, plus 1 where GAMMALN vanishes.
`d` is `ours minus Excel` in ulps of the result; the signed counts are `d:count` for the nonzero
values. High means our answer is numerically larger than Excel's.


```
GAMMA: region | rows | matched | ours high | ours low | signed ulp (ours minus Excel): count of nonzero values
  x<-10                  72     51     8    13   -1:13 +1:8
  [-10,-1)              303    118    37   148   -4:2 -3:3 -2:50 -1:93 +1:29 +2:8
  [-1,0)                301     95   159    47   -3:2 -2:7 -1:38 +1:93 +2:52 +3:9 +4:5
  (1e-16,0.1)          3356   1325   487  1544   -4:1 -3:29 -2:360 -1:1154 +1:442 +2:43 +3:2
  [0.1,0.5)             648    253   104   291   -3:8 -2:66 -1:217 +1:101 +2:3
  [0.5,1)               264    103    42   119   -3:2 -2:25 -1:92 +1:34 +2:8
  [1,1.5)               261     95    41   125   -3:3 -2:31 -1:91 +1:36 +2:5
  [1.5,2)               261     81    38   142   -4:1 -3:11 -2:40 -1:90 +1:32 +2:6
  [2,3]                 529    229    67   233   -4:2 -3:9 -2:45 -1:177 +1:57 +2:8 +3:2
  (3,4)                 352    171    34   147   -3:2 -2:29 -1:116 +1:32 +2:2
  [4,8)                 787    361   100   326   -5:1 -3:12 -2:80 -1:233 +1:88 +2:10 +3:2
  [8,10)                262    113    22   127   -3:4 -2:24 -1:99 +1:19 +2:3
  [10,12)               218    218     0     0   
  [12,36)              1647   1640     6     1   -2:1 +1:4 +2:1 +3:1
  [36,171.6]           3342   3340     1     1   -1:1 +2:1
  (0 further rows are error values, #NUM! on both sides, not counted above)

GAMMALN.PRECISE: region | rows | matched | ours high | ours low | signed ulp (ours minus Excel): count of nonzero values
  (0,1e-16]              21     21     0     0   
  (1e-16,0.1)           193    193     0     0   
  [0.1,0.5)             355    295    12    48   -1:48 +1:12
  [0.5,0.7)            1096    766   269    61   -1:61 +1:269
  [0.7,0.9)             680    225   192   263   -5:2 -4:13 -3:25 -2:81 -1:142 +1:120 +2:43 +3:25 +4:4
  [0.9,1)               433    113    59   261   -7:1 -6:2 -5:10 -4:15 -3:34 -2:82 -1:117 +1:42 +2:12 +3:3 +4:2
  [1,1.1)               134     49    61    24   -3:3 -2:8 -1:13 +1:31 +2:25 +3:3 +4:1 +5:1
  [1.1,1.5)             497    115    78   304   -7:1 -6:6 -5:11 -4:35 -3:40 -2:89 -1:122 +1:46 +2:21 +3:9 +4:2
  [1.5,1.9)            1030    239   631   160   -5:1 -4:1 -3:13 -2:32 -1:113 +1:301 +2:227 +3:87 +4:15 +5:1
  [1.9,2)               157     49    96    12   -2:2 -1:10 +1:55 +2:33 +3:7 +4:1
  [2,2.1)               220     68    45   107   -4:1 -3:14 -2:38 -1:54 +1:20 +2:20 +3:4 +4:1
  [2.1,2.5)             418    150   196    72   -3:6 -2:8 -1:58 +1:81 +2:72 +3:33 +4:6 +5:3 +6:1
  [2.5,3]               478    197   167   114   -5:2 -4:3 -3:13 -2:23 -1:73 +1:87 +2:44 +3:32 +4:3 +5:1
  (3,4)                1117    833   192    92   -2:9 -1:83 +1:164 +2:28
  [4,8)                1611   1494    76    41   -1:41 +1:76
  x>=8                 5822   5822     0     0   
  (16 further rows are error values, #NUM! on both sides, not counted above)
```


**What the counts say.**

GAMMA:
- Matching stops at x = 10. At x >= 10, 5,198 of 5,207 rows match ([10,12) 218/218, [12,36)
  1,640/1,647, [36,171.6] 3,340/3,342). The 9 misses are at x = 12.92, 16.95, 17.14, 19.81, 26.66,
  29.99, 33.25, 38.37, 47.73, of 1 to 3 ulp, five high and four low; none from 48 to 171.6.
- Below 10 (non-integers of both signs, 7,324 rows) only 40% match, and about the same in every
  sub-region (33% to 47%): the rate does not depend on x, its sign or the distance from 1 or 2.
  In magnitude ours is smaller than Excel's about three times as often as larger (x > 0, for example
  [0.1,0.5): 253 matched, 104 high, 291 low). On (-1, 0) the values are negative, so there "ours high"
  also means smaller magnitude. Differences are 1 to 5 ulp, about half of them +-1.
- x <= -10: 51 of 72 match; the 21 misses are +-1 ulp (13 low, 8 high).
- Integers and half-integers are special-cased and match (GAMMA(0.5) = 0x3ffc5bf891b4ef6b), but
  GAMMA(-0.5) is 0xc00c5bf891b4ef6a in Excel, one ulp off the correctly rounded -2 sqrt(pi)
  (0xc00c5bf891b4ef6b): Excel's non-integer arm is inexact even where the answer is a known constant.

GAMMALN and GAMMALN.PRECISE (the same bits wherever both were read; PRECISE is the larger sample):
- Matching starts at x = 8: 364/364 and 5,822/5,822 rows at x >= 8 (PRECISE has 5 misses just below
  8, between 7.9999998807907104 and 7.999999999999997, in the recurrence band). On (0, 0.1) all 229
  rows match; the smallest x with a miss is 0.126 (PRECISE) and 0.17 (GAMMALN), a single ulp.
- Match rate GAMMALN / PRECISE by region: [4,8) 94% / 93%, (3,4) 77% / 75%, [2.1,2.5) 41% / 36%,
  [2.5,3] 39% / 41%, [1.5,1.9) 26% / 23%, [1.9,2) 33% / 31%, [1.1,1.5) 28% / 23%, [0.9,1) 19% / 26%,
  [0.7,0.9) 34% / 33%, [0.5,0.7) 73% / 70%, [0.1,0.5) 90% / 83%.
- The mismatch sign is regional: on [1.5,2) ours is high about three times as often as low (463:139
  and 631:160, mostly +1, +2); on [1.1,1.5) ours is low about three times as often (443:132,
  304:78, down to -7); on (3,4) and [4,8) the rare misses are mostly high; on [2,3] and [0.7,1.1)
  mixed. Excel's own systematic error (appendix D) flips sign at about 0.8, at 1.5 and at about 2.65.
- Below 0.7 ours is `core(x+1) - LN(x)`: the mismatches there (PRECISE [0.5,0.7): 269 high, 61 low,
  never beyond 1 ulp) are the band [1.5,2) seen through the subtraction, with the same sign.
- Near the roots the ulps are small numbers of a tiny result: GAMMALN(1 + 2^-52) is 0xbca2788cfc6fb61a
  in Excel, ...617 in ours (d = +3, about 8e-32 absolute); GAMMALN(2 - 2^-52) is 0xbc9b0ee6072093cc in
  Excel, ...3cd in ours (d = -1). Excel's absolute error shrinks toward the roots (appendix D).
- The worst misses (up to 7 ulp) are on [1.1, 1.5), where the result is about -0.05 to -0.12.


---

# Appendix D: Excel's own error against the true value (the central black-box evidence)

Match counts say how far we are from Excel; this appendix says how far Excel is from the truth. For
every numeric row we computed the true value with 50-digit arithmetic and the signed error of
Excel's bits and of ours, then averaged over bins of x. Averaging over hundreds of rows removes the
rounding noise (one bin's standard error is given) and leaves the **systematic** part of Excel's
error: the error of its approximation, of its constants and of its stored stages. This is how we
read the accuracy class of Excel's tables without seeing them.

Please treat these tables as data to interpret. Our own reading is in D.3, with the evidence
against it.

**D.1 GAMMALN (and PRECISE, pooled, 18,636 distinct x).** Error in units of 1e-18, and in ulps of
the true value; "ours" is our stand-in (Cody-Hillstrom table below 1.5, SPECFUN's table on
[1.5, 2), the read rational B3 on [2, 3]).


```
GAMMALN curve: 18636 distinct x read (GAMMALN and GAMMALN.PRECISE pooled; they agree wherever both were read). Truth = 50-digit mpmath loggamma of the exact binary64 x.
Bin | n | mean(Excel - true) in units of 1e-18, with standard error | mean(Excel - true) in ulps of the true value | mean(ours - true) in 1e-18 | sd of Excel - true in 1e-18
  [0.7,0.8) n= 402     -5.5 (se  1.6)  -0.20 ulp  ours    -8.8  sd   32.2
  [0.8,0.9) n= 404    +10.9 (se  0.9)  +0.72 ulp  ours    +5.2  sd   18.7
  [0.9,1.0) n= 507     +1.6 (se  0.2)  +0.40 ulp  ours    -1.7  sd    5.3
  [1.0,1.1) n= 497     +0.4 (se  0.2)  -0.15 ulp  ours    +0.6  sd    3.5
  [1.1,1.2) n= 310    +13.4 (se  0.7)  +1.19 ulp  ours    +6.9  sd   11.9
  [1.2,1.3) n= 307    +13.3 (se  0.9)  +0.96 ulp  ours    +1.5  sd   15.7
  [1.3,1.4) n= 281    +14.0 (se  1.2)  +1.01 ulp  ours    +2.0  sd   19.8
  [1.4,1.5) n= 388    +15.4 (se  1.0)  +1.11 ulp  ours    -6.7  sd   20.2
  [1.5,1.6) n= 373     -4.8 (se  1.1)  -0.34 ulp  ours    -3.6  sd   21.5
  [1.6,1.7) n= 841    -14.2 (se  0.6)  -1.02 ulp  ours    +0.0  sd   18.1
  [1.7,1.8) n= 334    -11.6 (se  0.8)  -0.83 ulp  ours    +0.6  sd   15.0
  [1.8,1.9) n= 289     -6.6 (se  0.5)  -0.79 ulp  ours    +0.6  sd    9.0
  [1.9,2.0) n= 556     -1.3 (se  0.1)  -0.62 ulp  ours    +0.2  sd    3.3
  [2.0,2.1) n= 287     -0.8 (se  0.2)  -0.39 ulp  ours    -1.3  sd    3.8
  [2.1,2.2) n= 189     -4.7 (se  0.8)  -0.44 ulp  ours    -3.5  sd   11.6
  [2.2,2.3) n= 172    -11.5 (se  1.6)  -0.53 ulp  ours    -1.7  sd   21.2
  [2.3,2.4) n= 171    -37.6 (se  2.5)  -1.36 ulp  ours    -0.9  sd   32.8
  [2.4,2.5) n= 170    -14.2 (se  3.9)  -0.48 ulp  ours    +2.4  sd   50.4
  [2.5,2.6) n= 175    -10.5 (se  3.7)  -0.19 ulp  ours    -5.4  sd   48.4
  [2.6,2.7) n= 169    +13.9 (se  5.7)  +0.25 ulp  ours    +9.4  sd   74.2
  [2.7,2.8) n= 151     +5.0 (se  6.8)  -0.01 ulp  ours   +57.9  sd   83.8
  [2.8,2.9) n= 166    +69.4 (se  7.9)  +0.62 ulp  ours   +90.1  sd  101.9
  [2.9,3.0) n= 156    +53.9 (se  8.2)  +0.49 ulp  ours   +86.6  sd  101.9
  [0.1,0.7) n=1509     -6.8 (se  1.5)  -0.13 ulp  ours    -2.4  sd   56.6
  [3,3.5) n= 725    -12.0 (se  2.4)  -0.07 ulp  ours    -1.3  sd   65.5
  [3.5,5) n=1520    +21.1 (se  3.8)  +0.09 ulp  ours   +47.4  sd  146.6
  [5,8) n=1266    +35.9 (se 10.7)  +0.03 ulp  ours   +49.9  sd  379.5

GAMMA relative error, (Excel - true)/true in units of 2^-53 (truth: mpmath gamma, 50 digits); ours (Cody stand-in below 10, the same Stirling above) for comparison:
  [1e-08,0.001) n=1119  Excel mean +0.63 (se 0.04) sd  1.18 | ours mean +0.01 sd  0.51
  [0.001,0.1) n=2237  Excel mean +0.59 (se 0.03) sd  1.18 | ours mean +0.01 sd  0.52
  [0.1,0.5) n= 648  Excel mean +0.65 (se 0.05) sd  1.18 | ours mean +0.08 sd  0.53
  [0.5,0.75) n= 130  Excel mean +0.87 (se 0.11) sd  1.31 | ours mean +0.22 sd  0.50
  [0.75,1) n= 134  Excel mean +0.75 (se 0.13) sd  1.52 | ours mean +0.23 sd  0.63
  [1,1.25) n= 131  Excel mean +0.59 (se 0.09) sd  1.00 | ours mean +0.07 sd  0.35
  [1.25,1.5) n= 130  Excel mean +0.66 (se 0.09) sd  1.04 | ours mean +0.21 sd  0.36
  [1.5,1.75) n= 131  Excel mean +0.87 (se 0.10) sd  1.09 | ours mean +0.24 sd  0.35
  [1.75,2) n= 130  Excel mean +0.93 (se 0.11) sd  1.25 | ours mean +0.21 sd  0.35
  [2,3) n= 528  Excel mean +0.74 (se 0.05) sd  1.25 | ours mean +0.15 sd  0.57
  [3,4) n= 353  Excel mean +0.76 (se 0.07) sd  1.22 | ours mean +0.14 sd  0.70
  [4,6) n= 452  Excel mean +0.69 (se 0.06) sd  1.31 | ours mean +0.22 sd  0.84
  [6,8) n= 335  Excel mean +0.83 (se 0.08) sd  1.55 | ours mean +0.16 sd  1.09
  [8,10) n= 262  Excel mean +0.79 (se 0.10) sd  1.56 | ours mean +0.05 sd  1.19
  [10,12) n= 218  Excel mean +0.72 (se 0.23) sd  3.38 | ours mean +0.72 sd  3.38
  [12,20) n= 659  Excel mean +0.80 (se 0.16) sd  4.22 | ours mean +0.81 sd  4.22
  [20,36) n= 988  Excel mean +0.65 (se 0.22) sd  7.04 | ours mean +0.65 sd  7.04
  [36,100) n=1907  Excel mean +0.46 (se 0.37) sd 16.29 | ours mean +0.46 sd 16.29
  [100,172) n=1435  Excel mean -0.52 (se 0.86) sd 32.43 | ours mean -0.52 sd 32.43
  [-171,-10) n=  72  Excel mean -0.36 (se 3.03) sd 25.68 | ours mean -0.38 sd 25.62
  [-10,-1) n= 303  Excel mean +0.92 (se 0.08) sd  1.42 | ours mean +0.14 sd  0.54
  [-1,0) n= 301  Excel mean +1.04 (se 0.09) sd  1.51 | ours mean +0.15 sd  0.60
```


**D.2 What the curves show (facts first, then our reading).**

GAMMALN:
1. Excel's error has a smooth systematic part: about +14e-18 flat over [1.1, 1.5), a sign flip at
   1.5 (+15e-18 just below, -1.4e-18 in [1.5,1.6)), about -14e-18 around 1.6 to 1.7 decaying to 0 at
   2.0, a localized dip of -37.6e-18 (-1.36 ulp) on [2.3,2.4), and +54e-18 to +69e-18 on [2.8,3.0).
   It also flips near 0.8 (-5.5e-18 below, +10.9e-18 above). In ulps of the true value the offsets
   are about +1.2, +1.0, +1.0, +1.1 on [1.1,1.5) and -1.0, -0.8, -0.8, -0.6 on [1.6,2.0): close to one
   result-ulp per band, with opposite signs across 1.5. The onset on [1.0,1.15) is steep (0.4e-18 on
   [1.0,1.1), 13e-18 on [1.1,1.2)).
2. Our stand-in for [1.5, 2) is SPECFUN's table and is **accurate**: ours minus true is within about
   0.6e-18 on [1.6,2.0) (sd 0.4e-18 to 4e-18), while Excel's error there reaches 14e-18. Excel's
   [1.5,2) approximation is therefore a *less accurate* one than SPECFUN's, not a more precise one;
   no choice of stores applied to SPECFUN's table can produce a systematic offset of that size and
   sign. (Earlier rounds found the same from a different angle: spectral peaks at 1.3 and 1.625
   refute the published tables' stage frequencies.)
3. Near the roots the error vanishes: [1.0,1.1) +0.4e-18 (sd 3.5e-18), [1.9,2.0) -1.3e-18, [2.0,2.1)
   -0.8e-18 (sd 3.3e-18, 3.8e-18); in 256-point windows (R49) the absolute sd of Excel minus true is
   about 1.2e-30 at x = 1 and 2.1e-30 at x = 2. So both roots are factors of Excel's forms
   (`(x-1) f(x)`, `(x-2) g(x)`), which rules out a Lanczos-style `-t + ln(...)` evaluation near the
   roots, where cancellation would leave an absolute error of order 1e-16.
4. The sd of Excel minus true is 15e-18 to 20e-18 inside bins whose mean is +-14e-18: the rounding noise
   of a few stored stages is as large as the systematic part.
5. Above 3 the mean error is within 0.2 ulp of zero and the match rate is 75% to 94%: the recurrence
   adds `LN(z)` of a stored product to the `[2,3)` value, so the band's offset enters once and is
   small against the ulp of the larger result.

GAMMA:
6. In relative terms Excel's error against the truth has a mean of **+0.6 to +0.9 units of 2^-53 in
   every bin from 1e-8 to 10, for both signs of x** (standard errors 0.03 to 0.13), and +0.7, +0.8,
   +0.65 on [10,12), [12,20), [20,36). The double nearest sqrt(2 pi), 2.5066282746310007
   (0x40040d931ff62706), is 0.6586 units above the true 2.50662827463100050241...; `ln(2 pi)/2` rounded
   (0x3fed67f1c864beb5) is 0.349 units above its true value. **That the mean offset equals the
   representation error of the sqrt(2 pi) double in every bin, the seed region [1, 2) included, is the
   strongest single fact we have about the GAMMA seed**: a rational like the Cody-Stoltz one in our
   stand-in has mean error +0.0 to +0.25 there and cannot carry it. The comment at `gammaln.rs:307-309`
   records the same bias for the x >= 10 branch, where we already match.
7. The sd of Excel's relative error is 1.0 to 1.25 units on [1, 2) (ours 0.35), 1.2 to 1.5 on [2, 8),
   1.6 at [8, 10), then 3.4, 4.2, 7.0, 16.3, 32.4 on [10,12), [12,20), [20,36), [36,100), [100,172]:
   about 0.25 x units from 10 up (the cost of `x^x` by binary powering and a stored logarithm,
   which our x >= 10 code reproduces). An sd of 1.2 below 10 with no growth in x means roughly 6 to 9
   independent double roundings and an otherwise unbiased remainder.
8. Negative arguments: mean +0.9 on [-10,-1) and +1.0 on (-1,0) (se 0.09), sd 1.4 and 1.5; below -10
   mean -0.4 (se 3.0), sd 26. The same bias as the positive side.

**D.3 Our reading (challenge it).** (a) GAMMA below 10 is `sqrt(2 pi) *` (an accurate smooth product of
about five stored quantities) on [1, 2), then recurrences. The Cody rational is the wrong family, though
right in value. Candidates: Stirling's formula `sqrt(2 pi) x^(x-1/2) e^-x w(x)` at the argument itself, with
`w = exp(s(x))` and `s` the Stirling error evaluated by the recurrence-from-6 that our POISSON.DIST mass
uses (`stirling_error`, described in G.8); or a Lanczos form `sqrt(2 pi) (z+g+1/2)^(z+1/2) e^-(z+g+1/2) A_g(z)`.
The flat accuracy (bias constant to +-0.15 over [1, 2)) argues against a low-order Lanczos, whose own
approximation error is 1e-15. (b) GAMMALN on [0.7, 3) is a set of rationals in x whose own
approximation errors are about one ulp, with the roots factored; the bands do not use SPECFUN's or
Cody-Hillstrom's accurate tables.


---

# Appendix E: sample rows and join neighbours

Sampling: for each region the row with the largest |d| and one matched row drawn at random from the
complete trace (seed fixed); then, for each join (GAMMA at 10; GAMMALN at 0.7, 1.5, 3, 8) the row
nearest below and the row nearest at or above, matched or not. Counts per region are
in appendix C; this is only to show what rows look like. Each line: `x` (decimal, then bits),
Excel's bits and decimal (17 significant digits), our bits and decimal, `d` = ours minus Excel in
ulps. GAMMALN and GAMMALN.PRECISE rows are pooled (identical bits wherever both exist).

Special points as Excel answered them: GAMMALN(1) = GAMMALN(2) = 0; GAMMALN(3) = 0x3fe62e42fefa39f0, one ulp
above `ln 2` = 0x3fe62e42fefa39ef; GAMMALN(0.5), (1.5), (3.5), (4) .. (10) match (ours); GAMMALN(0.75)
= 0x3fca051c372609ec (ours ...09ed), GAMMALN(0.7) = 0x3fd0b20c891cde72 (ours ...de74), GAMMALN(2.5) =
0x3fd2383e809a67e8 (ours ...67e9). GAMMA(n) for 2 <= n <= 10 and GAMMA(2.5), (3.5), (-2.5) match;
GAMMA(0.75) = 0x3ff39b4e8b50f62d (ours ...62c), GAMMA(0.1) = 0x402306ea7b280d88 (ours ...d87),
GAMMA(-1.5) = 0x4002e7fb0bcdf4f1 (ours ...f2).


```
GAMMA samples (x as bits, then decimal; Excel and ours as bits, then decimal)
 region x<=-10
  x=0xc061e008161d7d05 (-143.00098710784383)  Excel 0x0d26dbf10e13a747 (2.6154930008005035e-245)  ours 0x0d26dbf10e13a746 (2.6154930008005031e-245)  d=-1
  x=0xc05780063951433b (-94.00037987647822)  Excel 0xa252dc77d06fc863 (-2.4167485517412116e-143)  ours 0xa252dc77d06fc863 (-2.4167485517412116e-143)  d=+0
 region (-10,-1)
  x=0xbffd9edc44238902 (-1.8512842809364547)  Excel 0x400fdce1acab3268 (3.982852314924731)  ours 0x400fdce1acab3264 (3.9828523149247292)  d=-4
  x=0xbff1154105b4a44e (-1.0676889632107023)  Excel 0x402ce385d32712a8 (14.444380377311361)  ours 0x402ce385d32712a8 (14.444380377311361)  d=+0
 region (0,0.1)
  x=0x3fa694f974cb3fd2 (0.04410533476550395)  Excel 0x4036234164979513 (22.137716567046549)  ours 0x4036234164979513 (22.137716567046549)  d=+0
  x=0x3faf25a0bb9041dc (0.06083395280450407)  Excel 0x402fd602f3b7d7fb (15.917991272155623)  ours 0x402fd602f3b7d7f7 (15.917991272155616)  d=-4
 region [1,1.5)
  x=0x3ff2dd665da7f210 (1.1790527018012007)  Excel 0x3fed91705501a903 (0.92400375941113599)  ours 0x3fed91705501a900 (0.92400375941113566)  d=-3
  x=0x3ff517f1848f51ab (1.3183455637091395)  Excel 0x3feca2af71003160 (0.89485904760799073)  ours 0x3feca2af71003160 (0.89485904760799073)  d=+0
 region [1.5,2)
  x=0x3ffd8b4122bc9c29 (1.8464976651100733)  Excel 0x3fee39c5664e048e (0.94455213529012538)  ours 0x3fee39c5664e048e (0.94455213529012538)  d=+0
  x=0x3ffeb85fe90c0e9e (1.920013342228152)  Excel 0x3fef003d0165788a (0.96877908966759319)  ours 0x3fef003d01657886 (0.96877908966759274)  d=-4
 region [2,3]
  x=0x4003d932dbc2a7d8 (2.481054036024016)  Excel 0x3ff4fd654c16eecf (1.3118641826867365)  ours 0x3ff4fd654c16eecf (1.3118641826867365)  d=+0
  x=0x40062b83cef3ab6e (2.7712474983322215)  Excel 0x3ffa30210ea61056 (1.6367502758230521)  ours 0x3ffa30210ea61052 (1.6367502758230512)  d=-4
 region [10,36)
  x=0x4033d058ed7b607b (19.81385692846423)  Excel 0x436f1a8bb84835cd (70039293380374120)  ours 0x436f1a8bb84835d0 (70039293380374144)  d=+3
  x=0x404185eb710ca48c (35.04624760739634)  Excel 0x47f05a3292775164 (3.4777568599273295e+38)  ours 0x47f05a3292775164 (3.4777568599273295e+38)  d=+0

 join 10.0: nearest row below and at or above
  x=0x4023ffffe0000000 (9.999999046325684)  Excel 0x411625fce2093aa1 (362879.22073833091)  ours 0x411625fce2093aa0 (362879.22073833086)  d=-1
  x=0x4024000000000000 (10.0)  Excel 0x4116260000000000 (362880)  ours 0x4116260000000000 (362880)  d=+0

GAMMALN / GAMMALN.PRECISE samples (pooled)
 region (0,0.7)
  x=0x3fc02de000000000 (0.12639999389648438)  Excel 0x40000fd902b703f8 (2.0077381336309337)  ours 0x40000fd902b703f7 (2.0077381336309332)  d=-1
  x=0x3fe65ac400000000 (0.6985797882080078)  Excel 0x3fd0ce7bf903fa9e (0.26260279958652599)  ours 0x3fd0ce7bf903fa9e (0.26260279958652599)  d=+0
 region [0.7,0.9)
  x=0x3fe895182a9930be (0.7682)  Excel 0x3fc78b267a6c1ccb (0.18393403031971042)  ours 0x3fc78b267a6c1cc6 (0.18393403031971028)  d=-5
  x=0x3fe8b5dcc63f1412 (0.7722)  Excel 0x3fc7036a65176ba6 (0.17979173599345905)  ours 0x3fc7036a65176ba6 (0.17979173599345905)  d=+0
 region [1.1,1.5)
  x=0x3ff322d0e519536a (1.1959999989678303)  Excel 0xbfb58ea408f65520 (-0.084207775295301435)  ours 0xbfb58ea408f65520 (-0.084207775295301435)  d=+0
  x=0x3ff67a0f9096bb99 (1.4048)  Excel 0xbfbeb17d2a32fc27 (-0.11989576606991702)  ours 0xbfbeb17d2a32fc2e (-0.11989576606991711)  d=-7
 region [1.5,1.9)
  x=0x3ff83f0923621147 (1.515389574256078)  Excel 0xbfbebf8f5d2c66ae (-0.1201104738687728)  ours 0xbfbebf8f5d2c66b3 (-0.12011047386877287)  d=-5
  x=0x3ffdccd5f1150885 (1.862508718230431)  Excel 0xbfaa8c25f385ce1a (-0.051850496270074056)  ours 0xbfaa8c25f385ce1a (-0.051850496270074056)  d=+0
 region [2.5,3]
  x=0x40060da2d0e9179f (2.7566582032331577)  Excel 0x3fdec3687bb992b1 (0.48067676622876393)  ours 0x3fdec3687bb992ab (0.4806767662287636)  d=-6
  x=0x40070bac710cb296 (2.8807)  Excel 0x3fe2bfceee206f3f (0.58591410168762781)  ours 0x3fe2bfceee206f3f (0.58591410168762781)  d=+0
 region (3,4)
  x=0x400d03e425aee632 (3.6269)  Excel 0x3ff57f562d61f73e (1.3435880444298642)  ours 0x3ff57f562d61f73c (1.3435880444298638)  d=-2
  x=0x400e62ff24b66ef3 (3.7983382099317793)  Excel 0x3ff8b595cd50b7d4 (1.5443323154572584)  ours 0x3ff8b595cd50b7d4 (1.5443323154572584)  d=+0

 join 0.7: nearest row below and at or above
  x=0x3fe6666666666665 (0.6999999999999998)  Excel 0x3fd0b20c891cde75 (0.26086724653166665)  ours 0x3fd0b20c891cde76 (0.26086724653166671)  d=+1
  x=0x3fe6666666666666 (0.7)  Excel 0x3fd0b20c891cde72 (0.26086724653166649)  ours 0x3fd0b20c891cde74 (0.2608672465316666)  d=+2
 join 1.5: nearest row below and at or above
  x=0x3ff7ffffffffffff (1.4999999999999998)  Excel 0xbfbeeb95b094c191 (-0.12078223763524522)  ours 0xbfbeeb95b094c193 (-0.12078223763524525)  d=-2
  x=0x3ff8000000000000 (1.5)  Excel 0xbfbeeb95b094c192 (-0.12078223763524523)  ours 0xbfbeeb95b094c192 (-0.12078223763524523)  d=+0
 join 3.0: nearest row below and at or above
  x=0x4007ff88537ae54e (2.9997717401389883)  Excel 0x3fe62c8948e60447 (0.69293655622471639)  ours 0x3fe62c8948e60446 (0.69293655622471628)  d=-1
  x=0x4008000000000000 (3.0)  Excel 0x3fe62e42fefa39f0 (0.6931471805599454)  ours 0x3fe62e42fefa39f0 (0.6931471805599454)  d=+0
 join 8.0: nearest row below and at or above
  x=0x401fffffffffffff (7.999999999999999)  Excel 0x40210ce1f32dcc2f (8.5251613610654129)  ours 0x40210ce1f32dcc2f (8.5251613610654129)  d=+0
  x=0x4020000000000000 (8.0)  Excel 0x40210ce1f32dcc2f (8.5251613610654129)  ours 0x40210ce1f32dcc2f (8.5251613610654129)  d=+0
```


---

# Appendix F: the other functions' failing rows

## F.1 GAMMA.DIST (77,975 rows, 167 misses)

The bank has 73,225 cumulative rows (Excel build 20131; scale 1 in 73,161 of them, scale 2 in 64)
and 4,750 density rows. The cumulative rows are almost all integer shapes (a = 1: 8,378 rows, 2:
27,202, 3: 27,296, 4 to 16: about 10,000; the integer shapes 2 to 16 together have 10 misses in
64,000 rows) and only 183 rows have a non-integer shape other than 0.5, of which **48 miss**:
a = 0.75 11 of 20, 1.5 5 of 27, 1.75 6 of 8, 2.5 0 of 16, 2.75 **20 of 20**, 5.5 6 of 8, and 0 of 100
for 1 + 2^-20, 1.25, 2 + 2^-20, 2.25, 2.5, 3.25 and 4.5. The bank is therefore thin exactly where the
misses are. The misses by arm of our code:

| Arm | rows | misses |
|---|---:|---:|
| cumulative, a = 0.5 or 1 (ERF.PRECISE / EXPON identities) | 8,562 | 0 |
| cumulative, a < 1 (Taylor / continued fraction with `gam1`) | 20 | 11 |
| cumulative, half-integer a, `a <= x/b < 31` (closed finite sum with `Q(1/2, x)`) | 19 | 5 |
| cumulative, integer a, `a <= x/b < 31` (closed finite sum) | 85 | 5 |
| cumulative, everything else (forward series, continued fraction, asymptotic, a >= 20) | 64,539 | 37 |
| density, first route of `poisson_mass` (a <= 1 or x/b < 1.1: `EXP` of the compensated difference with GAMMALN(a+1)) | 3,781 | 99 |
| density, Loader route, 1 < a < 2 | 54 | 10 |
| density, Loader route, a >= 2 | 915 | 0 |

The densities with scale `beta != 1` (4,000 of 4,750 rows) all match. So the 109 density misses are
all at `beta = 1`, and 99 of them use GAMMALN(a + 1) as a leaf (a in (0, 2)). Where the density
route takes the Loader saddle point with `a >= 2` there is no miss at all in 915 rows: this is the
shared helper (`stirling_error` and the deviance) at work and it is exact. The cumulative misses
concentrate in the `a < 1` arm (11 of 20 rows; it uses 15-digit rational-approximation coefficients from the
Fortran source, F.2), the closed finite sums (5 half-integer, 5 integer) and in fractional shapes such as 2.75 (20 rows), which go through the
forward series with the `poisson_mass` prefactor (the GAMMALN route when `x/b < 1.1`).

All 58 cumulative misses (`d` = ours minus Excel in ulps), then the density misses by alpha (the
densities miss mostly at small x, below 1, where the first route of `poisson_mass` applies):


```
GAMMA.DIST cumulative, all 58 mismatching rows of 73,225 (alpha, beta, x to 7 digits; d = ours minus Excel in ulps; exact arguments are in the trace):
  a=0.75 b=1 x=9.094947e-13 d=-1 | a=0.75 b=1 x=0.000213623 d=+2 | a=0.75 b=1 x=0.0002441406 d=-1
  a=0.75 b=1 x=0.015625 d=+3 | a=0.75 b=1 x=0.0625 d=+3 | a=0.75 b=1 x=0.125 d=+4
  a=0.75 b=1 x=0.25 d=+1 | a=0.75 b=1 x=0.375 d=+5 | a=0.75 b=1 x=0.5 d=-1
  a=0.75 b=1 x=0.75 d=+1 | a=0.75 b=1 x=0.9296875 d=+2 | a=1.5 b=1 x=1.5 d=-1
  a=1.5 b=1 x=2 d=-1 | a=1.5 b=1 x=3 d=-1 | a=1.5 b=2 x=4 d=-1
  a=1.5 b=2 x=6 d=-1 | a=1.75 b=1 x=2.980232e-08 d=-2 | a=1.75 b=1 x=1.525879e-05 d=-1
  a=1.75 b=1 x=0.00390625 d=-1 | a=1.75 b=1 x=0.03125 d=-2 | a=1.75 b=1 x=0.21875 d=-1
  a=1.75 b=1 x=0.703125 d=-1 | a=2 b=1 x=1.391152 d=+1 | a=2 b=1 x=1.392777 d=-1
  a=2 b=1 x=1.418537 d=-1 | a=2 b=1 x=1.421486 d=-1 | a=2.75 b=1 x=9.094947e-13 d=-2
  a=2.75 b=1 x=6.984919e-10 d=-2 | a=2.75 b=1 x=9.313226e-10 d=-1 | a=2.75 b=1 x=1.490116e-07 d=-1
  a=2.75 b=1 x=9.536743e-07 d=-2 | a=2.75 b=1 x=0.000213623 d=-1 | a=2.75 b=1 x=0.0002441406 d=-2
  a=2.75 b=1 x=0.00390625 d=-2 | a=2.75 b=1 x=0.015625 d=-2 | a=2.75 b=1 x=0.05078125 d=-2
  a=2.75 b=1 x=0.0625 d=-2 | a=2.75 b=1 x=0.1015625 d=-1 | a=2.75 b=1 x=0.125 d=-2
  a=2.75 b=1 x=0.25 d=-1 | a=2.75 b=1 x=0.3398438 d=-1 | a=2.75 b=1 x=0.375 d=-2
  a=2.75 b=1 x=0.5 d=-1 | a=2.75 b=1 x=0.75 d=-3 | a=2.75 b=1 x=0.9296875 d=-2
  a=2.75 b=1 x=1 d=-3 | a=3 b=1 x=3 d=-1 | a=3 b=1 x=3 d=+1
  a=3 b=1 x=3.875 d=-1 | a=3 b=1 x=5.625 d=+1 | a=3 b=2 x=6 d=-1
  a=5.5 b=1 x=2.980232e-08 d=-5 | a=5.5 b=1 x=1.525879e-05 d=-7 | a=5.5 b=1 x=0.00390625 d=-7
  a=5.5 b=1 x=0.03125 d=-5 | a=5.5 b=1 x=0.21875 d=-6 | a=5.5 b=1 x=0.703125 d=-4
  a=12 b=1 x=7.269106e-07 d=+1

GAMMA.DIST density (FALSE), the 109 mismatching rows of 4,750 by alpha (beta = 1 in every one; the 4,000 rows with beta != 1 all match): alpha bin: rows, misses, ours high, ours low, d histogram
  alpha [0,0.5): 130 rows (beta = 1 and beta != 1), 17 misses, high 15, low 2: -1:2 +1:14 +2:1
  alpha [0.5,1): 192 rows (beta = 1 and beta != 1), 18 misses, high 7, low 11: -2:1 -1:10 +1:7
  alpha [1,1.5): 870 rows (beta = 1 and beta != 1), 26 misses, high 10, low 16: -4:1 -3:1 -2:6 -1:8 +1:2 +2:2 +3:1 +4:2 +6:1 +7:1 +8:1
  alpha [1.5,2): 73 rows (beta = 1 and beta != 1), 22 misses, high 9, low 13: -3:2 -2:5 -1:6 +1:5 +2:4
  alpha [2,3): 860 rows (beta = 1 and beta != 1), 13 misses, high 2, low 11: -3:1 -2:5 -1:5 +2:2
  alpha [3,5): 879 rows (beta = 1 and beta != 1), 8 misses, high 1, low 7: -4:4 -3:1 -2:1 -1:1 +5:1
  alpha [5,7.5): 881 rows (beta = 1 and beta != 1), 5 misses, high 3, low 2: -7:1 -4:1 +4:1 +5:2
  alpha [7.5,20): 865 rows (beta = 1 and beta != 1), 0 misses, high 0, low 0: 
  a few of the misses (alpha, x, d): 0.3124 0.0001543 +1 | 0.4216 1.762 +1 | 0.5149 0.2863 +1 | 0.6639 0.02737 +1 | 1.022 4.536 +3 | 1.094 12.43 +7 | 1.358 0.001447 -2 | 1.618 0.02057 +2 | 1.853 0.0005628 -2 | 1.994 0.001674 -3 | 2.765 0.4307 -2 | 3.814 0.0006369 -2 | 6.436 0.05647 -4
```


## F.2 CHIDIST (388 rows, 16 misses; the sqrt(pi) constant that was fixed)

CHIDIST(x, k) is `Q(k/2, x/2)` (the same function as POISSON.DIST's cumulative). The 388 rows: degrees of
freedom 1 (181 rows), 2 (43) and 4, 6, 10, 20 and others; df = 1 (ERFC) and df = 2 (EXP) match on all 224 rows.
The 16 remaining misses (df, x, d = ours minus Excel in ulps): df 3: x = 3 (+3), 4 (+1), 6 (+1), 8 (-1), 16 (-3),
32 (+1); df 3.5 (truncated to 3): x = 3.3125 (+1); df 4: x = 35.395054586014794, 35.3950545860148,
35.39505458601481 (-1, -1, +1, a window of adjacent doubles); df 5: x = 6 (+1), 16 (-6), 32 (+2); df 6: x = 6
(+2), 48 (-1); df 10: x = 32 (+1). All are within 6 ulp.

For the record, these rows used to be 7 to 24 ulp low in the half-integer-shape arm (df 3 and 5, `a = 1.5`,
`2.5`, the closed finite sum from `Q(1/2, x/2)` and `t = exp(-x/2) / (sqrt(pi) sqrt(x/2))`), because our
code typed sqrt(pi) with 15 digits (1.77245385090552, relative error 2.2e-15). The fix (see H) replaced it by
the binary64 value. What is left in that arm (df 3 and 5 rows within +-6 ulp, GAMMA.DIST half-integer shapes 5
of 19 rows) is staging, not the constant. Other 15-digit constants of the ported Fortran
(`RT2PIN = 0.398942280401433` in the Temme arm, `GRATIO_THIRD`, the NSWC `gam1` and `rexp` rational
coefficients used by the a < 1 arm, where 11 of 20 GAMMA.DIST rows miss, the Temme `D` tables) are
still as in the source: setting the Temme sqrt(pi) and `THIRD` to full precision changed no row, and
setting `RT2PIN` to full precision lost one (CHISQ.INV oc38), so it was not landed. Whether Excel's code carries
the `gam1`/`rexp` approximations at all, and which published routine its a < 1 arm resembles, is a question for you.


## F.3 The inverses: GAMMA.INV (204 rows), CHIINV (100), CHISQ.INV (40)

Facts about the rows: all from Excel build 20131; p from 0.001 to 0.99; GAMMA.INV shapes 0.5 to 20
and scales 0.5, 1, 2, 3 (60 rows at scale 1, 48 at each other scale); CHIINV and CHISQ.INV df 1 to
50. Earlier rounds measured (fable-52, 300 points each): `GAMMA.INV(p, a, b) = st(b * GAMMA.INV(p,
a, 1))` and `CHISQ.INV(p, 2a) = GAMMA.INV(p, a, 2)` bit for bit; and that
`GAMMA.DIST(GAMMA.INV(p, a, 1), a, 1, TRUE) = p` holds on only 132 of 300 (within 7 ulp of p): the
search does not stop on the public CDF's exact preimage. R35: a fresh set of 81 quantiles refuted
the premise "the answer is the lattice ceiling on the public cumulative" (only 41 satisfy it).
Microsoft's published documentation of these functions says they use an iterative search, and
that the function returns #N/A if the search has not converged after 64 iterations (please check
this against your own knowledge of the documentation and say how much weight it deserves).

Our implementation (appendix B.3) bisects over adjacent doubles on **our own** `gratio` forward,
for CHIINV on the right tail directly, and publishes the upper end, so it returns the exact
preimage of our forward on the lattice. Measured against the true quantile (60-digit root of the
true regularized incomplete gamma; ulps of the true quantile):

| function | rows | rows matching Excel | Excel: mean abs error vs true, max | ours: mean abs error vs true, max |
|---|---:|---:|---|---|
| GAMMA.INV | 204 | 66 | 1.13 ulp, 10.0 | 2.96 ulp, 53.8 |
| CHIINV | 100 | 37 | 1.00 ulp, 20.7 | 3.37 ulp, 90.4 |
| CHISQ.INV | 40 | 14 | 0.67 ulp, 2.7 | 1.67 ulp, 22.0 |

Excel's histogram of |error| against the truth for GAMMA.INV is: 0 ulp (rounded): 79 rows, 1: 78,
2: 26, 3: 11, 4: 6, 5: 1, 7: 2, 10: 1; for CHIINV: 61, 23, 8, 3, 2, 1, 1 and one at 21. So **Excel's
inverse lands within 1 to 3 ulp of the true quantile on 95% of rows: an accurate answer, but not
the correctly rounded one.** The probability-space view, `(P_true(x) - p)/p` in units of 2^-53:
Excel's x has a worst case of 15.1 (GAMMA.INV) and 16.6 (CHIINV), mean absolute 2.5 and 2.3; ours
472.8 and 46.4, mean absolute 14.8 and 3.4. Our worst GAMMA.INV rows (d = +55, +55, +41) are p = 0.07 at shape 20 (scales 0.5, 2, 3), the
a >= 20 Temme arm of our forward, and p = 0.99 at shape 0.5 (d = -14). Excel's inverse values are
accurate to the true quantile even at rows where our forward is inaccurate, so either Excel's
forward is accurate there (its CHIDIST is within about 1 ulp of the truth on most rows, F.2) and
its search converges close to the root, or its search stops on a criterion unrelated to the last
bit of its own forward; we cannot tell which, and a ranking of such search designs (bracketing,
Newton or Halley steps on the forward with a starting guess, secant, bisection with a tolerance,
the 64-iteration cap) against these rows is part of what we ask.

The rows with the largest differences, and the histogram of `d`, per function (the full rows are in the trace files):


```
GAMMA.INV (p, alpha, beta): 204 rows, 66 match; d histogram (ours minus Excel, ulp): -14:1 -9:3 -6:6 -4:5 -3:7 -2:8 -1:29 +0:66 +1:39 +2:21 +3:2 +4:2 +6:1 +7:2 +8:2 +11:1 +12:2 +18:1 +23:1 +31:2 +41:1 +55:2
  the 16 rows with the largest |d| (Excel bits; d; Excel-true; ours-true, in ulps of the true quantile):
    0.07 20 0.5        0x401bac06c70d46b8 d=+55 E-t=-1.2 o-t=+53.8
    0.07 20 2          0x403bac06c70d46b8 d=+55 E-t=-1.2 o-t=+53.8
    0.07 20 3          0x4044c1051549f50a d=+41 E-t=-0.9 o-t=+40.1
    0.15 20 0.5        0x401edb3812f66f4c d=+31 E-t=+1.4 o-t=+32.4
    0.15 20 2          0x403edb3812f66f4c d=+31 E-t=+1.4 o-t=+32.4
    0.15 20 3          0x4047246a0e38d379 d=+23 E-t=+1.1 o-t=+24.1
    0.3 20 3           0x404a27698fd2034e d=+18 E-t=-0.8 o-t=+17.2
    0.99 0.5 1         0x400a8a2255a6e921 d=-14 E-t=-2.0 o-t=-16.0
    0.3 20 0.5         0x40216f9bb536acdf d=+12 E-t=-0.2 o-t=+11.8
    0.3 20 2           0x40416f9bb536acdf d=+12 E-t=-0.2 o-t=+11.8
    0.6 20 3           0x404f377606538c95 d=+11 E-t=+0.7 o-t=+11.7
    0.15 0.75 3        0x3fcc860d8f07da15 d=-9 E-t=+10.0 o-t=+1.0
    0.97 0.75 0.5      0x3ff7bbe7a88e280e d=-9 E-t=+4.3 o-t=-4.7
    0.97 0.75 2        0x4017bbe7a88e280e d=-9 E-t=+4.3 o-t=-4.7
    0.85 20 0.5        0x40289f367bb5c61d d=+8 E-t=-0.1 o-t=+7.9
    0.85 20 2          0x40489f367bb5c61d d=+8 E-t=-0.1 o-t=+7.9

CHIINV (p, df): 100 rows, 37 match; d histogram (ours minus Excel, ulp): -91:1 -32:1 -29:1 -14:1 -13:2 -12:1 -9:1 -8:2 -7:1 -4:5 -3:3 -2:1 -1:10 +0:37 +1:24 +2:5 +4:1 +5:1 +6:1 +7:1
  the 12 rows with the largest |d| (Excel bits; d; Excel-true; ours-true, in ulps of the true quantile):
    0.99 1             0x3f2496fe6cb55211 d=-91 E-t=+0.6 o-t=-90.4
    0.99 2             0x3f9495453e6fd4bc d=-32 E-t=+0.2 o-t=-31.8
    0.99 3             0x3fbd659df1b28c92 d=-29 E-t=-20.7 o-t=-49.7
    0.99 10            0x40047737efe1de5b d=-14 E-t=+3.3 o-t=-10.7
    0.95 1             0x3f701b25cb410893 d=-13 E-t=-0.5 o-t=-13.5
    0.99 5             0x3fe1bccf51f913a7 d=-13 E-t=-2.0 o-t=-15.0
    0.97 4             0x3fe11f28e184d03a d=-12 E-t=+6.7 o-t=-5.3
    0.95 3             0x3fd684a66afcdf95 d=-9 E-t=+1.8 o-t=-7.2
    0.95 2             0x3fba431d5bcc193e d=-8 E-t=-0.3 o-t=-8.3
    0.95 10            0x400f85bb8db0b518 d=-8 E-t=+2.7 o-t=-5.3
    0.97 50            0x404079b778eaef20 d=+7 E-t=+0.5 o-t=+7.5
    0.99 30            0x402de82b743fbf63 d=-7 E-t=-1.1 o-t=-8.1

CHISQ.INV (p, df): 40 rows, 14 match; d histogram (ours minus Excel, ulp): -2:5 -1:5 +0:14 +1:11 +2:2 +4:1 +7:1 +20:1
  the 8 rows with the largest |d| (Excel bits; d; Excel-true; ours-true, in ulps of the true quantile):
    0.02 50            0x403fa9f2e27552b5 d=+20 E-t=+2.0 o-t=+22.0
    0.07 50            0x40420d70bc97df73 d=+7 E-t=-0.3 o-t=+6.7
    0.15 50            0x4043e080c014da6c d=+4 E-t=-0.4 o-t=+3.6
    0.003 4            0x3fc45c518826418d d=-2 E-t=+1.4 o-t=-0.6
    0.3 12             0x4022118cb3722939 d=-2 E-t=+0.9 o-t=-1.1
    0.3 50             0x4046281a716a9b8b d=+2 E-t=-0.2 o-t=+1.8
    0.85 50            0x404e2c498962b028 d=+2 E-t=-0.5 o-t=+1.6
    0.97 7             0x402f04a767d07bf2 d=-2 E-t=-1.9 o-t=-3.9
```


## F.4 POISSON.DIST (49,714 rows; 551 misses and 4 unread rows, 555 failures)

Composition of the bank (rows, misses; k is the truncated count):

| Rows | n | misses |
|---|---:|---:|
| cumulative, k = 0 (= `EXP(-mean)`), all means | 34,277 | 1 (mean 60.0147, +1 ulp) |
| cumulative, k >= 1, mean < 31 | 31 | 3 |
| cumulative, k >= 1, mean 31 to 700 | 4 | 3 |
| cumulative, k >= 1, mean >= 700 (Excel build 20430, "publication" probes) | 1,224 | **542** |
| mass (FALSE), k = 0 | 285 | 0 |
| mass, k >= 1 | 13,449 | 2 (k = 1, mean 2.43e-6 and 13.6, +1 and -1) |
| error rows (#NUM! on both sides, not counted) | 440 | 0 |

The mass is exact on 13,732 of 13,734 rows (and 11,996 of 11,997 fresh ones in the latest
round), so the prefactor machinery is right. The cumulative with `k >= 1` is thinly sampled: only
1,259 rows, 1,224 of them from one probe at means 700 to 1,010, where results sit around the
smallest normal double (2.2250738585072014e-308) to a few times 1e-307, in the zone where Excel
publishes a subnormal or flushes to zero. Of those 1,224 rows 409 have the result 0 (Excel flushes a subnormal result to 0 for k >= 1; all
409 match, and the 94 rows with k = 0 publish subnormals and match too), 719 are in
[2.2250738585072014e-308, 1e-307) (**470 miss**, 249 match) and 96 are larger, up to 1e-192 (72
miss). The 470 misses are -3 to +4 ulp (-1: 179, +1: 131, -2: 77, +2: 49, +3: 18, -3: 13, +4: 3);
they occur for every k tested (1, 2, 3, 4, 7, 11, 19, 29, 53, 89: 29 to 65 misses each) and for
means from 714 to 1,010; **all 719 rows have `EXP(-mean)` subnormal (mean > 708.3964)**. The few cumulative rows with `k >= 1` and mean
below 700 miss 3 of 4 (mean 696.0073, k = 1, +1 ulp; mean 699.45, k = 11, -3; mean 696.44, k = 29,
+3).

Misses at small means, for the record: `POISSON.DIST(0, 60.01467269596327, TRUE)` (+1: Excel
0x3a855d41f796bd11), `POISSON.DIST(1, 5, TRUE)` and `POISSON.DIST(1.1, 5, TRUE)` (build 20430, +1:
Excel 0x3fa4b2efe809fe98, ours ...fe99), `POISSON.DIST(4.25, 6.125, TRUE)` (-1), and the two mass
rows above.

Sample of misses and matches at the large-mean probe (x = truncated count; Excel bits, ours; d):

```
k=1  mean=714.9686572379662  Excel 0x001005ba9aa99af6 (2.2281859863415146e-308)  ours ...9af5 d=-1
k=1  mean=714.9686572379665  Excel 0x001005ba9aa996f2 (2.2281859863410067e-308)  ours ...96f3 d=+1
k=4  mean=731.5992965217341  Excel 0x0010167ca29d0b06 (2.2372894141903697e-308)  ours ...0b05 d=-1
k=7  mean=746.1854892200472  Excel 0x00100000000008f2 (2.2250738585083328e-308)  ours ...08f4 d=+2
k=11 mean=763.9171636323437  Excel 0x00103bc2f0b259c6 (2.2575381452632741e-308)  ours ...59c7 d=+1
k=19 mean=795.9928765080592  Excel 0x0010000000000091 (2.225073858507273e-308)   ours ...0090 d=-1
k=53 mean=909.1269836788059  Excel 0x0010fd41928b615b (2.362650426336584e-308)   ours ...615d d=+2
k=11 mean=699.4526054940352  Excel 0x05c8b339c7e05c23                              ours ...5c20 d=-3
k=29 mean=696.4428875220112  Excel 0x0bd4c4e313ac81b0                              ours ...81b3 d=+3
matched, same probe:
k=89 mean=1010.4599049077266 Excel 0x00118b2563eff8e1 (2.4397294151195152e-308)  d=0
k=53 mean=909.1269836788031  Excel 0x0010fd41928b9465 (2.3626504263430395e-308)  d=0
k=89 mean=1010.5608861606299 Excel 0x0010000000000c39 (2.2250738585087473e-308)  d=0
```

What the earlier rounds established for this function (fable-2, R35, R37):
- The identity `POISSON.DIST(k, m, TRUE) = CHIDIST(2m, 2k+2) = CHISQ.DIST.RT(2m, 2k+2)` bit for bit in
  every branch; and `= 1 - GAMMA.DIST(m, k+1, 1, TRUE)` only in part (89/108 Taylor, 7/98 between
  a and 31, 0/88 asymptotic, 112/168 Temme), since P and Q are published separately.
- The Taylor region (`m < a = k+1`): P = `st(M * ans)` with M the public mass at `a` and `ans` the
  stored forward series `c = st(c * st(m / rr)); ans = st(ans + c)` until `st(c/ans) <= 2^-53`,
  Q = 0.5 + (0.5 - P): 69/69 fresh rows and 16/16 of the W111 rows. This is our code.
- `a <= m < 31`: open. Excel's error against the exact Q has sd 2.9 ulp, max 17.5, uncorrelated
  with Q's size or k. For `m >= 31` the error grows with m (sd 12 ulp below 100, 62 up to 450, 295
  up to 700), the shape of a stored exponent of size m.
- Refuted for `m >= a` (so please do not propose these again without a new argument): the finite sum
  `e^-m sum m^j/j!` forward in binary64, in the register, and by Horner (40/94, 41/66, 47/66 fresh); the
  mass at `a` (or at `k`) times a backward series `1 + k/m + k(k-1)/m^2 + ...` in thirteen store
  arrangements (at most 17/94; 0/252 at `k`); the Cephes-style continued fraction with the mass
  (21/94); Morris's `rcomp` prefactor with five store variants (at most 10/94); the Taylor series
  continued past `a` and complemented (at most 4/94); and for `m >= 31` the mass at `a` with the NSWC
  asymptotic tail (23/110 fresh, 61/252) or with the continued fraction (23/110).
- R35 research (not landed; it lost 4 inverse exact pins): for integer shape 2 to 20 with `shape < x
  <= 30`, a stored Horner with `RN53` reciprocal-factorial loads matched 2,523 fresh shape 2..19
  rows and 261 shape 20 rows on POISSON.DIST, CHIDIST and GAMMA.DIST aliases; it does not extend
  to shape 21 and above.


---

# Appendix G: what already matches exactly, and the methods that got it there

These are results with fresh Excel answers asked after the hypothesis was frozen (round numbers
are our research rounds; "fresh" means inputs not used to build the hypothesis). They are the
vocabulary of shapes that Excel's code evidently uses; a shared helper among them (a log, an exp, a
Bernoulli series, a constant) may be the common cause of the misses.

1. **GAMMALN, x >= 8** (matched 364/364 and 5,822/5,822 in the bank; R44: 9,312 fresh inputs plus
   900 controls all exact, 2,048/2,048 more fresh in [8,128], reciprocal division separating PC64
   from binary64): `q1 = st((x - 0.5) * LN(x))`, `q2 = st(q1 - x)`, `q = st(q2 + c)` with
   `c = 0.91893853320467278` (the double nearest ln(2 pi)/2), then
   `st(q + S(x))` where `S` is the ten-term Bernoulli series in `z = st(1/x)`, `t = st(z*z)`:
   Horner from the last coefficient, each product and each sum stored, then `st(z * w)`. The ten
   coefficients are the exact quotients 1/12, -1/360, 1/1260, -1/1680, 1/1188, -691/360360, 1/156,
   -3617/122400, 43867/244188, -174611/125400, each rounded to double. Nine and eleven terms and
   an fdlibm-style correction were refuted (the fdlibm form disagrees on 525 of 9,408 designed
   inputs). The coefficients above the leading 1/12 and the store sites inside the Horner loop are
   not individually proven; the program as a whole is.
2. **GAMMALN, 3 < x < 8** (R49: 1,348/1,348 fresh in (3,8) with the public lower answer; 30,000 of
   30,000 earlier): with `y = x - floor(x)`, `n = floor(x)`: `z = y + 2`, then `z = st(z * (y+k))`
   for `k = 3 .. n-1` (from the lowest factor up, each product stored), result `st(LN(z) +
   GAMMALN(2 + y))` where `GAMMALN(2 + y)` is Excel's own published answer on [2,3). The same
   product with the highest factor first fails 104 to 113 of 120; a sum of logs fails 61 to 99 of
   120. The endpoint x = 3 is exceptional (GAMMALN(3) is one ulp above LN(2)). It is exact whenever
   Excel's own lower band value is used, so (our inference) the (3,8) misses come from our [2,3)
   band alone.
3. **GAMMALN, x < 0.7** (R33: 1,217 public compositions and 257 mined discriminants; R45: 5,769 of
   5,769; R49: 274 of 274): `GAMMALN(x) = st(GAMMALN(x + 1) - LN(x))`, the join at the literal 0.7
   (last rule row 0x3fe6666666666664, first fail 0x3fe6666666666666, RN(0.7)), with Excel's own LN.
   Cody's 0.6796875 is excluded: someone typed `0.7`.
4. **GAMMA at integers and half-integers**: `(n-1)!` as the plain double product of the integers from
   n-1 down to 2, and `Gamma(n + 1/2)` as the plain double product of `k + 1/2` for `k = 0 .. n-1`
   followed by one multiply by RN(sqrt(pi)) = 0x3ffc5bf891b4ef6b (the bank has only 19 positive integer and 4 positive
   half-integer rows, all matching; #NUM! beyond 171 and 171.5).
5. **GAMMA, x >= 10** (matched 5,198 of 5,207): `sqrt(2 pi) * x^x * exp(-x) * sqrt(1/x) *
   exp(s(x))` with every stage stored as described in the comment of `gammaln.rs:365-380` (B.2),
   `x^x` as `(x^n) * EXP(st(f * LN(x)))` with `n = floor(x/2)`, `x^n` by binary powering, `v` used
   twice, and `s(x)` the same ten-term Bernoulli series (stored once; 5,970 of 6,000 on 10..36,
   17,988/18,000 adjacent). A truncated series left a unit slope near 11.
6. **GAMMA below 10** (R-rounds: 14,000/14,000 given Excel's own `GAMMA(1+y)`): `Gamma(x) =
   st(st((x-1)(x-2)...) * seed)` with `seed = Gamma(1 + y)`, `y = x - floor(x)`; for `x < 1` the seed
   divided by `y`, then by each lower factor, in the register. Only the seed is unknown. For
   `0 < x <= 1e-16`: `1/x` (`excel_x87_recip`; recorded on the decade grid from 1e-307; 2e-16 misses).
7. **GAMMA, x <= -10** (71% exact; misses +-1): `pi / (Gamma(m) * z)`, `m = -x`, `z = st(sin_st(pi t) *
   m)`, `t` the fraction of `m` folded to at most 1/2 (`pi t` stored, SIN our port of the Microsoft CRT sine), sign by
   the parity of `floor(m)`.
8. **The Poisson mass** (11,996 of 11,997 fresh): below `lambda = 1.1` (or `x <= 1`):
   `EXP(st(x LN(lambda)) - lambda - GAMMALN(x+1))` with the two subtractions compensated (the exp of
   the last stored difference times the stored `1 + dropped` per subtraction); above it, Loader's
   saddle point `EXP(-s(x) - dev(x, lambda)) / sqrt(2 pi x)` with `s` a table to n = 6 (each value cut,
   not rounded, to 16 decimals) then ten series terms, and for non-integer n a downward
   recurrence `s(n) = s(n+1) + (n + 1/2) ln(1 + 1/n) - 1` from the series at `n + k` in [6,7); the
   deviance by a stored series of `atanh v` inside `|x - np| < 0.2 (x + np)`, else
   `x LN(x/np) + (np - x)` with each stage stored. All exact on a 1,000-row window at x = 100, 1,199 of
   1,200 at x = 1,024.
9. **GAMMA.DIST**: shape 1 cumulative `= EXPON.DIST(x/b, 1, TRUE)` (85/85 plus 11/11 at b = 3, 5);
   shape 0.5 cumulative `= ERF.PRECISE(sqrt(x/b))` (68/68); the cumulative divides by the scale once
   (stored) and runs a unit-scale body shared with CHISQ.DIST (300/300); the density is the Poisson
   mass at `a` with mean `y = st(x/b)`, times the stored `a/y` (R30: 128 anchor groups, 512 scaled
   answers; every one of the 750 unit-shape rows). Integer-shape cumulatives, 64,000 rows: 10 misses.
10. **CHIDIST**: df = 1 `= ERFC.PRECISE(sqrt(x/2))`, df = 2 `= EXP(-(x/2))` (68/68); all but 6 of the 83
    even df 4 to 10 rows match, and all 30 with df 12 and above. **GAMMA.INV `= st(b * unit-scale inverse)`** (300/300) and
    `CHISQ.INV(p, 2a) = GAMMA.INV(p, a, 2)` (300/300).
11. **Elementary leaves**: LN and EXP chains (x87 `FYL2X`; `F2XM1`/`FSCALE`) and the x87 double-rounded
    operations are bit-exact on huge banks, as are the deviance and series helpers of BINOM.DIST (70,320 of
    70,349 rows).

**Common-cause candidates for the remaining misses** (our ranking, challenge it): (i) the double-
rounded constants `sqrt(2 pi)` (+0.659 units) and `ln(2 pi)/2` (+0.349 units) appear in GAMMA and
GAMMALN respectively; the GAMMA seed must carry the first. (ii) GAMMALN is a leaf of the Poisson mass
and of GAMMA.DIST's density below `lambda = 1.1`, so its unidentified [0.7, 3) bands propagate into
the incomplete-gamma family (99 of the 109 density misses, and POISSON.DIST at small means). (iii)
Our incomplete gamma ratio's a < 1 arm uses NSWC coefficients typed with 15 digits (F.2; the 15-digit sqrt(pi) was one such cause and is fixed). (iv) The inverses use a
stopping rule unlike Excel's.


---

# Appendix H: what earlier research rounds concluded (and where they disagree or were refuted)

Each entry names the round (R<n> is one of our four-hour research rounds by Codex agents; "Fable" and
"fable-<n>" are rounds by another model; "guide" is a statement by the project's owner).
**Earlier conclusions may be wrong.** Where reports disagree, both are given.

**Refuted: the math.lgamma reference (R49, correcting a Fable-1 round of 2026-11-09).** A Fable
round (notes `fresh-ideas-fable-1`, scripts `e1c`, `e1e`) used Python's `math.lgamma` as the
reference for the GAMMALN core on [0.7, 3) and concluded the core has a "flat absolute error of
about 2e-16 to 5e-16 that is the same at the roots as elsewhere", hence "not `(x-1) R(x)`, not Cody's
shape, not the logarithm of a stored double, probably a Lanczos-style difference of large terms", and
proposed a "binade jump" signature to find Lanczos `g`. R49 re-computed the same 600 answers at 80
digits: the maximum error against the true value is 7.3e-17, 6.3e-17, 6.3e-17, 1.3e-16, 3.2e-16 on
[0.7,1), [1,1.5), [1.5,2), [2,2.5), [2.5,3), and `math.lgamma` itself errs by up to 1.1e-15, so the
"noise" was the fallback's own error. Fresh windows (55 windows of 256 adjacent inputs, 14,080
answers) show the absolute error shrinking at both roots, and six predicted temporary-binade
crossings for `g` = 5, 7, 607/128, 9 gave adjacent sd ratios of 0.914 to 1.044, not a doubling.
**Any "proof" must be against Excel's answers only, never against a library's value as truth.**
What stood from that Fable round and was independently confirmed (R49, public-leaf controls): the
(3,8) reduction (600/600, then 1,348/1,348) and the literal-0.7 join. What did not stand: the flat
error, "not a rational", and the Lanczos conclusion.

**R2 to R5 (Oct 31).** Rounds 2 and 3 added fitted "Cody and libm disagreement bounds" for GAMMA and a
fitted residual correction for GAMMA.DIST shapes 2 and 3 (GAMMA from 439 to about 1,140 matches);
these were empirical emulations, not identified branches, and round 5 (owner correction) reverted all
of them. The product-and-seed, Stirling and reflection forms of G.5 to G.7 were identified later.
The rounds also ruled out, on the GAMMA.DIST cumulative (dc-j9k.5): series order and summation,
shifted density prefactors, directed whole-kernel rounding, full-log gamma CDF graphs (88,704 per
cohort), published AMD split exp and FMA graphs; and measured that `P` is not `1 - Q` in Excel.

**R30 (Nov 6).** GAMMA.DIST density: scaled calls identify the final stored `alpha/x` ratio followed
by multiplication (128 anchor groups, 512 scaled answers); the prefix was left open then; most of
it is now produced by the Poisson-mass route (F.1: 915 of 915 exact for `a >= 2`).

**R32 and R33 (Nov 6 and 7), the GAMMA seed.** Windows of 1,024 consecutive doubles (208 windows,
212,992 distinct inputs) between 1 and 2: a degree-six phase model of the first stage line `L1`
predicts seven coarse windows within 0.014 cycles; a conditional relation `L5 = 2 (x-1) L1 + const`
predicts them within 0.025 cycles. The coarse windows selected the initial slopes, so this is not a
blind test. Conclusion at the time: a smooth phase relation among stage lines, **no absolute
coefficient table recovered**; "the imposed derivative identity is tautological" (R33). The bead
`dc-j9k.20` states: "the GAMMA seed on (1,2): a chain of stored products from binary64 sqrt(2 pi),
about five stored smooth quantities; the Cody rational stands in for its value." R33 also refuted
eight recurrence trees for `x < 1` on 142 of 864 calls and showed that even an arbitrary common
resident seed leaves 61 of 288 groups inconsistent: **the below-one GAMMA is not simply seed/x in any
of eight stored/resident arrangements**.

**R33 (GAMMALN below 0.7).** Final subtraction uses the worksheet LN (fresh controls 4 to 250 of
257; three losses localized to the upper band).

**R44 (Nov 9).** GAMMALN x >= 8, ten stored Bernoulli terms (G.1).

**R45 (Nov 9), the lower GAMMALN tables.** 25,187 answers: the independent lower recurrence matches
5,769/5,769; shift-to-8 programs (one stored product then LN; reverse subtraction of public logs;
sum of logs), 32 band/program combinations of the Cody basis changes (x or 1/x, normalized or
original coefficients, four store patterns), and 12 sampled combinations of the cleared g = 7,
nine-coefficient Lanczos form all contradict the lower windows; a reflection composition with host
SIN, ln(pi), LN and GAMMALN contradicts them; the first-order small-x series contradicts them. R33's
spectral peaks (eight at 1.3, seven at 1.625) refute the published tables' stage frequencies. The
approximate B3 table extrapolated below 2 is refuted. **No Lanczos form is identified by any of
this, and none is excluded in general.**

**The B3 table.** Its provenance and reliability (98% of rows within 0.002 of 2.5 exact, 70% on [2.4666, 2.6031],
32% elsewhere on [2,3]; `Q(0)` from an identity, `P(0)` from a minimum at 2.5) are in the code comment in B.1.

**POISSON.DIST and the incomplete gamma (fable-2, R35, R36, R37).** See F.4. Also R36/R37: "POISSON.DIST
and CHISQ.DIST.RT agree on 120 and then 252 paired controls; one minus GAMMA.DIST agrees on 76 and
118". R37: the known finite polynomial (R35's stored Horner with RN53 reciprocal-factorial loads, shape
2..20) matches 45 of 45 interior controls and 25 of 207 outside the dispatch.

**GAMMA.INV (R35, R24, fable-52).** R35: 81 fresh quantiles refute "lattice ceiling on the public
CDF". R24: three still-failing GAMMA.INV outputs changed with the Temme final correction. fable-52:
`GAMMA.DIST(GAMMA.INV(p, a, 1), a, 1, TRUE) = p` only 132 of 300 within 7 ulp.

**dc-ymjf.6 fix pass (commit 4cc4ba08, Nov 10): landed.** The half-integer finite sum of `gratio` used a 15-digit
sqrt(pi) (found in this question's first version). Setting it to binary64 moved CHIDIST 371 to 372 matched
(worst 24 to 6 ulp), CHIINV 30 to 37, CHISQ.INV 13 to 14, GAMMA.DIST 77,804 to 77,808, GAMMA.INV 63 to 66, with
one row lost (CHIINV oc3, now 1 ulp high). Tried and not landed: Temme's sqrt(pi) and `THIRD` at full precision
changed no row; `RT2PIN` at full precision lost one row (CHISQ.INV oc38). GAMMA, GAMMALN, GAMMALN.PRECISE
and POISSON.DIST counts did not change. Full bank at this baseline: 3,889,129 of 3,910,795.

**What kind of evidence has worked.** Store-mask enumeration, graph search and "fit a coefficient until
the count rises" were tried many times (R2 to R5, R24, R30, R33, R45) and are not accepted. What has
worked: *regime signatures* (where a join is, whether the error vanishes at a root, which public
function is the leaf of another) and *stage-line analysis* on windows of consecutive doubles (Excel's
error against a candidate, as a function of the argument's bits, has a sawtooth whose frequency
names a stored quantity; its phase and slope give the quantity's size).


---

# Appendix I: the constants of our stand-in tables, as numbers

All coefficients ascending in powers of `x` (the argument itself) unless said otherwise, 17
significant digits. The B2 table (SPECFUN dlgama `P2`, `Q2`: 8 coefficients each, in `t = x - 2`) and the
GAMMA seed table (SPECFUN gamma `P`, `Q`: 8 coefficients each, in `y`) are published tier-2 tables and are not
reproduced; both are stand-ins that Excel does not use.

**B1, [0.7, 1.5): Cody-Hillstrom 1967, Table II, n = m = 7 (plain double Horner descending).**
`GAMMALN(x) = (x - 1) * N(x) / D(x)`, evaluated as `(x-1) * (N/D)`:

```
P1 (N, degree 7), c0..c7:
 -2.2088439972161824   -77.410640713329528   -517.63834980232184   -922.26137288015218
 -261.7218583856145    243.17524352442103     85.689820628313171     4.1208431858477699
Q1 (D, degree 7, monic), c0..c7:
  0.40977929210926151   24.435196625063117    262.30834702694602    846.07553620207818
  951.32359767970593    377.83724848239422     45.646771875859081    1
```

**B3, [2, 3]: read from Excel's answers (the low bits of the coefficients are not all read).**
`GAMMALN(x) = st( st(P(x)/Q(x)) * (x - 2) )` with `P`, `Q` degree 8 in `x`, Horner from the top coefficient,
each product `st(w*x)` and each sum `st(.. + c)` stored except the last sum of each polynomial,
which stays in the register for the division; `x = 3` is returned as the constant 0x3fe62e42fefa39f0.

```
P (degree 8), c0..c8:
 -2.266786438167228     -29.907511646102087     -66.721979913050035    -2.0639757251365638
 62.082398083771913     33.248586310798345       5.3620587366274837     0.26453758918906523
  0.0026730135451372965
Q (degree 8), c0..c8:
  1.0000000000157898     25.696574388696767     123.4678964737532      189.61325507101793
  113.27059743840934     27.704029458859928       2.648541159441014      0.082760427448873825
  0.00049551480844350372
```

Other constants: `D2 = 0.42278433509846713` (1 - Euler's gamma, B2's linear term),
`D4 = 1.791759469228055` (ln 6, an unused band), `LS2PI = 0.91893853320467278` (ln(2 pi)/2),
`SQRT_2PI = 2.5066282746310007` (0x40040d931ff62706), `THR07 = 0.7` (0x3fe6666666666666),
`sqrt(pi) = 1.7724538509055159` (0x3ffc5bf891b4ef6b), `ln 2` = 0x3fe62e42fefa39ef, `ln 6` =
0x3ffcab0bfa2a2002 (this is also Excel's GAMMALN(4)).

(End of the question. Thank you. If you can only do part of it, please do deliverable A for GAMMALN and
for the GAMMA seed first, then B for those two.)
