//! Excel `GAMMALN` / `GAMMALN.PRECISE` positive-argument kernel and worksheet `GAMMA` (dc-j9k.20).
//!
//! Bands of `GAMMALN`, dispatching on the positive argument `x`:
//!   * `x < 0.7`         `core(x+1) - ln(x)`, one double subtract.
//!   * `0.7 <= x < 1.5`  B1: stands in the Cody-Hillstrom 1967 n=7 table in plain double.
//!   * `1.5 <= x <= 3`   B2: stands in SPECFUN's table in x87 continuous arithmetic.
//!   * `3 < x < 8`       the recurrence from the `[2,3)` band, identified: see [`recurrence`].
//!   * `x >= 8`          Stirling: fdlibm w-tail, worksheet LN and stored stages.
//!
//! Excel's bands below 3 are rationals in `x` itself: noise reaches a full unit of the result at
//! `x = 1 + 2^-52` and `2 + 2^-51`, where a polynomial in `x - 1` or `x - 2` could carry none, and the
//! steps of the mean error mark Horner products stored before each coefficient is added. Their
//! tables are not identified, and `[1.5,2)` and `[2,3]` are separate bands.
//!
//! The x87 operations run on `super::x87::raw` on `x86_64`, and on `super::soft87` elsewhere.
use crate::ported::excel_numeric::crt_log;

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

// ---- STIRLING (x>=8): fdlibm e_lgamma_r w1..w6 verbatim -------------------
const W1: f64 = f64::from_bits(0x3FB5_5555_5555_553B);
const W2: f64 = f64::from_bits(0xBF66_C16C_16B0_2E5C);
const W3: f64 = f64::from_bits(0x3F4A_019F_98CF_38B6);
const W4: f64 = f64::from_bits(0xBF43_80CB_8C0F_E741);
const W5: f64 = f64::from_bits(0x3F4B_67BA_4CDA_D5D1);
const W6: f64 = f64::from_bits(0xBF5A_B89D_0B9E_43E4);

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

/// STIRLING band `x >= 8`: fdlibm w-tail with worksheet LN and x87
/// PC64-to-binary64 stores on the q chain and the final add.
///
/// Identified (W109 G3-02 high-band, 2026-08-09 reconstructed graph, replayed
/// 2026-09-12 on 1711 unique x>=8 rows): `ln = LN87(x)`, then
/// `q = DR(DR(DR((x-0.5)*ln) - x) + LS2PI)`, native fdlibm `z*w` Horner,
/// `out = DR(q + corr)`. Scores 1709/1711 (max 1 ULP) versus production
/// native-ln 1701/1711 (max 2). The two remaining misses
/// (`x=0x40215bf4d43f4d44`, `x=0x40234ce3244e3245`) sit between adjacent
/// stored-log effects; they stay open.
fn stirl8(x: f64) -> f64 {
    let lg = super::excel_log(x);
    let q1 = super::excel_x87_mul(x - 0.5, lg);
    let q2 = super::excel_x87_sub(q1, x);
    let q = super::excel_x87_add(q2, LS2PI);
    let z = 1.0 / x;
    let y = z * z;
    let mut w = W6;
    w = w * y + W5;
    w = w * y + W4;
    w = w * y + W3;
    w = w * y + W2;
    w = w * y + W1;
    super::excel_x87_add(q, z * w)
}

// -------------------------------------------------------------------------
// B2 / B4 extended-precision bands.
// -------------------------------------------------------------------------

