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

// ---- B1 [0.7,1.5): Cody & Hillstrom 1967 Table II, n=m=7 (ascending i=0..7) --
const P1: [f64; 8] = [
    f64::from_bits(0xC001_ABB6_66D0_134A),
    f64::from_bits(0xC053_5A47_EFFC_8A03),
    f64::from_bits(0xC080_2D1B_5724_230B),
    f64::from_bits(0xC08C_D217_4AAA_2285),
    f64::from_bits(0xC070_5B8C_BB60_E8EC),
    f64::from_bits(0x406E_659B_984E_C72C),
    f64::from_bits(0x4055_6C26_056B_AD85),
    f64::from_bits(0x4010_7BBE_50EC_A47A),
];
const Q1: [f64; 8] = [
    f64::from_bits(0x3FDA_39D2_EC8B_FCDF),
    f64::from_bits(0x4038_6F69_0BC7_F9C5),
    f64::from_bits(0x4070_64EE_FD4A_C8D5),
    f64::from_bits(0x408A_709A_B2B9_6CB8),
    f64::from_bits(0x408D_BA96_BA61_5B30),
    f64::from_bits(0x4077_9D65_5EAA_2824),
    f64::from_bits(0x4046_D2C9_6BBB_64C7),
    f64::from_bits(0x3FF0_0000_0000_0000), // 1.0 (monic)
];

// SPECFUN DLGAMA P2/Q2: https://netlib.org/specfun/algama.
// Published coefficients in the original Horner order, with continuous x87
// arithmetic. Exact Excel agreement for this band is not established.
const P2: [f64; 8] = [
    4.974607845568932035012064e0,
    5.424138599891070494101986e2,
    1.550693864978364947665077e4,
    1.847932904445632425417223e5,
    1.088204769468828767498470e6,
    3.338152967987029735917223e6,
    5.106661678927352456275255e6,
    3.074109054850539556250927e6,
];
const Q2: [f64; 8] = [
    1.830328399370592604055942e2,
    7.765049321445005871323047e3,
    1.331903827966074194402448e5,
    1.136705821321969608938755e6,
    5.267964117437946917577538e6,
    1.346701454311101692290052e7,
    1.782736530353274213975932e7,
    9.533095591844353613395747e6,
];

// ---- B3 [2,3]: read from Excel's answers, lowest power first. See `b3_with`. ------------------
const B3_P: [u64; 9] = [
    0xC002_2260_ED97_8E7B,
    0xC03D_E852_AEE8_BF62,
    0xC050_AE34_EB3C_BACE,
    0xC000_8305_B479_986D,
    0x404F_0A8C_0539_86D7,
    0x4040_9FD1_AD1D_8E5C,
    0x4015_72BF_8684_2F3A,
    0x3FD0_EE2F_1188_4D4E,
    0x3F65_E5B7_3842_993C,
];
const B3_Q: [u64; 9] = [
    0x3FF0_0000_0001_15C7,
    0x4039_B252_B2FA_AF0E,
    0x405E_DDF2_040D_2BC2,
    0x4067_B39F_C919_441A,
    0x405C_5151_77EB_165E,
    0x403B_B43B_464D_3956,
    0x4005_3036_58EF_45CA,
    0x3FB5_2FC9_914B_BFB2,
    0x3F40_3CAD_F2CF_35CF,
];

// ---- B4 [4.0,8.0): SPECFUN dlgama published P4/Q4 (ascending Horner order) --
const P4: [f64; 8] = [
    f64::from_bits(0x40CC_CC82_C5C6_4704),
    f64::from_bits(0x4142_83DE_AF4B_5720),
    f64::from_bits(0x419C_F647_959E_37B1),
    f64::from_bits(0x41E3_D818_2034_30F7),
    f64::from_bits(0x421B_6268_E3F9_61D5),
    f64::from_bits(0x4243_D256_DDE8_451B),
    f64::from_bits(0x425C_AC7F_DC02_6F8F),
    f64::from_bits(0x4260_50FB_AE6A_CCA5),
];
const Q4: [f64; 8] = [
    f64::from_bits(0x40A5_050F_7336_3548),
    f64::from_bits(0x4123_8339_2180_0ACC),
    f64::from_bits(0x4183_B856_FA6B_57F8),
    f64::from_bits(0x41D0_B3C9_AB67_6EF8),
    f64::from_bits(0x420B_BA43_6E36_4E1B),
    f64::from_bits(0x4237_AC9F_24E3_3E6B),
    f64::from_bits(0x4253_E46F_C45D_AF37),
    f64::from_bits(0x4259_FA9F_BBBD_7E2A),
];

