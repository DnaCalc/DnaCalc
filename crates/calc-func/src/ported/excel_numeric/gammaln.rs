//! Excel `GAMMALN` / `GAMMALN.PRECISE` positive-argument kernel and worksheet `GAMMA` (dc-j9k.20).
//!
//! Bands of `GAMMALN`, dispatching on the positive argument `x`:
//!   * `x < 0.7`         `core(x+1) - ln(x)`, one double subtract.
//!   * `0.7 <= x < 1.5`  B1: a stored degree-9/8 Horner quotient in raw `x`, times `x-1`.
//!   * `1.5 <= x < 2`    B2: stands in SPECFUN's table in x87 continuous arithmetic.
//!   * `2 <= x <= 3`     B3: Excel's own form, read from its answers: see [`b3_with`].
//!   * `3 < x < 8`       the recurrence from the `[2,3)` band, identified: see [`recurrence`].
//!   * `x >= 8`          Stirling: ten Bernoulli terms, worksheet LN and stored stages.
//!
//! Excel's bands below 3 are rationals in `x` itself: noise reaches a full unit of the result at
//! `x = 1 + 2^-52` and `2 + 2^-51`, where a polynomial in `x - 1` or `x - 2` could carry none, and the
//! steps of the mean error mark Horner products stored before each coefficient is added. The table
//! for `[1.5,2)` remains under study. The bands `[1.5,2)` and `[2,3]` are separate.
//!
//! The x87 operations run on `super::x87::raw` on `x86_64`, and on `super::soft87` elsewhere.
// ---- band anchors (correctly-rounded transcendentals) --------------------
const D2: f64 = f64::from_bits(0x3FDB_0EE6_0720_93CE); // 1 - EulerGamma
const D4: f64 = f64::from_bits(0x3FFC_AB0B_FA2A_2002); // ln 6
const LS2PI: f64 = f64::from_bits(0x3FED_67F1_C864_BEB5); // ln(2*pi)/2

// ---- dispatch threshold ---------------------------------------------------
const THR07: f64 = f64::from_bits(0x3FE6_6666_6666_6666); // 0.7