/// B2 band `[1.5, 4.0)`: SPECFUN's `t*(D2 + t*(P/Q))`, `t = x-2`,
/// with one final binary64 store after continuous extended arithmetic.
fn b2(x: f64) -> f64 {
    // The top of the band, as Excel gives it: one ulp above stored ln 2. It is read, because the
    // SPECFUN stand-in cannot give it, and GAMMA.DIST's shape 2 takes it in every prefactor.
    if x == 3.0 {
        return f64::from_bits(0x3FE6_2E42_FEFA_39F0);
    }
    b2_with::<super::Native>(x)
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
fn recurrence(x: f64) -> f64 {
    recurrence_from(x, b2(2.0 + (x - x.floor())))
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
    } else if x <= 3.0 {
        b2(x)
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
        inner - crt_log(x)
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

// SPECFUN GAMMA (Cody): `1 + N(y)/D(y)` for `Gamma(1+y)`, `0 <= y < 1`. Excel's own seed on
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

/// The multiplicative Stirling series `1 + 1/(12x) + ...` for `x >= 10`. Excel's stored value is
/// binary64; the true series stands in for its coefficients, which are not identified.
fn stirling_series(x: f64) -> f64 {
    let z = 1.0 / x;
    let u = z * z;
    let mut c = 3617.0 / 122400.0;
    for term in [1.0 / 156.0, 691.0 / 360360.0, 1.0 / 1188.0, 1.0 / 1680.0, 1.0 / 1260.0, 1.0 / 360.0, 1.0 / 12.0] {
        c = term - u * c;
    }
    c *= z;
    1.0 + c * (1.0 + c / 2.0 * (1.0 + c / 3.0 * (1.0 + c / 4.0 * (1.0 + c / 5.0))))
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

/// `GAMMA(x)` for `x >= 10`: `sqrt(2 pi) * x^x * exp(-x) / sqrt(x) * w`, with `w` the Stirling
/// series.
///
/// With `n = floor(x/2)` and `f = x/2 - n`, `v = x^(x/2)` is `x^n`, by binary powering with stored
/// squares, times `exp(f * ln x)`, where `ln x`, their product and the exponential are stored, and
/// `v` is stored. Then `u = v * exp(-x) / sqrt(x)` runs in the 64-bit registers from the stored
/// `exp(-x)` and an unstored square root, and each of `u`, `u * v`, the product with the constant
/// and the product with `w` is stored.
///
/// Read from Excel 16.0.20527 answers on windows of 2,048 and 4,096 consecutive doubles from 10
/// to 170. Lines at `2m`, `8(m-1)`, `32(m-1)` (`m` the significand of `x`) with weights `x/2`, `x/4`
/// and `x/8` are the stored `x^2`, `x^4`, `x^8`; a line at `1/x` in units of `ulp(ln x)`, weight
/// `2f`, is the stored logarithm; the residual bias of +0.66 of 2^-53 is the constant. Of 59,200
/// answers from 36 to 170, 69% agree in every bit with `w` the true series; the misses are 1 or 2
/// ULP and stand as the next target.
fn gamma_stirling(x: f64) -> f64 {
    use super::soft87::ext_from_f64 as ext;
    use super::{CW, Fpu, Native};

    let half = 0.5 * x;
    let n = half.floor();
    let root = super::excel_exp(super::excel_x87_mul(half - n, super::excel_log(x)));
    let v = super::excel_x87_mul(integer_power(x, n as u32), root);
    let scaled = Native::store(
        &Native::div(&Native::mul(&ext(v), &ext(super::excel_exp(-x)), CW), &Native::sqrt(&ext(x), CW), CW),
        CW,
    );
    let body = super::excel_x87_mul(super::excel_x87_mul(scaled, v), SQRT_2PI);
    super::excel_x87_mul(body, stirling_series(x))
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
            } else {
                assert_eq!(b4_with::<Hard>(x).to_bits(), b4_with::<Soft>(x).to_bits(), "B4 {x:e}");
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn stirling_ln87_dr_matches_current_build_hits_and_keeps_two_open() {
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
        // Two remaining open rows: still exactly 1 ULP below Excel.
        assert_eq!(
            gammaln_excel(f64::from_bits(0x40215bf4d43f4d44)).to_bits(),
            0x4023d98694477878
        );
        assert_eq!(
            gammaln_excel(f64::from_bits(0x40234ce3244e3245)).to_bits(),
            0x40280a8dd6771c99
        );
    }
}