/// B1 band `[0.7, 1.5)`: `(x-1) * N(x)/D(x)`, every op plain double, Horner
/// descending over the ascending-stored coefficients.
fn b1(x: f64) -> f64 {
    let mut n = 0.0_f64;
    for &c in P1.iter().rev() {
        n = n * x + c;
    }
    let mut d = 0.0_f64;
    for &c in Q1.iter().rev() {
        d = d * x + c;
    }
    (x - 1.0) * (n / d)
}

/// STIRLING band `x >= 8`: worksheet LN, stored main expression, and the
/// ten-term Bernoulli correction with each operation stored from PC64.
///
/// R44: single-bit windows around integers and exponent edges locate the
/// correction's rounding changes. At 192 designed half-grid crossings the
/// old fdlibm tail disagrees on 525/9408 inputs. The ten-term correction
/// agrees on all; nine and eleven terms and unstored arithmetic are separated
/// by additional controls. A second frozen set agrees on all 10,212 inputs
/// (9,312 new arguments and 900 retained boundary controls).
/// Coefficient neighbours above the leading 1/12 remain undetermined in
/// the searched domain; the coefficients are the existing Bernoulli table,
/// not fitted last bits.
fn stirl8(x: f64) -> f64 {
    let lg = super::excel_log(x);
    let q1 = super::excel_x87_mul(x - 0.5, lg);
    let q2 = super::excel_x87_sub(q1, x);
    let q = super::excel_x87_add(q2, LS2PI);
    super::excel_x87_add(q, super::x87_chains::stirling_series_stored(x))
}

// -------------------------------------------------------------------------
// B2 / B4 extended-precision bands.
// -------------------------------------------------------------------------

/// B2 band `[1.5, 4.0)`: SPECFUN's `t*(D2 + t*(P/Q))`, `t = x-2`,
/// with one final binary64 store after continuous extended arithmetic.
fn b2(x: f64) -> f64 {
    b2_with::<super::Native>(x)
}

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

/// B4 band `[4.0, 8.0)`: `D4 + u*(XNUM/XDEN)`, `u = x-4`. Everything extended
/// (x87 continuous), one final round. `XDEN` leading coefficient is `-1`.
fn b4(x: f64) -> f64 {
    b4_with::<super::Native>(x)
}

fn b4_with<F: super::Fpu>(x: f64) -> f64 {
    use super::soft87::{CW_PC64_RN as CW, ext_from_f64};

    let u = ext_from_f64(x - 4.0);
    let mut xnum = ext_from_f64(0.0);
    let mut xden = ext_from_f64(-1.0);
    for i in 0..8 {
        xnum = F::add(&F::mul(&xnum, &u, CW), &ext_from_f64(P4[i]), CW);
        xden = F::add(&F::mul(&xden, &u, CW), &ext_from_f64(Q4[i]), CW);
    }
    let r = F::div(&xnum, &xden, CW);
    let t1 = F::mul(&u, &r, CW); // u * r
    let t2 = F::add(&ext_from_f64(D4), &t1, CW); // D4 + u*r
    F::store(&t2, CW)
}