// Read from the phases of Excel 20527's Horner products and the changes in
// rounding when a partial sum crosses a power of two. Lowest power first.
const B1_N: [u64; 10] = [
    0xbf55_9f97_cb25_82ff, 0xbfa9_2587_e5c1_24df, 0xbfd8_558f_6ad8_f440, 0xbfed_3f13_70f3_47b5,
    0xbfe4_c00f_820c_a19d, 0x3fb5_9e7c_382c_45e3, 0x3fc6_d0d4_bff8_a504, 0x3fa3_038d_a170_c471,
    0x3f5c_3a9d_0165_803a, 0x3ed3_3ada_3fab_ee89,
];
const B1_D: [u64; 9] = [
    0x3f30_0000_0000_0001, 0x3f8f_168c_3e77_cf01, 0x3fc7_2eb2_7395_650d, 0x3fe6_b0b4_a083_5670,
    0x3ff1_8e83_8fb3_5c8f, 0x3fe6_e6b1_d4f6_36f5, 0x3fc8_a4a6_67dd_ba05, 0x3f93_5397_cc53_8025,
    0x3f3e_bffd_da5d_d26d,
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

// B3 on [2,3], read from the stored Horner partials of Excel 20527. Lowest power first.
const B3_P: [u64; 9] = [
    0xC002_2260_ED97_8401,
    0xC03D_E852_AEE8_C149,
    0xC050_AE34_EB3C_BAD5,
    0xC000_8305_B479_9884,
    0x404F_0A8C_0539_86D7,
    0x4040_9FD1_AD1D_8E61,
    0x4015_72BF_8684_2EB2,
    0x3FD0_EE2F_1188_52D6,
    0x3F65_E5B7_3842_17A4,
];
const B3_Q: [u64; 9] = [
    0x3FF0_0000_0000_0001,
    0x4039_B252_B2FA_B379,
    0x405E_DDF2_040D_2BC2,
    0x4067_B39F_C919_4438,
    0x405C_5151_77EB_1641,
    0x403B_B43B_464D_3960,
    0x4005_3036_58EF_44AB,
    0x3FB5_2FC9_914B_DB8B,
    0x3F40_3CAD_F2CC_735D,
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

/// Store every product and sum, the quotient, and the product by x-1 from PC64.
/// See the 119 million retained observations and the independent frozen probes
/// in the ax1 section of docs/research/gamma-family-2026-11-12.md.
fn b1(x: f64) -> f64 {
    if x == 1.0 {
        return 0.0;
    }
    stored_log_gamma_ratio::<super::Native>(x, 1.0, &B1_N, &B1_D)
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
    b3_with::<super::Native>(x)
}

/// The fifth partial of each chain constrains the last four literals. Storing
/// both final sums reproduces the 56 million consecutive B3 observations.
fn b3_with<F: super::Fpu>(x: f64) -> f64 {
    stored_log_gamma_ratio::<F>(x, 2.0, &B3_P, &B3_Q)
}

fn stored_log_gamma_ratio<F: super::Fpu>(x: f64, root: f64, numerator: &[u64], denominator: &[u64]) -> f64 {
    use super::soft87::{CW_PC64_RN as CW, ext_from_f64 as ext};

    let n = stored_horner::<F>(x, numerator);
    let d = stored_horner::<F>(x, denominator);
    let r = F::store(&F::div(&ext(n), &ext(d), CW), CW);
    F::store(&F::mul(&ext(x - root), &ext(r), CW), CW)
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
/// answer; the B3 table is verified separately. The literal `0.7` join
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

/// `2.5066...`: binary64 `sqrt(2 pi)`, used by the large-argument Stirling path.
const SQRT_2PI: f64 = f64::from_bits(0x4004_0D93_1FF6_2706);

// Excel 16.0.20527's seed on [1,2], lowest power first. Own work: the stored-operation
// graph and partial sums were read from spectral lines; integer phase constraints recover
// the coefficient words. All 21,000,000 retained inputs and 100,000 fresh frozen inputs agree.
// See docs/research/gamma-family-2026-11-12.md, ax1. This is a quotient in raw x, not x-1.
const SEED_N: [u64; 8] = [
    0x400b_5834_abbe_a656,
    0x3ff5_1477_3450_caef,
    0x3fe6_6479_5a0f_149d,
    0x3fc3_6c63_6bf9_ecf4,
    0x3fa5_ff5a_1f13_5517,
    0x3f7b_983b_9311_5194,
    0x3f4c_11d5_cf5a_3d24,
    0x3f24_aa35_6fc7_bc7c,
];
const SEED_D: [u64; 7] = [
    0x400b_584a_e618_5b27,
    0x400a_5269_4226_b55e,
    0xbfe8_fd07_2611_5c8e,
    0xbfdd_079c_bb8d_7055,
    0x3fc8_11c8_9b66_9325,
    0xbf99_f2c6_b220_93da,
    0x3f53_f2e0_3e39_0c8c,
];

fn stored_horner<F: super::Fpu>(x: f64, coefficients: &[u64]) -> f64 {
    use super::soft87::{CW_PC64_RN as CW, ext_from_f64 as ext};

    let mut value = f64::from_bits(coefficients[coefficients.len() - 1]);
    for &bits in coefficients[..coefficients.len() - 1].iter().rev() {
        let product = F::store(&F::mul(&ext(x), &ext(value), CW), CW);
        value = F::store(&F::add(&ext(f64::from_bits(bits)), &ext(product), CW), CW);
    }
    value
}

/// The internal seed, including at 1.5; the worksheet's exact half-integer branch is separate.
fn gamma_seed_with<F: super::Fpu>(x: f64) -> f64 {
    use super::soft87::{CW_PC64_RN as CW, ext_from_f64 as ext};

    let product = F::store(&F::mul(&ext(x), &ext(stored_horner::<F>(x, &SEED_D)), CW), CW);
    // -2^-18 is a representative of the measured interval: the final denominator store
    // cannot distinguish it from the recovered -2^-18 - 4.066e-16 anywhere on [1,2].
    let q0 = f64::from_bits(0xbed0_0000_0000_0000);
    let denominator = F::store(&F::add(&ext(q0), &ext(product), CW), CW);
    F::store(&F::div(&ext(stored_horner::<F>(x, &SEED_N)), &ext(denominator), CW), CW)
}

fn gamma_seed(x: f64) -> f64 {
    gamma_seed_with::<super::Native>(x)
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

// Coefficients of exp(B2*t/2 + B4*t^3/12 + ...), lowest power first,
// rounded from the exact rationals. At t = st(1/x) Excel uses degrees 0..15.
const STIRLING_MULTIPLIER: [u64; 16] = [
    0x3ff0_0000_0000_0000, 0x3fb5_5555_5555_5555, 0x3f6c_71c7_1c71_c71c, 0xbf65_f726_8eda_b4c8,
    0xbf2e_13ce_465f_a859, 0x3f49_b0ff_6874_f2c4, 0x3f12_4760_4839_c038, 0xbf43_6773_bdb9_7b48,
    0xbf0b_1d75_d334_6711, 0x3f4b_8239_c670_e690, 0x3f12_e31f_9b79_13ea, 0xbf5f_5dbc_af75_6cde,
    0xbf25_4d24_1144_693f, 0x3f7a_3a69_9f4a_401b, 0x3f41_b33b_019b_3e6f, 0xbf9e_3c8e_8bed_86bb,
];

/// `GAMMA(x)` for `x >= 10`: `sqrt(2 pi) * x^x * exp(-x) * sqrt(1/x)` times the Stirling multiplier.
///
/// With `n = floor(x/2)` and `f = x/2 - n`, `v = x^(x/2)` is `x^n`, by binary powering with stored
/// squares, times `exp(f * ln x)`, where `ln x`, their product and the exponential are stored, and
/// `v` is stored. `1/x` and its square root are stored; so are their product with the stored
/// `exp(-x)`, then that times `v`, and again times `v`, then the constant, then the multiplier.
///
/// Read from Excel 16.0.20527 answers on windows of consecutive doubles from 10 to 170. Lines at
/// `2m`, `8(m-1)`, `32(m-1)` (`m` the significand of `x`) with weights `x/2`, `x/4` and `x/8` are the
/// stored `x^2`, `x^4`, `x^8`; a line at `1/x` in units of `ulp(ln x)`, weight `2f`, is the stored
/// logarithm; the residual bias of +0.66 of 2^-53 is the constant. With those in, the lines left
/// in the misses at 50.3 stood at the frequencies of `1/x` and `sqrt(1/x)`: 17,988 of 18,000 adjacent
/// doubles there, 5,994 of 6,000 from 36 to 170. With Excel's LN, EXP and SQRT leaves,
/// the direct degree-15 multiplier matched all 13,229 fresh degree controls and 16,210 arithmetic controls.
fn gamma_stirling(x: f64) -> f64 {
    use super::soft87::ext_from_f64 as ext;
    use super::{CW, Fpu, Native};

    let half = 0.5 * x;
    let n = half.floor();
    let root = super::excel_exp(super::excel_x87_mul(half - n, super::excel_log(x)));
    let v = super::excel_x87_mul(integer_power(x, n as u32), root);
    let reciprocal = super::excel_x87_recip(x);
    let root_of_reciprocal = Native::store(&Native::sqrt(&ext(reciprocal), CW), CW);
    let scaled = super::excel_x87_mul(
        v,
        super::excel_x87_mul(super::excel_exp(-x), root_of_reciprocal),
    );
    let body = super::excel_x87_mul(super::excel_x87_mul(scaled, v), SQRT_2PI);
    super::excel_x87_mul(body, stored_horner::<Native>(reciprocal, &STIRLING_MULTIPLIER))
}

/// Excel stores pi / GAMMA(m) before dividing by the stored m * sine. With the
/// worksheet leaves this gives all 24,171 frozen reflection observations (Excel 20527).
pub(crate) fn gamma_from_reflection(m: f64, positive_gamma: f64, sine_product: f64) -> f64 {
    let numerator = super::excel_x87_div(core::f64::consts::PI, positive_gamma);
    let magnitude = super::excel_x87_div(numerator, sine_product);
    if magnitude < f64::MIN_POSITIVE {
        0.0
    } else if m.floor() % 2.0 == 0.0 {
        -magnitude
    } else {
        magnitude
    }
}

/// The sine argument uses the distance to the nearest integer.
fn gamma_reflected(m: f64) -> f64 {
    // The positive Gamma leaf has already overflowed throughout this range.
    if m >= 172.0 {
        return 0.0;
    }
    let fraction = m.fract();
    let folded = if fraction > 0.5 {
        1.0 - fraction
    } else {
        fraction
    };
    let sine = super::excel_sin(super::excel_x87_mul(core::f64::consts::PI, folded));
    let z = super::excel_x87_mul(sine, m);
    gamma_from_reflection(m, gamma_stirling(m), z)
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
/// the Excel 16.0.20527 answers at each of `x = 3+y ... 9+y`, given `GAMMA(1+y)`.
/// Below one, start a reciprocal accumulator at 1, divide by each ascending argument and store
/// every quotient. Multiply the final accumulator by the seed at the stored reduced argument.
/// This gives the (0,1) rule `st(seed(st(1+x)) * st(1/x))` and agrees on all 20,000 retained
/// negative random inputs and 144 separate pole controls (ax1, 2026-11-18).
pub(crate) fn gamma_excel(x: f64) -> f64 {
    if x >= 10.0 {
        return gamma_stirling(x);
    }
    if x <= -10.0 {
        return gamma_reflected(-x);
    }
    // At the pole Excel stores the reciprocal before the constant Laurent term.
    if x.abs() <= f64::EPSILON {
        return super::excel_x87_sub(super::excel_x87_recip(x), 1.0 - D2);
    }
    if x < 1.0 {
        let mut argument = x;
        let mut reciprocal = 1.0;
        while argument < 1.0 {
            reciprocal = super::excel_x87_div(reciprocal, argument);
            argument = super::excel_x87_add(argument, 1.0);
        }
        return super::excel_x87_mul(reciprocal, gamma_seed(argument));
    }
    let seed = gamma_seed(super::excel_x87_add(1.0, x - x.floor()));
    product_times_seed(x, seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: compare the hardware x87 with the portable arithmetic on the same seed graph.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn gamma_seed_agrees_on_hardware_and_software_x87() {
        use super::super::{Hard, Soft};
        let mut state = 0x0923_7401_86af_dc51_u64;
        for _ in 0..4096 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let x = f64::from_bits(0x3ff0_0000_0000_0000 | (state >> 12));
            assert_eq!(
                gamma_seed_with::<Hard>(x).to_bits(),
                gamma_seed_with::<Soft>(x).to_bits(),
                "seed at {:016x}",
                x.to_bits()
            );
        }
    }

    /// The read table stays on the true function over the whole band.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn b3_stays_on_the_true_function() {
        for k in 1..=99 {
            let x = 2.0 + f64::from(k) / 100.0;
            let want = libm::lgamma(x);
            assert!(
                (b3(x) - want).abs() <= 8.0 * f64::EPSILON * want.abs(),
                "b3({x})"
            );
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
            assert_eq!(
                recurrence_from(f64::from_bits(x), band).to_bits(),
                answer,
                "GAMMALN({x:#x})"
            );
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
            assert_eq!(
                product_times_seed(f64::from_bits(x), seed).to_bits(),
                answer,
                "GAMMA({x:#x})"
            );
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
        // Both worksheet roots return the positive zero word.
        assert_eq!(gammaln_excel(1.0).to_bits(), 0.0_f64.to_bits());
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
            let x = if i % 2 == 0 {
                1.5 + 2.5 * unit
            } else {
                4.0 + 4.0 * unit
            };
            if x < 4.0 {
                assert_eq!(
                    b2_with::<Hard>(x).to_bits(),
                    b2_with::<Soft>(x).to_bits(),
                    "B2 {x:e}"
                );
                assert_eq!(
                    b3_with::<Hard>(x).to_bits(),
                    b3_with::<Soft>(x).to_bits(),
                    "B3 {x:e}"
                );
            } else {
                assert_eq!(
                    b4_with::<Hard>(x).to_bits(),
                    b4_with::<Soft>(x).to_bits(),
                    "B4 {x:e}"
                );
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