/// The band `3 < x < 8`. With `y` the fraction of `x` and `n` its integer part, Excel forms
/// `z = (y+2)(y+3)...(y+n-1)` from the left, stores every product to binary64, stores `ln z`,
/// and adds the stored value of the `[2,3)` band at `2+y`.
///
/// Read from 30,000 Excel 16.0.20527 answers at 6,000 random fractions: for `n = 3..=7` this
/// identity holds 6,000 of 6,000 on the answers `GAMMALN(2+y)`. It takes no fitted constant.
/// R49 independently confirms 1,348 more inputs in `(3,8)`, including adjacent integer
/// boundaries. The endpoint `x=3` is excluded: its published answer is one ulp above
/// `LN(2)`, and is handled by `b3`. This identity is conditional on the public lower
/// answer; it does not identify the unresolved core table. The literal `0.7` join
/// and this reduction were already implemented before the R49 recheck.
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

// -------------------------------------------------------------------------
// Worksheet GAMMA.
// -------------------------------------------------------------------------

/// `2.5066...`: binary64 `sqrt(2 pi)`. Its rounding error, +0.66 of 2^-53, is the bias Excel's
/// answers show on every band of GAMMA.
const SQRT_2PI: f64 = f64::from_bits(0x4004_0D93_1FF6_2706);

// SPECFUN GAMMA: `1 + N(y)/D(y)` for `Gamma(1+y)`, `0 <= y < 1`. W. J. Cody and L. Stoltz, latest
// modification October 12, 1989, https://netlib.org/specfun/gamma; every digit below compared
// with that page's double-precision DATA P and Q on 2026-11-04. Excel's own seed on
// this interval is not identified: it is a chain of stored multiplications that starts from
// `SQRT_2PI`, and this rational stands in for its value only.
const SEED_P: [f64; 8] = [
    -1.71618513886549492533811e0,
    2.47656508055759199108314e1,
    -3.79804256470945635097577e2,
    6.29331155312818442661052e2,
    8.66966202790413211295064e2,
    -3.14512729688483675254357e4,
    -3.61444134186911729807069e4,
    6.64561438202405440627855e4,
];
const SEED_Q: [f64; 8] = [
    -3.08402300119738975254353e1,
    3.15350626979604161529144e2,
    -1.01515636749021914166146e3,
    -3.10777167157231109440444e3,
    2.25381184209801510330112e4,
    4.75584627752788110767815e3,
    -1.34659959864969306392456e5,
    -1.15132259675553483497211e5,
];

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
    let root_of_reciprocal = Native::store(&Native::sqrt(&ext(super::excel_x87_div(1.0, x)), CW), CW);
    let scaled = super::excel_x87_mul(v, super::excel_x87_mul(super::excel_exp(-x), root_of_reciprocal));
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
    let folded = if fraction > 0.5 { 1.0 - fraction } else { fraction };
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The read table stays on the true function over the whole band.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn b3_stays_on_the_true_function() {
        for k in 1..=99 {
            let x = 2.0 + f64::from(k) / 100.0;
            let want = libm::lgamma(x);
            assert!((b3(x) - want).abs() <= 8.0 * f64::EPSILON * want.abs(), "b3({x})");
        }
    }

    /// The bands above 3 and below 10 from Excel's own answer at the bottom of the recurrence.
    /// Excel 16.0.20527, at `y = 0x3fe437dbd9dd94d0`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn recurrences_reproduce_excel_from_its_own_lower_answer() {
        let band = f64::from_bits(0x3fd86b3b37f8004f);
        for (x, answer) in [
            (0x400d0df6f6776534_u64, 0x3ff59667b84cf002_u64),
            (0x401286fb7b3bb29a, 0x40051c93ade853cc),
            (0x401686fb7b3bb29a, 0x4010b00761197e21),
            (0x401a86fb7b3bb29a, 0x401799f191ccab24),
            (0x401e86fb7b3bb29a, 0x401f2b3a51b521e9),
        ] {
            assert_eq!(recurrence_from(f64::from_bits(x), band).to_bits(), answer, "GAMMALN({x:#x})");
        }
        let seed = f64::from_bits(0x3fecb842a2a183d9);
        for (x, answer) in [
            (0x40050df6f6776534_u64, 0x3ff76ec725af336b_u64),
            (0x400d0df6f6776534, 0x400ed5d913708740),
            (0x401286fb7b3bb29a, 0x402bff419341a743),
            (0x401686fb7b3bb29a, 0x405035adcb694aff),
            (0x401a86fb7b3bb29a, 0x4076d28f4e27e2e5),
            (0x401e86fb7b3bb29a, 0x40a2eb59830fa6d7),
            (0x4021437dbd9dd94d, 0x40d20c722095858b),
            (0x4023437dbd9dd94d, 0x4103795b37a801f2),
        ] {
            assert_eq!(product_times_seed(f64::from_bits(x), seed).to_bits(), answer, "GAMMA({x:#x})");
        }
    }

    // Analytic sanity (loose): the published surface must land near the true
    // values across every band. Bit-exactness is checked by the W109 racer
    // (`smart-fuzzer/tools/calc_graph_racer` bin `check_gammaln_port`).
    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "got {a}, want {b}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn zeros_at_1_and_2() {
        // Both are exactly 0 by construction. x=1.0 yields -0.0 (the (x-1)
        // prefactor times a negative N/D), exactly as the identified op-graph
        // and the reference model do; x=2.0 (B2, t=0) yields +0.0.
        assert_eq!(gammaln_excel(1.0), 0.0);
        assert_eq!(gammaln_excel(1.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(gammaln_excel(2.0).to_bits(), 0.0_f64.to_bits());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn matches_reference_across_bands() {
        close(gammaln_excel(0.5), 0.5723649429247001, 1e-12); // composed -> B2
        close(gammaln_excel(1.2), -0.0853740900033158, 1e-12); // B1 (negative)
        close(gammaln_excel(3.0), 0.6931471805599453, 1e-12); // B2  (ln 2)
        close(gammaln_excel(5.0), 3.1780538303479458, 1e-12); // B4  (ln 24)
        close(gammaln_excel(10.0), 12.801827480081471, 1e-11); // Stirling (ln 362880)
        close(gammaln_excel(1e-300), 690.7755278982137, 1e-9); // composed, xp->1
    }

    /// dc-8dg: the B2/B4 bands give the same bits on the software x87 as on the hardware.
    #[cfg(target_arch = "x86_64")]
    /// Native-only: it runs the x87 hardware, which only x86_64 has.
    #[test]
    fn extended_bands_equal_on_the_software_x87() {
        use super::super::{Hard, Soft};
        let mut s = 0x0123_4567_89ab_cdefu64;
        for i in 0..100_000 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            let unit = (s >> 11) as f64 / (1u64 << 53) as f64;
            let x = if i % 2 == 0 { 1.5 + 2.5 * unit } else { 4.0 + 4.0 * unit };
            if x < 4.0 {
                assert_eq!(b2_with::<Hard>(x).to_bits(), b2_with::<Soft>(x).to_bits(), "B2 {x:e}");
                assert_eq!(b3_with::<Hard>(x).to_bits(), b3_with::<Soft>(x).to_bits(), "B3 {x:e}");
            } else {
                assert_eq!(b4_with::<Hard>(x).to_bits(), b4_with::<Soft>(x).to_bits(), "B4 {x:e}");
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn stirling_ln87_dr_matches_recorded_controls() {
        // Current-build discovery (build 20228, Value2). Native ln missed
        // old-resid-00/08; LN87+DR matches Excel.
        assert_eq!(
            gammaln_excel(f64::from_bits(0x4020e83b9403b940)).to_bits(),
            0x4022e7db126472c6
        );
        assert_eq!(
            gammaln_excel(f64::from_bits(0x4029800000000000)).to_bits(),
            0x40335bb50a8f4512
        );
        // R44 re-read these former misses on Excel build 20527. The ten-term
        // stored correction now gives the observed answers, one unit higher.
        assert_eq!(
            gammaln_excel(f64::from_bits(0x40215bf4d43f4d44)).to_bits(),
            0x4023d98694477879
        );
        assert_eq!(
            gammaln_excel(f64::from_bits(0x40234ce3244e3245)).to_bits(),
            0x40280a8dd6771c9a
        );
    }
}
